use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use rig_core::client::ProviderClient;
use rig_core::completion::Prompt;
use tracing::info;
use tracing::info_span;
use tracing_subscriber::EnvFilter;

mod agents;
mod config;
mod judge;
mod reporting;

use config::CliArgs;
use reporting::{load_golden_datasets, write_report, GoldenCommentEntry, PrResult};
use agents::{build_agent, Finding, AGENT_ROLES};
use judge::{build_judge, compute_metrics, run_judge};

/// Main entry point for the review benchmark harness.
#[tokio::main]
async fn main() -> Result<()> {
    // ── Tracing ───────────────────────────────────────────────────────────
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // ── CLI ───────────────────────────────────────────────────────────────
    let args = CliArgs::parse();
    let output_dir = PathBuf::from(&args.output_dir);
    let dataset_dir = PathBuf::from(&args.dataset_dir);
    let repos_dir = PathBuf::from(&args.repos_dir);

    let _span = info_span!("harness", model = %args.model, concurrency = %args.concurrency).entered();

    // ── Load datasets ─────────────────────────────────────────────────────
    info!("Loading golden datasets from: {}", dataset_dir.display());
    let all_prs = load_golden_datasets(&dataset_dir)?;
    info!("Loaded {} PR entries total", all_prs.len());

    // ── Dry run ───────────────────────────────────────────────────────────
    if args.dry_run {
        println!("[DRY RUN] Would evaluate {} PR(s)", all_prs.len());
        println!("  Model:       {}", args.model);
        println!("  Judge model: {}", args.judge_model);
        println!("  Concurrency: {}", args.concurrency);
        println!("  Dataset:     {}", dataset_dir.display());
        println!("  Output:      {}", output_dir.display());
        return Ok(());
    }

    // ── Resume support ────────────────────────────────────────────────────
    let prs_to_evaluate: Vec<&GoldenCommentEntry> = if args.resume {
        let existing: std::collections::HashSet<String> = if output_dir.exists() {
            std::fs::read_dir(&output_dir)?
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map_or(false, |ext| ext == "json"))
                .filter_map(|e| e.file_name().into_string().ok())
                .collect()
        } else {
            std::collections::HashSet::new()
        };

        all_prs
            .iter()
            .filter(|pr| {
                let filename = utils::sanitize_filename(&pr.pr_title);
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

    info!("Evaluating {} PR(s) ({} skipped)", prs_to_evaluate.len(), all_prs.len() - prs_to_evaluate.len());

    if prs_to_evaluate.is_empty() {
        println!("No PRs to evaluate (all already processed or dataset empty).");
        return Ok(());
    }

    // ── Clients ───────────────────────────────────────────────────────────
    // NOTE: Provider is configured via OPENAI_API_KEY / OPENAI_BASE_URL env vars.
    // For OpenRouter, set OPENAI_BASE_URL=https://openrouter.ai/api/v1
    let client = rig_core::providers::openai::Client::from_env()
        .map_err(|e| anyhow::anyhow!("Failed to create OpenAI client: {e}"))?;

    let judge = build_judge(&client, &args.judge_model);

    // ── Concurrency ───────────────────────────────────────────────────────
    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(args.concurrency));
    let mut set = tokio::task::JoinSet::new();

    for pr in prs_to_evaluate {
        let client = client.clone();
        let sem = sem.clone();
        let judge = judge.clone();
        let pr = pr.clone();
        let model = args.model.clone();
        let repos_dir = repos_dir.clone();

        set.spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore closed");
            evaluate_pr(&pr, &client, &model, &judge, &repos_dir).await
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

    // ── Report ────────────────────────────────────────────────────────────
    write_report(&results, &output_dir)?;
    info!("Done. {} PR(s) evaluated, results in {}", results.len(), output_dir.display());

    Ok(())
}

/// Evaluate a single PR by running all 4 agent roles concurrently, then
/// judging each finding against the golden comments.
#[tracing::instrument(skip_all, fields(pr_title = %pr.pr_title))]
async fn evaluate_pr(
    pr: &GoldenCommentEntry,
    client: &rig_core::providers::openai::Client,
    model: &str,
    judge: &rig_core::agent::Agent<
        rig_core::providers::openai::responses_api::ResponsesCompletionModel,
    >,
    _repos_dir: &PathBuf,
) -> Result<PrResult> {
    // ── Diff loading (placeholder — no real diffs in MVP) ─────────────────
    // TODO: load diff from repos_dir / pr url mapping. For MVP we pass an
    // empty diff so agents can still be exercised structurally.
    let _diff = String::new();

    // ── Agent evaluation (concurrent) ─────────────────────────────────────
    let mut agent_set = tokio::task::JoinSet::new();
    for &role in AGENT_ROLES {
        let client = client.clone();
        let model = model.to_string();
        let role = role.to_string();
        let diff = _diff.clone();

        agent_set.spawn(async move {
            let span = info_span!("agent", role = %role);
            let _guard = span.enter();
            let agent = build_agent(&client, &model, &role);
            // For MVP, we prompt the agent with the diff and attempt to
            // extract findings.  When no real diff is provided the agent
            // should return an empty list.
            let result: Result<Vec<Finding>, _> = agent
                .prompt(&diff)
                .await
                .and_then(|_| Ok(Vec::new())); // placeholder extraction
            result
        });
    }

    let mut all_findings: Vec<Finding> = Vec::new();
    while let Some(res) = agent_set.join_next().await {
        match res {
            Ok(Ok(mut findings)) => all_findings.append(&mut findings),
            Ok(Err(e)) => tracing::warn!("Agent failed: {e}"),
            Err(e) => tracing::warn!("Agent join error: {e}"),
        }
    }

    // ── Judge evaluation ──────────────────────────────────────────────────
    let mut verdicts = Vec::new();
    for finding in &all_findings {
        for gc in &pr.comments {
            let span = info_span!("judge", severity = %finding.severity);
            let _guard = span.enter();

            match run_judge(judge, &gc.comment, &finding.message).await {
                Ok(verdict) => verdicts.push(verdict),
                Err(e) => tracing::warn!("Judge call failed: {e}"),
            }
        }
    }

    let metrics = compute_metrics(&verdicts, pr.comments.len());

    Ok(PrResult {
        pr_title: pr.pr_title.clone(),
        url: pr.url.clone(),
        findings_count: all_findings.len(),
        golden_count: pr.comments.len(),
        metrics,
        verdicts,
    })
}

/// Internal helper module for reporting utilities that are not public API.
mod utils {
    /// Sanitize a string for use as a filename.
    pub fn sanitize_filename(name: &str) -> String {
        name.chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            })
            .collect()
    }
}
