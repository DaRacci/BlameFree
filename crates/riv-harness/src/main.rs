use std::env;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use riv_agents::prompts::PromptLibrary;
use riv_harness::config::ReviewArgs;
use riv_harness::paths::OUTPUT_DIR_DEFAULT;
use riv_harness::review;
use riv_shared::diff::Diff;
use riv_stor::store::sqlite::SqliteStore;
use riv_stor::traits::Store;
use riv_types::review::{Review, ReviewMetadata, ReviewStatus};
use tracing::info;
use tracing_subscriber::util::SubscriberInitExt;

#[derive(Debug, Clone, Parser)]
pub enum Cli {
    /// Review a git diff (working tree or commit range).
    ///
    /// Resolves agent roles through `PromptLibrary` and dispatches via
    /// the typed pipeline (pipeline::evaluate) with full EvalConfig.
    Review(ReviewArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    riv_shared::init_dotenv();
    riv_shared::init_logging(None).try_init()?;
    let cli = Cli::parse();

    PromptLibrary::new().map_err(|e| anyhow!("Failed to initialize prompt library: {e}"))?;

    match cli {
        Cli::Review(args) => run_review(args).await,
    }
}

/// Run the `review` subcommand.
async fn run_review(args: ReviewArgs) -> Result<()> {
    let started_at = Instant::now();

    // Obtain the diff from git
    let output = if let Some(ref commits) = args.commits {
        Command::new("git")
            .args(["diff", commits])
            .current_dir(&args.path)
            .output()
            .context("Failed to run git diff for commit range")?
    } else if args.working {
        // Working tree changes (staged + unstaged)
        Command::new("git")
            .args(["diff", "HEAD"])
            .current_dir(&args.path)
            .output()
            .context("Failed to run git diff for working tree")?
    } else {
        return Err(anyhow!("Either --commits or --working must be specified"));
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("git diff failed: {stderr}"));
    }

    let diff_str =
        String::from_utf8(output.stdout).context("git diff output is not valid UTF-8")?;

    if diff_str.is_empty() {
        eprintln!("No changes to review (empty diff).");
        return Ok(());
    }

    eprintln!(
        "Loaded diff ({} bytes) from {}",
        diff_str.len(),
        if let Some(ref c) = args.commits {
            format!("commit range {c}")
        } else {
            "working tree".to_string()
        }
    );

    let review_id = riv_harness::cli_review_id();
    let config = review::build_review_config(&args)?;
    let findings = review::review_diff(Diff::new(diff_str), &config).await?;
    let analytics = config.cost_tracker.to_snapshot().await;
    let duration = started_at.elapsed();

    // Persist the review to the same DB the webui uses, so CLI runs surface
    // identically. Only the id prefix differs (`riv-cli`).
    if let Err(error) = persist_cli_review(&review_id, &analytics, duration, &args).await {
        eprintln!("Failed to persist review to DB: {error}");
    }

    // Print findings to stderr (stdout reserved for structured output)
    if findings.is_empty() {
        eprintln!("No findings from review.");
    } else {
        eprintln!("\n=== Review Findings ({} total) ===\n", findings.len());
        for (i, finding) in findings.iter().enumerate() {
            let file_str = finding.file.as_deref().unwrap_or("<unknown>");
            let line_str = finding.line.map(|l| format!(":{}", l)).unwrap_or_default();
            eprintln!("{}. [{}] {}{}", i + 1, finding.severity, file_str, line_str,);
            eprintln!("   {}", finding.message);
            if let Some(ref evidence) = finding.evidence {
                eprintln!("   Evidence: {evidence}");
            }
            eprintln!();
        }
    }

    Ok(())
}

/// Persist a CLI review to the sqlite store next to the output dir, mirroring
/// the webui Review shape (analytics + duration via the compressed JSON blob).
async fn persist_cli_review(
    review_id: &mti::prelude::MagicTypeId,
    analytics: &riv_types::cost::AnalyticsSnapshot,
    duration: Duration,
    _args: &ReviewArgs,
) -> Result<()> {
    let output_dir = env::var("OUTPUT_DIR").unwrap_or_else(|_| OUTPUT_DIR_DEFAULT.to_string());
    let store_path = PathBuf::from(output_dir).join("riv-stor.db");
    let store = SqliteStore::new(&store_path.to_string_lossy())
        .await
        .context("failed to init store")?;

    let review = Review {
        id: review_id.clone(),
        agent_sessions: Vec::new(),
        analytics: Some(analytics.clone()),
        duration: Some(duration),
        status: ReviewStatus::Completed,
        metadata: ReviewMetadata::Plain,
    };
    store
        .save::<Review>(&review)
        .await
        .context("failed to save review")?;
    info!("Persisted review {} to {}", review_id, store_path.display());
    Ok(())
}
