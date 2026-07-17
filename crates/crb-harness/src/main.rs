use std::env;
use std::process::Command;

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use crb_agents::prompts::PromptLibrary;
use crb_harness::config::ReviewArgs;
use crb_harness::review;
use crb_shared::diff::Diff;
use tracing_subscriber::EnvFilter;

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
    crb_shared::init_dotload();
    crb_shared::init_logging(None);
    let cli = Cli::parse();

    PromptLibrary::new().map_err(|e| anyhow!("Failed to initialize prompt library: {e}"))?;

    match cli {
        Cli::Review(args) => run_review(args).await,
    }
}

/// Run the `review` subcommand.
async fn run_review(args: ReviewArgs) -> Result<()> {
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

    let diff = Diff::new(diff_str);
    let config = review::build_review_config(&args)?;

    let findings = review::review_pr(diff, &config).await?;

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
