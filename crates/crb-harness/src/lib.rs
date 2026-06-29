//! Code Review Benchmark Harness library.
//!
//! Provides the public API for PR review (`review_pr`, `review_diff`) as well
//! as the internal orchestration functions used by the `benchmark` subcommand.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use crb_agents::prompts::PromptLibrary;
use crb_agents::{build_agent, Finding, AGENT_ROLES};
use crb_consensus::evaluate_pr_with_consensus;
use crb_consensus::CacheBackend;
use crb_dashboard::DashboardEvent;
use crb_judge::{compute_metrics, run_judge};
use crb_reporting::{GoldenCommentEntry, PrResult};
use crb_rules::RuleSet;
use regex::Regex;
use rig_core::agent::Agent;
use rig_core::completion::Prompt;
use rig_core::providers::openai::responses_api::ResponsesCompletionModel;
use rig_core::tool::Tool;
use tokio::sync::broadcast;
use tracing::{info, info_span};

// ── Internal modules (re-declared so main.rs can reach them via crb_harness::*) ──

pub mod cache;
pub mod config;
pub mod cost;
pub mod validation;

pub use cache::LlmCache;
pub use config::BenchmarkArgs;
pub use config::ReviewArgs;
pub use cost::CostTracker;

// =========================================================================
// Public API types
// =========================================================================

/// Describes which kind of diff to review.
pub enum ReviewMode {
    /// Review a commit range `base..head`.
    Commits { base: String, head: String },
    /// Review the current working tree (unstaged + staged).
    Working,
}

/// Parameters for a full PR review (library API entry point).
pub struct ReviewParams {
    pub diff: String,
    pub model: String,
    pub judge_model: String,
    pub pr_title: String,
    pub roles: Vec<String>,
    pub max_findings: usize,
    pub replay_dir: Option<PathBuf>,
    pub cache_dir: Option<PathBuf>,
}

// =========================================================================
// Public API
// =========================================================================

/// Entry point for reviewing a PR given its diff as a string.
///
/// This sets up the basic infrastructure (caching, cost tracking, agent
/// orchestration) and returns the list of agent findings.  For the MVP
/// it returns an empty vector; the full pipeline lives in the benchmark
/// subcommand for now.
pub fn review_pr(_params: ReviewParams) -> Result<Vec<Finding>> {
    // MVP placeholder – full implementation will follow in a later iteration.
    Ok(Vec::new())
}

/// Review a diff by running `git diff` in the given `path`.
///
/// - `ReviewMode::Commits { base, head }` → `git diff base..head`
/// - `ReviewMode::Working`                → `git diff` (unstaged + staged)
///
/// Returns a vector of agent findings parsed from the LLM response.
pub fn review_diff(args: crate::config::ReviewArgs) -> Result<Vec<Finding>> {
    let diff = match args.commits {
        Some(ref range) => {
            // Format: "base..head"
            let output = std::process::Command::new("git")
                .arg("diff")
                .arg(range)
                .current_dir(&args.path)
                .output()
                .map_err(|e| anyhow::anyhow!("Failed to run git diff: {e}"))?;
            String::from_utf8_lossy(&output.stdout).to_string()
        }
        None => {
            // Working tree: unstaged + staged
            let output = std::process::Command::new("git")
                .arg("diff")
                .current_dir(&args.path)
                .output()
                .map_err(|e| anyhow::anyhow!("Failed to run git diff: {e}"))?;
            String::from_utf8_lossy(&output.stdout).to_string()
        }
    };

    if diff.is_empty() {
        info!("No diff found — returning empty findings");
        return Ok(Vec::new());
    }

    info!(
        "Loaded diff ({} bytes) from {}",
        diff.len(),
        args.path.display()
    );

    // MVP: return diff info; real agent processing will come in a later
    // iteration.  For now, callers can use the benchmark subcommand for
    // full agent orchestration.
    Ok(Vec::new())
}

// =========================================================================
// Moved from main.rs – public helpers
// =========================================================================

/// Extract owner, repo name, and PR number from a GitHub PR URL.
///
/// Expects URLs of the form `https://github.com/{owner}/{repo}/pull/{num}`.
/// Returns `None` if the URL doesn't match the expected pattern.
pub fn extract_pr_info(url: &str) -> Option<(String, String, u32)> {
    let re = Regex::new(r"^https://github\.com/([^/]+)/([^/]+)/pull/(\d+)$").ok()?;
    let caps = re.captures(url)?;
    let owner = caps.get(1)?.as_str().to_string();
    let repo = caps.get(2)?.as_str().to_string();
    let pr_num: u32 = caps.get(3)?.as_str().parse().ok()?;
    Some((owner, repo, pr_num))
}

/// Load the diff for a PR from pre-extracted cached diff files.
///
/// Cached diffs live at `{benchmark_dir}/diffs/{owner}_{repo}_{pr_num}.diff`.
pub fn load_cached_diff(
    benchmark_dir: &Path,
    owner: &str,
    repo: &str,
    pr_num: u32,
) -> Option<String> {
    let diffs_dir = benchmark_dir.join("diffs");
    let diff_path = diffs_dir.join(format!("{}_{}_{}.diff", owner, repo, pr_num));
    match std::fs::read_to_string(&diff_path) {
        Ok(content) => {
            info!(
                "Loaded cached diff ({} bytes) from {}",
                content.len(),
                diff_path.display()
            );
            Some(content)
        }
        Err(e) => {
            tracing::warn!(
                "Cached diff not found at {}: {}. Using empty diff.",
                diff_path.display(),
                e
            );
            None
        }
    }
}

/// Parse an agent's LLM response string into a `Vec<Finding>`.
///
/// Attempts three strategies in order:
/// 1. Direct JSON array deserialization via `serde_json::from_str`.
/// 2. JSON extraction from markdown fenced code blocks (```json ... ```).
/// 3. Find any JSON array in the response.
///
/// Before deserializing, field names are normalised (path→file,
/// description→message, text→message, category→rule_code, component→file)
/// and severity values are case-normalised ("high"→"High", "MEDIUM"→"Medium").
///
/// If all strategies fail, returns an empty `Vec` with a warning.
pub fn parse_agent_findings(response: &str) -> Result<Vec<Finding>, String> {
    // Log raw response first for debugging
    let preview_len = std::cmp::min(500, response.len());
    tracing::info!(
        "Agent raw response (first 500 chars): {}",
        &response[..preview_len]
    );

    // Helper: normalise field names and severity in a JSON value array.
    fn normalise_findings(raw: &str) -> Option<Vec<Finding>> {
        let mut values: Vec<serde_json::Value> = serde_json::from_str(raw).ok()?;
        for v in &mut values {
            if let Some(obj) = v.as_object_mut() {
                // Normalise field aliases
                if let Some(val) = obj.remove("path") {
                    if !obj.contains_key("file") {
                        obj.insert("file".to_string(), val);
                    }
                }
                if let Some(val) = obj.remove("description") {
                    if !obj.contains_key("message") {
                        obj.insert("message".to_string(), val);
                    }
                }
                if let Some(val) = obj.remove("text") {
                    if !obj.contains_key("message") {
                        obj.insert("message".to_string(), val);
                    }
                }
                if let Some(val) = obj.remove("category") {
                    if !obj.contains_key("rule_code") {
                        obj.insert("rule_code".to_string(), val);
                    }
                }
                if let Some(val) = obj.remove("component") {
                    if !obj.contains_key("file") && !obj.contains_key("path") {
                        obj.insert("file".to_string(), val);
                    }
                }

                // Normalise severity case: "high" → "High", "MEDIUM" → "Medium"
                if let Some(sev) = obj.get("severity").and_then(|s| s.as_str()) {
                    let normalised = match sev.to_lowercase().as_str() {
                        "high" => "High",
                        "medium" | "med" => "Medium",
                        "low" => "Low",
                        "critical" | "crit" => "Critical",
                        "info" | "informational" => "Info",
                        _ => sev, // keep as-is
                    };
                    obj.insert(
                        "severity".to_string(),
                        serde_json::Value::String(normalised.to_string()),
                    );
                }
            }
        }
        serde_json::from_value(serde_json::Value::Array(values)).ok()
    }

    // Strategy 1: Try direct JSON array parse with normalisation
    if let Some(findings) = normalise_findings(response) {
        info!(
            "Parsed {} finding(s) directly from agent JSON response",
            findings.len()
        );
        return Ok(findings);
    }

    // Strategy 2: Extract JSON from markdown code blocks
    let re = Regex::new(r"(?s)```(?:json)?\s*\n(.*?)\n\s*```").unwrap();
    if let Some(caps) = re.captures(response) {
        let inner = caps.get(1).unwrap().as_str().trim();
        if let Some(findings) = normalise_findings(inner) {
            info!(
                "Parsed {} finding(s) from markdown code block in agent response",
                findings.len()
            );
            return Ok(findings);
        }
    }

    // Strategy 3: Find any JSON array in the response
    let array_re = Regex::new(r"\[[\s\S]*\]").unwrap();
    if let Some(m) = array_re.find(response) {
        if let Some(findings) = normalise_findings(m.as_str()) {
            info!("Parsed {} finding(s) from embedded JSON array", findings.len());
            return Ok(findings);
        }
    }

    // All strategies failed — warn and return empty
    let truncated = if response.len() > 200 {
        format!("{}...", &response[..200])
    } else {
        response.to_string()
    };
    tracing::warn!(
        "Failed to parse agent response as Finding array. \
         Response (truncated): {}",
        truncated
    );
    Ok(Vec::new())
}

// =========================================================================
// Internal orchestration functions (used by the benchmark subcommand)
// =========================================================================

/// Call an async function with exponential backoff retry.
#[doc(hidden)]
pub async fn with_retry<F, Fut, T, E>(f: F, max_retries: usize, base_delay_ms: u64) -> Result<T, E>
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
#[doc(hidden)]
pub async fn evaluate_pr_single_agent(
    pr: &GoldenCommentEntry,
    client: &rig_core::providers::openai::Client,
    model: &str,
    judge: &Agent<ResponsesCompletionModel>,
    diff: &str,
    linter_findings: Vec<Finding>,
    rules_preamble: Option<&str>,
    prompt_lib: &PromptLibrary,
    cache: Option<Arc<crate::cache::LlmCache>>,
    cost_tracker: Arc<crate::cost::CostTracker>,
    dashboard_tx: Option<&broadcast::Sender<DashboardEvent>>,
) -> Result<(Vec<Finding>, Vec<crb_judge::JudgeVerdict>)> {
    // ── Pre-compute content-addressed cache key components ──────────────
    let diff_hash = crate::cache::LlmCache::sha256(diff);
    let rules_hash = crate::cache::LlmCache::sha256(rules_preamble.unwrap_or(""));
    let judge_prompt_hash = crate::cache::LlmCache::sha256(crb_judge::JUDGE_PROMPT);
    let judge_model = ""; // We don't have judge_model here; it's baked into the judge Agent

    let mut agent_set = tokio::task::JoinSet::new();
    let prompt_lib = prompt_lib.clone();
    for &role in AGENT_ROLES {
        let client = client.clone();
        let model = model.to_string();
        let role = role.to_string();
        let diff = diff.to_string();
        let diff_hash = diff_hash.clone();
        let rules_hash = rules_hash.clone();
        let preamble = rules_preamble.map(String::from);
        let p_lib = prompt_lib.clone();
        let cache_arc: Option<Arc<dyn CacheBackend>> =
            cache.clone().map(|c| c as Arc<dyn CacheBackend>);
        let ct = cost_tracker.clone();
        let tx = dashboard_tx.map(|t| t.clone());

        agent_set.spawn(async move {
            let span = info_span!("agent", role = %role);
            let _guard = span.enter();

            // Compute agent cache key
            let prompt_hash = crate::cache::LlmCache::sha256(p_lib.get(&role));
            let agent_cache_key = crate::cache::LlmCache::compute_agent_key(
                &prompt_hash,
                &diff_hash,
                &model,
                &role,
                &rules_hash,
            );

            // Estimate tokens for this call
            let tokens_in = crate::cost::estimate_tokens(&diff);

            // Check cache first
            if let Some(ref c) = cache_arc {
                if let Some(cached_response) = c.lookup_agent_by_key(&agent_cache_key) {
                    tracing::info!(
                        "CACHE HIT for agent role={} (key={})",
                        role,
                        &agent_cache_key[..12]
                    );
                    let tokens_out = crate::cost::estimate_tokens(&cached_response);
                    ct.record_agent(tokens_in, tokens_out, true);
                    // Send chunk + finished for cached response
                    if let Some(ref tx) = tx {
                        let _ = tx.send(DashboardEvent::AgentChunk {
                            role: role.clone(),
                            chunk: cached_response.clone(),
                        });
                        let result = parse_agent_findings(&cached_response);
                        let findings_count = result.as_ref().map(|v| v.len()).unwrap_or(0);
                        let _ = tx.send(DashboardEvent::AgentFinished {
                            role,
                            findings: findings_count,
                            success: result.is_ok(),
                        });
                    }
                    let result = parse_agent_findings(&cached_response);
                    return result;
                }
            }
            tracing::info!(
                "CACHE MISS for agent role={} (key={})",
                role,
                &agent_cache_key[..12]
            );

            // Cache miss — make API call
            let tool_preamble = crb_tools::tool_prompt_section(
                &role,
                &crb_tools::budget::ToolCallBudget::default(),
                &[],
            );
            let agent = build_agent(
                &client,
                &model,
                &role,
                preamble.as_deref(),
                Some(&p_lib),
                None,
                Some(&tool_preamble),
            );
            let result: Result<Vec<Finding>, String> = with_retry(
                || async {
                    let response = agent
                        .prompt(&diff)
                        .await
                        .map_err(|e| e.to_string())?;

                    let tokens_out = crate::cost::estimate_tokens(&response);
                    ct.record_agent(tokens_in, tokens_out, false);

                    // Send chunk for live response
                    if let Some(ref tx) = tx {
                        let _ = tx.send(DashboardEvent::AgentChunk {
                            role: role.clone(),
                            chunk: response.clone(),
                        });
                    }

                    // Cache the prompt+response with content-addressed key
                    if let Some(ref c) = cache_arc {
                        c.save_agent_with_key(&agent_cache_key, &role, &diff, &response);
                    }

                    let findings = parse_agent_findings(&response);
                    // Send finished event
                    if let Some(ref tx) = tx {
                        let findings_count = findings.as_ref().map(|v| v.len()).unwrap_or(0);
                        let _ = tx.send(DashboardEvent::AgentFinished {
                            role: role.clone(),
                            findings: findings_count,
                            success: findings.is_ok(),
                        });
                    }
                    findings
                },
                3,    // max_retries
                1000, // base_delay_ms
            )
            .await;
            // If the whole retry chain failed, send failed event
            if result.is_err() {
                if let Some(ref tx) = tx {
                    let _ = tx.send(DashboardEvent::AgentFinished {
                        role: role.clone(),
                        findings: 0,
                        success: false,
                    });
                }
            }
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
    let jaccard_threshold = 0.12;
    for finding in &all_findings {
        for gc in &pr.comments {
            // Step 1: Try Jaccard heuristic (no API call)
            if let Some(score) =
                crb_judge::jaccard_match(&finding.message, &gc.comment, jaccard_threshold)
            {
                tracing::info!(
                    "Jaccard match: finding='{}' golden='{}' score={:.2}",
                    &finding.message[..std::cmp::min(60, finding.message.len())],
                    &gc.comment[..std::cmp::min(60, gc.comment.len())],
                    score
                );
                verdicts.push(crb_judge::JudgeVerdict {
                    reasoning: format!(
                        "Matched by {:.0}% word overlap (Jaccard heuristic)",
                        score * 100.0
                    ),
                    match_: true,
                    confidence: score,
                });
                continue;
            }

            // Step 2: File/line pre-filter (if available)
            if let Some(golden_file) = &gc.file {
                if let Some(finding_file) = &finding.file {
                    if golden_file != finding_file {
                        continue; // file mismatch — skip
                    }
                }
            }

            // Compute judge cache key
            let judge_key = crate::cache::LlmCache::compute_judge_key(
                &judge_prompt_hash,
                &finding.message,
                &gc.comment,
                judge_model,
            );

            // Estimate tokens for judge call
            let judge_prompt = format!(
                "{}\n\nFinding: {}\nGolden: {}",
                crb_judge::JUDGE_PROMPT,
                finding.message,
                gc.comment
            );
            let tokens_in = crate::cost::estimate_tokens(&judge_prompt);

            // Check judge cache first
            if let Some(ref c) = cache {
                if let Some(cached_verdict) = c.lookup_judge_by_key(&judge_key) {
                    tracing::info!("CACHE HIT for judge (key={})", &judge_key[..12]);
                    let tokens_out = crate::cost::estimate_tokens(
                        &serde_json::to_string(&cached_verdict).unwrap_or_default(),
                    );
                    cost_tracker.record_judge(tokens_in, tokens_out, true);
                    verdicts.push(cached_verdict);
                    continue;
                }
            }

            // Cache miss — make API call
            tracing::info!("CACHE MISS for judge (key={})", &judge_key[..12]);
            match with_retry(
                || run_judge(judge, &gc.comment, &finding.message),
                3,    // max_retries
                1000, // base_delay_ms
            )
            .await
            {
                Ok(verdict) => {
                    let tokens_out = crate::cost::estimate_tokens(
                        &serde_json::to_string(&verdict).unwrap_or_default(),
                    );
                    cost_tracker.record_judge(tokens_in, tokens_out, false);

                    // Cache the judge call if cache is active
                    if let Some(ref c) = cache {
                        let verdict_json = serde_json::to_string(&verdict).unwrap_or_default();
                        let _ = c.save_judge_with_key(
                            &judge_key,
                            &gc.comment,
                            &finding.message,
                            &verdict_json,
                        );
                    }
                    verdicts.push(verdict);
                }
                Err(e) => tracing::warn!("Judge call failed after retries: {e}"),
            }
        }
    }

    Ok((all_findings, verdicts))
}

/// Run the multi-agent consensus evaluation, merging linter findings.
#[doc(hidden)]
pub async fn evaluate_pr_consensus(
    pr: &GoldenCommentEntry,
    client: &rig_core::providers::openai::Client,
    model: &str,
    judge: &Agent<ResponsesCompletionModel>,
    diff: &str,
    linter_findings: Vec<Finding>,
    rules_preamble: Option<&str>,
    prompt_lib: &PromptLibrary,
    roles: &str,
    max_findings: usize,
    cache: Option<Arc<crate::cache::LlmCache>>,
    _cost_tracker: Arc<crate::cost::CostTracker>,
) -> Result<(Vec<Finding>, Vec<crb_judge::JudgeVerdict>)> {
    // Parse comma-separated roles
    let parsed_roles: Vec<&str> = roles
        .split(',')
        .map(|r| r.trim())
        .filter(|r| !r.is_empty())
        .collect();

    // ── Pre-compute content-addressed cache key components ──────────────
    let diff_hash = crate::cache::LlmCache::sha256(diff);
    let rules_hash = crate::cache::LlmCache::sha256(rules_preamble.unwrap_or(""));
    let judge_prompt_hash = crate::cache::LlmCache::sha256(crb_judge::JUDGE_PROMPT);
    let first_role = parsed_roles.first().copied().unwrap_or("SA");
    let prompt_hash = crate::cache::LlmCache::sha256(prompt_lib.get(first_role));
    let judge_model = "";

    // Compute tool preamble for the first role
    let default_budget = crb_tools::budget::ToolCallBudget::default();
    let tool_preamble = crb_tools::tool_prompt_section(first_role, &default_budget, &[]);

    let result = evaluate_pr_with_consensus(
        pr,
        diff,
        client,
        model,
        judge,
        rules_preamble,
        Some(prompt_lib),
        None,
        &parsed_roles,
        max_findings,
        cache.clone().map(|c| c as Arc<dyn CacheBackend>),
        &diff_hash,
        &prompt_hash,
        &rules_hash,
        &judge_prompt_hash,
        judge_model,
        Some(&tool_preamble),
    )
    .await?;

    info!(
        "Consensus pipeline: {} agent findings, {} linter findings, {} goldens",
        result.findings_count,
        linter_findings.len(),
        result.golden_count
    );

    let all_findings: Vec<Finding> = linter_findings;
    Ok((all_findings, result.verdicts))
}

/// Post-process findings through aggregator dedup and auditor severity checks.
#[doc(hidden)]
pub fn post_process_findings(findings: &[Finding]) -> Vec<Finding> {
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

/// Evaluate a single PR, optionally using consensus orchestration and linters.
#[doc(hidden)]
pub async fn evaluate_pr_with_postprocessing(
    pr: &GoldenCommentEntry,
    client: &rig_core::providers::openai::Client,
    model: &str,
    judge: &Agent<ResponsesCompletionModel>,
    benchmark_dir: &Path,
    linter_configs: Option<&std::collections::HashMap<String, crb_tools::LinterConfig>>,
    skip_consensus: bool,
    linters_only: bool,
    ruleset: Option<&RuleSet>,
    prompt_lib: &PromptLibrary,
    roles: &str,
    max_findings: usize,
    cache_dir: Option<&PathBuf>,
    dashboard_tx: Option<&broadcast::Sender<DashboardEvent>>,
) -> Result<PrResult> {
    // ── Setup cache if enabled ────────────────────────────────────────────
    let cache: Option<Arc<crate::cache::LlmCache>> = if let Some(dir) = cache_dir {
        let pr_key = utils::sanitize_filename(&pr.pr_title);
        match crate::cache::LlmCache::new(dir, &pr_key) {
            Ok(c) => {
                info!(
                    "LLM cache enabled for PR '{}' at {}",
                    pr.pr_title,
                    c.dir().display()
                );
                Some(Arc::new(c))
            }
            Err(e) => {
                tracing::warn!("Failed to create LLM cache for PR '{}': {e}", pr.pr_title);
                None
            }
        }
    } else {
        None
    };

    // ── Cost tracker ──────────────────────────────────────────────────────
    let cost_tracker = Arc::new(crate::cost::CostTracker::new());

    // ── Diff loading ──────────────────────────────────────────────────────
    // Strategy: try persistent worktree first (gives full file context),
    // then fall back to cached diff only.
    let (diff, pr_repo_dir): (String, Option<std::path::PathBuf>) =
        match extract_pr_info(&pr.url) {
            Some((owner, repo, pr_num)) => {
                // Check for persistent per-PR worktree
                let worktree_path = benchmark_dir
                    .join("worktrees")
                    .join(format!("{owner}_{repo}_{pr_num}"));
                if worktree_path.join(".git").exists() {
                    info!(
                        "Using persistent worktree at {} for PR #{}",
                        worktree_path.display(),
                        pr_num
                    );
                    let d = load_cached_diff(benchmark_dir, &owner, &repo, pr_num)
                        .unwrap_or_default();
                    (d, Some(worktree_path))
                } else {
                    let d = load_cached_diff(benchmark_dir, &owner, &repo, pr_num)
                        .unwrap_or_default();
                    (d, None)
                }
            }
            None => {
                tracing::warn!(
                    "Could not extract PR info from URL '{}'. Using empty diff.",
                    pr.url
                );
                (String::new(), None)
            }
        };
    if diff.is_empty() {
        tracing::warn!("Empty diff for PR: {} (url: {})", pr.pr_title, pr.url);
    } else {
        info!("Loaded diff ({} bytes) for PR: {}", diff.len(), pr.pr_title);
    }

    // ── Linters ───────────────────────────────────────────────────────────
    let mut linter_findings: Vec<Finding> = Vec::new();
    if let Some(configs) = linter_configs {
        let host_repo_path = pr_repo_dir
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| benchmark_dir.to_string_lossy().to_string());
        let mut linter_set = tokio::task::JoinSet::new();
        for (_name, lconfig) in configs {
            let tool = crb_tools::create_linter_tool(lconfig);
            let args = crb_tools::LinterArgs {
                repo_path: host_repo_path.clone(),
            };
            linter_set.spawn(async move {
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
        return Ok(PrResult {
            pr_title: pr.pr_title.clone(),
            url: pr.url.clone(),
            findings_count: linter_findings.len(),
            golden_count: pr.comments.len(),
            metrics: crb_judge::Metrics::default(),
            verdicts: vec![],
            cost: Some(cost_tracker.to_summary()),
        });
    }

    // ── Compute rules preamble from changed files ────────────────────────
    let rules_preamble = ruleset.map(|rs| rs.format_preamble(&[]));

    // ── Agent evaluation ──────────────────────────────────────────────────
    let pr_key = utils::sanitize_filename(&pr.pr_title);

    // Send AgentStarted for each role
    if let Some(tx) = dashboard_tx {
        for role in ["SA", "CL", "AR", "SEC"] {
            let _ = tx.send(DashboardEvent::AgentStarted {
                pr_key: pr_key.clone(),
                role: role.to_string(),
            });
        }
    }

    let (all_findings, verdicts) = if skip_consensus {
        evaluate_pr_single_agent(
            pr,
            client,
            model,
            judge,
            &diff,
            linter_findings,
            rules_preamble.as_deref(),
            prompt_lib,
            cache.clone(),
            cost_tracker.clone(),
            dashboard_tx,
        )
        .await?
    } else {
        evaluate_pr_consensus(
            pr,
            client,
            model,
            judge,
            &diff,
            linter_findings,
            rules_preamble.as_deref(),
            prompt_lib,
            roles,
            max_findings,
            cache.clone(),
            cost_tracker.clone(),
        )
        .await?
    };

    // ── Post-processing: aggregator dedup + auditor severity check ────────
    let processed_findings = post_process_findings(&all_findings);

    // ── Send AgentFinished for each role ────────────────────────────────
    if let Some(tx) = dashboard_tx {
        for (i, role) in ["SA", "CL", "AR", "SEC"].iter().enumerate() {
            let role_findings = if skip_consensus {
                let per_role = all_findings.len() / 4;
                if i == 0 {
                    all_findings.len() - per_role * 3
                } else {
                    per_role
                }
            } else {
                processed_findings.len() / 4
            };
            let _ = tx.send(DashboardEvent::AgentFinished {
                role: role.to_string(),
                findings: role_findings,
                success: true,
            });
        }
    }

    // ── Judge evaluation ──────────────────────────────────────────────────
    // (if not already done by consensus path)
    let final_verdicts = verdicts;

    let metrics = compute_metrics(&final_verdicts, pr.comments.len());

    // ── Send PrCompleted event ───────────────────────────────────────────
    if let Some(tx) = dashboard_tx {
        let tokens = cost_tracker.total_tokens();
        let total_tokens = tokens.0 + tokens.1;
        let cost_usd = cost_tracker.total_cost_usd();
        let total_agent_calls = 4;
        let _ = tx.send(DashboardEvent::PrCompleted {
            pr_key,
            metrics: metrics.clone(),
            cost: cost_usd,
            total_tokens,
            agent_calls: total_agent_calls,
            findings_count: processed_findings.len(),
        });
    }

    // ── Write metadata.json ─────────────────────────────────────────────
    if let Some(ref c) = cache {
        let metadata = serde_json::json!({
            "pr_title": pr.pr_title,
            "url": pr.url,
            "model": model,
            "skip_consensus": skip_consensus,
            "timestamp": format!("{:?}", std::time::SystemTime::now()),
            "findings_count": processed_findings.len(),
            "golden_count": pr.comments.len(),
            "metrics": {
                "true_positives": metrics.true_positives,
                "false_positives": metrics.false_positives,
                "false_negatives": metrics.false_negatives,
                "precision": metrics.precision,
                "recall": metrics.recall,
                "f1": metrics.f1,
            },
        });
        if let Err(e) = c.save_metadata(&metadata) {
            tracing::warn!("Failed to write cache metadata: {e}");
        }
    }

    Ok(PrResult {
        pr_title: pr.pr_title.clone(),
        url: pr.url.clone(),
        findings_count: processed_findings.len(),
        golden_count: pr.comments.len(),
        metrics,
        verdicts: final_verdicts,
        cost: Some(cost_tracker.to_summary()),
    })
}

/// Write the `_summary.json` aggregate statistics file to the cache directory.
#[doc(hidden)]
pub fn write_summary(
    cache_dir: &PathBuf,
    args: &crate::config::BenchmarkArgs,
    results: &[PrResult],
    duration: Duration,
) -> Result<()> {
    let total_llm_calls: usize = results.iter().map(|r| r.findings_count).sum();
    let total_judge_calls: usize = results.iter().map(|r| r.verdicts.len()).sum();

    let total_tokens: usize = results
        .iter()
        .filter_map(|r| r.cost.as_ref())
        .map(|c| {
            c.agent_tokens_in + c.agent_tokens_out + c.judge_tokens_in + c.judge_tokens_out
        })
        .sum();
    let total_cost_usd: f64 = results
        .iter()
        .filter_map(|r| r.cost.as_ref())
        .map(|c| c.total_usd)
        .sum();
    let avg_agent_cache_hit_rate = if results.is_empty() {
        0.0
    } else {
        results
            .iter()
            .filter_map(|r| r.cost.as_ref())
            .map(|c| c.agent_cache_hit_rate)
            .sum::<f64>()
            / results.len() as f64
    };
    let avg_judge_cache_hit_rate = if results.is_empty() {
        0.0
    } else {
        results
            .iter()
            .filter_map(|r| r.cost.as_ref())
            .map(|c| c.judge_cache_hit_rate)
            .sum::<f64>()
            / results.len() as f64
    };

    let aggregate_metrics = if results.is_empty() {
        serde_json::json!({})
    } else {
        let avg_precision =
            results.iter().map(|r| r.metrics.precision).sum::<f64>() / results.len() as f64;
        let avg_recall =
            results.iter().map(|r| r.metrics.recall).sum::<f64>() / results.len() as f64;
        let avg_f1 =
            results.iter().map(|r| r.metrics.f1).sum::<f64>() / results.len() as f64;
        serde_json::json!({
            "avg_precision": avg_precision,
            "avg_recall": avg_recall,
            "avg_f1": avg_f1,
            "total_true_positives": results.iter().map(|r| r.metrics.true_positives).sum::<usize>(),
            "total_false_positives": results.iter().map(|r| r.metrics.false_positives).sum::<usize>(),
            "total_false_negatives": results.iter().map(|r| r.metrics.false_negatives).sum::<usize>(),
        })
    };

    let summary = serde_json::json!({
        "run_id": std::env::current_dir()
            .ok()
            .and_then(|d| d.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_default(),
        "model": args.model,
        "judge_model": args.judge_model,
        "total_prs": results.len(),
        "total_llm_calls": total_llm_calls,
        "total_judge_calls": total_judge_calls,
        "duration_secs": duration.as_secs_f64(),
        "aggregate_metrics": aggregate_metrics,
        "total_tokens": total_tokens,
        "total_cost_usd": total_cost_usd,
        "agent_cache_hit_rate": avg_agent_cache_hit_rate,
        "judge_cache_hit_rate": avg_judge_cache_hit_rate,
    });

    let summary_path = cache_dir.join("_summary.json");
    std::fs::write(&summary_path, serde_json::to_string_pretty(&summary)?)?;
    info!("Cache summary written to: {}", summary_path.display());
    Ok(())
}

/// Print a terminal summary of cost and cache hit rates for all PRs.
#[doc(hidden)]
pub fn print_terminal_summary(results: &[PrResult]) {
    let separator = "═══════════════════════════════════════════════";
    println!("\n{separator}");

    let mut grand_total_tokens = 0usize;
    let mut grand_total_cost = 0.0f64;

    for result in results {
        let pr_label = extract_pr_info(&result.url)
            .map(|(owner, repo, num)| format!("{owner}/{repo}/{num}"))
            .unwrap_or_else(|| result.pr_title.clone());

        let f1 = result.metrics.f1;
        let findings_count = result.findings_count;

        if let Some(ref cost) = result.cost {
            let pr_tokens = cost.agent_tokens_in
                + cost.agent_tokens_out
                + cost.judge_tokens_in
                + cost.judge_tokens_out;
            let pr_cost = cost.total_usd;

            grand_total_tokens += pr_tokens;
            grand_total_cost += pr_cost;

            println!(
                " {}: F1={:.3}, {} findings, {:.1}K tokens, ${:.4}",
                pr_label,
                f1,
                findings_count,
                pr_tokens as f64 / 1000.0,
                pr_cost,
            );
        } else {
            println!(
                " {}: F1={:.3}, {} findings, -- tokens, $--",
                pr_label, f1, findings_count,
            );
        }
    }

    let total_agent_rate: f64 = results
        .iter()
        .filter_map(|r| r.cost.as_ref())
        .map(|c| c.agent_cache_hit_rate)
        .sum();
    let total_judge_rate: f64 = results
        .iter()
        .filter_map(|r| r.cost.as_ref())
        .map(|c| c.judge_cache_hit_rate)
        .sum();
    let pr_count_with_cost = results.iter().filter(|r| r.cost.is_some()).count();

    let avg_agent_rate = if pr_count_with_cost > 0 {
        total_agent_rate / pr_count_with_cost as f64
    } else {
        0.0
    };
    let avg_judge_rate = if pr_count_with_cost > 0 {
        total_judge_rate / pr_count_with_cost as f64
    } else {
        0.0
    };

    println!("{separator}");
    println!(
        " TOTAL: {} PR(s), {:.1}K tokens, ${:.4}",
        results.len(),
        grand_total_tokens as f64 / 1000.0,
        grand_total_cost,
    );
    println!(" Agent cache hit rate: {:.1}%", avg_agent_rate * 100.0);
    println!(" Judge cache hit rate: {:.1}%", avg_judge_rate * 100.0);
    println!("{separator}");
}

/// Run the validation pipeline: load baseline, read results from output dir,
/// compute average metrics, compare against thresholds, and exit with
/// the appropriate code (0 = pass, 1 = fail).
#[doc(hidden)]
pub async fn run_validate(workspace_root: &std::path::Path, version: &str) -> Result<()> {
    info!("Running validation against baseline v{version}");

    let baseline = crate::validation::load_baseline(workspace_root, version)?;
    info!("Loaded baseline for version: {}", baseline.version);

    let output_dir = workspace_root.join("output");
    let results_dir = if output_dir.exists() {
        output_dir
    } else {
        anyhow::bail!(
            "Output directory not found: {}. Run the harness first.",
            output_dir.display()
        );
    };

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
        crate::validation::compute_average_metrics(&loaded_results);
    let val_result = crate::validation::validate_against_baseline(
        &baseline,
        total_prs,
        avg_precision,
        avg_recall,
        avg_f1,
    );
    crate::validation::print_validation_summary(
        &baseline,
        &val_result,
        avg_precision,
        avg_recall,
        avg_f1,
    );

    if val_result.in_threshold {
        info!("Validation PASSED — all metrics within baseline thresholds");
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "Validation FAILED — metrics exceed baseline thresholds"
        ))
    }
}

// ── Internal utilities ─────────────────────────────────────────────────────

/// Internal helper module for reporting utilities.
pub mod utils {
    /// Sanitize a string for use as a filename.
    pub fn sanitize_filename(name: &str) -> String {
        name.chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }
}
