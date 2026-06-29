use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use clap::{Parser, Subcommand};
use crb_agents::prompts::PromptLibrary;
use crb_dashboard::DashboardEvent;
use crb_judge::build_judge;
use crb_reporting::{load_golden_datasets, write_report, GoldenCommentEntry};
use crb_rules::RuleSet;
use rig_core::client::ProviderClient;
use tokio::sync::broadcast;
use tracing::{info, info_span};

mod diffs;
mod scaffold;
mod validate;

/// CLI tool for code review benchmark preparation tasks.
#[derive(Debug, Parser)]
#[command(name = "crb-benchmark", about = "Benchmark preparation CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Clone/fetch all benchmark repos for a dataset.
    Scaffold {
        /// Directory containing golden comment dataset JSONs.
        #[arg(long, default_value = "datasets/golden_comments")]
        dataset_dir: PathBuf,

        /// Benchmark directory (contains base-repos/, diffs/, worktrees/).
        #[arg(long, default_value = "benchmark")]
        benchmark_dir: PathBuf,
    },
    /// Extract diffs from scaffolded repos into persistent worktrees.
    FetchDiffs {
        /// Benchmark directory (contains base-repos/, diffs/, worktrees/).
        #[arg(long, default_value = "benchmark")]
        benchmark_dir: PathBuf,
    },
    /// Validate golden datasets for integrity.
    Validate {
        /// Directory containing golden comment datasets.
        #[arg(long, default_value = "datasets/golden_comments")]
        dataset_dir: PathBuf,
    },
    /// Show all PRs in a dataset with URLs.
    List {
        /// Directory containing golden comment datasets.
        #[arg(long, default_value = "datasets/golden_comments")]
        dataset_dir: PathBuf,
    },
    /// Remove worktrees and optionally diffs from a benchmark directory.
    Clean {
        /// Benchmark directory (contains base-repos/, diffs/, worktrees/).
        #[arg(long, default_value = "benchmark")]
        benchmark_dir: PathBuf,

        /// Also remove diffs directory.
        #[arg(long, default_value_t = false)]
        all: bool,
    },
    /// Run the full benchmark evaluation pipeline over a dataset.
    Run {
        /// Benchmark directory (contains base-repos/, diffs/, worktrees/).
        #[arg(long, env = "BENCHMARK_DIR", default_value = "benchmark")]
        benchmark_dir: String,
        /// Directory containing golden comment datasets.
        #[arg(long, env = "DATASET_DIR", default_value = "datasets/golden_comments")]
        dataset_dir: String,
        /// Directory for evaluation output (JSON per-PR + summary CSV).
        #[arg(long, short = 'o', env = "OUTPUT_DIR", default_value = "output")]
        output_dir: String,
        /// Model for agent reviews.
        #[arg(long, env = "MODEL", default_value = "deepseek/deepseek-v4-pro")]
        model: String,
        /// Model for the LLM judge.
        #[arg(long, env = "JUDGE_MODEL", default_value = "deepseek/deepseek-v4-flash")]
        judge_model: String,
        /// Maximum concurrent PR evaluations.
        #[arg(long, env = "CONCURRENCY", default_value_t = 4)]
        concurrency: usize,
        /// Dry run: load config and datasets, print stats, then exit.
        #[arg(long, default_value_t = false)]
        dry_run: bool,
        /// Resume mode: skip PRs with existing result files.
        #[arg(long, default_value_t = false)]
        resume: bool,
        /// Skip linter execution.
        #[arg(long, default_value_t = false)]
        skip_linters: bool,
        /// Only run linters, skip LLM agents.
        #[arg(long, default_value_t = false)]
        linters_only: bool,
        /// Skip consensus orchestration.
        #[arg(long, default_value_t = false)]
        skip_consensus: bool,
        /// Path to linters.toml.
        #[arg(long, env = "LINTERS_CONFIG", default_value = "linters.toml")]
        linters_config: String,
        /// Skip rule loading.
        #[arg(long, default_value_t = false)]
        skip_rules: bool,
        /// Path to prompts directory.
        #[arg(long, env = "PROMPTS_DIR", default_value = "prompts/builtin")]
        prompts_dir: PathBuf,
        /// Validate mode.
        #[arg(long, default_value_t = false)]
        validate: bool,
        /// CI mode.
        #[arg(long, default_value_t = false)]
        ci: bool,
        /// Comma-separated agent roles.
        #[arg(long, env = "ROLES", default_value = "SA,CL,AR,SEC")]
        roles: String,
        /// Max findings per agent.
        #[arg(long, env = "MAX_FINDINGS", default_value_t = 20)]
        max_findings: usize,
        /// PR filter pattern.
        #[arg(long)]
        pr_filter: Option<String>,
        /// Cache directory.
        #[arg(long, env = "CACHE_DIR")]
        cache_dir: Option<PathBuf>,
        /// Dashboard mode (TUI).
        #[arg(long, default_value_t = false)]
        dashboard: bool,
        /// Dashboard events JSON output.
        #[arg(long, default_value_t = false)]
        dashboard_events: bool,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Scaffold { dataset_dir, benchmark_dir } => {
            scaffold::run(&dataset_dir, &benchmark_dir)?;
        }
        Commands::FetchDiffs { benchmark_dir } => {
            diffs::run(&benchmark_dir)?;
        }
        Commands::Validate { dataset_dir } => {
            validate::run_validate(&dataset_dir)?;
        }
        Commands::List { dataset_dir } => {
            run_list(&dataset_dir)?;
        }
        Commands::Clean { benchmark_dir, all } => {
            run_clean(&benchmark_dir, all)?;
        }
        Commands::Run {
            benchmark_dir,
            dataset_dir,
            output_dir,
            model,
            judge_model,
            concurrency,
            dry_run,
            resume,
            skip_linters,
            linters_only,
            skip_consensus,
            linters_config,
            skip_rules,
            prompts_dir,
            validate,
            ci,
            roles,
            max_findings,
            pr_filter,
            cache_dir,
            dashboard,
            dashboard_events,
        } => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(run_benchmark(
                benchmark_dir,
                dataset_dir,
                output_dir,
                model,
                judge_model,
                concurrency,
                dry_run,
                resume,
                skip_linters,
                linters_only,
                skip_consensus,
                linters_config,
                skip_rules,
                prompts_dir,
                validate,
                ci,
                roles,
                max_findings,
                pr_filter,
                cache_dir,
                dashboard,
                dashboard_events,
            ))?;
        }
    }

    Ok(())
}

/// List all PRs in a dataset with their URLs and titles.
fn run_list(dataset_dir: &PathBuf) -> Result<()> {
    let entries = crb_reporting::load_golden_datasets(dataset_dir)?;
    let mut repos = std::collections::BTreeSet::new();

    for entry in &entries {
        // Extract repo name from URL: "https://github.com/repo-owner/repo-name/pull/N"
        let repo_name = entry
            .url
            .trim_end_matches('/')
            .rsplit('/')
            .nth(2)
            .unwrap_or("unknown");
        let pr_number = entry
            .url
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("0");
        println!("{}/{}   {}", repo_name, pr_number, entry.pr_title);
        repos.insert(repo_name.to_string());
    }

    println!("\nTotal: {} PRs across {} repos", entries.len(), repos.len());
    Ok(())
}

/// Remove worktrees and optionally diffs from a benchmark directory.
fn run_clean(benchmark_dir: &PathBuf, all: bool) -> Result<()> {
    let worktrees_dir = benchmark_dir.join("worktrees");

    if worktrees_dir.exists() {
        // Remove each worktree using `git worktree remove --force`
        for entry in std::fs::read_dir(&worktrees_dir)? {
            let entry = entry?;
            let wt_path = entry.path();
            if !wt_path.is_dir() {
                continue;
            }
            if wt_path.join(".git").exists() {
                let status = std::process::Command::new("git")
                    .args(["worktree", "remove", "--force"])
                    .arg(&wt_path)
                    .status()?;
                if status.success() {
                    println!("Removed worktree: {}", wt_path.display());
                } else {
                    tracing::warn!("Failed to remove worktree at {}", wt_path.display());
                }
            }
        }

        // Prune orphaned worktree metadata
        let _ = std::process::Command::new("git")
            .args(["worktree", "prune"])
            .status();

        // Remove the worktrees directory itself
        std::fs::remove_dir_all(&worktrees_dir)?;
        println!("Removed worktrees directory: {}", worktrees_dir.display());
    } else {
        println!("No worktrees directory found at {}", worktrees_dir.display());
    }

    if all {
        let diffs_dir = benchmark_dir.join("diffs");
        if diffs_dir.exists() {
            std::fs::remove_dir_all(&diffs_dir)?;
            println!("Removed diffs directory: {}", diffs_dir.display());
        } else {
            println!("No diffs directory found at {}", diffs_dir.display());
        }
    }

    Ok(())
}

/// Run the full benchmark evaluation pipeline over a dataset.
#[allow(clippy::too_many_arguments)]
async fn run_benchmark(
    benchmark_dir: String,
    dataset_dir: String,
    output_dir: String,
    model: String,
    judge_model: String,
    concurrency: usize,
    dry_run: bool,
    resume: bool,
    skip_linters: bool,
    linters_only: bool,
    skip_consensus: bool,
    linters_config: String,
    skip_rules: bool,
    prompts_dir: PathBuf,
    validate: bool,
    ci: bool,
    roles: String,
    max_findings: usize,
    pr_filter: Option<String>,
    cache_dir: Option<PathBuf>,
    dashboard: bool,
    dashboard_events: bool,
) -> Result<()> {

    let output_dir = PathBuf::from(&output_dir);
    let dataset_dir = PathBuf::from(&dataset_dir);
    let benchmark_dir = PathBuf::from(&benchmark_dir);

    // ── --validate flag ────────────────────────────────────────────────────
    let workspace_root = std::env::current_dir()
        .context("Failed to determine current working directory")?;
    if validate {
        return crb_harness::run_validate(&workspace_root, "5.14").await;
    }

    let _span =
        info_span!("harness", model = %model, concurrency = %concurrency).entered();

    // ── Load datasets ─────────────────────────────────────────────────────
    info!("Loading golden datasets from: {}", dataset_dir.display());
    let all_prs = load_golden_datasets(&dataset_dir)?;
    info!("Loaded {} PR entries total", all_prs.len());

    // ── --pr-filter flag with exact PR number match ──────────────────────
    let all_prs = if let Some(ref filter) = pr_filter {
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
    if dry_run {
        println!("[DRY RUN] Would evaluate {} PR(s)", all_prs.len());
        println!("  Model:              {}", model);
        println!("  Judge model:        {}", judge_model);
        println!("  Concurrency:        {}", concurrency);
        println!("  Dataset:            {}", dataset_dir.display());
        println!("  Output:             {}", output_dir.display());
        println!("  Benchmark dir:      {}", benchmark_dir.display());
        println!("  Skip consensus:     {}", skip_consensus);
        println!("  Skip linters:       {}", skip_linters);
        println!("  Linters only:       {}", linters_only);
        if let Some(ref cache_dir) = cache_dir {
            println!("  Cache dir:          {}", cache_dir.display());
        }
        return Ok(());
    }

    // ── Resume support ────────────────────────────────────────────────────
    let prs_to_evaluate: Vec<&GoldenCommentEntry> = if resume {
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

    let judge = build_judge(&client, &judge_model);

    // ── Linter config ─────────────────────────────────────────────────────
    let linter_config_path = std::path::Path::new(&linters_config);
    let linter_configs = if linter_config_path.exists() && !skip_linters {
        match crb_tools::load_linter_config(&linters_config) {
            Ok(configs) => {
                info!(
                    "Loaded {} linter(s) from {}",
                    configs.len(),
                    linters_config
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
    let ruleset = if !skip_rules {
        let rules_dir = std::path::Path::new(".crb/rules/");
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
        if prompts_dir.exists() {
            match lib.load_from_dir(&prompts_dir) {
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
        } else if prompts_dir.to_string_lossy() != "prompts/builtin" {
            tracing::warn!(
                "Custom prompts directory '{}' not found — using built-in defaults",
                prompts_dir.display()
            );
        }
        lib
    });

    // ── Cache directory ───────────────────────────────────────────────────
    let start_time = std::time::Instant::now();

    // ── Dashboard event system (TUI and/or JSON stdout) ─────────────────
    let (event_broadcast_tx, _) = broadcast::channel::<DashboardEvent>(256);

    let dashboard_tx: Option<broadcast::Sender<DashboardEvent>> = if dashboard {
        let mut rx = event_broadcast_tx.subscribe();
        let total_prs = prs_to_evaluate.len();
        let (mpsc_tx, mpsc_rx) = tokio::sync::mpsc::channel::<DashboardEvent>(1024);
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
    } else if dashboard_events {
        Some(event_broadcast_tx.clone())
    } else {
        None
    };

    // ── Dashboard Events (JSON stdout) ──────────────────────────────────
    if dashboard_events {
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
    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(concurrency));
    let mut set = tokio::task::JoinSet::new();

    for pr in prs_to_evaluate {
        let client = client.clone();
        let sem = sem.clone();
        let judge = judge.clone();
        let pr = pr.clone();
        let model = model.clone();
        let benchmark_dir = benchmark_dir.clone();
        let linter_configs = linter_configs.clone();
        let skip_consensus = skip_consensus;
        let linters_only = linters_only;
        let ruleset = ruleset.clone();
        let prompt_lib = prompt_lib.clone();
        let roles = roles.clone();
        let max_findings = max_findings;
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
            &model,
            &judge_model,
            &results,
            start_time.elapsed(),
        )?;
    }

    // ── --ci flag: validate and exit with proper code ─────────────────────
    if ci {
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
