use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use clap::Parser;
use crb_agents::prompts::PromptLibrary;
use crb_dashboard::DashboardEvent;
use crb_judge::build_judge;
use crb_reporting::{load_golden_datasets, write_report, GoldenCommentEntry};
use crb_rules::RuleSet;
use rig_core::client::ProviderClient;
use tokio::sync::{broadcast, mpsc};
use tracing::info;
use tracing::info_span;
use tracing_subscriber::EnvFilter;

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
    let cli = crb_harness::config::Cli::parse();

    match cli {
        crb_harness::config::Cli::Review(args) => run_review(args).await,
        crb_harness::config::Cli::Benchmark(args) => run_benchmark(args).await,
    }
}

/// Run the `review` subcommand: get a git diff and print findings.
async fn run_review(args: crb_harness::config::ReviewArgs) -> Result<()> {
    let findings = crb_harness::review_diff(args)?;
    println!("{}", serde_json::to_string_pretty(&findings)?);
    Ok(())
}

/// Run the full benchmark pipeline.
async fn run_benchmark(args: crb_harness::config::BenchmarkArgs) -> Result<()> {
    let output_dir = PathBuf::from(&args.output_dir);
    let dataset_dir = PathBuf::from(&args.dataset_dir);
    let benchmark_dir = PathBuf::from(&args.benchmark_dir);

    // ── --validate flag ────────────────────────────────────────────────────
    let workspace_root = std::env::current_dir()
        .context("Failed to determine current working directory")?;
    if args.validate {
        return crb_harness::run_validate(&workspace_root, "5.14").await;
    }

    let _span =
        info_span!("harness", model = %args.model, concurrency = %args.concurrency).entered();

    // ── Load datasets ─────────────────────────────────────────────────────
    info!("Loading golden datasets from: {}", dataset_dir.display());
    let all_prs = load_golden_datasets(&dataset_dir)?;
    info!("Loaded {} PR entries total", all_prs.len());

    // ── --pr-filter flag with exact PR number match fix ──────────────────
    let all_prs = if let Some(ref filter) = args.pr_filter {
        let filter_patterns: HashSet<String> = filter
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .collect();

        let available_urls: Vec<String> = all_prs.iter().map(|pr| pr.url.clone()).collect();

        let filtered: Vec<GoldenCommentEntry> = all_prs
            .iter()
            .filter(|pr| {
                let url_lower = pr.url.to_lowercase();
                filter_patterns.iter().any(|pattern| {
                    // Parse pattern as "repo/N" where N is a PR number
                    if let Some((repo_part, pr_num_str)) = pattern.split_once('/') {
                        if let Ok(pr_num) = pr_num_str.parse::<u32>() {
                            // Exact PR number match: find `/pull/N` in the URL
                            let target = format!("/pull/{}", pr_num);
                            if url_lower.contains(&target) && url_lower.contains(repo_part) {
                                return true;
                            }
                        }
                    }
                    // Fallback: substring match on the full URL
                    url_lower.contains(pattern)
                })
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
        println!("  Benchmark dir:      {}", benchmark_dir.display());
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
        let existing: HashSet<String> = if output_dir.exists() {
            std::fs::read_dir(&output_dir)?
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map_or(false, |ext| ext == "json"))
                .filter_map(|e| e.file_name().into_string().ok())
                .collect()
        } else {
            HashSet::new()
        };

        all_prs
            .iter()
            .filter(|pr| {
                let filename = crb_harness::utils::sanitize_filename(&pr.pr_title);
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
        if prompts_dir.exists() {
            match lib.load_from_dir(prompts_dir) {
                Ok(()) => {
                    info!("Loaded prompts from: {}", prompts_dir.display());
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to load prompts from {}: {e}",
                        prompts_dir.display()
                    );
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
    let (event_broadcast_tx, _) = broadcast::channel::<DashboardEvent>(256);

    let dashboard_tx: Option<broadcast::Sender<DashboardEvent>> = if args.dashboard {
        let mut rx = event_broadcast_tx.subscribe();
        let total_prs = prs_to_evaluate.len();
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
        let benchmark_dir = benchmark_dir.clone();
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
            crb_harness::evaluate_pr_with_postprocessing(
                &pr,
                &client,
                &model,
                &judge,
                &benchmark_dir,
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
                total_tokens +=
                    c.agent_tokens_in + c.agent_tokens_out + c.judge_tokens_in + c.judge_tokens_out;
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
    crb_harness::print_terminal_summary(&results);

    // ── Write _summary.json ──────────────────────────────────────────────
    if let Some(ref cache_dir_path) = cache_dir {
        crb_harness::write_summary(
            cache_dir_path,
            &args,
            &results,
            start_time.elapsed(),
        )?;
    }

    // ── --ci flag: validate and exit with proper code ─────────────────────
    if args.ci {
        let metrics: Vec<crb_judge::Metrics> =
            results.iter().map(|r| r.metrics.clone()).collect();
        let (avg_precision, avg_recall, avg_f1) =
            crb_harness::validation::compute_average_metrics(&metrics);
        let baseline = crb_harness::validation::load_baseline(&workspace_root, "5.14")?;
        let val_result = crb_harness::validation::validate_against_baseline(
            &baseline,
            results.len(),
            avg_precision,
            avg_recall,
            avg_f1,
        );
        crb_harness::validation::print_validation_summary(
            &baseline,
            &val_result,
            avg_precision,
            avg_recall,
            avg_f1,
        );

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
