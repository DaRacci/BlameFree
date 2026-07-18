//! In-process harness execution via library calls.
//!
//! Calls `crb_harness::pipeline::evaluate` directly, handling only
//! EvalConfig setup, SSE event forwarding, and result file writing.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use crate::api::runs::BenchmarkConfig;
use crate::server::ActiveRun;
use crb_agents::prompts::PromptLibrary;
use crb_benchmark::pr;
use crb_harness::eval::EvalConfig;
use crb_harness::pipeline;
use crb_reporting::golden::load_golden_datasets;
use crb_reporting::write_report;
use crb_rules::RuleSet;
use crb_shared::diff::Diff;
use crb_shared::url::parse_github_url;
use crb_tools::linters::config::load_linter_config;
use crb_types::RunEvent;
use crb_types::benchmark::metrics::{Metrics, MetricsProvider};
use mti::prelude::{MagicTypeIdExt, V7};
use rig_core::client::CompletionClient;
use rig_core::client::ProviderClient;
use rig_core::providers::openrouter;
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
    active_runs: Arc<RwLock<HashMap<String, ActiveRun>>>,
    dataset_dir: &Path,
) -> anyhow::Result<()> {
    let output_subdir = output_dir.join(run_id);
    fs::create_dir_all(&output_subdir)?;

    let client = Arc::new(
        openrouter::Client::from_env()
            .map_err(|e| anyhow::anyhow!("Failed to create OpenAI client: {e}"))?,
    );
    let judge = client
        .agent(&config.judge_model)
        .preamble("You are evaluating AI code review tools.\nDetermine if the candidate issue matches the golden comment.\nRespond with ONLY a JSON object: {\"reasoning\":\"...\",\"match\":true/false,\"confidence\":0.0-1.0}")
        .temperature(0.3)
        .build();

    let prompt_lib = PromptLibrary::get_instance();
    let agents: Vec<&'static _> = if config.roles.is_empty() {
        prompt_lib.agents()
    } else {
        config
            .roles
            .iter()
            .filter_map(|r| prompt_lib.config(r.trim()))
            .collect()
    };
    anyhow::ensure!(!agents.is_empty(), "No agents resolved from PromptLibrary");
    let agents: &'static [&'static _] = Box::leak(agents.into_boxed_slice());

    let bench_dir = benchmark_dir
        .unwrap_or(Path::new("benchmark"))
        .to_path_buf();
    let reasoning_effort = config
        .reasoning_effort
        .as_deref()
        .filter(|s| !s.is_empty() && *s != "none")
        .and_then(ReasoningEffort::from_str);

    let ruleset = RuleSet::load_from_dir(Path::new(".crb/rules/"))
        .ok()
        .map(Arc::new);
    let linter_configs = if config.linters_only {
        None
    } else {
        load_linter_config("linters.toml").ok().map(Arc::new)
    };

    // --- Dataset ---
    let all_prs = load_golden_datasets(dataset_dir)?;
    let filtered_prs = config
        .pr_filter
        .as_ref()
        .map(|f| pr::filter_prs_by_pattern(all_prs.clone(), f))
        .unwrap_or(all_prs);

    if filtered_prs.is_empty() {
        warn!("No PRs to evaluate");
        let _ = webui_tx.send(RunEvent::RunFinished {
            total_prs: 0,
            aggregated: Metrics::default(),
            total_cost: 0.0,
            total_tokens: 0,
            total_agent_calls: 0,
        });
        return Ok(());
    }

    let total = filtered_prs.len();
    let mut results = Vec::with_capacity(total);
    let start = std::time::Instant::now();

    // --- Evaluate each PR ---
    for pr in filtered_prs {
        let diff_str = parse_github_url(&pr.url)
            .ok()
            .and_then(|(owner, repo, num)| {
                crb_benchmark::diff_cache::load_cached_diff(&bench_dir, &owner, &repo, num)
            })
            .unwrap_or_default();

        let mut aggregate = Metrics::default();
        let config = EvalConfig {
            review_id: "run".create_type_id::<V7>(),
            client: client.clone(),
            context: todo!(),
            strategy: todo!(),
            model: todo!(),
            reasoning_effort,
            cache: todo!(),
            cost_tracker: todo!(),
            dashboard_tx: todo!(),
            agents,
            repo_root: todo!(),
            max_findings: todo!(),
            ruleset,
            template_vars: todo!(),
        };

        match pipeline::evaluate(Diff::new(diff_str), &cfg).await {
            Ok(findings) => {
                let result = pipeline::build_pr_result(
                    &findings,
                    &cfg,
                    &pr.pr_title,
                    &pr.url,
                    pr.comments.len(),
                )
                .await;
                let _ = write_report(&[result.clone()], &output_subdir);
                aggregate += result.metrics.clone();
                results.push(result);

                let n = results.len();
                {
                    let mut runs = active_runs.write().await;
                    if let Some(run) = runs.get_mut(run_id) {
                        run.completed_prs = n;
                    }
                }
                let _ = webui_tx.send(RunEvent::RunProgress {
                    completed_prs: n,
                    total_prs: total,
                    elapsed_secs: start.elapsed().as_secs_f64(),
                    total_cost: 0.0,
                    current_pr: results.last().map(|r| r.pr_title.clone()),
                });
            }
            Err(e) => error!("PR '{}' evaluation failed: {e}", pr.pr_title),
        }
    }

    // --- Post-run: summary and RunFinished ---
    {
        let mut runs = active_runs.write().await;
        if let Some(run) = runs.get_mut(run_id) {
            run.finished = true;
            run.completed_prs = results.len();
        }
    }

    let _ = write_report(&results, &output_subdir);

    let mut agg = Metrics::default();
    let mut total_cost = 0.0f64;
    let mut total_tokens = 0usize;
    for r in &results {
        agg += r.metrics.clone();
        if let Some(ref c) = r.cost {
            total_cost += c.total_cost();
            let (tin, tout) = c.total_tokens().await;
            total_tokens += (tin + tout) as usize;
        }
    }

    let summary = serde_json::json!({
        "run_id": run_id,
        "model": config.model,
        "judge_model": config.judge_model,
        "total_prs": results.len(),
        "duration_secs": start.elapsed().as_secs_f64(),
        "aggregate_metrics": {
            "avg_precision": agg.precision(),
            "avg_recall": agg.recall(),
            "avg_f1": agg.f1(),
            "total_true_positives": agg.true_positives,
            "total_false_positives": agg.false_positives,
            "total_false_negatives": agg.false_negatives,
        },
        "total_tokens": total_tokens,
        "total_cost_usd": total_cost,
    });
    let summary_path = output_subdir.join(crb_harness::paths::SUMMARY_FILE);
    if let Ok(json) = serde_json::to_string_pretty(&summary) {
        let _ = fs::write(&summary_path, &json);
    }

    let _ = webui_tx.send(RunEvent::RunFinished {
        total_prs: results.len(),
        aggregated: agg,
        total_cost,
        total_tokens,
        total_agent_calls: results.len(),
    });

    info!(run_id = %run_id, prs = results.len(), elapsed_secs = %start.elapsed().as_secs_f64(), "Harness run finished");
    Ok(())
}
