//! In-process harness execution via library calls.
//!
//! Calls `crb_harness::pipeline::evaluate` directly, handling only
//! EvalConfig setup, SSE event forwarding, and result file writing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::api::runs::BenchmarkConfig;
use crate::server::ActiveRun;
use crb_agents::agent::AgentEntry;
use crb_agents::prompts::PromptLibrary;
use crb_benchmark::pr;
use crb_harness::eval::{EvalConfig, EvalStrategy};
use crb_harness::model_capabilities::ReasoningEffort;
use crb_harness::pipeline;
use crb_reporting::cost::AnalyticsTracker;
use crb_reporting::golden::load_golden_datasets;
use crb_reporting::write_report;
use crb_rules::RuleSet;
use crb_shared::diff::Diff;
use crb_shared::url::parse_github_url;
use crb_tools::linters::config::LinterConfig;
use crb_tools::linters::config::load_linter_config;
use crb_types::RunEvent;
use crb_types::benchmark::metrics::{Metrics, MetricsProvider};
use crb_types::wrappers::Model;
use crb_webui_shared::config::DatasetConfig;
use rig_core::client::CompletionClient;
use rig_core::client::ProviderClient;
use rig_core::providers::openai;
use rig_core::tool::server::ToolServer;
use rig_core::tool::server::ToolServerHandle;
use tokio::sync::{RwLock, broadcast};
use tracing::{error, info, warn};

/// Build an `EvalConfig` from a `BenchmarkConfig` and runtime dependencies.
///
/// Encapsulates the mapping so callers don't need to construct `EvalConfig`
/// field-by-field.
#[allow(clippy::too_many_arguments)]
fn build_eval_config(
    run_id: &str,
    config: &BenchmarkConfig,
    client: Arc<openai::Client>,
    judge: rig_core::agent::Agent<openai::responses_api::ResponsesCompletionModel>,
    agents: &'static [&'static AgentEntry],
    webui_tx: broadcast::Sender<RunEvent>,
    repo_root: PathBuf,
    reasoning_effort: Option<ReasoningEffort>,
    linter_configs: Option<Arc<HashMap<String, LinterConfig>>>,
    ruleset: Option<Arc<RuleSet>>,
) -> EvalConfig {
    EvalConfig {
        review_id: run_id.to_string(),
        strategy: if config.skip_consensus {
            EvalStrategy::Single
        } else {
            EvalStrategy::Panel
        },
        model: Model(config.model.clone()),
        reasoning_effort,
        client,
        cache: None,
        cost_tracker: Arc::new(AnalyticsTracker::new()),
        dashboard_tx: Some(webui_tx),
        agents,
        repo_root,
        max_findings: config.max_findings,
        judge_model: config.judge_model.clone(),
        judge,
        linters_only: false,
        linter_configs,
        ruleset,
        template_vars: None,
    }
}

/// Apply dataset-level defaults from `dataset.toml` (if present) as overrides
/// on a `BenchmarkConfig`.
fn apply_dataset_defaults(config: &BenchmarkConfig, dataset_dir: &Path) -> BenchmarkConfig {
    let dataset_config_path = dataset_dir.join("dataset.toml");
    if !dataset_config_path.exists() {
        return config.clone();
    }
    let content = match std::fs::read_to_string(&dataset_config_path) {
        Ok(c) => c,
        Err(e) => {
            warn!("Failed to read {}: {e}", dataset_config_path.display());
            return config.clone();
        }
    };
    let ds_config: DatasetConfig = match toml::from_str(&content) {
        Ok(c) => c,
        Err(e) => {
            warn!(
                "Failed to parse {}: {e}. Ignoring dataset defaults.",
                dataset_config_path.display()
            );
            return config.clone();
        }
    };
    let defaults = &ds_config.defaults;
    let mut overridden = config.clone();
    if let Some(ref model) = defaults.model {
        overridden.model = model.clone();
    }
    if let Some(max_findings) = defaults.max_findings {
        overridden.max_findings = max_findings;
    }
    if let Some(ref roles) = defaults.roles {
        if !roles.is_empty() {
            overridden.roles = roles.clone();
        }
    }
    overridden
}

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
    std::fs::create_dir_all(&output_subdir)?;

    // --- Apply dataset-level defaults ---
    let config = apply_dataset_defaults(config, dataset_dir);
    info!(
        "Benchmark config after dataset defaults: model={}, roles={:?}, max_findings={}",
        config.model, config.roles, config.max_findings
    );

    // --- EvalConfig setup ---
    let client = Arc::new(
        openai::Client::from_env()
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
        let cfg = build_eval_config(
            run_id,
            &config,
            client.clone(),
            judge.clone(),
            agents,
            webui_tx.clone(),
            bench_dir.clone(),
            reasoning_effort,
            linter_configs.clone(),
            ruleset.clone(),
        );

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
        let _ = std::fs::write(&summary_path, &json);
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
