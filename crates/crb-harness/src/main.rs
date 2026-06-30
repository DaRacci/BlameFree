use anyhow::Result;
use clap::Parser;
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
                "[dotenv] OPENAI_API_KEY not found - falling back to OPENROUTER_API_KEY"
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
    }
}

/// Run the `review` subcommand: get a git diff and print findings.
async fn run_review(args: crb_harness::config::ReviewArgs) -> Result<()> {
    let findings = crb_harness::review_diff(args).await?;
    println!("{}", serde_json::to_string_pretty(&findings)?);
    Ok(())
}
