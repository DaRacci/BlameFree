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

        /// Directory to clone/fetch repos into.
        #[arg(long, default_value = "repos")]
        repos_dir: PathBuf,
    },
    /// Extract diffs from scaffolded repos.
    FetchDiffs {
        /// Directory containing scaffolded repos.
        #[arg(long, default_value = "repos")]
        repos_dir: PathBuf,

        /// Directory to write extracted diffs.
        #[arg(long, default_value = "diffs")]
        output_dir: PathBuf,
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
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Scaffold { dataset_dir, repos_dir } => {
            scaffold::run(&dataset_dir, &repos_dir)?;
        }
        Commands::FetchDiffs { repos_dir, output_dir } => {
            diffs::run(&repos_dir, &output_dir)?;
        }
        Commands::Validate { dataset_dir } => {
            validate::run_validate(&dataset_dir)?;
        }
        Commands::List { dataset_dir } => {
            run_list(&dataset_dir)?;
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
