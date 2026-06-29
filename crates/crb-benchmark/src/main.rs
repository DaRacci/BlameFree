use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

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
