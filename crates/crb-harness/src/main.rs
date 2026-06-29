use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use clap::Parser;
use crb_agents::prompts::PromptLibrary;
use crb_agents::{build_agent, Finding, AGENT_ROLES};
use crb_consensus::evaluate_pr_with_consensus;
use crb_consensus::CacheBackend;
use crb_dashboard::DashboardEvent;
use crb_judge::{build_judge, compute_metrics, run_judge};
use crb_reporting::{load_golden_datasets, write_report, GoldenCommentEntry, PrResult};
use crb_rules::RuleSet;
use regex::Regex;
use rig_core::client::ProviderClient;
use rig_core::completion::Prompt;
use rig_core::tool::Tool;
use tokio::sync::{broadcast, mpsc};
use tracing::info;
use tracing::info_span;
use tracing_subscriber::EnvFilter;

mod cache;
mod config;
mod cost;
use cache::LlmCache;
use config::CliArgs;
use cost::CostTracker;

mod validation;

/// Main entry point for the review benchmark harness.
#[tokio::main]
async fn main() -> Result<()> {
    // Load .env from CWD (and parent directories)
    match dotenvy::dotenv() {
        Ok(path) => eprintln!("[dotenv] Loaded .env from: {}", path.display()),
        Err(e) => eprintln!("[dotenv] No .env file loaded: {e}"),
    }

    // Fallback: if OPENAI_API_KEY is not set but OPENROUTER_API_KEY is, use that
    if std::env::var("OPENAI_API_KEY").is_err() {
        if let Ok(key) = std::env::var("OPENROUTER_API_KEY") {
            std::env::set_var("OPENAI_API_KEY", key);
            eprintln!(
                "[dotenv] OPENAI_API_KEY not found — falling back to OPENROUTER_API_KEY"
            );
        }
    }

    // ── Tracing ───────────────────────────────────────────────────────────
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // ── CLI ───────────────────────────────────────────────────────────────
    let args = CliArgs::parse();
    let output_dir = PathBuf::from(&args.output_dir);
    let dataset_dir = PathBuf::from(&args.dataset_dir);
    let repos_dir = PathBuf::from(&args.repos_dir);

    // Determine workspace root (where baselines/ lives)
    let workspace_root = std::env::current_dir()
        .context("Failed to determine current working directory")?;

    // ── --validate flag ────────────────────────────────────────────────────
    if args.validate {
        return run_validate(&workspace_root, "5.14").await;
    }

    let _span = info_span!("harness", model = %args.model, concurrency = %args.concurrency).entered();

    // ── --cached-diffs flag ────────────────────────────────────────────────
    if args.cached_diffs {
        info!("--cached-diffs: skipping scaffold step, using pre-extracted diffs");
    }

    // ── Load datasets ─────────────────────────────────────────────────────
    info!("Loading golden datasets from: {}", dataset_dir.display());
    let all_prs = load_golden_datasets(&dataset_dir)?;
    info!("Loaded {} PR entries total", all_prs.len());

    // ── --pr-filter flag ──────────────────────────────────────────────────
    let all_prs = if let Some(ref filter) = args.pr_filter {
        let filter_patterns: std::collections::HashSet<String> = filter
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .collect();

        let available_urls: Vec<String> = all_prs.iter().map(|pr| pr.url.clone()).collect();

        let filtered: Vec<GoldenCommentEntry> = all_prs
            .iter()
            .filter(|pr| {
                let url_lower = pr.url.to_lowercase();
                filter_patterns.iter().any(|pattern| url_lower.contains(pattern))
            })
            .cloned()
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

    info!("After --pr-filter: {} PR(s) to evaluate", all_prs.len());

    // ── Dry run ───────────────────────────────────────────────────────────
    if args.dry_run {
        println!("[DRY RUN] Would evaluate {} PR(s)", all_prs.len());
        println!("  Model:              {}", args.model);
        println!("  Judge model:        {}", args.judge_model);
        println!("  Concurrency:        {}", args.concurrency);
        println!("  Dataset:            {}", dataset_dir.display());
        println!("  Output:             {}", output_dir.display());
        println!("  Skip consensus:     {}", args.skip_consensus);
        println!("  Skip linters:       {}", args.skip_linters);
        println!("  Linters only:       {}", args.linters_only);
        if let Some(ref cache_dir) = args.cache_dir {
            println!("  Cache dir:          {}", cache_dir.display());
        }
        return Ok(());
    }

    // ── Resume support ────────────────────────────────────────────────────
    let prs_to_evaluate: Vec<&GoldenCommentEntry> = if args.resume {
        let existing: std::collections::HashSet<String> = if output_dir.exists() {
            std::fs::read_dir(&output_dir)?
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map_or(false, |ext| ext == "json"))
                .filter_map(|e| e.file_name().into_string().ok())
                .collect()
        } else {
            std::collections::HashSet::new()
        };

        all_prs
            .iter()
            .filter(|pr| {
                let filename = utils::sanitize_filename(&pr.pr_title);
                let exists = existing.contains(&format!("{filename}.json"));
                if exists {
                    info!("Skipping already-evaluated PR: {}", pr.pr_title);
                }
                !exists
            })
            .collect()
    } else {
        all_prs.iter().collect()
    };

    info!(
        "Evaluating {} PR(s) ({} skipped)",
        prs_to_evaluate.len(),
        all_prs.len() - prs_to_evaluate.len()
    );

    if prs_to_evaluate.is_empty() {
        println!("No PRs to evaluate (all already processed or dataset empty).");
        return Ok(());
    }

    // ── Clients ───────────────────────────────────────────────────────────
    let client = rig_core::providers::openai::Client::from_env()
        .map_err(|e| anyhow::anyhow!("Failed to create OpenAI client: {e}"))?;

    let judge = build_judge(&client, &args.judge_model);

    // ── Linter config ─────────────────────────────────────────────────────
    let linter_config_path = std::path::Path::new(&args.linters_config);
    let linter_configs = if linter_config_path.exists() && !args.skip_linters {
        match crb_tools::load_linter_config(&args.linters_config) {
            Ok(configs) => {
                info!(
                    "Loaded {} linter(s) from {}",
                    configs.len(),
                    args.linters_config
                );
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

    // ── Rule loading ──────────────────────────────────────────────────────
    let ruleset = if !args.skip_rules {
        let rules_dir = std::path::Path::new(&args.rules_dir);
        match RuleSet::load_from_dir(rules_dir) {
            Ok(rs) => {
                info!(
                    "Loaded {} rules ({} always-apply) from {}",
                    rs.rules.len() + rs.always_rules.len(),
                    rs.always_rules.len(),
                    rules_dir.display()
                );
                Some(rs)
            }
            Err(e) => {
                tracing::warn!("Failed to load rules from {}: {e}", rules_dir.display());
                None
            }
        }
    } else {
        None
    };

    // ── Prompt library ───────────────────────────────────────────────────
    let prompt_lib = std::sync::Arc::new({
        let mut lib = PromptLibrary::new();
        let prompts_dir = std::path::Path::new(&args.prompts_dir);
        // Always try to load from the configured prompts directory.
        // If it's "prompts/builtin" and doesn't exist, that's fine — we fall
        // back to built-in defaults.  For custom directories, load or warn.
        if prompts_dir.exists() {
            match lib.load_from_dir(prompts_dir) {
                Ok(()) => {
                    info!("Loaded prompts from: {}", prompts_dir.display());
                }
                Err(e) => {
                    tracing::warn!("Failed to load prompts from {}: {e}", prompts_dir.display());
                }
            }
        } else if args.prompts_dir.to_string_lossy() != "prompts/builtin" {
            tracing::warn!(
                "Custom prompts directory '{}' not found — using built-in defaults",
                prompts_dir.display()
            );
        }
        lib
    });

    // ── Cache directory ───────────────────────────────────────────────────
    let start_time = std::time::Instant::now();
    let cache_dir = args.cache_dir.clone();

    // ── Dashboard event system (TUI and/or JSON stdout) ─────────────────
    // We use a broadcast channel so events can fan out to multiple consumers.
    let (event_broadcast_tx, _) = broadcast::channel::<DashboardEvent>(256);

    let dashboard_tx: Option<broadcast::Sender<DashboardEvent>> = if args.dashboard {
        let mut rx = event_broadcast_tx.subscribe();
        let total_prs = prs_to_evaluate.len();
        // Bridge broadcast → mpsc for the TUI (which expects mpsc::Receiver)
        let (mpsc_tx, mpsc_rx) = mpsc::channel::<DashboardEvent>(1024);
        tokio::spawn(async move {
            while let Ok(event) = rx.recv().await {
                if mpsc_tx.send(event).await.is_err() {
                    break;
                }
            }
        });
        tokio::spawn(async move {
            if let Err(e) = crb_dashboard::run_dashboard(total_prs, mpsc_rx).await {
                tracing::error!("Dashboard error: {e}");
            }
        });
        Some(event_broadcast_tx.clone())
    } else if args.dashboard_events {
        // --dashboard-events alone also needs a sender
        Some(event_broadcast_tx.clone())
    } else {
        None
    };

    // ── Dashboard Events (JSON stdout) ──────────────────────────────────
    if args.dashboard_events {
        let mut rx = event_broadcast_tx.subscribe();
        tokio::spawn(async move {
            use std::io::Write;
            while let Ok(event) = rx.recv().await {
                if let Ok(json) = serde_json::to_string(&event) {
                    let stdout = std::io::stdout();
                    let mut handle = stdout.lock();
                    let _ = writeln!(handle, "{json}");
                    let _ = handle.flush();
                    // handle's Drop unlocks stdout before the next await
                }
            }
        });
    }

    // ── Concurrency ───────────────────────────────────────────────────────
    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(args.concurrency));
    let mut set = tokio::task::JoinSet::new();

    for pr in prs_to_evaluate {
        let client = client.clone();
        let sem = sem.clone();
        let judge = judge.clone();
        let pr = pr.clone();
        let model = args.model.clone();
        let repos_dir = repos_dir.clone();
        let linter_configs = linter_configs.clone();
        let skip_consensus = args.skip_consensus;
        let linters_only = args.linters_only;
        let ruleset = ruleset.clone();
        let prompt_lib = prompt_lib.clone();
        let roles = args.roles.clone();
        let max_findings = args.max_findings;
        let cache_dir = cache_dir.clone();
        let dashboard_tx = dashboard_tx.clone();

        set.spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore closed");
            evaluate_pr_with_postprocessing(
                &pr,
                &client,
                &model,
                &judge,
                &repos_dir,
                linter_configs.as_ref(),
                skip_consensus,
                linters_only,
                ruleset.as_ref(),
                prompt_lib.as_ref(),
                &roles,
                max_findings,
                cache_dir.as_ref(),
                dashboard_tx.as_ref(),
            )
            .await
        });
    }

    // ── Collect results ───────────────────────────────────────────────────
    let mut results = Vec::new();
    while let Some(res) = set.join_next().await {
        match res {
            Ok(Ok(result)) => {
                info!("Completed: {}", result.pr_title);
                results.push(result);
            }
            Ok(Err(e)) => {
                tracing::error!("PR evaluation failed: {e}");
            }
            Err(e) => {
                tracing::error!("Join error: {e}");
            }
        }
    }

    // ── Send RunFinished event (if dashboard active) ─────────────────────
    if let Some(tx) = &dashboard_tx {
        let total_prs = results.len();
        let mut total_cost = 0.0f64;
        let mut total_tokens = 0usize;
        let mut total_tp = 0usize;
        let mut total_fp = 0usize;
        let mut total_fn = 0usize;
        let mut total_agent_calls = 0usize;

        for r in &results {
            total_tp += r.metrics.true_positives;
            total_fp += r.metrics.false_positives;
            total_fn += r.metrics.false_negatives;
            total_agent_calls += 4; // 4 agents per PR
            if let Some(ref c) = r.cost {
                total_cost += c.total_usd;
                total_tokens += c.agent_tokens_in + c.agent_tokens_out
                    + c.judge_tokens_in + c.judge_tokens_out;
            }
        }

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

        let _ = tx.send(DashboardEvent::RunFinished {
            total_prs,
            aggregated: crb_dashboard::AggregateMetrics {
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
    }

    // ── Report ────────────────────────────────────────────────────────────
    write_report(&results, &output_dir)?;
    info!(
        "Done. {} PR(s) evaluated, results in {}",
        results.len(),
        output_dir.display()
    );

    // ── Terminal cost summary ───────────────────────────────────────────
    print_terminal_summary(&results);

    // ── Write _summary.json ──────────────────────────────────────────────
    if let Some(ref cache_dir_path) = cache_dir {
        write_summary(cache_dir_path, &args, &results, start_time.elapsed())?;
    }

    // ── --ci flag: validate and exit with proper code ─────────────────────
    if args.ci {
        let metrics: Vec<crb_judge::Metrics> = results.iter().map(|r| r.metrics.clone()).collect();
        let (avg_precision, avg_recall, avg_f1) =
            validation::compute_average_metrics(&metrics);
        let baseline = validation::load_baseline(&workspace_root, "5.14")?;
        let val_result = validation::validate_against_baseline(
            &baseline,
            results.len(),
            avg_precision,
            avg_recall,
            avg_f1,
        );
        validation::print_validation_summary(&baseline, &val_result, avg_precision, avg_recall, avg_f1);

        if val_result.in_threshold {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "CI validation failed: metrics exceed baseline thresholds"
            ))
        }
    } else {
        Ok(())
    }
}

/// Write the `_summary.json` aggregate statistics file to the cache directory.
fn write_summary(
    cache_dir: &PathBuf,
    args: &CliArgs,
    results: &[PrResult],
    duration: Duration,
) -> Result<()> {
    let total_llm_calls: usize = results.iter().map(|r| r.findings_count).sum();
    let total_judge_calls: usize = results.iter().map(|r| r.verdicts.len()).sum();

    // Aggregate cost tracking
    let total_tokens: usize = results
        .iter()
        .filter_map(|r| r.cost.as_ref())
        .map(|c| c.agent_tokens_in + c.agent_tokens_out + c.judge_tokens_in + c.judge_tokens_out)
        .sum();
    let total_cost_usd: f64 = results
        .iter()
        .filter_map(|r| r.cost.as_ref())
        .map(|c| c.total_usd)
        .sum();
    let avg_agent_cache_hit_rate = if results.is_empty() {
        0.0
    } else {
        results
            .iter()
            .filter_map(|r| r.cost.as_ref())
            .map(|c| c.agent_cache_hit_rate)
            .sum::<f64>() / results.len() as f64
    };
    let avg_judge_cache_hit_rate = if results.is_empty() {
        0.0
    } else {
        results
            .iter()
            .filter_map(|r| r.cost.as_ref())
            .map(|c| c.judge_cache_hit_rate)
            .sum::<f64>() / results.len() as f64
    };

    let aggregate_metrics = if results.is_empty() {
        serde_json::json!({})
    } else {
        let avg_precision = results.iter().map(|r| r.metrics.precision).sum::<f64>() / results.len() as f64;
        let avg_recall = results.iter().map(|r| r.metrics.recall).sum::<f64>() / results.len() as f64;
        let avg_f1 = results.iter().map(|r| r.metrics.f1).sum::<f64>() / results.len() as f64;
        serde_json::json!({
            "avg_precision": avg_precision,
            "avg_recall": avg_recall,
            "avg_f1": avg_f1,
            "total_true_positives": results.iter().map(|r| r.metrics.true_positives).sum::<usize>(),
            "total_false_positives": results.iter().map(|r| r.metrics.false_positives).sum::<usize>(),
            "total_false_negatives": results.iter().map(|r| r.metrics.false_negatives).sum::<usize>(),
        })
    };

    let summary = serde_json::json!({
        "run_id": std::env::current_dir()
            .ok()
            .and_then(|d| d.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_default(),
        "model": args.model,
        "judge_model": args.judge_model,
        "total_prs": results.len(),
        "total_llm_calls": total_llm_calls,
        "total_judge_calls": total_judge_calls,
        "duration_secs": duration.as_secs_f64(),
        "aggregate_metrics": aggregate_metrics,
        "total_tokens": total_tokens,
        "total_cost_usd": total_cost_usd,
        "agent_cache_hit_rate": avg_agent_cache_hit_rate,
        "judge_cache_hit_rate": avg_judge_cache_hit_rate,
    });

    let summary_path = cache_dir.join("_summary.json");
    std::fs::write(&summary_path, serde_json::to_string_pretty(&summary)?)?;
    info!("Cache summary written to: {}", summary_path.display());
    Ok(())
}

/// Print a terminal summary of cost and cache hit rates for all PRs.
fn print_terminal_summary(results: &[PrResult]) {
    let separator = "═══════════════════════════════════════════════";
    println!("\n{separator}");

    let mut grand_total_tokens = 0usize;
    let mut grand_total_cost = 0.0f64;

    for result in results {
        // Extract repo/PR info from URL for display
        let pr_label = extract_pr_info(&result.url)
            .map(|(owner, repo, num)| format!("{owner}/{repo}/{num}"))
            .unwrap_or_else(|| result.pr_title.clone());

        let f1 = result.metrics.f1;
        let findings_count = result.findings_count;

        if let Some(ref cost) = result.cost {
            let pr_tokens = cost.agent_tokens_in + cost.agent_tokens_out
                + cost.judge_tokens_in + cost.judge_tokens_out;
            let pr_cost = cost.total_usd;

            grand_total_tokens += pr_tokens;
            grand_total_cost += pr_cost;

            println!(
                " {}: F1={:.3}, {} findings, {:.1}K tokens, ${:.4}",
                pr_label,
                f1,
                findings_count,
                pr_tokens as f64 / 1000.0,
                pr_cost,
            );
        } else {
            println!(
                " {}: F1={:.3}, {} findings, -- tokens, $--",
                pr_label, f1, findings_count,
            );
        }
    }

    // Compute aggregate cache hit rates from CostSummary ratios
    let total_agent_rate: f64 = results
        .iter()
        .filter_map(|r| r.cost.as_ref())
        .map(|c| c.agent_cache_hit_rate)
        .sum();
    let total_judge_rate: f64 = results
        .iter()
        .filter_map(|r| r.cost.as_ref())
        .map(|c| c.judge_cache_hit_rate)
        .sum();
    let pr_count_with_cost = results.iter().filter(|r| r.cost.is_some()).count();

    let avg_agent_rate = if pr_count_with_cost > 0 {
        total_agent_rate / pr_count_with_cost as f64
    } else {
        0.0
    };
    let avg_judge_rate = if pr_count_with_cost > 0 {
        total_judge_rate / pr_count_with_cost as f64
    } else {
        0.0
    };

    println!("{separator}");
    println!(
        " TOTAL: {} PR(s), {:.1}K tokens, ${:.4}",
        results.len(),
        grand_total_tokens as f64 / 1000.0,
        grand_total_cost,
    );
    println!(" Agent cache hit rate: {:.1}%", avg_agent_rate * 100.0);
    println!(" Judge cache hit rate: {:.1}%", avg_judge_rate * 100.0);
    println!("{separator}");
}

/// Extract owner, repo name, and PR number from a GitHub PR URL.
///
/// Expects URLs of the form `https://github.com/{owner}/{repo}/pull/{num}`.
/// Returns `None` if the URL doesn't match the expected pattern.
fn extract_pr_info(url: &str) -> Option<(String, String, u32)> {
    let re = Regex::new(r"^https://github\.com/([^/]+)/([^/]+)/pull/(\d+)$").ok()?;
    let caps = re.captures(url)?;
    let owner = caps.get(1)?.as_str().to_string();
    let repo = caps.get(2)?.as_str().to_string();
    let pr_num: u32 = caps.get(3)?.as_str().parse().ok()?;
    Some((owner, repo, pr_num))
}

/// Load the diff for a PR from pre-extracted cached diff files.
///
/// Cached diffs live at `{repos_dir}/../hermes_data/diffs/{owner}_{repo}_{pr_num}.diff`.
/// This path is derived from the `repos_dir` CLI argument by going up one level
/// to the shared parent (`offline/`) and into the sibling `hermes_data/diffs/` dir.
fn load_cached_diff(repos_dir: &PathBuf, owner: &str, repo: &str, pr_num: u32) -> Option<String> {
    let diffs_dir = repos_dir
        .parent()
        .map(|p| p.join("hermes_data").join("diffs"))
        .unwrap_or_else(|| repos_dir.join("../hermes_data/diffs"));
    let diff_path = diffs_dir.join(format!("{}_{}_{}.diff", owner, repo, pr_num));
    match std::fs::read_to_string(&diff_path) {
        Ok(content) => {
            info!("Loaded cached diff ({} bytes) from {}", content.len(), diff_path.display());
            Some(content)
        }
        Err(e) => {
            tracing::warn!(
                "Cached diff not found at {}: {}. Using empty diff.",
                diff_path.display(),
                e
            );
            None
        }
    }
}

/// Parse an agent's LLM response string into a `Vec<Finding>`.
///
/// Attempts two strategies in order:
/// 1. Direct JSON array deserialization via `serde_json::from_str`.
/// 2. JSON extraction from markdown fenced code blocks (```json ... ```).
///
/// If both fail, returns an empty `Vec` with a warning and the truncated
/// response text for debugging.
fn parse_agent_findings(response: &str) -> Result<Vec<Finding>, String> {
    // Log raw response first for debugging
    let preview_len = std::cmp::min(500, response.len());
    tracing::info!("Agent raw response (first 500 chars): {}", &response[..preview_len]);

    // Strategy 1: Try direct JSON array parse
    if let Ok(findings) = serde_json::from_str::<Vec<Finding>>(response) {
        info!("Parsed {} finding(s) directly from agent JSON response", findings.len());
        return Ok(findings);
    }

    // Strategy 2: Extract JSON from markdown code blocks
    // Match ```json ... ``` or ``` ... ``` blocks
    let re = Regex::new(r"(?s)```(?:json)?\s*\n(.*?)\n\s*```").unwrap();
    if let Some(caps) = re.captures(response) {
        let inner = caps.get(1).unwrap().as_str().trim();
        if let Ok(findings) = serde_json::from_str::<Vec<Finding>>(inner) {
            info!(
                "Parsed {} finding(s) from markdown code block in agent response",
                findings.len()
            );
            return Ok(findings);
        }
    }

    // Strategy 3: Find any JSON array in the response
    let array_re = Regex::new(r"\[[\s\S]*\]").unwrap();
    if let Some(m) = array_re.find(response) {
        if let Ok(findings) = serde_json::from_str::<Vec<Finding>>(m.as_str()) {
            info!("Parsed {} finding(s) from embedded JSON array", findings.len());
            return Ok(findings);
        }
    }

    // All strategies failed — warn and return empty
    let truncated = if response.len() > 200 {
        format!("{}...", &response[..200])
    } else {
        response.to_string()
    };
    tracing::warn!(
        "Failed to parse agent response as Finding array. \
         Response (truncated): {}",
        truncated
    );
    Ok(Vec::new())
}

/// Evaluate a single PR, optionally using consensus orchestration and linters.
///
/// When `--linters-only` is set, only static analysis linters are run (no LLM
/// agents). When `--skip-consensus` is set, the original single-agent evaluation
/// is used instead of the multi-agent consensus pipeline.
#[tracing::instrument(skip_all, fields(pr_title = %pr.pr_title))]
async fn evaluate_pr_with_postprocessing(
    pr: &GoldenCommentEntry,
    client: &rig_core::providers::openai::Client,
    model: &str,
    judge: &rig_core::agent::Agent<
        rig_core::providers::openai::responses_api::ResponsesCompletionModel,
    >,
    repos_dir: &PathBuf,
    linter_configs: Option<&std::collections::HashMap<String, crb_tools::LinterConfig>>,
    skip_consensus: bool,
    linters_only: bool,
    ruleset: Option<&RuleSet>,
    prompt_lib: &PromptLibrary,
    roles: &str,
    max_findings: usize,
    cache_dir: Option<&PathBuf>,
    dashboard_tx: Option<&broadcast::Sender<DashboardEvent>>,
) -> Result<PrResult> {

    // ── Setup cache if enabled ────────────────────────────────────────────
    let cache: Option<Arc<LlmCache>> = if let Some(dir) = cache_dir {
        let pr_key = utils::sanitize_filename(&pr.pr_title);
        match LlmCache::new(dir, &pr_key) {
            Ok(c) => {
                info!("LLM cache enabled for PR '{}' at {}", pr.pr_title, c.dir().display());
                Some(Arc::new(c))
            }
            Err(e) => {
                tracing::warn!("Failed to create LLM cache for PR '{}': {e}", pr.pr_title);
                None
            }
        }
    } else {
        None
    };

    // ── Cost tracker ──────────────────────────────────────────────────────
    let cost_tracker = Arc::new(CostTracker::new());

    // ── Diff loading ──────────────────────────────────────────────────────
    let diff = match extract_pr_info(&pr.url) {
        Some((owner, repo, pr_num)) => {
            load_cached_diff(repos_dir, &owner, &repo, pr_num)
                .unwrap_or_default()
        }
        None => {
            tracing::warn!(
                "Could not extract PR info from URL '{}'. Using empty diff.",
                pr.url
            );
            String::new()
        }
    };
    if diff.is_empty() {
        tracing::warn!("Empty diff for PR: {} (url: {})", pr.pr_title, pr.url);
    } else {
        info!("Loaded diff ({} bytes) for PR: {}", diff.len(), pr.pr_title);
    }

    // ── Linters ───────────────────────────────────────────────────────────
    let mut linter_findings: Vec<Finding> = Vec::new();
    if let Some(configs) = linter_configs {
        let mut linter_set = tokio::task::JoinSet::new();
        for (_name, lconfig) in configs {
            let tool = crb_tools::create_linter_tool(lconfig);
            let args = crb_tools::LinterArgs {
                repo_path: repos_dir.to_string_lossy().to_string(),
            };
            linter_set.spawn(async move {
                // Run the linter via the rig Tool interface
                let result = tool.call(args).await;
                result
            });
        }

        while let Some(res) = linter_set.join_next().await {
            match res {
                Ok(Ok(findings)) => linter_findings.extend(findings),
                Ok(Err(e)) => tracing::warn!("Linter failed: {e}"),
                Err(e) => tracing::warn!("Linter join error: {e}"),
            }
        }

        info!(
            "Found {} linter finding(s) for PR: {}",
            linter_findings.len(),
            pr.pr_title
        );
    }

    if linters_only {
        // Return a PrResult with only linter findings and no judge evaluation
        return Ok(PrResult {
            pr_title: pr.pr_title.clone(),
            url: pr.url.clone(),
            findings_count: linter_findings.len(),
            golden_count: pr.comments.len(),
            metrics: crb_judge::Metrics::default(),
            verdicts: vec![],
            cost: Some(cost_tracker.to_summary()),
        });
    }

    // ── Compute rules preamble from changed files ────────────────────────
    // For the MVP we don't have real changed file paths, so pass empty slice.
    let rules_preamble = ruleset.map(|rs| rs.format_preamble(&[]));

    // ── Agent evaluation ──────────────────────────────────────────────────
    let pr_key = utils::sanitize_filename(&pr.pr_title);

    // Send AgentStarted for each role
    if let Some(tx) = dashboard_tx {
        for role in ["SA", "CL", "AR", "SEC"] {
            let _ = tx.send(DashboardEvent::AgentStarted {
                pr_key: pr_key.clone(),
                role: role.to_string(),
            });
        }
    }

    let (all_findings, verdicts) = if skip_consensus {
        // Original single-agent evaluation
        evaluate_pr_single_agent(
            pr, client, model, judge, &diff, linter_findings, rules_preamble.as_deref(), prompt_lib,
            cache.clone(), cost_tracker.clone(), dashboard_tx,
        )
        .await?
    } else {
        // Multi-agent consensus evaluation
        evaluate_pr_consensus(
            pr, client, model, judge, &diff, linter_findings, rules_preamble.as_deref(), prompt_lib,
            roles, max_findings, cache.clone(), cost_tracker.clone(),
        )
        .await?
    };

    // ── Post-processing: aggregator dedup + auditor severity check ────────
    let processed_findings = post_process_findings(&all_findings);

    // ── Send AgentFinished for each role ────────────────────────────────
    if let Some(tx) = dashboard_tx {
        for (i, role) in ["SA", "CL", "AR", "SEC"].iter().enumerate() {
            // Distribute findings count across the roles (rough estimate)
            let role_findings = if skip_consensus {
                // In single-agent mode, count how many the specific agent produced
                let per_role = all_findings.len() / 4;
                if i == 0 { all_findings.len() - per_role * 3 } else { per_role }
            } else {
                processed_findings.len() / 4
            };
            let _ = tx.send(DashboardEvent::AgentFinished {
                role: role.to_string(),
                findings: role_findings,
                success: true,
            });
        }
    }

    // ── Judge evaluation ──────────────────────────────────────────────────
    // (if not already done by consensus path)
    let final_verdicts = if skip_consensus {
        verdicts
    } else {
        // Already computed in consensus path
        verdicts
    };

    let metrics = compute_metrics(&final_verdicts, pr.comments.len());

    // ── Send PrCompleted event ───────────────────────────────────────────
    if let Some(tx) = dashboard_tx {
        let pr_key = pr_key; // already computed above
        let tokens = cost_tracker.total_tokens();
        let total_tokens = tokens.0 + tokens.1;
        let cost_usd = cost_tracker.total_cost_usd();
        let total_agent_calls = 4; // SA, CL, AR, SEC
        let _ = tx.send(DashboardEvent::PrCompleted {
            pr_key,
            metrics: metrics.clone(),
            cost: cost_usd,
            total_tokens,
            agent_calls: total_agent_calls,
            findings_count: processed_findings.len(),
        });
    }

    // ── Write metadata.json ─────────────────────────────────────────────
    if let Some(ref c) = cache {
        let metadata = serde_json::json!({
            "pr_title": pr.pr_title,
            "url": pr.url,
            "model": model,
            "skip_consensus": skip_consensus,
            "timestamp": format!("{:?}", std::time::SystemTime::now()),
            "findings_count": processed_findings.len(),
            "golden_count": pr.comments.len(),
            "metrics": {
                "true_positives": metrics.true_positives,
                "false_positives": metrics.false_positives,
                "false_negatives": metrics.false_negatives,
                "precision": metrics.precision,
                "recall": metrics.recall,
                "f1": metrics.f1,
            },
        });
        if let Err(e) = c.save_metadata(&metadata) {
            tracing::warn!("Failed to write cache metadata: {e}");
        }
    }

    Ok(PrResult {
        pr_title: pr.pr_title.clone(),
        url: pr.url.clone(),
        findings_count: processed_findings.len(),
        golden_count: pr.comments.len(),
        metrics,
        verdicts: final_verdicts,
        cost: Some(cost_tracker.to_summary()),
    })
}

/// Call an async function with exponential backoff retry.
///
/// The closure is called on each attempt, returning a `Result<T, E>`.
/// On error, if retries remain, the function waits with exponential backoff
/// (`base_delay_ms * 2^attempt`) before retrying.
async fn with_retry<F, Fut, T, E>(f: F, max_retries: usize, base_delay_ms: u64) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut attempt = 0usize;
    loop {
        match f().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                attempt += 1;
                if attempt >= max_retries {
                    return Err(e);
                }
                let delay = Duration::from_millis(base_delay_ms * 2u64.pow(attempt as u32));
                tracing::warn!(
                    "Attempt {}/{} failed: {}. Retrying in {}ms",
                    attempt,
                    max_retries,
                    e,
                    delay.as_millis()
                );
                tokio::time::sleep(delay).await;
            }
        }
    }
}

/// Run the original single-agent evaluation with finding collection.
/// Uses content-addressed caching: computes SHA256 keys from inputs,
/// skips API calls on cache hit.
async fn evaluate_pr_single_agent(
    pr: &GoldenCommentEntry,
    client: &rig_core::providers::openai::Client,
    model: &str,
    judge: &rig_core::agent::Agent<
        rig_core::providers::openai::responses_api::ResponsesCompletionModel,
    >,
    diff: &str,
    linter_findings: Vec<Finding>,
    rules_preamble: Option<&str>,
    prompt_lib: &PromptLibrary,
    cache: Option<Arc<LlmCache>>,
    cost_tracker: Arc<CostTracker>,
    dashboard_tx: Option<&broadcast::Sender<DashboardEvent>>,
) -> Result<(Vec<Finding>, Vec<crb_judge::JudgeVerdict>)> {
    // ── Pre-compute content-addressed cache key components ──────────────
    let diff_hash = LlmCache::sha256(diff);
    let rules_hash = LlmCache::sha256(rules_preamble.unwrap_or(""));
    let judge_prompt_hash = LlmCache::sha256(crb_judge::JUDGE_PROMPT);
    let judge_model = ""; // We don't have judge_model here; it's baked into the judge Agent

    let mut agent_set = tokio::task::JoinSet::new();
    let prompt_lib = prompt_lib.clone();
    for &role in AGENT_ROLES {
        let client = client.clone();
        let model = model.to_string();
        let role = role.to_string();
        let diff = diff.to_string();
        let diff_hash = diff_hash.clone();
        let rules_hash = rules_hash.clone();
        let preamble = rules_preamble.map(String::from);
        let p_lib = prompt_lib.clone();
        let cache_arc: Option<Arc<dyn CacheBackend>> = cache.clone().map(|c| c as Arc<dyn CacheBackend>);
        let ct = cost_tracker.clone();
        let tx = dashboard_tx.map(|t| t.clone());

        agent_set.spawn(async move {
            let span = info_span!("agent", role = %role);
            let _guard = span.enter();

            // Compute agent cache key
            let prompt_hash = LlmCache::sha256(p_lib.get(&role));
            let agent_cache_key = LlmCache::compute_agent_key(
                &prompt_hash,
                &diff_hash,
                &model,
                &role,
                &rules_hash,
            );

            // Estimate tokens for this call
            let tokens_in = cost::estimate_tokens(&diff);

            // Check cache first
            if let Some(ref c) = cache_arc {
                if let Some(cached_response) = c.lookup_agent_by_key(&agent_cache_key) {
                    tracing::info!("CACHE HIT for agent role={} (key={})", role, &agent_cache_key[..12]);
                    let tokens_out = cost::estimate_tokens(&cached_response);
                    ct.record_agent(tokens_in, tokens_out, true);
                    // Send chunk + finished for cached response
                    if let Some(ref tx) = tx {
                        let _ = tx.send(DashboardEvent::AgentChunk {
                            role: role.clone(),
                            chunk: cached_response.clone(),
                        });
                        let result = parse_agent_findings(&cached_response);
                        let findings_count = result.as_ref().map(|v| v.len()).unwrap_or(0);
                        let _ = tx.send(DashboardEvent::AgentFinished {
                            role,
                            findings: findings_count,
                            success: result.is_ok(),
                        });
                    }
                    let result = parse_agent_findings(&cached_response);
                    return result;
                }
            }
            tracing::info!("CACHE MISS for agent role={} (key={})", role, &agent_cache_key[..12]);

            // Cache miss — make API call
            let tool_preamble = crb_tools::tool_prompt_section(&role, &crb_tools::budget::ToolCallBudget::default(), &[]);
            let agent = build_agent(&client, &model, &role, preamble.as_deref(), Some(&p_lib), None, Some(&tool_preamble));
            let result: Result<Vec<Finding>, String> = with_retry(
                || async {
                    let response = agent
                        .prompt(&diff)
                        .await
                        .map_err(|e| e.to_string())?;

                    let tokens_out = cost::estimate_tokens(&response);
                    ct.record_agent(tokens_in, tokens_out, false);

                    // Send chunk for live response
                    if let Some(ref tx) = tx {
                        let _ = tx.send(DashboardEvent::AgentChunk {
                            role: role.clone(),
                            chunk: response.clone(),
                        });
                    }

                    // Cache the prompt+response with content-addressed key
                    if let Some(ref c) = cache_arc {
                        c.save_agent_with_key(&agent_cache_key, &role, &diff, &response);
                    }

                    let findings = parse_agent_findings(&response);
                    // Send finished event
                    if let Some(ref tx) = tx {
                        let findings_count = findings.as_ref().map(|v| v.len()).unwrap_or(0);
                        let _ = tx.send(DashboardEvent::AgentFinished {
                            role: role.clone(),
                            findings: findings_count,
                            success: findings.is_ok(),
                        });
                    }
                    findings
                },
                3,    // max_retries
                1000, // base_delay_ms
            )
            .await;
            // If the whole retry chain failed, send failed event
            if result.is_err() {
                if let Some(ref tx) = tx {
                    let _ = tx.send(DashboardEvent::AgentFinished {
                        role: role.clone(),
                        findings: 0,
                        success: false,
                    });
                }
            }
            result
        });
    }

    let mut all_findings: Vec<Finding> = linter_findings;
    while let Some(res) = agent_set.join_next().await {
        match res {
            Ok(Ok(mut findings)) => all_findings.append(&mut findings),
            Ok(Err(e)) => tracing::warn!("Agent failed: {e}"),
            Err(e) => tracing::warn!("Agent join error: {e}"),
        }
    }

    // Judge evaluation: compare each finding against golden comments
    // Uses hybrid approach: Jaccard heuristic first, LLM judge fallback
    let mut verdicts = Vec::new();
    let jaccard_threshold = 0.12; // Matches Python step3_judge_comments threshold
    for finding in &all_findings {
        for gc in &pr.comments {
            // Step 1: Try Jaccard heuristic (no API call)
            if let Some(score) = crb_judge::jaccard_match(&finding.message, &gc.comment, jaccard_threshold) {
                tracing::info!(
                    "Jaccard match: finding='{}' golden='{}' score={:.2}",
                    &finding.message[..std::cmp::min(60, finding.message.len())],
                    &gc.comment[..std::cmp::min(60, gc.comment.len())],
                    score
                );
                verdicts.push(crb_judge::JudgeVerdict {
                    reasoning: format!("Matched by {:.0}% word overlap (Jaccard heuristic)", score * 100.0),
                    match_: true,
                    confidence: score,
                });
                continue;
            }

            // Step 2: File/line pre-filter (if available)
            if let Some(golden_file) = &gc.file {
                if let Some(finding_file) = &finding.file {
                    if golden_file != finding_file {
                        continue; // file mismatch — skip
                    }
                }
            }

            // Compute judge cache key
            let judge_key = LlmCache::compute_judge_key(
                &judge_prompt_hash,
                &finding.message,
                &gc.comment,
                judge_model,
            );

            // Estimate tokens for judge call
            let judge_prompt = format!("{}\n\nFinding: {}\nGolden: {}", crb_judge::JUDGE_PROMPT, finding.message, gc.comment);
            let tokens_in = cost::estimate_tokens(&judge_prompt);

            // Check judge cache first
            if let Some(ref c) = cache {
                if let Some(cached_verdict) = c.lookup_judge_by_key(&judge_key) {
                    tracing::info!("CACHE HIT for judge (key={})", &judge_key[..12]);
                    let tokens_out = cost::estimate_tokens(&serde_json::to_string(&cached_verdict).unwrap_or_default());
                    cost_tracker.record_judge(tokens_in, tokens_out, true);
                    verdicts.push(cached_verdict);
                    continue;
                }
            }

            // Cache miss — make API call
            tracing::info!("CACHE MISS for judge (key={})", &judge_key[..12]);
            match with_retry(
                || run_judge(judge, &gc.comment, &finding.message),
                3,    // max_retries
                1000, // base_delay_ms
            )
            .await
            {
                Ok(verdict) => {
                    let tokens_out = cost::estimate_tokens(&serde_json::to_string(&verdict).unwrap_or_default());
                    cost_tracker.record_judge(tokens_in, tokens_out, false);

                    // Cache the judge call if cache is active
                    if let Some(ref c) = cache {
                        let verdict_json = serde_json::to_string(&verdict).unwrap_or_default();
                        let _ = c.save_judge_with_key(&judge_key, &gc.comment, &finding.message, &verdict_json);
                    }
                    verdicts.push(verdict);
                }
                Err(e) => tracing::warn!("Judge call failed after retries: {e}"),
            }
        }
    }

    Ok((all_findings, verdicts))
}

/// Run the multi-agent consensus evaluation, merging linter findings.
async fn evaluate_pr_consensus(
    pr: &GoldenCommentEntry,
    client: &rig_core::providers::openai::Client,
    model: &str,
    judge: &rig_core::agent::Agent<
        rig_core::providers::openai::responses_api::ResponsesCompletionModel,
    >,
    diff: &str,
    linter_findings: Vec<Finding>,
    rules_preamble: Option<&str>,
    prompt_lib: &PromptLibrary,
    roles: &str,
    max_findings: usize,
    cache: Option<Arc<LlmCache>>,
    _cost_tracker: Arc<CostTracker>,
) -> Result<(Vec<Finding>, Vec<crb_judge::JudgeVerdict>)> {
    // Parse comma-separated roles
    let parsed_roles: Vec<&str> = roles.split(',').map(|r| r.trim()).filter(|r| !r.is_empty()).collect();

    // ── Pre-compute content-addressed cache key components ──────────────
    let diff_hash = LlmCache::sha256(diff);
    let rules_hash = LlmCache::sha256(rules_preamble.unwrap_or(""));
    let judge_prompt_hash = LlmCache::sha256(crb_judge::JUDGE_PROMPT);
    // Use the first agent role's prompt hash as the prompt hash — in practice
    // each role has its own prompt template, but the consensus pipeline uses
    // one prompt_hash for all reviewers. For a more granular approach, we'd
    // compute per-role cache keys inside run_reviewers.
    let first_role = parsed_roles.first().copied().unwrap_or("SA");
    let prompt_hash = LlmCache::sha256(prompt_lib.get(first_role));
    let judge_model = ""; // We don't have judge_model here

    // Compute tool preamble for the first role (all roles get same tool description for now)
    let default_budget = crb_tools::budget::ToolCallBudget::default();
    let tool_preamble = crb_tools::tool_prompt_section(first_role, &default_budget, &[]);

    let result = evaluate_pr_with_consensus(
        pr, diff, client, model, judge, rules_preamble, Some(prompt_lib), None,
        &parsed_roles, max_findings, cache.clone().map(|c| c as Arc<dyn CacheBackend>),
        &diff_hash, &prompt_hash, &rules_hash, &judge_prompt_hash, judge_model,
        Some(&tool_preamble),
    )
    .await?;

    info!(
        "Consensus pipeline: {} agent findings, {} linter findings, {} goldens",
        result.findings_count,
        linter_findings.len(),
        result.golden_count
    );

    // Merge linter findings into the result
    let all_findings: Vec<Finding> = linter_findings;

    Ok((all_findings, result.verdicts))
}

/// Post-process findings through aggregator dedup and auditor severity checks.
///
/// Converts `Finding` structs to the serde_json `Map` format used by the
/// aggregator and auditor crates, then runs the full pipeline:
/// 1. `semantic_dedup` — merge semantically identical findings
/// 2. `apply_severity_auditor` — downgrade inflated severity labels
fn post_process_findings(findings: &[Finding]) -> Vec<Finding> {
    if findings.is_empty() {
        return findings.to_vec();
    }

    // Convert Finding → serde_json::Map<String, Value> using helper
    let maps: Vec<serde_json::Map<String, serde_json::Value>> = findings
        .iter()
        .map(crb_agents::finding_to_map)
        .collect();

    // Step 1: semantic dedup
    let deduped = crb_aggregator::semantic_dedup(maps);

    // Step 2: severity auditor
    let audited = crb_auditor::apply_severity_auditor(deduped);

    // Convert back to Finding using helper
    audited
        .iter()
        .filter_map(crb_agents::map_to_finding)
        .collect()
}

/// Run the validation pipeline: load baseline, read results from output dir,
/// compute average metrics, compare against thresholds, and exit with
/// the appropriate code (0 = pass, 1 = fail).
async fn run_validate(workspace_root: &std::path::Path, version: &str) -> Result<()> {
    info!("Running validation against baseline v{version}");

    let baseline = validation::load_baseline(workspace_root, version)?;
    info!("Loaded baseline for version: {}", baseline.version);

    // Read results from the output directory
    let output_dir = workspace_root.join("output");
    let results_dir = if output_dir.exists() {
        output_dir
    } else {
        anyhow::bail!(
            "Output directory not found: {}. Run the harness first.",
            output_dir.display()
        );
    };

    // Collect all PR result JSON files from the output directory
    let mut loaded_results: Vec<crb_judge::Metrics> = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(&results_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "json"))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in &entries {
        let path = entry.path();
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read result: {}", path.display()))?;
        // Try to deserialize as Metrics directly
        match serde_json::from_str::<crb_judge::Metrics>(&content) {
            Ok(metrics) => loaded_results.push(metrics),
            Err(e) => {
                tracing::warn!(
                    "Skipping {}: could not parse as Metrics: {e}",
                    path.display()
                );
            }
        }
    }

    if loaded_results.is_empty() {
        anyhow::bail!(
            "No valid PR result files found in {}",
            results_dir.display()
        );
    }

    let total_prs = loaded_results.len();
    let (avg_precision, avg_recall, avg_f1) =
        validation::compute_average_metrics(&loaded_results);
    let val_result = validation::validate_against_baseline(
        &baseline,
        total_prs,
        avg_precision,
        avg_recall,
        avg_f1,
    );
    validation::print_validation_summary(&baseline, &val_result, avg_precision, avg_recall, avg_f1);

    if val_result.in_threshold {
        info!("Validation PASSED — all metrics within baseline thresholds");
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "Validation FAILED — metrics exceed baseline thresholds"
        ))
    }
}

/// Internal helper module for reporting utilities that are not public API.
mod utils {
    /// Sanitize a string for use as a filename.
    pub fn sanitize_filename(name: &str) -> String {
        name.chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            })
            .collect()
    }
}
