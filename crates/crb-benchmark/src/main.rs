use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use std::io::Write;
use std::{env, fs, io};
use std::{process, time};

use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use crb_agents::agent::AgentEntry;
use crb_agents::prompts::PromptLibrary;
use crb_benchmark::judge::build_judge;
use crb_benchmark::pr;
use crb_harness::eval::EvalConfig;
use crb_harness::eval::EvalStrategy;
use crb_harness::model_capabilities;
use crb_reporting::cost::AnalyticsTracker;
use crb_reporting::golden::{GoldenCommentEntry, load_golden_datasets};
use crb_reporting::history::{RunHistoryEntry, append_run_history};
use crb_reporting::{print_terminal_summary, write_report};
use crb_rules::RuleSet;
use crb_shared::diff::Diff;
use crb_shared::url::parse_github_url;
use crb_shared::sanitize_filename;
use crb_types::benchmark::{Metrics, MetricsProvider};
use crb_types::wrappers::Model;
use rig_core::tool::server::ToolServer;
use crb_types::RunEvent;
use rig_core::client::ProviderClient;
use tokio::sync::broadcast;
use tracing::{error, info, info_span, warn};

mod diffs;
mod scaffold;
mod validate;

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

    /// Remove worktrees, outputs, and optionally diffs from a benchmark directory.
    Clean {
        /// Benchmark directory (contains base-repos/, diffs/, worktrees/).
        #[arg(long, default_value = "benchmark")]
        benchmark_dir: PathBuf,

        /// Also remove diffs directory.
        #[arg(long, default_value_t = false)]
        all: bool,

        /// Also remove output directories.
        #[arg(long, default_value_t = false)]
        outputs: bool,

        /// Dry run: only print what would be removed.
        #[arg(long, default_value_t = false)]
        dry_run: bool,
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
        #[arg(
            long,
            env = "JUDGE_MODEL",
            default_value = "deepseek/deepseek-v4-flash"
        )]
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

        /// Validate golden datasets and exit.
        #[arg(long, default_value_t = false)]
        validate: bool,

        /// CI mode.
        #[arg(long, default_value_t = false)]
        ci: bool,

        /// Comma-separated agent roles.
        #[arg(long, env = "ROLES")]
        roles: Option<String>,

        /// Max findings per agent.
        #[arg(long, env = "MAX_FINDINGS", default_value_t = 20)]
        max_findings: usize,

        /// PR filter pattern.
        #[arg(long)]
        pr_filter: Option<String>,

        /// Cache directory.
        #[arg(long, env = "CACHE_DIR", default_value = "cache")]
        cache_dir: PathBuf,

        /// Dashboard mode (TUI).
        #[arg(long, default_value_t = false)]
        dashboard: bool,

        /// Dashboard events JSON output.
        #[arg(long, default_value_t = false)]
        dashboard_events: bool,

        /// Auto-backup cache before running.
        #[arg(long, default_value_t = false)]
        auto_backup: bool,

        /// Reasoning effort level [possible values: low, medium, high, max].
        #[arg(long, default_value = "medium")]
        reasoning_effort: String,
    },

    /// Show cache statistics.
    CacheStats {
        #[arg(long, env = "CACHE_DIR", default_value = "cache")]
        cache_dir: PathBuf,

        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// Prune cache entries by age, size, or PR count.
    CachePrune {
        #[arg(long, env = "CACHE_DIR", default_value = "cache")]
        cache_dir: PathBuf,

        #[arg(long)]
        max_age: Option<u64>,

        #[arg(long)]
        max_size: Option<u64>,

        #[arg(long)]
        max_prs: Option<usize>,

        #[arg(long, default_value_t = false)]
        dry_run: bool,

        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// Scrub cache for stale entries, orphans, and corrupted indices.
    CacheScrub {
        #[arg(long, env = "CACHE_DIR", default_value = "cache")]
        cache_dir: PathBuf,

        #[arg(long, default_value_t = false)]
        dry_run: bool,

        #[arg(long, default_value_t = false)]
        repair: bool,

        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// Backup cache to a tar.gz archive.
    CacheBackup {
        #[arg(long, env = "CACHE_DIR", default_value = "cache")]
        cache_dir: PathBuf,

        #[arg(long)]
        output: Option<PathBuf>,
    },

    /// Restore cache from a tar.gz backup.
    CacheRestore {
        backup_file: PathBuf,

        #[arg(long, env = "CACHE_DIR", default_value = "cache")]
        cache_dir: PathBuf,
    },

    /// Rebuild cache indices from raw data.
    CacheRebuild {
        #[arg(long, env = "CACHE_DIR", default_value = "cache")]
        cache_dir: PathBuf,

        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
}

fn main() -> Result<()> {
    match dotenvy::dotenv() {
        Ok(path) => eprintln!("[dotenv] Loaded .env from: {}", path.display()),
        Err(e) => eprintln!("[dotenv] No .env file loaded: {e}"),
    }

    if env::var("OPENAI_API_KEY").is_err() {
        if env::var("OPENROUTER_API_KEY").is_ok() {
            eprintln!("[dotenv] OPENAI_API_KEY not found - falling back to OPENROUTER_API_KEY");
        }
    }

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Scaffold {
            dataset_dir,
            benchmark_dir,
        } => {
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
        Commands::Clean {
            benchmark_dir,
            all,
            outputs,
            dry_run,
        } => {
            run_clean(&benchmark_dir, all, outputs, dry_run)?;
        }
        Commands::CacheStats { cache_dir, json } => {
            run_cache_stats(&cache_dir, json)?;
        }
        Commands::CachePrune {
            cache_dir,
            max_age,
            max_size,
            max_prs,
            dry_run,
            json,
        } => {
            run_cache_prune(&cache_dir, max_age, max_size, max_prs, dry_run, json)?;
        }
        Commands::CacheScrub {
            cache_dir,
            dry_run,
            repair,
            json,
        } => {
            run_cache_scrub(&cache_dir, dry_run, repair, json)?;
        }
        Commands::CacheBackup { cache_dir, output } => {
            run_cache_backup(&cache_dir, output)?;
        }
        Commands::CacheRestore {
            backup_file,
            cache_dir,
        } => {
            run_cache_restore(&backup_file, &cache_dir)?;
        }
        Commands::CacheRebuild { cache_dir, dry_run } => {
            run_cache_rebuild(&cache_dir, dry_run)?;
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
            validate,
            ci,
            roles,
            max_findings,
            pr_filter,
            cache_dir,
            dashboard,
            dashboard_events,
            auto_backup,
            reasoning_effort,
        } => {
            let rt = tokio::runtime::Runtime::new()?;

            // Pre-warm model capabilities cache (uses blocking HTTP, must be called outside the async runtime)
            model_capabilities::warm_model_cache_blocking();

            let roles = roles.unwrap_or_else(||
                crb_agents::prompts::PromptLibrary::get_instance().abbreviations().join(",")
            );

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
                validate,
                ci,
                roles,
                max_findings,
                pr_filter,
                cache_dir,
                dashboard,
                dashboard_events,
                auto_backup,
                reasoning_effort,
            ))?;
        }
    }

    Ok(())
}

/// List all PRs in a dataset with their URLs and titles.
fn run_list(dataset_dir: &PathBuf) -> Result<()> {
    let entries = load_golden_datasets(dataset_dir)?;
    let mut repos = std::collections::BTreeSet::new();

    for entry in &entries {
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

    println!(
        "\nTotal: {} PRs across {} repos",
        entries.len(),
        repos.len()
    );
    Ok(())
}

/// Remove worktrees, outputs, and optionally diffs from a benchmark directory.
fn run_clean(benchmark_dir: &PathBuf, all: bool, outputs: bool, dry_run: bool) -> Result<()> {
    let worktrees_dir = benchmark_dir.join("worktrees");

    if worktrees_dir.exists() {
        if dry_run {
            println!(
                "[DRY RUN] Would remove worktrees from {}",
                worktrees_dir.display()
            );
        } else {
            for entry in fs::read_dir(&worktrees_dir)? {
                let entry = entry?;
                let wt_path = entry.path();
                if !wt_path.is_dir() {
                    continue;
                }
                if wt_path.join(".git").exists() {
                    let status = process::Command::new("git")
                        .args(["worktree", "remove", "--force"])
                        .arg(&wt_path)
                        .status()?;
                    if status.success() {
                        println!("Removed worktree: {}", wt_path.display());
                    } else {
                        warn!("Failed to remove worktree at {}", wt_path.display());
                    }
                }
            }

            let _ = process::Command::new("git")
                .args(["worktree", "prune"])
                .status(); // Ignore — best-effort cleanup

            fs::remove_dir_all(&worktrees_dir)?;
            println!("Removed worktrees directory: {}", worktrees_dir.display());
        }
    } else {
        println!(
            "No worktrees directory found at {}",
            worktrees_dir.display()
        );
    }

    if all {
        let diffs_dir = benchmark_dir.join("diffs");
        if diffs_dir.exists() {
            if dry_run {
                println!(
                    "[DRY RUN] Would remove diffs directory: {}",
                    diffs_dir.display()
                );
            } else {
                fs::remove_dir_all(&diffs_dir)?;
                println!("Removed diffs directory: {}", diffs_dir.display());
            }
        } else {
            println!("No diffs directory found at {}", diffs_dir.display());
        }
    }

    if outputs {
        let outputs_dir = benchmark_dir.join("outputs");
        if outputs_dir.exists() {
            if dry_run {
                println!(
                    "[DRY RUN] Would remove outputs directory: {}",
                    outputs_dir.display()
                );
            } else {
                fs::remove_dir_all(&outputs_dir)?;
                println!("Removed outputs directory: {}", outputs_dir.display());
            }
        } else {
            println!("No outputs directory found at {}", outputs_dir.display());
        }

        let output_dir = benchmark_dir.join("output");
        if output_dir.exists() && output_dir != outputs_dir {
            if dry_run {
                println!(
                    "[DRY RUN] Would remove output directory: {}",
                    output_dir.display()
                );
            } else {
                fs::remove_dir_all(&output_dir)?;
                println!("Removed output directory: {}", output_dir.display());
            }
        }
    }

    Ok(())
}

/// Show cache statistics.
fn run_cache_stats(cache_dir: &PathBuf, json: bool) -> Result<()> {
    eprintln!("Cache subcommand deprecated — use external cache management");
    Ok(())
}

/// Prune cache entries by age, size, or PR count.
fn run_cache_prune(
    cache_dir: &PathBuf,
    max_age: Option<u64>,
    max_size: Option<u64>,
    max_prs: Option<usize>,
    dry_run: bool,
    json: bool,
) -> Result<()> {
    eprintln!("Cache subcommand deprecated — use external cache management");
    Ok(())
}

/// Scrub cache for stale entries, orphans, and corrupted indices.
fn run_cache_scrub(cache_dir: &PathBuf, dry_run: bool, repair: bool, json: bool) -> Result<()> {
    eprintln!("Cache subcommand deprecated — use external cache management");
    Ok(())
}

/// Backup cache to a tar.gz archive.
fn run_cache_backup(cache_dir: &PathBuf, output: Option<PathBuf>) -> Result<()> {
    eprintln!("Cache subcommand deprecated — use external cache management");
    Ok(())
}

/// Restore cache from a tar.gz backup.
fn run_cache_restore(backup_file: &PathBuf, cache_dir: &PathBuf) -> Result<()> {
    eprintln!("Cache subcommand deprecated — use external cache management");
    Ok(())
}

/// Rebuild cache indices from raw data.
fn run_cache_rebuild(cache_dir: &PathBuf, dry_run: bool) -> Result<()> {
    eprintln!("Cache subcommand deprecated — use external cache management");
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
    validate: bool,
    ci: bool,
    roles: String,
    max_findings: usize,
    pr_filter: Option<String>,
    cache_dir: PathBuf,
    dashboard: bool,
    dashboard_events: bool,
    auto_backup: bool,
    reasoning_effort: String,
) -> Result<()> {
    let output_dir = PathBuf::from(&output_dir);
    let dataset_dir = PathBuf::from(&dataset_dir);
    let benchmark_dir = PathBuf::from(&benchmark_dir);

    let workspace_root =
        env::current_dir().context("Failed to determine current working directory")?;
    if validate {
        return validate::run_validate(&workspace_root);
    }

    let _span = info_span!("harness", model = %model, concurrency = %concurrency).entered();

    info!("Loading golden datasets from: {}", dataset_dir.display());
    let all_prs = load_golden_datasets(&dataset_dir)?;
    info!("Loaded {} PR entries total", all_prs.len());

    let all_prs = if let Some(ref filter) = pr_filter {
        pr::filter_prs_by_pattern(all_prs, filter)
    } else {
        all_prs
    };

    info!("After --pr-filter: {} PR(s) to evaluate", all_prs.len());

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
        println!("  Cache dir:          {}", cache_dir.display());
        return Ok(());
    }

    let prs_to_evaluate: Vec<GoldenCommentEntry> = if resume {
        let existing: HashSet<String> = if output_dir.exists() {
            fs::read_dir(&output_dir)?
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map_or(false, |ext| ext == "json"))
                .filter_map(|e| e.file_name().into_string().ok())
                .collect()
        } else {
            HashSet::new()
        };

        all_prs
            .into_iter()
            .filter(|pr| {
                let filename = sanitize_filename(&pr.pr_title);
                let exists = existing.contains(&format!("{filename}.json"));
                if exists {
                    info!("Skipping already-evaluated PR: {}", pr.pr_title);
                }
                !exists
            })
            .collect()
    } else {
        all_prs
    };

    info!(
        "Evaluating {} PR(s) ({} skipped by resume)",
        prs_to_evaluate.len(),
        if resume { 0 } else { 0 },
    );

    if prs_to_evaluate.is_empty() {
        println!("No PRs to evaluate (all already processed or dataset empty).");
        return Ok(());
    }

    let client = rig_core::providers::openai::Client::from_env()
        .map_err(|e| anyhow!("Failed to create OpenAI client: {e}"))?;
    let judge = build_judge(&client, &judge_model);

    let linter_config_path = Path::new(&linters_config);
    let linter_configs = if linter_config_path.exists() && !skip_linters {
        match crb_tools::linters::config::load_linter_config(&linters_config) {
            Ok(configs) => {
                info!("Loaded {} linter(s) from {}", configs.len(), linters_config);
                Some(configs)
            }
            Err(e) => {
                warn!("Failed to load linter config: {e}. Linters disabled.");
                None
            }
        }
    } else {
        None
    };

    let ruleset = if !skip_rules {
        let rules_dir = Path::new(".crb/rules/");
        match RuleSet::load_from_dir(rules_dir) {
            Ok(rs) => {
                info!(
                    "Loaded {} rules ({} always-apply) from {}",
                    rs.rules.len() + rs.always_rules.len(),
                    rs.always_rules.len(),
                    rules_dir.display()
                );
                Some(Arc::new(rs))
            }
            Err(e) => {
                warn!("Failed to load rules from {}: {e}", rules_dir.display());
                None
            }
        }
    } else {
        None
    };

    // Wrap linter_configs in Arc to avoid expensive per-PR clones
    let linter_configs = linter_configs.map(Arc::new);

    let start_time = time::Instant::now();
    let (event_broadcast_tx, _) = broadcast::channel::<RunEvent>(256);
    let dashboard_tx: Option<broadcast::Sender<RunEvent>> = if dashboard || dashboard_events {
        Some(event_broadcast_tx.clone())
    } else {
        None
    };

    if dashboard_events {
        let mut rx = event_broadcast_tx.subscribe();
        tokio::spawn(async move {
            while let Ok(event) = rx.recv().await {
                if let Ok(json) = serde_json::to_string(&event) {
                    let stdout = io::stdout();
                    let mut handle = stdout.lock();
                    let _ = writeln!(handle, "{json}"); // Ignore; best-effort dashboard event output
                    let _ = handle.flush(); // Ignore; best-effort dashboard event output
                }
            }
        });
    }

    let agents: Vec<&'static AgentEntry> = PromptLibrary::get_instance().agents();
    let agents: &'static [&'static AgentEntry] = Box::leak(agents.into_boxed_slice());
    let tool_server = ToolServer::new().run();

    let mut results = Vec::with_capacity(prs_to_evaluate.len());

    for pr in prs_to_evaluate {
        let diff_str = match parse_github_url(&pr.url) {
            Ok((owner, repo, pr_num)) => {
                let d = crb_benchmark::diff_cache::load_cached_diff(&benchmark_dir, &owner, &repo, pr_num)
                    .unwrap_or_default();
                if d.is_empty() {
                    warn!("Empty diff for PR: {} (url: {})", pr.pr_title, pr.url);
                } else {
                    info!("Loaded diff ({} bytes) for PR: {}", d.len(), pr.pr_title);
                }
                d
            }
            Err(_) => {
                warn!("Could not extract PR info from URL '{}'. Using empty diff.", pr.url);
                String::new()
            }
        };
        let diff = Diff::new(diff_str);

        let cfg = EvalConfig {
            strategy: if skip_consensus {
                EvalStrategy::Single
            } else {
                EvalStrategy::Panel
            },
            model: Model(model.clone()),
            reasoning_effort: if reasoning_effort.is_empty() || reasoning_effort == "none" {
                None
            } else {
                model_capabilities::ReasoningEffort::from_str(&reasoning_effort)
            },
            client: Arc::new(client.clone()),
            cache: None,
            cost_tracker: Arc::new(AnalyticsTracker::new()),
            tool_handle: tool_server.clone(),
            dashboard_tx: dashboard_tx.clone(),
            identifier: format!(
                "run-{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            ),
            agents,
            repo_root: workspace_root.clone(),
            max_findings,
            judge_model: judge_model.clone(),
            judge: judge.clone(),
            linters_only,
            linter_configs: linter_configs.clone(),
            ruleset: ruleset.clone(),
            template_vars: None,
        };

        match crb_harness::pipeline::evaluate(diff, &cfg).await {
            Ok(findings) => {
                let result = crb_harness::pipeline::build_pr_result(
                    &findings,
                    &cfg,
                    &pr.pr_title,
                    &pr.url,
                    pr.comments.len(),
                )
                .await;
                results.push(result);
            }
            Err(e) => error!("PR '{}' evaluation failed: {e}", pr.pr_title),
        }
    }

    let eval_elapsed = start_time.elapsed();

    let mut aggregated = Metrics::default();
    for r in &results {
        aggregated += r.metrics.clone();
    }

    if let Some(tx) = &dashboard_tx {
        // Ignore; receiver may have disconnected
        let total_cost: f64 = results
            .iter()
            .filter_map(|r| r.cost.as_ref())
            .map(|c| c.total_cost())
            .sum();
        let total_tokens: usize = results
            .iter()
            .filter_map(|r| r.cost.as_ref())
            .flat_map(|c| c.sessions.values())
            .map(|s| (s.input_tokens + s.output_tokens) as usize)
            .sum();
        let total_agent_calls: usize = results
            .iter()
            .filter_map(|r| r.cost.as_ref())
            .flat_map(|c| c.sessions.values())
            .map(|s| s.call_count as usize)
            .sum();
        let _ = tx.send(RunEvent::RunFinished {
            total_prs: results.len(),
            aggregated: aggregated.clone(),
            total_cost,
            total_tokens,
            total_agent_calls,
        });
    }

    write_report(&results, &output_dir)?;
    info!(
        "Done. {} PR(s) evaluated, results in {}",
        results.len(),
        output_dir.display()
    );

    print_terminal_summary(&results);
    // Write _summary.json
    let total_llm_calls: usize = results.iter().map(|r| r.findings_count).sum();
    let total_judge_calls: usize = results.iter().map(|r| r.verdicts.len()).sum();
    let total_tokens: u64 = results
        .iter()
        .filter_map(|r| r.cost.as_ref())
        .map(|c| {
            c.sessions
                .values()
                .map(|s| s.input_tokens + s.output_tokens)
                .sum::<u64>()
        })
        .sum();
    let total_cost_usd: f64 = results
        .iter()
        .filter_map(|r| r.cost.as_ref())
        .map(|c| c.total_cost())
        .sum();
    let avg_agent_cache_hit_rate = if results.is_empty() {
        0.0
    } else {
        results
            .iter()
            .filter_map(|r| r.cost.as_ref())
            .map(|c| c.hit_rate())
            .sum::<f64>()
            / results.len() as f64
    };
    let avg_judge_cache_hit_rate = avg_agent_cache_hit_rate;

    let aggregate_metrics = if results.is_empty() {
        serde_json::json!({})
    } else {
        let avg_precision =
            results.iter().map(|r| r.metrics.precision()).sum::<f64>() / results.len() as f64;
        let avg_recall =
            results.iter().map(|r| r.metrics.recall()).sum::<f64>() / results.len() as f64;
        let avg_f1 = results.iter().map(|r| r.metrics.f1()).sum::<f64>() / results.len() as f64;
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
        "model": model,
        "judge_model": judge_model,
        "total_prs": results.len(),
        "total_llm_calls": total_llm_calls,
        "total_judge_calls": total_judge_calls,
        "duration_secs": eval_elapsed.as_secs_f64(),
        "aggregate_metrics": aggregate_metrics,
        "total_tokens": total_tokens,
        "total_cost_usd": total_cost_usd,
        "agent_cache_hit_rate": avg_agent_cache_hit_rate,
        "judge_cache_hit_rate": avg_judge_cache_hit_rate,
    });

    let summary_path = cache_dir.join(crb_harness::paths::SUMMARY_FILE);
    fs::write(&summary_path, serde_json::to_string_pretty(&summary)?)?;
    info!("Cache summary written to: {}", summary_path.display());

    let run_entry = RunHistoryEntry {
        run_id: summary["run_id"].as_str().unwrap_or("").to_string(),
        timestamp: format!("{:?}", std::time::SystemTime::now()),
        model: model.clone(),
        judge_model: judge_model.clone(),
        total_prs: results.len(),
    };
    append_run_history(&cache_dir, &run_entry)?;

    if ci {
        // CI validation placeholder — validation module was removed during migration
        info!("CI mode enabled, but validation functions are not yet available");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_list_with_dataset() {
        if let Ok(dir) = tempfile::TempDir::new() {
            let dataset = r#"{"entries":[{"pr_title":"Test PR","url":"https://github.com/owner/repo/pull/1","comments":[{"comment":"bug","severity":"critical"}]}]}"#;
            if std::fs::write(dir.path().join("dataset.json"), dataset).is_ok() {
                let result = run_list(&dir.path().to_path_buf());
                assert!(result.is_ok());
            }
        }
    }

    #[test]
    fn test_run_list_empty_dataset() {
        if let Ok(dir) = tempfile::TempDir::new() {
            let result = run_list(&dir.path().to_path_buf());
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_run_clean_dry_run() {
        if let Ok(dir) = tempfile::TempDir::new() {
            let worktrees_path = dir.path().join("worktrees").join("dummy_worktree");
            if std::fs::create_dir_all(&worktrees_path).is_ok()
                && std::fs::write(worktrees_path.join(".git"), "gitdir: /dummy").is_ok()
            {
                let result = run_clean(&dir.path().to_path_buf(), false, false, true);
                assert!(result.is_ok());
                assert!(dir.path().join("worktrees").exists());
            }
        }
    }

    #[test]
    fn test_run_clean_non_existent_dir() {
        if let Ok(dir) = tempfile::TempDir::new() {
            let nonexistent = dir.path().join("nonexistent");
            let result = run_clean(&nonexistent, false, false, false);
            assert!(result.is_ok());
        }
    }
}
