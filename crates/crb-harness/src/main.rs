use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use clap::Parser;
use crb_agents::prompts::PromptLibrary;
use crb_agents::{build_agent, Finding, AGENT_ROLES};
use crb_consensus::evaluate_pr_with_consensus;
use crb_judge::{build_judge, compute_metrics, run_judge};
use crb_reporting::{load_golden_datasets, write_report, GoldenCommentEntry, PrResult};
use crb_rules::RuleSet;
use rig_core::client::ProviderClient;
use rig_core::completion::Prompt;
use rig_core::tool::Tool;
use tracing::info;
use tracing::info_span;
use tracing_subscriber::EnvFilter;

mod config;
use config::CliArgs;

mod validation;

/// Main entry point for the review benchmark harness.
#[tokio::main]
async fn main() -> Result<()> {
    // Load .env from CWD (and parent directories)
    dotenvy::dotenv().ok();

    // ── Tracing ───────────────────────────────────────────────────────────
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // ── CLI ───────────────────────────────────────────────────────────────
    let args = CliArgs::parse();
    let output_dir = PathBuf::from(&args.output_dir);
    let dataset_dir = PathBuf::from(&args.dataset_dir);
    let repos_dir = PathBuf::from(&args.repos_dir);

    // Determine workspace root (where baselines/ lives)
    let workspace_root = std::env::current_dir()
        .context("Failed to determine current working directory")?;

    // ── --validate flag ────────────────────────────────────────────────────
    if args.validate {
        return run_validate(&workspace_root, "5.14").await;
    }

    let _span = info_span!("harness", model = %args.model, concurrency = %args.concurrency).entered();

    // ── --cached-diffs flag ────────────────────────────────────────────────
    if args.cached_diffs {
        info!("--cached-diffs: skipping scaffold step, using pre-extracted diffs");
    }

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

    // ── Rule loading ──────────────────────────────────────────────────────
    let ruleset = if !args.skip_rules {
        let rules_dir = std::path::Path::new(&args.rules_dir);
        match RuleSet::load_from_dir(rules_dir) {
            Ok(rs) => {
                info!(
                    "Loaded {} rules ({} always-apply) from {}",
                    rs.rules.len() + rs.always_rules.len(),
                    rs.always_rules.len(),
                    rules_dir.display()
                );
                Some(rs)
            }
            Err(e) => {
                tracing::warn!("Failed to load rules from {}: {e}", rules_dir.display());
                None
            }
        }
    } else {
        None
    };

    // ── Prompt library ───────────────────────────────────────────────────
    let prompt_lib = std::sync::Arc::new({
        let mut lib = PromptLibrary::new();
        let prompts_dir = std::path::Path::new(&args.prompts_dir);
        // Always try to load from the configured prompts directory.
        // If it's "prompts/builtin" and doesn't exist, that's fine — we fall
        // back to built-in defaults.  For custom directories, load or warn.
        if prompts_dir.exists() {
            match lib.load_from_dir(prompts_dir) {
                Ok(()) => {
                    info!("Loaded prompts from: {}", prompts_dir.display());
                }
                Err(e) => {
                    tracing::warn!("Failed to load prompts from {}: {e}", prompts_dir.display());
                }
            }
        } else if args.prompts_dir.to_string_lossy() != "prompts/builtin" {
            tracing::warn!(
                "Custom prompts directory '{}' not found — using built-in defaults",
                prompts_dir.display()
            );
        }
        lib
    });

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
        let ruleset = ruleset.clone();
        let prompt_lib = prompt_lib.clone();
        let roles = args.roles.clone();
        let max_findings = args.max_findings;

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
                ruleset.as_ref(),
                prompt_lib.as_ref(),
                &roles,
                max_findings,
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

    // ── --ci flag: validate and exit with proper code ─────────────────────
    if args.ci {
        let metrics: Vec<crb_judge::Metrics> = results.iter().map(|r| r.metrics.clone()).collect();
        let (avg_precision, avg_recall, avg_f1) =
            validation::compute_average_metrics(&metrics);
        let baseline = validation::load_baseline(&workspace_root, "5.14")?;
        let val_result = validation::validate_against_baseline(
            &baseline,
            results.len(),
            avg_precision,
            avg_recall,
            avg_f1,
        );
        validation::print_validation_summary(&baseline, &val_result, avg_precision, avg_recall, avg_f1);

        if val_result.in_threshold {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "CI validation failed: metrics exceed baseline thresholds"
            ))
        }
    } else {
        Ok(())
    }
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
    ruleset: Option<&RuleSet>,
    prompt_lib: &PromptLibrary,
    roles: &str,
    max_findings: usize,
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

    // ── Compute rules preamble from changed files ────────────────────────
    // For the MVP we don't have real changed file paths, so pass empty slice.
    let rules_preamble = ruleset.map(|rs| rs.format_preamble(&[]));

    // ── Agent evaluation ──────────────────────────────────────────────────
    let (all_findings, verdicts) = if skip_consensus {
        // Original single-agent evaluation
        evaluate_pr_single_agent(
            pr, client, model, judge, &diff, linter_findings, rules_preamble.as_deref(), prompt_lib,
        )
        .await?
    } else {
        // Multi-agent consensus evaluation
        evaluate_pr_consensus(
            pr, client, model, judge, &diff, linter_findings, rules_preamble.as_deref(), prompt_lib,
            roles, max_findings,
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

/// Call an async function with exponential backoff retry.
///
/// The closure is called on each attempt, returning a `Result<T, E>`.
/// On error, if retries remain, the function waits with exponential backoff
/// (`base_delay_ms * 2^attempt`) before retrying.
async fn with_retry<F, Fut, T, E>(f: F, max_retries: usize, base_delay_ms: u64) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut attempt = 0usize;
    loop {
        match f().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                attempt += 1;
                if attempt >= max_retries {
                    return Err(e);
                }
                let delay = Duration::from_millis(base_delay_ms * 2u64.pow(attempt as u32));
                tracing::warn!(
                    "Attempt {}/{} failed: {}. Retrying in {}ms",
                    attempt,
                    max_retries,
                    e,
                    delay.as_millis()
                );
                tokio::time::sleep(delay).await;
            }
        }
    }
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
    rules_preamble: Option<&str>,
    prompt_lib: &PromptLibrary,
) -> Result<(Vec<Finding>, Vec<crb_judge::JudgeVerdict>)> {
    let mut agent_set = tokio::task::JoinSet::new();
    let prompt_lib = prompt_lib.clone();
    for &role in AGENT_ROLES {
        let client = client.clone();
        let model = model.to_string();
        let role = role.to_string();
        let diff = diff.to_string();
        let preamble = rules_preamble.map(String::from);
        let p_lib = prompt_lib.clone();

        agent_set.spawn(async move {
            let span = info_span!("agent", role = %role);
            let _guard = span.enter();
            let agent = build_agent(&client, &model, &role, preamble.as_deref(), Some(&p_lib), None);
            let result: Result<Vec<Finding>, String> = with_retry(
                || async {
                    agent
                        .prompt(&diff)
                        .await
                        .map_err(|e| e.to_string())
                        .and_then(|_| Ok(Vec::new())) // placeholder extraction
                },
                3,    // max_retries
                1000, // base_delay_ms
            )
            .await;
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
            match with_retry(
                || run_judge(judge, &gc.comment, &finding.message),
                3,    // max_retries
                1000, // base_delay_ms
            )
            .await
            {
                Ok(verdict) => verdicts.push(verdict),
                Err(e) => tracing::warn!("Judge call failed after retries: {e}"),
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
    _diff: &str,
    linter_findings: Vec<Finding>,
    rules_preamble: Option<&str>,
    prompt_lib: &PromptLibrary,
    roles: &str,
    max_findings: usize,
) -> Result<(Vec<Finding>, Vec<crb_judge::JudgeVerdict>)> {
    // Parse comma-separated roles
    let parsed_roles: Vec<&str> = roles.split(',').map(|r| r.trim()).filter(|r| !r.is_empty()).collect();
    let result = evaluate_pr_with_consensus(
        pr, client, model, judge, rules_preamble, Some(prompt_lib), None,
        &parsed_roles, max_findings,
    )
    .await?;

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

    // Convert Finding → serde_json::Map<String, Value> using helper
    let maps: Vec<serde_json::Map<String, serde_json::Value>> = findings
        .iter()
        .map(crb_agents::finding_to_map)
        .collect();

    // Step 1: semantic dedup
    let deduped = crb_aggregator::semantic_dedup(maps);

    // Step 2: severity auditor
    let audited = crb_auditor::apply_severity_auditor(deduped);

    // Convert back to Finding using helper
    audited
        .iter()
        .filter_map(crb_agents::map_to_finding)
        .collect()
}

/// Run the validation pipeline: load baseline, read results from output dir,
/// compute average metrics, compare against thresholds, and exit with
/// the appropriate code (0 = pass, 1 = fail).
async fn run_validate(workspace_root: &std::path::Path, version: &str) -> Result<()> {
    info!("Running validation against baseline v{version}");

    let baseline = validation::load_baseline(workspace_root, version)?;
    info!("Loaded baseline for version: {}", baseline.version);

    // Read results from the output directory
    let output_dir = workspace_root.join("output");
    let results_dir = if output_dir.exists() {
        output_dir
    } else {
        anyhow::bail!(
            "Output directory not found: {}. Run the harness first.",
            output_dir.display()
        );
    };

    // Collect all PR result JSON files from the output directory
    let mut loaded_results: Vec<crb_judge::Metrics> = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(&results_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "json"))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in &entries {
        let path = entry.path();
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read result: {}", path.display()))?;
        // Try to deserialize as Metrics directly
        match serde_json::from_str::<crb_judge::Metrics>(&content) {
            Ok(metrics) => loaded_results.push(metrics),
            Err(e) => {
                tracing::warn!(
                    "Skipping {}: could not parse as Metrics: {e}",
                    path.display()
                );
            }
        }
    }

    if loaded_results.is_empty() {
        anyhow::bail!(
            "No valid PR result files found in {}",
            results_dir.display()
        );
    }

    let total_prs = loaded_results.len();
    let (avg_precision, avg_recall, avg_f1) =
        validation::compute_average_metrics(&loaded_results);
    let val_result = validation::validate_against_baseline(
        &baseline,
        total_prs,
        avg_precision,
        avg_recall,
        avg_f1,
    );
    validation::print_validation_summary(&baseline, &val_result, avg_precision, avg_recall, avg_f1);

    if val_result.in_threshold {
        info!("Validation PASSED — all metrics within baseline thresholds");
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "Validation FAILED — metrics exceed baseline thresholds"
        ))
    }
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
