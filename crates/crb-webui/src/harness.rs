//! In-process harness execution via library calls.
//!
//! Previously this module spawned `crb-harness --dashboard-events` as a
//! subprocess.  Now it calls `crb_harness::evaluate_pr_with_postprocessing`
//! directly, forwarding progress events to all SSE clients via the same
//! broadcast channel.

use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use crb_agents::prompts::PromptLibrary;
use crb_dashboard::DashboardEvent as HarnessEvent;
use crb_judge::build_judge;
use crb_reporting::{load_golden_datasets, write_report, PrResult};
use crb_rules::RuleSet;
use rig_core::client::ProviderClient;
use tokio::sync::{broadcast, RwLock};

use crate::api::BenchmarkConfig;
use crate::events::DashboardEvent;
use crate::events::MetricsData;
use crate::events::AggregateMetrics;
use crate::server::ActiveRun;
use crate::server::AppState;

/// Run the harness inline, calling library functions directly.
///
/// This function:
/// 1. Sets up the OpenAI client, judge agent, prompt library, rules, linters
/// 2. Loads the dataset and filters PRs according to config
/// 3. Spawns a bridge task that converts `crb_dashboard::DashboardEvent` to
///    the web UI's `DashboardEvent` on the SSE broadcast channel
/// 4. Evaluates each PR via `crb_harness::evaluate_pr_with_postprocessing`
/// 5. Writes per-PR result files + a `_summary.json`
/// 6. Sends a final `RunFinished` event
pub async fn run_harness(
    run_id: &str,
    config: &BenchmarkConfig,
    output_dir: &Path,
    benchmark_dir: Option<&Path>,
    webui_tx: broadcast::Sender<DashboardEvent>,
    active_runs: Arc<RwLock<HashMap<String, ActiveRun>>>,
    dataset_dir: &Path,
) -> anyhow::Result<()> {
    let output_subdir = output_dir.join(run_id);
    let cache_dir = output_dir.join("_cache");

    tracing::info!(
        run_id = %run_id,
        output_dir = %output_subdir.display(),
        dataset = %dataset_dir.display(),
        roles = %config.roles,
        concurrency = config.concurrency,
        "Starting harness run via library"
    );

    // ── OpenAI client ────────────────────────────────────────────────────────
    let client = rig_core::providers::openai::Client::from_env()
        .map_err(|e| anyhow::anyhow!("Failed to create OpenAI client: {e}"))?;

    // ── Judge agent ──────────────────────────────────────────────────────────
    let judge = build_judge(&client, &config.judge_model);

    // ── Prompt library (embedded at compile time) ───────────────────────────────
    let prompts_dir = Path::new(&config.prompts_dir);
    let prompt_lib = Arc::new(
        PromptLibrary::new().expect("Embedded prompts should be available"),
    );

    // ── Rule loading ─────────────────────────────────────────────────────────
    let ruleset = {
        let rules_dir = Path::new(".crb/rules/");
        match RuleSet::load_from_dir(rules_dir) {
            Ok(rs) => {
                tracing::info!(
                    "Loaded {} rules from {}",
                    rs.rules.len() + rs.always_rules.len(),
                    rules_dir.display()
                );
                Some(rs)
            }
            Err(e) => {
                tracing::warn!("Failed to load rules from {}: {e}", rules_dir.display());
                None
            }
        }
    };

    // ── Linter config ────────────────────────────────────────────────────────
    let linter_config_path = Path::new("linters.toml");
    let linter_configs = if linter_config_path.exists() && !config.skip_linters {
        match crb_tools::load_linter_config("linters.toml") {
            Ok(configs) => {
                tracing::info!("Loaded {} linter(s) from linters.toml", configs.len());
                Some(configs)
            }
            Err(e) => {
                tracing::warn!("Failed to load linter config: {e}. Linters disabled.");
                None
            }
        }
    } else {
        None
    };

    // ── Load dataset ─────────────────────────────────────────────────────────
    tracing::info!("Loading golden datasets from: {}", dataset_dir.display());
    let all_prs = match load_golden_datasets(dataset_dir) {
        Ok(prs) => prs,
        Err(e) => {
            tracing::error!("Failed to load dataset: {e}");
            return Err(e.context("Failed to load golden datasets"));
        }
    };
    tracing::info!("Loaded {} PR entries total", all_prs.len());

    // ── PR filter ─────────────────────────────────────────────────────────────
    use std::collections::HashSet;
    let filtered_prs: Vec<crb_reporting::GoldenCommentEntry> = if let Some(ref filter) = config.pr_filter {
        let filter_patterns: HashSet<String> = filter
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .collect();

        let available_urls: Vec<String> = all_prs.iter().map(|pr| pr.url.clone()).collect();

        let filtered: Vec<_> = all_prs
            .into_iter()
            .filter(|pr| {
                let url_lower = pr.url.to_lowercase();
                filter_patterns.iter().any(|pattern| {
                    if let Some((repo_part, pr_num_str)) = pattern.split_once("/pull/") {
                        if let Ok(pr_num) = pr_num_str.parse::<u32>() {
                            let pr_tag = format!("/pull/{}", pr_num);
                            if let Some(pos) = url_lower.find(&pr_tag) {
                                let after = &url_lower[pos + pr_tag.len()..];
                                if after.is_empty() || !after.chars().next().unwrap().is_ascii_digit() {
                                    if url_lower.contains(repo_part) {
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                    url_lower.contains(pattern)
                })
            })
            .collect();

        if filtered.is_empty() {
            tracing::warn!(
                "--pr-filter \"{}\" matched no PRs. Available URLs:\n  {}",
                filter,
                available_urls.join("\n  ")
            );
        }
        filtered
    } else {
        all_prs
    };

    tracing::info!("After PR filter: {} PR(s) to evaluate", filtered_prs.len());

    if filtered_prs.is_empty() {
        tracing::warn!("No PRs to evaluate");
        let _ = webui_tx.send(DashboardEvent::RunFinished {
            total_prs: 0,
            aggregated: AggregateMetrics::default(),
            total_cost: 0.0,
            total_tokens: 0,
            total_agent_calls: 0,
        });
        return Ok(());
    }

    let total_prs = filtered_prs.len();

    // ── Event bridge: harness → web UI ─────────────────────────────────────
    // The library emits `crb_dashboard::DashboardEvent`; we convert and
    // forward to the SSE broadcast channel.
    let (harness_tx, mut harness_rx) = broadcast::channel::<HarnessEvent>(256);
    let forward_tx = webui_tx.clone();

    tokio::spawn(async move {
        while let Ok(event) = harness_rx.recv().await {
            if let Some(converted) = convert_harness_event(event) {
                let _ = forward_tx.send(converted);
            }
        }
    });

    let dashboard_tx: Option<broadcast::Sender<HarnessEvent>> = Some(harness_tx);

    // ── Benchmark directory ──────────────────────────────────────────────────
    // If no explicit benchmark_dir was passed, default to the `benchmark/`
    // subdirectory, which is the standard project convention (contains
    // base-repos/, diffs/, worktrees/).
    let bench_dir = benchmark_dir.unwrap_or_else(|| {
        tracing::warn!(
            "No --benchmark-dir set; defaulting to 'benchmark/' directory"
        );
        Path::new("benchmark")
    });

    // ── Concurrency ───────────────────────────────────────────────────────────
    let sem = Arc::new(tokio::sync::Semaphore::new(config.concurrency));
    let mut set = tokio::task::JoinSet::new();
    let start_time = std::time::Instant::now();

    // ── Conditional cache directory setup ─────────────────────────────────────
    let cache_dir_opt: Option<PathBuf> = if config.use_cache {
        let cd = cache_dir.clone();
        std::fs::create_dir_all(&cd)?;
        Some(cd)
    } else {
        None
    };

    for pr in filtered_prs {
        let permit = sem.clone().acquire_owned().await.expect("semaphore closed");
        let client = client.clone();
        let judge = judge.clone();
        let model = config.model.clone();
        let bench_dir = bench_dir.to_path_buf();
        let linter_configs = linter_configs.clone();
        let skip_consensus = config.skip_consensus;
        let ruleset = ruleset.clone();
        let prompt_lib = prompt_lib.clone();
        let roles = config.roles.clone();
        let max_findings = config.max_findings;
        let cache_dir_opt = cache_dir_opt.clone();
        let dashboard_tx = dashboard_tx.clone();

        set.spawn(async move {
            let _permit = permit;
            crb_harness::evaluate_pr_with_postprocessing(
                &pr,
                &client,
                &model,
                &judge,
                &bench_dir,
                linter_configs.as_ref(),
                skip_consensus,
                false, // linters_only
                ruleset.as_ref(),
                prompt_lib.as_ref(),
                &roles,
                max_findings,
                cache_dir_opt.as_ref(),
                dashboard_tx.as_ref(),
            )
            .await
        });
    }

    // ── Collect results ───────────────────────────────────────────────────────
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
                tracing::info!("Completed: {}", result.pr_title);
                completed_prs += 1;

                // Accumulate totals
                total_tp += result.metrics.true_positives;
                total_fp += result.metrics.false_positives;
                total_fn += result.metrics.false_negatives;
                total_agent_calls += 4;
                if let Some(ref c) = result.cost {
                    total_cost += c.total_usd;
                    total_tokens +=
                        c.agent_tokens_in + c.agent_tokens_out + c.judge_tokens_in + c.judge_tokens_out;
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
                let _ = webui_tx.send(DashboardEvent::RunProgress {
                    completed_prs,
                    total_prs,
                    elapsed_secs: start_time.elapsed().as_secs_f64(),
                    total_cost,
                    current_pr: results.last().map(|r| r.pr_title.clone()),
                });
            }
            Ok(Err(e)) => {
                tracing::error!("PR evaluation failed: {e}");
            }
            Err(e) => {
                tracing::error!("Join error: {e}");
            }
        }
    }

    let elapsed = start_time.elapsed();

    // ── Compute aggregate metrics ────────────────────────────────────────────
    let avg_precision = if total_tp + total_fp > 0 {
        total_tp as f64 / (total_tp + total_fp) as f64
    } else {
        0.0
    };
    let avg_recall = if total_tp + total_fn > 0 {
        total_tp as f64 / (total_tp + total_fn) as f64
    } else {
        0.0
    };
    let avg_f1 = if (avg_precision + avg_recall) > 0.0 {
        2.0 * avg_precision * avg_recall / (avg_precision + avg_recall)
    } else {
        0.0
    };

    // Mark run as finished
    {
        let mut runs = active_runs.write().await;
        if let Some(run) = runs.get_mut(run_id) {
            run.finished = true;
            run.completed_prs = completed_prs;
        }
    }

    tracing::info!(
        run_id = %run_id,
        prs_completed = completed_prs,
        prs_total = total_prs,
        elapsed_secs = elapsed.as_secs_f64(),
        total_cost = total_cost,
        "Harness run finished"
    );

    // ── Write per-PR result files ────────────────────────────────────────────
    std::fs::create_dir_all(&output_subdir)?;
    if let Err(e) = write_report(&results, &output_subdir) {
        tracing::error!("Failed to write per-PR results: {e}");
    }

    // ── Write _summary.json ──────────────────────────────────────────────────
    if let Err(e) = crb_harness::write_summary(
        &output_subdir,
        &config.model,
        &config.judge_model,
        &results,
        elapsed,
    ) {
        tracing::error!("Failed to write summary: {e}");
    }

    // ── Send RunFinished event ───────────────────────────────────────────────
    let _ = webui_tx.send(DashboardEvent::RunFinished {
        total_prs: results.len(),
        aggregated: AggregateMetrics {
            total_tp,
            total_fp,
            total_fn,
            precision: avg_precision,
            recall: avg_recall,
            f1: avg_f1,
        },
        total_cost,
        total_tokens,
        total_agent_calls,
    });

    // ── Terminal summary ─────────────────────────────────────────────────────
    crb_harness::print_terminal_summary(&results);

    Ok(())
}

/// Convert a `crb_dashboard::DashboardEvent` (used by the harne ss library) to
/// a web UI `DashboardEvent` (used for SSE streaming to the frontend).
fn convert_harness_event(event: HarnessEvent) -> Option<DashboardEvent> {
    match event {
        HarnessEvent::AgentStarted { pr_key, role } => {
            Some(DashboardEvent::AgentStarted { pr_key, role })
        }
        HarnessEvent::AgentChunk { role, chunk } => {
            Some(DashboardEvent::AgentChunk { role, chunk })
        }
        HarnessEvent::AgentFinished { role, findings, success } => {
            Some(DashboardEvent::AgentFinished { role, findings, success })
        }
        HarnessEvent::PrCompleted {
            pr_key,
            metrics,
            cost,
            total_tokens,
            agent_calls,
            findings_count,
        } => {
            Some(DashboardEvent::PrCompleted {
                pr_key,
                metrics: MetricsData {
                    true_positives: metrics.true_positives,
                    false_positives: metrics.false_positives,
                    false_negatives: metrics.false_negatives,
                    precision: metrics.precision,
                    recall: metrics.recall,
                    f1: metrics.f1,
                },
                cost,
                total_tokens,
                agent_calls,
                findings_count,
            })
        }
        HarnessEvent::RunFinished {
            total_prs,
            aggregated,
            total_cost,
            total_tokens,
            total_agent_calls,
        } => {
            Some(DashboardEvent::RunFinished {
                total_prs,
                aggregated: AggregateMetrics {
                    total_tp: aggregated.total_tp,
                    total_fp: aggregated.total_fp,
                    total_fn: aggregated.total_fn,
                    precision: aggregated.precision,
                    recall: aggregated.recall,
                    f1: aggregated.f1,
                },
                total_cost,
                total_tokens,
                total_agent_calls,
            })
        }
    }
}

/// Run a replay of a previous run using existing cached LLM responses.
///
/// This is called by POST /api/runs/:id/replay. It re-runs the evaluation
/// with the existing cache directory so that cached responses are used
/// instead of making new API calls.  Results are written to `output_subdir`.
#[allow(clippy::too_many_arguments)]
pub async fn run_replay_via_library(
    run_id: &str,
    model: &str,
    dataset_dir: &str,
    roles: &str,
    cache_dir: &PathBuf,
    output_subdir: &PathBuf,
    state: &AppState,
) -> anyhow::Result<()> {
    let dataset_path = Path::new(dataset_dir);

    tracing::info!(
        run_id = %run_id,
        model = %model,
        dataset = %dataset_dir,
        roles = %roles,
        cache_dir = %cache_dir.display(),
        output = %output_subdir.display(),
        "Starting replay via library"
    );

    let client = rig_core::providers::openai::Client::from_env()
        .map_err(|e| anyhow::anyhow!("Failed to create OpenAI client: {e}"))?;

    let judge = build_judge(&client, model);

    // Use a default prompt library (built-in prompts)
    let prompt_lib = Arc::new(
        PromptLibrary::new().expect("Embedded prompts should be available"),
    );

    // No rules or linters for replay
    let ruleset: Option<RuleSet> = None;
    let linter_configs: Option<std::collections::HashMap<String, crb_tools::LinterConfig>> = None;
    let max_findings = 20;
    let skip_consensus = false;

    // ── Load dataset ─────────────────────────────────────────────────────────
    tracing::info!("Loading golden datasets from: {}", dataset_path.display());
    let all_prs = match load_golden_datasets(dataset_path) {
        Ok(prs) => prs,
        Err(e) => {
            tracing::error!("Failed to load dataset: {e}");
            return Err(e.context("Failed to load golden datasets"));
        }
    };
    tracing::info!("Loaded {} PR entries total", all_prs.len());

    if all_prs.is_empty() {
        return Ok(());
    }

    let _total_prs = all_prs.len();

    std::fs::create_dir_all(output_subdir)?;

    // ── Evaluate each PR with existing cache ────────────────────────────────
    let bench_dir = state
        .benchmark_dir
        .clone()
        .unwrap_or_else(|| Path::new(".").to_path_buf());

    let mut results = Vec::new();
    let start_time = std::time::Instant::now();

    for pr in &all_prs {
        let result = crb_harness::evaluate_pr_with_postprocessing(
            pr,
            &client,
            model,
            &judge,
            &bench_dir,
            linter_configs.as_ref(),
            skip_consensus,
            false, // linters_only
            ruleset.as_ref(),
            prompt_lib.as_ref(),
            roles,
            max_findings,
            Some(cache_dir),
            None, // No dashboard tx for replay
        )
        .await;

        match result {
            Ok(result) => {
                tracing::info!("Replayed: {}", result.pr_title);
                results.push(result);
            }
            Err(e) => {
                tracing::error!("Replay failed for PR {}: {e}", pr.pr_title);
            }
        }
    }

    let elapsed = start_time.elapsed();

    // ── Write results ────────────────────────────────────────────────────────
    if let Err(e) = write_report(&results, output_subdir) {
        tracing::error!("Failed to write replay results: {e}");
    }

    if let Err(e) = crb_harness::write_summary(output_subdir, model, model, &results, elapsed) {
        tracing::error!("Failed to write replay summary: {e}");
    }

    tracing::info!(
        run_id = %run_id,
        prs = results.len(),
        elapsed_secs = elapsed.as_secs_f64(),
        "Replay finished"
    );

    Ok(())
}
