use std::env;

use anyhow::Result;
use clap::Parser;
use crb_shared::DEFAULT_MODEL;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Clone, Parser)]
pub enum Cli {
    /// Review a git diff (working tree or commit range)
    Review {
        /// Commit range to review (format: base..head, e.g. "HEAD~3..HEAD")
        #[arg(long)]
        pub commits: Option<String>,

        /// Review working tree changes (unstaged + staged)
        #[arg(long, conflicts_with = "commits")]
        pub working: bool,

        /// Path to the git repository
        #[arg(long, default_value = ".")]
        pub path: PathBuf,

        /// Model to use for agent reviews.
        #[arg(long, env = "MODEL", default_value = "default_model")]
        pub model: String,
    },
}

fn default_model() -> String {
    DEFAULT_MODEL.to_string()
}

#[tokio::main]
async fn main() -> Result<()> {
    match dotenvy::dotenv() {
        Ok(path) => eprintln!("[dotenv] Loaded .env from: {}", path.display()),
        Err(e) => eprintln!("[dotenv] No .env file loaded: {e}"),
    }

    if env::var("OPENAI_API_KEY").is_err() {
        if let Ok(key) = env::var("OPENROUTER_API_KEY") {
            env::set_var("OPENAI_API_KEY", key);
            eprintln!("[dotenv] OPENAI_API_KEY not found, falling back to OPENROUTER_API_KEY");
        }
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli {
        Cli::Review(args) => run_review(args).await,
    }
}

/// Run the `review` subcommand: get a git diff and print findings.
async fn run_review(args: crb_harness::config::ReviewArgs) -> Result<()> {
    let findings = crb_harness::review_diff(args).await?;
    println!("{}", serde_json::to_string_pretty(&findings)?);
    Ok(())
}
