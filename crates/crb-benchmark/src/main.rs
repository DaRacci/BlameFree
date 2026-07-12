use std::collections::HashSet;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use std::time;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use clap::{Parser, Subcommand};
use crb_agents::prompts::PromptLibrary;
use crb_harness::validation;
use crb_judge::build_judge;
use crb_reporting::{GoldenCommentEntry, load_golden_datasets, write_report};
use crb_rules::RuleSet;
use crb_shared::benchmark_pipeline;
use crb_shared::metrics::MetricsOutput;
use crb_shared::sanitize_filename;
use crb_types::AggregateMetrics;
use crb_types::RunEvent;
use rig_core::client::ProviderClient;
use tokio::sync::broadcast;
use tracing::error;
use tracing::warn;
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
        #[arg(long, env = "ROLES", default_value = "SA,CL,AR,SEC")]
        roles: String,

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
    // Load .env from CWD (and parent directories)
    match dotenvy::dotenv() {
        Ok(path) => eprintln!("[dotenv] Loaded .env from: {}", path.display()),
        Err(e) => eprintln!("[dotenv] No .env file loaded: {e}"),
    }

    // Fallback: if OPENAI_API_KEY is not set but OPENROUTER_API_KEY is, use that
    if env::var("OPENAI_API_KEY").is_err() {
        if env::var("OPENROUTER_API_KEY").is_ok() {
            // Dotenv: set it via the process env since dotenvy already loaded
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

            // Pre-warm model capabilities cache (uses blocking HTTP, must be
            // called outside the async runtime)
            crb_harness::model_capabilities::warm_model_cache_blocking();

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
                        warn!("Failed to remove worktree at {}", wt_path.display());
                    }
                }
            }

            // Prune orphaned worktree metadata
            let _ = std::process::Command::new("git")
                .args(["worktree", "prune"])
                .status(); // Ignore — best-effort cleanup

            // Remove the worktrees directory itself
            std::fs::remove_dir_all(&worktrees_dir)?;
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
                std::fs::remove_dir_all(&diffs_dir)?;
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
                std::fs::remove_dir_all(&outputs_dir)?;
                println!("Removed outputs directory: {}", outputs_dir.display());
            }
        } else {
            println!("No outputs directory found at {}", outputs_dir.display());
        }

        // Also check for top-level "output/" as used by the benchmark run command
        let output_dir = benchmark_dir.join("output");
        if output_dir.exists() && output_dir != outputs_dir {
            if dry_run {
                println!(
                    "[DRY RUN] Would remove output directory: {}",
                    output_dir.display()
                );
            } else {
                std::fs::remove_dir_all(&output_dir)?;
                println!("Removed output directory: {}", output_dir.display());
            }
        }
    }

    Ok(())
}

/// Show cache statistics.
fn run_cache_stats(cache_dir: &PathBuf, json: bool) -> Result<()> {
    let stats = crb_harness::LlmCache::stats(cache_dir).map_err(|e| anyhow!("{}", e))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&stats)?);
    } else {
        println!("Cache Statistics");
        println!("  PR directories:   {}", stats.pr_count);
        println!("  Total entries:    {}", stats.total_entries);
        println!("  Total size:       {} bytes", stats.total_size_bytes);
        println!();
        for pr in &stats.per_pr {
            println!(
                "  {}: {} entries, {} bytes",
                pr.pr_key, pr.entry_count, pr.total_size_bytes
            );
        }
    }
    Ok(())
}

/// Print the result of a cache operation.  When `json` is true, the result
/// is printed as pretty-printed JSON; otherwise a human-readable message
/// is printed (with an optional `"[DRY RUN] "` prefix).
fn print_cache_output(json: bool, dry_run: bool, message: &str) {
    if json {
        // JSON is already printed by the caller via serde_json::to_string_pretty
        return;
    }
    if dry_run {
        print!("[DRY RUN] ");
    }
    println!("{message}");
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
    let result = crb_harness::LlmCache::prune(cache_dir, max_age, max_size, max_prs, dry_run)
        .map_err(|e| anyhow!("{}", e))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        print_cache_output(
            json,
            dry_run,
            &format!(
                "Prune: {} entries removed from {} PRs, {} bytes freed ({} PRs kept)",
                result.entries_removed, result.prs_removed, result.bytes_freed, result.prs_kept
            ),
        );
    }
    Ok(())
}

/// Scrub cache for stale entries, orphans, and corrupted indices.
fn run_cache_scrub(cache_dir: &PathBuf, dry_run: bool, repair: bool, json: bool) -> Result<()> {
    let result =
        crb_harness::LlmCache::scrub(cache_dir, dry_run, repair).map_err(|e| anyhow!("{}", e))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        print_cache_output(
            json,
            dry_run,
            &format!(
                "Scrub: scanned {} PR dirs, {} stale entries, {} orphans, {} corrupted indices",
                result.pr_dirs_scanned,
                result.stale_entries_found,
                result.orphan_files_found,
                result.corrupted_indices_found
            ),
        );
        if repair {
            println!(
                "  Repaired: {} indices rebuilt, {} stale removed, {} orphans removed",
                result.indices_rebuilt, result.stale_entries_removed, result.orphan_files_removed
            );
        }
    }
    Ok(())
}

/// Backup cache to a tar.gz archive.
fn run_cache_backup(cache_dir: &PathBuf, output: Option<PathBuf>) -> Result<()> {
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let output_path = output.unwrap_or_else(|| {
        let mut p = cache_dir.clone();
        p.push(format!("cache_backup_{}.tar.gz", ts));
        p
    });
    crb_harness::LlmCache::backup(cache_dir, &output_path).map_err(|e| anyhow!("{}", e))?;
    println!("Backup created: {}", output_path.display());
    Ok(())
}

/// Restore cache from a tar.gz backup.
fn run_cache_restore(backup_file: &PathBuf, cache_dir: &PathBuf) -> Result<()> {
    crb_harness::LlmCache::restore(cache_dir, backup_file).map_err(|e| anyhow!("{}", e))?;
    println!(
        "Restored from {} to {}",
        backup_file.display(),
        cache_dir.display()
    );
    Ok(())
}

/// Rebuild cache indices from raw data.
fn run_cache_rebuild(cache_dir: &PathBuf, dry_run: bool) -> Result<()> {
    crb_harness::LlmCache::rebuild(cache_dir, dry_run).map_err(|e| anyhow!("{}", e))?;
    if dry_run {
        print!("[DRY RUN] ");
    }
    println!("Cache rebuild completed");
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

    if auto_backup {
        use std::time::SystemTime;
        use std::time::UNIX_EPOCH;
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let backup_path = PathBuf::from(format!("cache_backup_{}.tar.gz", ts));
        match crb_harness::LlmCache::backup(&cache_dir, &backup_path) {
            Ok(()) => info!("Auto-backup created at {}", backup_path.display()),
            Err(e) => warn!("Auto-backup failed: {e}"),
        }
    }

    let workspace_root =
        env::current_dir().context("Failed to determine current working directory")?;
    if validate {
        return crb_harness::run_validate(&workspace_root, "5.14").await;
    }

    let _span = info_span!("harness", model = %model, concurrency = %concurrency).entered();

    info!("Loading golden datasets from: {}", dataset_dir.display());
    let all_prs = load_golden_datasets(&dataset_dir)?;
    info!("Loaded {} PR entries total", all_prs.len());

    let all_prs = if let Some(ref filter) = pr_filter {
        benchmark_pipeline::filter_prs_by_pattern(all_prs, filter)
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
            std::fs::read_dir(&output_dir)?
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

    let linter_config_path = std::path::Path::new(&linters_config);
    let linter_configs = if linter_config_path.exists() && !skip_linters {
        match crb_tools::load_linter_config(&linters_config) {
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
        let rules_dir = std::path::Path::new(".crb/rules/");
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

    let prompt_lib = Arc::new(PromptLibrary::get_instance());

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
            use std::io::Write;
            while let Ok(event) = rx.recv().await {
                if let Ok(json) = serde_json::to_string(&event) {
                    let stdout = std::io::stdout();
                    let mut handle = stdout.lock();
                    let _ = writeln!(handle, "{json}"); // Ignore — best-effort dashboard event output
                    let _ = handle.flush(); // Ignore — best-effort dashboard event output
                }
            }
        });
    }

    let pipeline_cfg = benchmark_pipeline::PipelineConfig::new(concurrency);
    let eval_cache_dir = cache_dir.clone();
    let eval_model = model.clone();
    let eval_judge_model = judge_model.clone();
    let eval_dashboard_tx = dashboard_tx.clone();
    let eval_fn = std::sync::Arc::new(move |pr: GoldenCommentEntry| {
        let client = client.clone();
        let judge = judge.clone();
        let model = eval_model.clone();
        let judge_model = eval_judge_model.clone();
        let benchmark_dir = benchmark_dir.clone();
        let linter_configs = linter_configs.clone();
        let ruleset = ruleset.clone();
        let prompt_lib = prompt_lib.clone();
        let roles = roles.clone();
        let cache_dir = eval_cache_dir.clone();
        let dashboard_tx = eval_dashboard_tx.clone();
        let reasoning_effort = reasoning_effort.clone();
        async move {
            let cost_tracker = Arc::new(crb_harness::CostTracker::new());
            let cfg = crb_harness::EvalConfig {
                strategy: if skip_consensus {
                    crb_harness::EvalStrategy::SingleAgent
                } else {
                    crb_harness::EvalStrategy::Consensus
                },
                model: model.clone(),
                judge_model: judge_model.clone(),
                reasoning_effort: if reasoning_effort.is_empty() || reasoning_effort == "none" {
                    None
                } else {
                    Some(reasoning_effort.clone())
                },
                client: Arc::new(client.clone()),
                judge: judge.clone(),
                cache: None,
                prompt_lib: prompt_lib.clone(),
                cost_tracker: cost_tracker.clone(),
                dashboard_tx: dashboard_tx.clone(),
                roles: roles.clone(),
                max_findings,
                linters_only,
                linter_configs: linter_configs.as_ref().map(|a| (**a).clone()),
                ruleset: ruleset.as_ref().map(|a| (**a).clone()),
                cache_dir: Some(cache_dir.clone()),
                benchmark_dir: Some(benchmark_dir.clone()),
                workdir: None,
                template_vars: None,
            };
            let diff = crb_harness::load_pr_diff(&pr, &benchmark_dir).await?;
            crb_harness::evaluate_pr(&pr, &diff, &cfg).await
        }
    });

    let (results, eval_elapsed) =
        benchmark_pipeline::run_concurrent_eval(prs_to_evaluate, &pipeline_cfg, eval_fn).await;

    // Accumulate aggregate metrics from all completed results
    let mut agg = benchmark_pipeline::AggregateResults::new();
    for r in &results {
        agg.add(r);
    }

    if let Some(tx) = &dashboard_tx {
        let _ = tx.send(RunEvent::RunFinished {
            // Ignore — receiver may have disconnected
            total_prs: results.len(),
            aggregated: AggregateMetrics {
                true_positives: agg.total_tp,
                false_positives: agg.total_fp,
                false_negatives: agg.total_fn,
                precision: agg.precision(),
                recall: agg.recall(),
                f1: agg.f1(),
            },
            total_cost: agg.total_cost,
            total_tokens: agg.total_tokens,
            total_agent_calls: agg.total_agent_calls,
        });
    }

    write_report(&results, &output_dir)?;
    info!(
        "Done. {} PR(s) evaluated, results in {}",
        results.len(),
        output_dir.display()
    );

    crb_harness::print_terminal_summary(&results);
    crb_harness::write_summary(&cache_dir, &model, &judge_model, &results, eval_elapsed)?;

    if ci {
        let metrics: Vec<crb_judge::Metrics> = results.iter().map(|r| r.metrics.clone()).collect();
        let (avg_precision, avg_recall, avg_f1) = validation::compute_average_metrics(&metrics);
        let baseline = validation::load_baseline(&workspace_root, "5.14")?;
        let val_result = validation::validate_against_baseline(
            &baseline,
            results.len(),
            avg_precision,
            avg_recall,
            avg_f1,
        );
        validation::print_validation_summary(
            &baseline,
            &val_result,
            avg_precision,
            avg_recall,
            avg_f1,
        );

        if val_result.in_threshold {
            Ok(())
        } else {
            Err(anyhow!(
                "CI validation failed: metrics exceed baseline thresholds"
            ))
        }
    } else {
        Ok(())
    }
}
