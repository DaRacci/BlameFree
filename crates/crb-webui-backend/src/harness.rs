//! In-process harness execution via library calls.
//!
//! Previously this module spawned `crb-harness --dashboard-events` as a
//! Now it calls `crb_harness::evaluate_pr`
//! directly, forwarding progress events to all SSE clients via the same
//! broadcast channel.

use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use crb_agents::prompts::PromptLibrary;
use crb_benchmark::pr;
use crb_harness::{EvalConfig, EvalStrategy, evaluate_pr, load_pr_diff};
use crb_reporting::golden::load_golden_datasets;
use crb_reporting::{PrResult, write_report};
use crb_rules::RuleSet;
use crb_shared::benchmark;
use crb_shared::metrics::MetricsOutput;
use rig_core::client::ProviderClient;
use tokio::sync::{RwLock, broadcast};
use tracing::error;
use tracing::info;
use tracing::warn;

use crate::api::runs::BenchmarkConfig;
use crate::server::ActiveRun;
use crate::server::AppState;
use crb_types::Metrics;
use crb_types::RunEvent;

/// Run the harness inline, calling library functions directly.
///
/// This function:
/// 1. Sets up the OpenAI client, judge agent, prompt library, rules, linters
/// 2. Loads the dataset and filters PRs according to config
/// 3. Evaluates each PR via `crb_harness::evaluate_pr`
/// 4. Writes per-PR result files + a `_summary.json`
/// 5. Sends a final `RunFinished` event
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
    let cache_dir = output_dir.join(crb_cache::paths::CACHE_DIR_NAME);

    info!(
        run_id = %run_id,
        output_dir = %output_subdir.display(),
        dataset = %dataset_dir.display(),
        roles = %config.roles.as_deref().unwrap_or(""),
        concurrency = config.concurrency,
        "Starting harness run via library"
    );

    let client = rig_core::providers::openai::Client::from_env()
        .map_err(|e| anyhow::anyhow!("Failed to create OpenAI client: {e}"))?;

    let judge = client
        .agent(&config.judge_model)
        .preamble(
            "You are evaluating AI code review tools.\n\
            Determine if the candidate issue matches the golden (expected) comment.\n\
            \n\
            Golden Comment (the issue we're looking for):\n\
            {golden_comment}\n\
            \n\
            Candidate Issue (from the tool's review):\n\
            {candidate}\n\
            \n\
            Instructions:\n\
            - Determine if the candidate identifies the SAME underlying issue as the golden comment\n\
            - Accept semantic matches - different wording is fine if it's the same problem\n\
            - Focus on whether they point to the same bug, concern, or code issue\n\
            \n\
            Respond with ONLY a JSON object:\n\
            {\"reasoning\": \"brief explanation\", \"match\": true/false, \"confidence\": 0.0-1.0}",
        )
        .temperature(0.3)
        .build();

    let prompt_lib = Arc::new(PromptLibrary::get_instance());

    let ruleset = {
        let rules_dir = Path::new(".crb/rules/");
        match RuleSet::load_from_dir(rules_dir) {
            Ok(rs) => {
                info!(
                    "Loaded {} rules from {}",
                    rs.rules.len() + rs.always_rules.len(),
                    rules_dir.display()
                );
                Some(Arc::new(rs))
            }
            Err(e) => {
                warn!("Failed to load rules from {}: {e}", rules_dir.display());
                None
            }
        }
    };

    let linter_config_path = Path::new("linters.toml");
    let linter_configs = if linter_config_path.exists() && !config.skip_linters {
        match crb_tools::load_linter_config("linters.toml") {
            Ok(configs) => {
                info!("Loaded {} linter(s) from linters.toml", configs.len());
                Some(Arc::new(configs))
            }
            Err(e) => {
                warn!("Failed to load linter config: {e}. Linters disabled.");
                None
            }
        }
    } else {
        None
    };

    info!("Loading golden datasets from: {}", dataset_dir.display());
    let all_prs = match load_golden_datasets(dataset_dir) {
        Ok(prs) => prs,
        Err(e) => {
            error!("Failed to load dataset: {e}");
            return Err(e.context("Failed to load golden datasets"));
        }
    };
    info!("Loaded {} PR entries total", all_prs.len());

    let filtered_prs = if let Some(ref filter) = config.pr_filter {
        pr::filter_prs_by_pattern(all_prs, filter)
    } else {
        all_prs
    };

    info!("After PR filter: {} PR(s) to evaluate", filtered_prs.len());

    if filtered_prs.is_empty() {
        warn!("No PRs to evaluate");
        let _ = webui_tx.send(RunEvent::RunFinished {
            // Ignore — receiver may have disconnected
            total_prs: 0,
            aggregated: AggregateMetrics::default(),
            total_cost: 0.0,
            total_tokens: 0,
            total_agent_calls: 0,
        });
        return Ok(());
    }

    let total_prs = filtered_prs.len();

    // Use the webui broadcast channel directly as the dashboard_tx
    // (both sides now use crb_types::RunEvent, no bridge needed).
    let dashboard_tx: Option<broadcast::Sender<RunEvent>> = Some(webui_tx.clone());

    // If no explicit benchmark_dir was passed, default to the `benchmark/`
    // subdirectory, which is the standard project convention (contains
    // base-repos/, diffs/, worktrees/).

    let sem = Arc::new(tokio::sync::Semaphore::new(config.concurrency));
    let mut set = tokio::task::JoinSet::new();
    let start_time = std::time::Instant::now();

    let cache_dir_opt: Option<PathBuf> = if config.use_cache {
        let cd = cache_dir.clone();
        std::fs::create_dir_all(&cd)?;
        Some(cd)
    } else {
        None
    };

    // Pre-wrap values that need to be owned inside the spawn loop
    let bench_dir = benchmark_dir
        .unwrap_or_else(|| {
            warn!("No --benchmark-dir set; defaulting to 'benchmark/' directory");
            Path::new("benchmark")
        })
        .to_path_buf();
    let model_owned = Arc::new(config.model.clone());
    let roles_owned = config.roles.clone();
    let reasoning_effort_owned = Arc::new(config.reasoning_effort.clone().unwrap_or_default());

    for pr in filtered_prs {
        let permit = sem.clone().acquire_owned().await.expect("semaphore closed");
        let client = client.clone();
        let judge = judge.clone();
        let model = Arc::clone(&model_owned);
        let bench_dir = bench_dir.clone();
        let linter_configs = linter_configs.clone();
        let skip_consensus = config.skip_consensus;
        let ruleset = ruleset.clone();
        let prompt_lib = prompt_lib.clone();
        let roles = roles_owned.clone();
        let max_findings = config.max_findings;
        let cache_dir_opt = cache_dir_opt.clone();
        let dashboard_tx = dashboard_tx.clone();
        let reasoning_effort = Arc::clone(&reasoning_effort_owned);

        set.spawn(async move {
            let _permit = permit;
            let cost_tracker = Arc::new(crb_harness::AnalyticsTracker::new());
            let cfg = crb_harness::EvalConfig {
                strategy: if skip_consensus {
                    crb_harness::EvalStrategy::Single
                } else {
                    crb_harness::EvalStrategy::Panel
                },
                model: model.to_string(),
                judge_model: String::new(), // not tracked in this path
                reasoning_effort: {
                    let s = reasoning_effort.as_str();
                    if s.is_empty() || s == "none" {
                        None
                    } else {
                        crb_harness::model_capabilities::ReasoningEffort::from_str(s)
                    }
                },
                client: client.clone(),
                judge: judge.clone(),
                cache: None,
                cost_tracker: cost_tracker.clone(),
                dashboard_tx: dashboard_tx.clone(),
                roles: roles.clone(),
                max_findings,
                linters_only: false,
                linter_configs: linter_configs.as_ref().map(|a| (*a).clone()),
                ruleset: ruleset.as_ref().map(|a| (*a).clone()),
                template_vars: None,
            };
            let diff = crb_harness::load_pr_diff(&pr, &bench_dir).await?;
            crb_harness::evaluate_pr(&pr, &diff, &cfg).await
        });
    }

    let mut results: Vec<PrResult> = Vec::new();
    let mut total_cost = 0.0f64;
    let mut total_tokens = 0usize;
    let mut total_tp = 0usize;
    let mut total_fp = 0usize;
    let mut total_fn = 0usize;
    let mut total_agent_calls = 0usize;
    let mut completed_prs = 0usize;

    while let Some(res) = set.join_next().await {
        match res {
            Ok(Ok(result)) => {
                info!("Completed: {}", result.pr_title);
                completed_prs += 1;

                // Accumulate totals
                total_tp += result.metrics.true_positives;
                total_fp += result.metrics.false_positives;
                total_fn += result.metrics.false_negatives;
                total_agent_calls += 4;
                if let Some(ref c) = result.cost {
                    total_cost += c.total_usd;
                    total_tokens += c.agent_tokens_in
                        + c.agent_tokens_out
                        + c.judge_tokens_in
                        + c.judge_tokens_out;
                }

                results.push(result);

                // Update active run state
                {
                    let mut runs = active_runs.write().await;
                    if let Some(run) = runs.get_mut(run_id) {
                        run.completed_prs = completed_prs;
                    }
                }

                // Send RunProgress
                let _ = webui_tx.send(RunEvent::RunProgress {
                    // Ignore — receiver may have disconnected
                    completed_prs,
                    total_prs,
                    elapsed_secs: start_time.elapsed().as_secs_f64(),
                    total_cost,
                    current_pr: results.last().map(|r| r.pr_title.clone()),
                });
            }
            Ok(Err(e)) => {
                error!("PR evaluation failed: {e}");
            }
            Err(e) => {
                error!("Join error: {e}");
            }
        }
    }

    let elapsed = start_time.elapsed();

    let MetricsOutput {
        precision: avg_precision,
        recall: avg_recall,
        f1: avg_f1,
    } = crb_shared::metrics::compute_aggregate_metrics(total_tp, total_fp, total_fn);

    {
        let mut runs = active_runs.write().await;
        if let Some(run) = runs.get_mut(run_id) {
            run.finished = true;
            run.completed_prs = completed_prs;
        }
    }

    info!(
        run_id = %run_id,
        prs_completed = completed_prs,
        prs_total = total_prs,
        elapsed_secs = elapsed.as_secs_f64(),
        total_cost = total_cost,
        "Harness run finished"
    );

    std::fs::create_dir_all(&output_subdir)?;
    if let Err(e) = write_report(&results, &output_subdir) {
        error!("Failed to write per-PR results: {e}");
    }

    if let Err(e) = crb_harness::write_summary(
        &output_subdir,
        &config.model,
        &config.judge_model,
        &results,
        elapsed,
    ) {
        error!("Failed to write summary: {e}");
    }

    let _ = webui_tx.send(RunEvent::RunFinished {
        // Ignore — receiver may have disconnected
        total_prs: results.len(),
        aggregated: AggregateMetrics {
            true_positives: total_tp,
            false_positives: total_fp,
            false_negatives: total_fn,
            precision: avg_precision,
            recall: avg_recall,
            f1: avg_f1,
        },
        total_cost,
        total_tokens,
        total_agent_calls,
    });

    crb_reporting::print_terminal_summary(&results).await;

    Ok(())
}
