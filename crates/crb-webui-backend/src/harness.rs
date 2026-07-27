//! In-process harness execution via library calls.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use crate::server::ActiveRun;
use crb_agents::AgentEntry;
use crb_agents::prompts::PromptLibrary;
use crb_harness::eval::{EvalConfig, EvalContext, EvalStrategy};
use crb_harness::pipeline;
use crb_reporting::cost::AnalyticsTracker;
use crb_reporting::golden::load_golden_datasets;
use crb_reporting::write_report;
use crb_shared::diff::Diff;
use crb_shared::string::sanitize_filename;
use crb_shared::url::parse_github_url;
use crb_types::RunEvent;
use crb_types::benchmark::judge::JudgeVerdict;
use crb_types::benchmark::result::PrResult;
use crb_types::cost::AnalyticsSnapshot;
use crb_types::review::{Review, ReviewMetadata, ReviewStatus};
use crb_types::vcs::pr::PrMeta;
use crb_types::vcs::repository::{RemoteRepositoryMeta, VCSPlatform};
use crb_types::wrappers::Model;
use mti::prelude::MagicTypeId;
use mti::prelude::{MagicTypeIdExt, V7};
use rig_core::client::ProviderClient;
use rig_core::providers::openrouter;
use riv_stor::traits::Store;
use tokio::sync::{RwLock, broadcast};
use tracing::{error, info, warn};

/// Run the harness inline, calling library functions directly.
///
/// Handles: EvalConfig setup, dataset loading, per-PR evaluation via
/// `pipeline::evaluate`, per-PR result files, SSE events, and summary.
pub async fn run_harness(
    run_id: &str,
    config: &BenchmarkConfig,
    output_dir: &Path,
    benchmark_dir: Option<&Path>,
    webui_tx: broadcast::Sender<RunEvent>,
    active_runs: Arc<RwLock<HashMap<MagicTypeId, ActiveRun>>>,
    dataset_dir: &Path,
    store: Arc<impl Store>,
) -> anyhow::Result<()> {
    let output_subdir = output_dir.join(run_id);
    fs::create_dir_all(&output_subdir)?;

    let client = Arc::new(
        openrouter::Client::from_env()
            .map_err(|e| anyhow::anyhow!("Failed to create OpenRouter client: {e}"))?,
    );

    let prompt_lib = PromptLibrary::get_instance();
    let agents: Vec<&'static AgentEntry> = if config.agents.is_empty() {
        prompt_lib.agents()
    } else {
        config
            .agents
            .iter()
            .filter_map(|r| prompt_lib.config(&r.abbreviation))
            .collect()
    };
    anyhow::ensure!(!agents.is_empty(), "No agents resolved from PromptLibrary");
    let agents: &'static [&'static AgentEntry] = Box::leak(agents.into_boxed_slice());

    let bench_dir = benchmark_dir
        .unwrap_or(Path::new("benchmark"))
        .to_path_buf();

    let wrapped_model = Model(config.model.clone());

    // --- Dataset ---
    let all_prs = load_golden_datasets(dataset_dir)?;

    // FIXME: pr_filter needs runtime-splitting, but filter_prs_by_pattern requires
    // `&'static str`.  Skipping filtering for now.
    let filtered_prs = all_prs;

    if filtered_prs.is_empty() {
        warn!("No PRs to evaluate");
        let _ = webui_tx.send(RunEvent::ReviewCompleted {
            review_id: run_id.to_string().create_type_id::<V7>(),
            analytics: AnalyticsSnapshot::default(),
        });
        return Ok(());
    }

    let total = filtered_prs.len();
    let mut results: Vec<PrResult> = Vec::with_capacity(total);
    let start = std::time::Instant::now();

    // --- Evaluate each PR ---
    for (_, pr_entry) in filtered_prs.into_iter().enumerate() {
        let pr_key = sanitize_filename(&pr_entry.pr_title);
        let review_id = format!("{run_id}/{pr_key}").create_type_id::<V7>();

        let diff_str = parse_github_url(&pr_entry.url)
            .ok()
            .and_then(|(owner, repo, num)| {
                crb_benchmark::diff_cache::load_cached_diff(&bench_dir, &owner, &repo, num)
            })
            .unwrap_or_default();

        let cost_tracker = Arc::new(AnalyticsTracker::new());

        let context = EvalContext {
            repo_root: output_subdir.clone(),
            ruleset: None,
            repository: RemoteRepositoryMeta {
                platform: VCSPlatform::GitHub,
                owner: String::new(),
                name: String::new(),
            },
            pull_request: Some(PrMeta {
                title: pr_entry.pr_title.clone(),
                url: pr_entry.url.clone(),
                number: 0,
            }),
        };

        let cfg = EvalConfig {
            review_id: review_id.clone(),
            context,
            strategy: EvalStrategy::Panel,
            model: wrapped_model.clone(),
            reasoning_effort: config.reasoning_effort,
            client: client.clone(),
            cache: None,
            cost_tracker: cost_tracker.clone(),
            dashboard_tx: Some(webui_tx.clone()),
            agents,
            max_findings: config.max_findings,
            template_vars: None,
        };

        match pipeline::evaluate(Diff::new(diff_str), &cfg).await {
            Ok(findings) => {
                // Build a PrResult from the findings.
                // FIXME: Add proper golden-comment comparison and JudgeVerdicts.
                #[allow(deprecated)]
                let result = PrResult {
                    id: review_id,
                    golden_comments: pr_entry.comments.clone(),
                    benchmark_id: None,
                    findings: findings
                        .into_iter()
                        .map(|mut f| {
                            f.verdict = Some(JudgeVerdict {
                                id: None,
                                finding_id: None, // TODO
                                linked_comment_id: None,
                                reasoning: "Pending judge evaluation".to_string(),
                                match_: false,
                                confidence: 0.0,
                            });
                            f
                        })
                        .collect(),
                };
                let _ = write_report(&[result.clone()], &output_subdir);
                results.push(result);

                // Save per-PR result to store
                if let Some(last) = results.last() {
                    let _ = store.save::<PrResult>(last).await;
                }

                let n = results.len();
                {
                    let mut runs = active_runs.write().await;
                    if let Some(run) = runs.get_mut(&run_id.to_string().create_type_id::<V7>()) {
                        // Progress tracking: n PRs completed
                    }
                }
            }
            Err(e) => error!("PR '{}' evaluation failed: {e}", pr_entry.pr_title),
        }
    }

    // --- Post-run: summary and notification ---
    {
        let mut runs = active_runs.write().await;
        if let Some(run) = runs.get_mut(&run_id.to_string().create_type_id::<V7>()) {
            // Run completed — will be removed from active_runs when result is on disk
        }
    }

    let _ = write_report(&results, &output_subdir);

    let overall_review = Review {
        id: run_id.to_string().create_type_id::<V7>(),
        agent_sessions: Vec::new(),
        analytics: Some(AnalyticsSnapshot::default()),
        duration: Some(std::time::Duration::from_secs_f64(
            start.elapsed().as_secs_f64(),
        )),
        status: ReviewStatus::Completed,
        metadata: ReviewMetadata::Plain,
    };
    let _ = store.save::<Review>(&overall_review).await;

    let _ = webui_tx.send(RunEvent::ReviewCompleted {
        review_id: run_id.to_string().create_type_id::<V7>(),
        analytics: AnalyticsSnapshot::default(), // FIXME: aggregate analytics
    });

    info!(run_id = %run_id, prs = results.len(), elapsed_secs = %start.elapsed().as_secs_f64(), "Harness run finished");
    Ok(())
}
