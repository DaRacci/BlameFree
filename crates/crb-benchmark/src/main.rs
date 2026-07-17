use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, fs, io};
use std::{process, time};

use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use crb_agents::agent::AgentEntry;
use crb_agents::prompts::PromptLibrary;
use crb_benchmark::judge::build_judge;
use crb_benchmark::{
    BENCHMARK_DIFFS_SUBDIR, BENCHMARK_DIR, BENCHMARK_WORKTREE_SUBDIR, DATASETS_DIR, pr,
};
use crb_harness::eval::EvalConfig;
use crb_harness::eval::EvalStrategy;
use crb_harness::paths::OUTPUT_DIR_DEFAULT;
use crb_harness::{model_capabilities, pipeline};
use crb_reporting::cost::AnalyticsTracker;
use crb_reporting::golden::load_golden_datasets;
use crb_reporting::history::append_run_history;
use crb_reporting::write_report;
use crb_rules::RULES_DIR;
use crb_rules::RuleSet;
use crb_shared::diff::Diff;
use crb_shared::url::parse_github_url;
use crb_shared::{DEFAULT_MODEL, OUTPUT_CACHE_DIR, sanitize_filename};
use crb_tools::linters::LINTER_CONFIG_FILE;
use crb_types::RunEvent;
use crb_types::benchmark::metrics::{Metrics, MetricsProvider};
use crb_types::capabilities::ReasoningEffort;
use crb_types::wrappers::Model;
use rig_core::client::ProviderClient;
use rig_core::tool::server::ToolServer;
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
        #[arg(long, default_value = DATASETS_DIR)]
        dataset_dir: PathBuf,

        /// Benchmark directory (contains base-repos/, diffs/, worktrees/).
        #[arg(long, default_value = BENCHMARK_DIR)]
        benchmark_dir: PathBuf,
    },

    /// Extract diffs from scaffolded repos into persistent worktrees.
    FetchDiffs {
        /// Benchmark directory (contains base-repos/, diffs/, worktrees/).
        #[arg(long, default_value = BENCHMARK_DIR)]
        benchmark_dir: PathBuf,
    },

    /// Validate golden datasets for integrity.
    Validate {
        /// Directory containing golden comment datasets.
        #[arg(long, default_value = DATASETS_DIR)]
        dataset_dir: PathBuf,
    },

    /// Show all PRs in a dataset with URLs.
    List {
        /// Directory containing golden comment datasets.
        #[arg(long, default_value = DATASETS_DIR)]
        dataset_dir: PathBuf,
    },

    /// Remove worktrees, outputs, and optionally diffs from a benchmark directory.
    Clean {
        /// Benchmark directory (contains base-repos/, diffs/, worktrees/).
        #[arg(long, default_value = BENCHMARK_DIR)]
        benchmark_dir: PathBuf,

        /// Also remove output directories.
        #[arg(long, default_value_t = false)]
        outputs: bool,

        /// Dry run: only print what would be removed.
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },

    /// Run the full benchmark evaluation pipeline over a dataset.
    Run {
        /// Model for agent reviews.
        #[arg(long, env = "MODEL", default_value = DEFAULT_MODEL)]
        model: String,

        /// Model for the LLM judge.
        #[arg(
            long,
            env = "JUDGE_MODEL",
            default_value = DEFAULT_MODEL
        )]
        judge_model: String,

        /// load config and datasets, print stats, then exit.
        #[arg(long, default_value_t = false)]
        dry_run: bool,

        /// Path to linters.toml.
        #[arg(long, env = "LINTERS_CONFIG", default_value = LINTER_CONFIG_FILE)]
        linters_config: String,

        // TODO: Const available agents
        roles: Option<String>,

        /// Max findings per agent.
        #[arg(long, env = "MAX_FINDINGS", default_value_t = 20)]
        max_findings: usize,

        /// PR filter pattern.
        #[arg(long)]
        pr_filter: Option<String>,

        /// Reasoning effort level
        #[arg(long, default_value = ReasoningEffort::Medium)]
        reasoning_effort: ReasoningEffort,
    },
}

fn main() -> Result<()> {
    crb_shared::init_load();
    crb_shared::init_logging(None);

    PromptLibrary::new().map_err(|e| anyhow!("Failed to initialize prompt library: {e}"))?;
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
            outputs,
            dry_run,
        } => {
            run_clean(&benchmark_dir, outputs, dry_run)?;
        }
        Commands::Run {
            model,
            judge_model,
            dry_run,
            linters_config,
            roles,
            max_findings,
            pr_filter,
            reasoning_effort,
        } => {
            let rt = tokio::runtime::Runtime::new()?;

            // Pre-warm model capabilities cache (uses blocking HTTP, must be called outside the async runtime)
            model_capabilities::warm_model_cache_blocking();

            let roles = roles.unwrap_or_else(|| {
                crb_agents::prompts::PromptLibrary::get_instance()
                    .abbreviations()
                    .join(",")
            });

            rt.block_on(run_benchmark(
                model,
                judge_model,
                dry_run,
                skip_rules,
                roles,
                max_findings,
                pr_filter,
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
        info!("{}/{}   {}", repo_name, pr_number, entry.pr_title);
        repos.insert(repo_name.to_string());
    }

    info!(
        "\nTotal: {} PRs across {} repos",
        entries.len(),
        repos.len()
    );
    Ok(())
}

/// Remove worktrees, outputs, and optionally diffs from a benchmark directory.
fn run_clean(benchmark_dir: &PathBuf, outputs: bool, dry_run: bool) -> Result<()> {
    fn remove_worktrees(benchmark_dir: &Path, dry_run: bool) -> Result<()> {
        let worktrees_dir = benchmark_dir.join(BENCHMARK_WORKTREE_SUBDIR);

        if !worktrees_dir.exists() {
            info!(
                "No worktrees directory found at {}",
                worktrees_dir.display()
            );
            return Ok(());
        }

        if dry_run {
            info!(
                "[DRY RUN] Would remove worktrees from {}",
                worktrees_dir.display()
            );
        }

        for entry in fs::read_dir(&worktrees_dir)? {
            let entry = entry?;

            let wt_path = entry.path();
            if !wt_path.is_dir() || !wt_path.join(".git").exists() {
                continue;
            }

            let status = process::Command::new("git")
                .args(["worktree", "remove", "--force"])
                .arg(&wt_path)
                .status()?;
            if status.success() {
                info!("Removed worktree: {}", wt_path.display());
            } else {
                warn!("Failed to remove worktree at {}", wt_path.display());
            }
        }

        let git_status = process::Command::new("git")
            .args(["worktree", "prune"])
            .status();
        match git_status {
            Ok(status) if status.success() => {
                info!("Pruned stale worktrees");
            }
            Ok(status) => {
                warn!("Failed to prune stale worktrees, exit code: {}", status);
            }
            Err(e) => {
                warn!("Failed to execute git worktree prune: {}", e);
            }
        }

        fs::remove_dir_all(&worktrees_dir)?;
        info!("Removed worktrees directory: {}", worktrees_dir.display());

        Ok(())
    }

    fn remove_diffs(benchmark_dir: &Path, dry_run: bool) -> Result<()> {
        let diffs_dir = benchmark_dir.join(BENCHMARK_DIFFS_SUBDIR);
        if !diffs_dir.exists() {
            info!("No diffs directory found at {}", diffs_dir.display());
            return Ok(());
        }

        if dry_run {
            info!(
                "[DRY RUN] Would remove diffs directory: {}",
                diffs_dir.display()
            );
            return Ok(());
        }

        fs::remove_dir_all(&diffs_dir)?;
        info!("Removed diffs directory: {}", diffs_dir.display());

        Ok(())
    }

    fn remove_outputs(benchmark_dir: &Path, dry_run: bool) -> Result<()> {
        let outputs_dir = benchmark_dir.join(OUTPUT_DIR_DEFAULT);
        if !outputs_dir.exists() {
            info!("No outputs directory found at {}", outputs_dir.display());
            return Ok(());
        }

        if dry_run {
            info!(
                "[DRY RUN] Would remove outputs directory: {}",
                outputs_dir.display()
            );
            return Ok(());
        }

        fs::remove_dir_all(&outputs_dir)?;
        info!("Removed outputs directory: {}", outputs_dir.display());

        Ok(())
    }

    remove_worktrees(&worktrees_dir, dry_run)?;
    remove_diffs(benchmark_dir, dry_run)?;

    if outputs {
        remove_outputs(benchmark_dir, dry_run)?;
    }

    Ok(())
}

/// Get all relevant paths.
///
/// Attempts to read from environment variables, falling back to default constants if not set.
///
/// Returns a tuple of (benchmark_dir, dataset_dir, output_dir, cache_dir, rules_dir).
pub fn get_paths() -> Result<(PathBuf, PathBuf, PathBuf, PathBuf, PathBuf, PathBuf)> {
    let cwd = env::current_dir().context("Failed to determine current working directory")?;

    let benchmark_dir = env::var("BENCHMARK_DIR").unwrap_or_else(|_| BENCHMARK_DIR.to_string());
    let dataset_dir = env::var("DATASET_DIR").unwrap_or_else(|_| DATASETS_DIR.to_string());
    let output_dir = env::var("OUTPUT_DIR").unwrap_or_else(|_| OUTPUT_DIR_DEFAULT.to_string());
    let cache_dir = env::var("CACHE_DIR")
        .unwrap_or_else(|_| format!("{}/{}", output_dir, OUTPUT_CACHE_DIR.to_string()));
    let rules_dir = env::var("RULES_DIR").unwrap_or_else(|_| RULES_DIR.to_string());
    let linter_path = env::var("LINTERS_CONFIG").unwrap_or_else(|_| LINTER_CONFIG_FILE.to_string());

    Ok((
        PathBuf::from(benchmark_dir),
        PathBuf::from(dataset_dir),
        PathBuf::from(output_dir),
        PathBuf::from(cache_dir),
        PathBuf::from(rules_dir),
        PathBuf::from(linter_path),
    ))
}

/// Run the full benchmark evaluation pipeline over a dataset.
#[allow(clippy::too_many_arguments)]
async fn run_benchmark(
    model: String,
    judge_model: String,
    dry_run: bool,
    skip_rules: bool,
    roles: String,
    max_findings: usize,
    pr_filter: Option<String>,
    reasoning_effort: ReasoningEffort,
) -> Result<()> {
    let (output_dir, dataset_dir, benchmark_dir, cache_dir, rules_dir, linter_path) = get_paths()?;

    info!("Loading golden datasets from: {}", dataset_dir.display());
    let all_prs = load_golden_datasets(&dataset_dir)?;
    info!("Loaded {} PR entries total", all_prs.len());

    let all_prs = if let Some(ref filter) = pr_filter {
        pr::filter_prs_by_pattern(all_prs, &[filter.as_str()])
    } else {
        all_prs
    };

    info!("After --pr-filter: {} PR(s) to evaluate", all_prs.len());

    if dry_run {
        info!("[DRY RUN] Would evaluate {} PR(s)", all_prs.len());
        info!("  Model:              {}", model);
        info!("  Judge model:        {}", judge_model);
        info!("  Dataset:            {}", dataset_dir.display());
        info!("  Output:             {}", output_dir.display());
        return Ok(());
    }

    let prs_to_evaluate = if resume {
        let existing: HashSet<_> = if output_dir.exists() {
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
        info!("No PRs to evaluate (all already processed or dataset empty).");
        return Ok(());
    }

    let client = crb_shared::build_client()?;
    let judge = build_judge(&client, &judge_model);

    let linter_configs = if linter_path.exists() && !skip_linters {
        crb_tools::linters::config::load_linter_config(&linters_config)?
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

    let start_time = time::Instant::now();
    let linter_configs = linter_configs.map(Arc::new);
    let agents: Vec<&'static AgentEntry> = PromptLibrary::get_instance().agents();
    let agents: &'static [&'static AgentEntry] = Box::leak(agents.into_boxed_slice());
    let tool_server = ToolServer::new().run();

    let mut results = Vec::with_capacity(prs_to_evaluate.len());

    for pr in prs_to_evaluate {
        let diff_str = match parse_github_url(&pr.url) {
            Ok((owner, repo, pr_num)) => {
                let d = crb_benchmark::diff_cache::load_cached_diff(
                    &benchmark_dir,
                    &owner,
                    &repo,
                    pr_num,
                )
                .unwrap_or_default();
                if d.is_empty() {
                    warn!("Empty diff for PR: {} (url: {})", pr.pr_title, pr.url);
                } else {
                    info!("Loaded diff ({} bytes) for PR: {}", d.len(), pr.pr_title);
                }
                d
            }
            Err(_) => {
                warn!(
                    "Could not extract PR info from URL '{}'. Using empty diff.",
                    pr.url
                );
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
            reasoning_effort: Some(reasoning_effort),
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
            judge_model: Model(judge_model),
            judge: judge.clone(),
            ruleset: ruleset.clone(),
            template_vars: None,
        };

        match pipeline::evaluate(diff, &cfg).await {
            Ok(findings) => {
                let result = pipeline::build_pr_result(
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
        aggregated += r.metrics;
    }

    if let Some(tx) = &dashboard_tx {
        // Ignore; receiver may have disconnected
        let total_cost: f64 = results.iter().map(|res| res.total_cost()).sum();
        let total_tokens: usize = results
            .iter()
            .flat_map(|res| res.agent_sessions)
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

    let total_llm_calls: usize = results.iter().map(|r| r.findings_count).sum();
    let total_judge_calls: usize = results.iter().map(|r| r.verdicts.len()).sum();
    let total_tokens: u64 = results
        .iter()
        .map(|res| {
            res.sessions
                .values()
                .map(|s| res.input_tokens + res.output_tokens)
                .sum::<u64>()
        })
        .sum();
    let total_cost_usd: f64 = results.iter().map(|res| res.total_cost()).sum();
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

    //TODO
    // let summary_path = cache_dir.join(crb_harness::paths::SUMMARY_FILE);
    // fs::write(&summary_path, serde_json::to_string_pretty(&summary)?)?;
    // info!("Cache summary written to: {}", summary_path.display());
    // let run_entry = crb_webui_shared::runs::RunMeta {
    //     id: summary["run_id"].as_str().unwrap_or("").to_string(),
    //     name: summary["run_id"].as_str().unwrap_or("").to_string(),
    //     pr_count: results.len(),
    //     total_cost: Some(total_cost_usd),
    //     total_tokens: total_tokens as usize,
    //     duration_secs: Some(eval_elapsed.as_secs_f64()),
    //     model: Some(model.clone()),
    //     status: crb_webui_shared::runs::RunStatus::Completed,
    // };
    // append_run_history(&cache_dir, &run_entry)?;

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
