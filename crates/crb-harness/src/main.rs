use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use crb_agents::{build_agent, Finding, AGENT_ROLES};
use crb_consensus::evaluate_pr_with_consensus;
use crb_judge::{build_judge, compute_metrics, run_judge};
use crb_reporting::{load_golden_datasets, write_report, GoldenCommentEntry, PrResult};
use rig_core::client::ProviderClient;
use rig_core::completion::Prompt;
use rig_core::tool::Tool;
use tracing::info;
use tracing::info_span;
use tracing_subscriber::EnvFilter;

mod config;
use config::CliArgs;

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
        println!("  Model:              {}", args.model);
        println!("  Judge model:        {}", args.judge_model);
        println!("  Concurrency:        {}", args.concurrency);
        println!("  Dataset:            {}", dataset_dir.display());
        println!("  Output:             {}", output_dir.display());
        println!("  Skip consensus:     {}", args.skip_consensus);
        println!("  Skip linters:       {}", args.skip_linters);
        println!("  Linters only:       {}", args.linters_only);
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

    info!(
        "Evaluating {} PR(s) ({} skipped)",
        prs_to_evaluate.len(),
        all_prs.len() - prs_to_evaluate.len()
    );

    if prs_to_evaluate.is_empty() {
        println!("No PRs to evaluate (all already processed or dataset empty).");
        return Ok(());
    }

    // ── Clients ───────────────────────────────────────────────────────────
    let client = rig_core::providers::openai::Client::from_env()
        .map_err(|e| anyhow::anyhow!("Failed to create OpenAI client: {e}"))?;

    let judge = build_judge(&client, &args.judge_model);

    // ── Linter config ─────────────────────────────────────────────────────
    let linter_config_path = std::path::Path::new(&args.linters_config);
    let linter_configs = if linter_config_path.exists() && !args.skip_linters {
        match crb_tools::load_linter_config(&args.linters_config) {
            Ok(configs) => {
                info!(
                    "Loaded {} linter(s) from {}",
                    configs.len(),
                    args.linters_config
                );
                Some(configs)
            }
            Err(e) => {
                tracing::warn!("Failed to load linter config: {e}. Linters disabled.");
                None
            }
        }
    } else {
        None
    };

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
        let linter_configs = linter_configs.clone();
        let skip_consensus = args.skip_consensus;
        let linters_only = args.linters_only;

        set.spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore closed");
            evaluate_pr_with_postprocessing(
                &pr,
                &client,
                &model,
                &judge,
                &repos_dir,
                linter_configs.as_ref(),
                skip_consensus,
                linters_only,
            )
            .await
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
    info!(
        "Done. {} PR(s) evaluated, results in {}",
        results.len(),
        output_dir.display()
    );

    Ok(())
}

/// Evaluate a single PR, optionally using consensus orchestration and linters.
///
/// When `--linters-only` is set, only static analysis linters are run (no LLM
/// agents). When `--skip-consensus` is set, the original single-agent evaluation
/// is used instead of the multi-agent consensus pipeline.
#[tracing::instrument(skip_all, fields(pr_title = %pr.pr_title))]
async fn evaluate_pr_with_postprocessing(
    pr: &GoldenCommentEntry,
    client: &rig_core::providers::openai::Client,
    model: &str,
    judge: &rig_core::agent::Agent<
        rig_core::providers::openai::responses_api::ResponsesCompletionModel,
    >,
    _repos_dir: &PathBuf,
    linter_configs: Option<&std::collections::HashMap<String, crb_tools::LinterConfig>>,
    skip_consensus: bool,
    linters_only: bool,
) -> Result<PrResult> {
    // ── Diff loading (placeholder — no real diffs in MVP) ─────────────────
    let diff = String::new();

    // ── Linters ───────────────────────────────────────────────────────────
    let mut linter_findings: Vec<Finding> = Vec::new();
    if let Some(configs) = linter_configs {
        let mut linter_set = tokio::task::JoinSet::new();
        for (_name, lconfig) in configs {
            let tool = crb_tools::create_linter_tool(lconfig);
            let args = crb_tools::LinterArgs {
                repo_path: _repos_dir.to_string_lossy().to_string(),
            };
            linter_set.spawn(async move {
                // Run the linter via the rig Tool interface
                let result = tool.call(args).await;
                result
            });
        }

        while let Some(res) = linter_set.join_next().await {
            match res {
                Ok(Ok(findings)) => linter_findings.extend(findings),
                Ok(Err(e)) => tracing::warn!("Linter failed: {e}"),
                Err(e) => tracing::warn!("Linter join error: {e}"),
            }
        }

        info!(
            "Found {} linter finding(s) for PR: {}",
            linter_findings.len(),
            pr.pr_title
        );
    }

    if linters_only {
        // Return a PrResult with only linter findings and no judge evaluation
        return Ok(PrResult {
            pr_title: pr.pr_title.clone(),
            url: pr.url.clone(),
            findings_count: linter_findings.len(),
            golden_count: pr.comments.len(),
            metrics: crb_judge::Metrics::default(),
            verdicts: vec![],
        });
    }

    // ── Agent evaluation ──────────────────────────────────────────────────
    let (all_findings, verdicts) = if skip_consensus {
        // Original single-agent evaluation
        evaluate_pr_single_agent(
            pr, client, model, judge, &diff, linter_findings,
        )
        .await?
    } else {
        // Multi-agent consensus evaluation
        evaluate_pr_consensus(
            pr, client, model, judge, &diff, linter_findings,
        )
        .await?
    };

    // ── Post-processing: aggregator dedup + auditor severity check ────────
    let processed_findings = post_process_findings(&all_findings);

    // ── Judge evaluation ──────────────────────────────────────────────────
    // (if not already done by consensus path)
    let final_verdicts = if skip_consensus {
        verdicts
    } else {
        // Already computed in consensus path
        verdicts
    };

    let metrics = compute_metrics(&final_verdicts, pr.comments.len());

    Ok(PrResult {
        pr_title: pr.pr_title.clone(),
        url: pr.url.clone(),
        findings_count: processed_findings.len(),
        golden_count: pr.comments.len(),
        metrics,
        verdicts: final_verdicts,
    })
}

/// Run the original single-agent evaluation with finding collection.
async fn evaluate_pr_single_agent(
    pr: &GoldenCommentEntry,
    client: &rig_core::providers::openai::Client,
    model: &str,
    judge: &rig_core::agent::Agent<
        rig_core::providers::openai::responses_api::ResponsesCompletionModel,
    >,
    diff: &str,
    linter_findings: Vec<Finding>,
) -> Result<(Vec<Finding>, Vec<crb_judge::JudgeVerdict>)> {
    let mut agent_set = tokio::task::JoinSet::new();
    for &role in AGENT_ROLES {
        let client = client.clone();
        let model = model.to_string();
        let role = role.to_string();
        let diff = diff.to_string();

        agent_set.spawn(async move {
            let span = info_span!("agent", role = %role);
            let _guard = span.enter();
            let agent = build_agent(&client, &model, &role);
            let result: Result<Vec<Finding>, _> = agent
                .prompt(&diff)
                .await
                .and_then(|_| Ok(Vec::new())); // placeholder extraction
            result
        });
    }

    let mut all_findings: Vec<Finding> = linter_findings;
    while let Some(res) = agent_set.join_next().await {
        match res {
            Ok(Ok(mut findings)) => all_findings.append(&mut findings),
            Ok(Err(e)) => tracing::warn!("Agent failed: {e}"),
            Err(e) => tracing::warn!("Agent join error: {e}"),
        }
    }

    // Judge evaluation: compare each finding against golden comments
    let mut verdicts = Vec::new();
    for finding in &all_findings {
        for gc in &pr.comments {
            match run_judge(judge, &gc.comment, &finding.message).await {
                Ok(verdict) => verdicts.push(verdict),
                Err(e) => tracing::warn!("Judge call failed: {e}"),
            }
        }
    }

    Ok((all_findings, verdicts))
}

/// Run the multi-agent consensus evaluation, merging linter findings.
async fn evaluate_pr_consensus(
    pr: &GoldenCommentEntry,
    client: &rig_core::providers::openai::Client,
    model: &str,
    judge: &rig_core::agent::Agent<
        rig_core::providers::openai::responses_api::ResponsesCompletionModel,
    >,
    diff: &str,
    linter_findings: Vec<Finding>,
) -> Result<(Vec<Finding>, Vec<crb_judge::JudgeVerdict>)> {
    let result = evaluate_pr_with_consensus(pr, client, model, judge).await?;

    info!(
        "Consensus pipeline: {} agent findings, {} linter findings, {} goldens",
        result.findings_count,
        linter_findings.len(),
        result.golden_count
    );

    // Merge linter findings into the result
    let all_findings: Vec<Finding> = linter_findings;

    Ok((all_findings, result.verdicts))
}

/// Post-process findings through aggregator dedup and auditor severity checks.
///
/// Converts `Finding` structs to the serde_json `Map` format used by the
/// aggregator and auditor crates, then runs the full pipeline:
/// 1. `semantic_dedup` — merge semantically identical findings
/// 2. `apply_severity_auditor` — downgrade inflated severity labels
fn post_process_findings(findings: &[Finding]) -> Vec<Finding> {
    if findings.is_empty() {
        return findings.to_vec();
    }

    // Convert Finding → serde_json::Map<String, Value>
    let maps: Vec<serde_json::Map<String, serde_json::Value>> = findings
        .iter()
        .map(|f| {
            let mut map = serde_json::Map::new();
            map.insert(
                "text".to_string(),
                serde_json::Value::String(f.message.clone()),
            );
            map.insert(
                "path".to_string(),
                match &f.file {
                    Some(p) => serde_json::Value::String(p.clone()),
                    None => serde_json::Value::Null,
                },
            );
            map.insert(
                "line".to_string(),
                match f.line {
                    Some(l) => serde_json::Value::Number(serde_json::Number::from(l)),
                    None => serde_json::Value::Null,
                },
            );
            map.insert(
                "severity".to_string(),
                serde_json::Value::String(f.severity.clone()),
            );
            map.insert(
                "evidence".to_string(),
                serde_json::Value::String(String::new()),
            );
            map.insert("num_agents".to_string(), serde_json::Value::Number(1.into()));
            map
        })
        .collect();

    // Step 1: semantic dedup
    let deduped = crb_aggregator::semantic_dedup(maps);

    // Step 2: severity auditor
    let audited = crb_auditor::apply_severity_auditor(deduped);

    // Convert back to Finding
    audited
        .iter()
        .map(|map| Finding {
            file: map
                .get("path")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string()),
            line: map.get("line").and_then(|v| v.as_u64()).map(|l| l as u32),
            message: map
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            severity: map
                .get("severity")
                .and_then(|v| v.as_str())
                .unwrap_or("medium")
                .to_string(),
            rule_code: None,
        })
        .collect()
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
