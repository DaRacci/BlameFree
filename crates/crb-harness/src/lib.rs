use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use crb_agents::build_agent;
use crb_agents::prompts::{AgentEntry, PromptLibrary};
use crb_auditor::apply_severity_auditor;
use crb_consensus::adaptive::get_roles_for_diff;
use crb_consensus::harness::evaluate_pr_with_consensus;
use crb_judge::{compute_metrics, run_judge};
use crb_reporting::PrResult;
use crb_reporting::golden::GoldenCommentEntry;
use crb_shared::cache::keys::compute_judge_cache_key;
use crb_shared::cache::{CacheBackend, LlmCache, RunHistoryEntry};
use crb_shared::deduplicate::semantic_dedup;
use crb_shared::finding::Finding;
use crb_shared::jaccard::jaccard_similarity;
use crb_shared::sanitize_filename;
use crb_shared::url::parse_github_url;
use crb_tools::linters::tool::LinterArgs;
use crb_tools::{build_tool_server, create_linter_tool, language_detector};
use crb_types::RunEvent;
use crb_types::wrappers::{Diff, Model};
use regex::Regex;
use rig_core::agent::{Agent, PromptResponse};
use rig_core::client::ProviderClient;
use rig_core::completion::{Prompt, Usage};
use rig_core::providers::openai;
use rig_core::providers::openai::responses_api::ResponsesCompletionModel;
use rig_core::tool::Tool;
use rig_core::tool::server::ToolServerHandle;
use tokio::sync::broadcast;
use tracing::{info, info_span, warn};

use crate::config::ReviewArgs;
use crate::cost::CostTracker;
use crate::diff::preprocess_diff;
use crate::eval::EvalStrategy;
use crate::model_capabilities::ReasoningEffort;

pub mod config;
pub mod cost;
pub mod diff;
pub mod eval;
pub mod filter;
pub mod model_capabilities;
pub mod paths;
pub mod test_utils;
pub mod validation;

/// Describes which kind of diff to review.
pub enum ReviewMode {
    /// Review a commit range `base..head`.
    Commits { base: String, head: String },

    /// Review the current working tree (unstaged + staged).
    Working,
}

/// Parameters for a full PR review.
pub struct ReviewParams {
    /// Unified diff of the PR to review.
    pub diff: String,

    /// Model identifier.
    pub model: String,

    /// Title of the PR being reviewed.
    pub pr_title: String,

    /// Reviewer role abbreviations.
    pub roles: Vec<String>,

    /// Maximum number of findings to return per agent.
    pub max_findings: usize,

    /// Optional cache directory for LLM response caching.
    pub cache_dir: Option<PathBuf>,
}

/// Run the shared agent loop for a set of roles, collecting findings.
async fn run_agent_roles(
    client: &openai::Client,
    model: &str,
    diff: &str,
    roles: &[&str],
    max_findings: usize,
    tool_server_handle: ToolServerHandle,
) -> Vec<Finding> {
    let mut all_findings = Vec::new();

    for &role in roles {
        let agent = build_agent(
            client,
            model,
            role,
            None,
            None,
            None,
            None,
            tool_server_handle.clone(),
        );

        match agent.prompt(diff).extended_details().await {
            Ok(resp) => {
                let response = resp.output;
                let _usage = resp.usage;
                match parse_agent_findings(&response) {
                    Ok(mut findings) => {
                        if findings.len() > max_findings {
                            findings.truncate(max_findings);
                        }
                        all_findings.append(&mut findings);
                    }
                    Err(e) => warn!("Failed to parse agent response for role {}: {}", role, e),
                }
            }
            Err(e) => warn!("Agent call failed for role {}: {}", role, e),
        }
    }

    all_findings
}

/// Entry point for reviewing a PR given its diff as a string.
///
/// Builds agents for each role, runs them with the diff, and returns findings.
pub async fn review_pr(
    params: ReviewParams,
    tool_server_handle: ToolServerHandle,
) -> Result<Vec<Finding>> {
    let client =
        openai::Client::from_env().map_err(|e| anyhow!("Failed to create OpenAI client: {e}"))?;

    let roles: Vec<&str> = if params.roles.is_empty() {
        PromptLibrary::get_instance().abbreviations()
    } else {
        params.roles.iter().map(|r| r.as_str()).collect()
    };

    let findings = run_agent_roles(
        &client,
        &params.model,
        &params.diff,
        &roles,
        params.max_findings,
        tool_server_handle,
    )
    .await;

    let findings = post_process_findings(&findings);
    Ok(findings)
}

/// Review a diff by running `git diff` in the given `path`, then call `review_pr()` with the diff to get agent findings.
///
/// - `ReviewMode::Commits { base, head }` -> `git diff base..head`
/// - `ReviewMode::Working`                -> `git diff` (unstaged + staged)
///
/// Returns a vector of agent findings parsed from the LLM response.
pub async fn review_diff(args: ReviewArgs) -> Result<Vec<Finding>> {
    let tool_server = build_tool_server(args.path.to_str(), None).run();

    let diff = {
        let cmd_args = if let Some(ref range) = args.commits {
            vec!["diff", range]
        } else {
            vec!["diff"]
        };

        let output = Command::new("git")
            .args(cmd_args)
            .current_dir(&args.path)
            .output()
            .map_err(|e| anyhow!("Failed to run git diff: {e}"))?;
        String::from_utf8_lossy(&output.stdout).to_string()
    };

    if diff.is_empty() {
        info!("No diff found; returning empty findings");
        return Ok(Vec::new());
    }

    info!(
        "Loaded diff ({} bytes) from {}",
        diff.len(),
        args.path.display()
    );

    let diff = preprocess_diff(&diff);

    let roles = PromptLibrary::get_instance().abbreviations();
    let params = ReviewParams {
        diff: diff.clone(),
        model: args.model.clone(),
        pr_title: "review".to_string(),
        roles: roles.iter().map(|s| s.to_string()).collect(),
        max_findings: 20,
        cache_dir: None,
    };
    review_pr(params, tool_server).await
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
    match fs::read_to_string(&diff_path) {
        Ok(content) => {
            info!(
                "Loaded cached diff ({} bytes) from {}",
                content.len(),
                diff_path.display()
            );
            Some(content)
        }
        Err(e) => {
            warn!(
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
/// Before deserializing, field names are normalised (path->file,
/// description->message, text->message, category->rule_code, component->file)
/// and severity values are case-normalised ("high"->"High", "MEDIUM"->"Medium").
///
/// If all strategies fail, returns an empty `Vec` with a warning.
pub fn parse_agent_findings(response: &str) -> Result<Vec<Finding>, String> {
    let preview_len = std::cmp::min(500, response.len());
    info!(
        "Agent raw response (first 500 chars): {}",
        &response[..preview_len]
    );

    // normalise field names and severity in a JSON value array.
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

                // Normalise severity case: "high" -> "High", "MEDIUM" -> "Medium"
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

    // Try direct JSON array parse with normalisation
    if let Some(findings) = normalise_findings(response) {
        info!(
            "Parsed {} finding(s) directly from agent JSON response",
            findings.len()
        );
        return Ok(findings);
    }

    // Extract JSON from markdown code blocks
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

    // Find any JSON array in the response
    let array_re = Regex::new(r"\[[\s\S]*\]").unwrap();
    if let Some(m) = array_re.find(response) {
        if let Some(findings) = normalise_findings(m.as_str()) {
            info!(
                "Parsed {} finding(s) from embedded JSON array",
                findings.len()
            );
            return Ok(findings);
        }
    }

    // All strategies failed; warn and return empty
    let truncated = if response.len() > 200 {
        format!("{}...", &response[..200])
    } else {
        response.to_string()
    };
    warn!(
        "Failed to parse agent response as Finding array. \
         Response (truncated): {}",
        truncated
    );
    Ok(Vec::new())
}

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
                warn!(
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

/// Pre-computed cache key parts used by the single-agent pipeline.
#[deprecated]
struct AgentCacheKeys {
    diff_hash: String,
    rules_hash: String,
    judge_prompt_hash: String,
    judge_model: String,
}

#[deprecated]
fn compute_cache_keys(diff: &str, rules_preamble: Option<&str>) -> AgentCacheKeys {
    AgentCacheKeys {
        diff_hash: crb_shared::cache::sha256_hex(diff),
        rules_hash: crb_shared::cache::sha256_hex(rules_preamble.unwrap_or("")),
        judge_prompt_hash: crb_shared::cache::sha256_hex(crb_judge::JUDGE_PROMPT),
        judge_model: String::new(),
    }
}

/// Spawn a single agent task for one role, with caching and retry.
#[allow(clippy::too_many_arguments)]
fn spawn_agent_task(
    role: String,
    client: openai::Client,
    model: Arc<String>,
    diff: Arc<String>,
    diff_hash: String,
    rules_hash: String,
    rules_preamble: Option<String>,
    cache: Option<Arc<dyn CacheBackend>>,
    cost_tracker: Arc<CostTracker>,
    dashboard_tx: Option<broadcast::Sender<RunEvent>>,
    additional_params: Option<serde_json::Value>,
) -> impl std::future::Future<Output = Result<Vec<Finding>, String>> {
    async move {
        let prompt_library = PromptLibrary::get_instance();
        let span = info_span!("agent", role = %role);
        let _guard = span.enter();

        let prompt_hash = crb_shared::cache::sha256_hex(prompt_library.get(&role).unwrap_or(""));
        let agent_cache_key = crb_shared::cache::compute_agent_cache_key(
            &prompt_hash,
            &diff_hash,
            model.as_str(),
            &role,
            &rules_hash,
        );

        // Check cache first
        if let Some(ref c) = cache {
            if let Some((cached_response, cached_usage)) =
                c.lookup_agent_by_key_with_usage(&agent_cache_key)
            {
                info!(
                    "CACHE HIT for agent role={} (key={})",
                    role,
                    &agent_cache_key[..12]
                );
                let usage = cached_usage.unwrap_or_default();
                cost_tracker.record_agent(&usage, true);
                if let Some(ref tx) = dashboard_tx {
                    // Ignore; receiver may have disconnected
                    let _ = tx.send(RunEvent::AgentChunk {
                        role: role.clone(),
                        chunk: cached_response.clone(),
                    });
                    let result = parse_agent_findings(&cached_response);
                    let findings_count = result.as_ref().map(|v| v.len()).unwrap_or(0);
                    // Ignore; receiver may have disconnected
                    let _ = tx.send(RunEvent::AgentFinished {
                        role,
                        findings: findings_count,
                        success: result.is_ok(),
                    });
                }
                let result = parse_agent_findings(&cached_response);
                return result;
            }
        }
        info!(
            "CACHE MISS for agent role={} (key={})",
            role,
            &agent_cache_key[..12]
        );

        let tool_preamble = crb_tools::tool_prompt_section(
            &role,
            &crb_tools::budget::ToolCallBudget::default(),
            &[],
        );
        let agent = build_agent(
            &client,
            model.as_str(),
            &role,
            rules_preamble.as_deref(),
            None,
            Some(&tool_preamble),
            additional_params.clone(),
        );

        let result: Result<Vec<Finding>, String> = with_retry(
            || {
                let agent = agent.clone();
                let role = role.clone();
                let diff = Arc::clone(&diff);
                let agent_cache_key = agent_cache_key.clone();
                let cache = cache.clone();
                let ct = cost_tracker.clone();
                let tx = dashboard_tx.clone();
                async move {
                    let resp: PromptResponse = agent
                        .prompt(diff.as_str())
                        .extended_details()
                        .await
                        .map_err(|e| e.to_string())?;
                    let response = resp.output;
                    let usage = resp.usage;

                    ct.record_agent(&usage, false);

                    if let Some(ref tx) = tx {
                        let _ = tx.send(RunEvent::AgentChunk {
                            // Ignore — receiver may have disconnected
                            role: role.clone(),
                            chunk: response.clone(),
                        });
                    }

                    if let Some(ref c) = cache {
                        c.save_agent_with_key_and_usage(
                            &agent_cache_key,
                            &role,
                            diff.as_str(),
                            &response,
                            &usage,
                        );
                    }

                    let findings = parse_agent_findings(&response);
                    if let Some(ref tx) = tx {
                        let findings_count = findings.as_ref().map(|v| v.len()).unwrap_or(0);
                        let _ = tx.send(RunEvent::AgentFinished {
                            // Ignore — receiver may have disconnected
                            role: role.clone(),
                            findings: findings_count,
                            success: findings.is_ok(),
                        });
                    }
                    findings
                }
            },
            3,
            1000,
        )
        .await;

        if result.is_err() {
            if let Some(ref tx) = dashboard_tx {
                let _ = tx.send(RunEvent::AgentFinished {
                    // Ignore — receiver may have disconnected
                    role: role.clone(),
                    findings: 0,
                    success: false,
                });
            }
        }
        result
    }
}

/// Run the judge evaluation loop comparing findings against golden comments,
/// using Jaccard pre-filtering and then LLM judge with caching.
async fn run_judge_evaluation(
    findings: &[Finding],
    pr: &GoldenCommentEntry,
    judge: &Agent<ResponsesCompletionModel>,
    cache_keys: &AgentCacheKeys,
    cache: Option<Arc<LlmCache>>,
    cost_tracker: &CostTracker,
) -> Vec<crb_judge::JudgeVerdict> {
    let jaccard_threshold = 0.12;
    let mut verdicts = Vec::new();

    for finding in findings {
        for gc in &pr.comments {
            let score = jaccard_similarity(&finding.message, &gc.comment, false);
            if score >= jaccard_threshold {
                info!(
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

            // File/line pre-filter
            if let Some(golden_file) = &gc.file {
                if let Some(finding_file) = &finding.file {
                    if golden_file != finding_file {
                        continue;
                    }
                }
            }

            // Judge cache key
            let judge_key = compute_judge_cache_key(
                &cache_keys.judge_prompt_hash,
                &finding.message,
                &gc.comment,
                &cache_keys.judge_model,
            );

            // Check judge cache
            if let Some(ref c) = cache {
                if let Some(cached_verdict) = c.lookup_judge_by_key(&judge_key) {
                    info!("CACHE HIT for judge (key={})", &judge_key[..12]);
                    cost_tracker.record_judge_empty(true);
                    verdicts.push(cached_verdict);
                    continue;
                }
            }

            // Cache miss - make API call
            info!("CACHE MISS for judge (key={})", &judge_key[..12]);
            match with_retry(|| run_judge(judge, &gc.comment, &finding.message), 3, 1000).await {
                Ok((verdict, usage)) => {
                    cost_tracker.record_judge(&usage, false);
                    if let Some(ref c) = cache {
                        let verdict_json = serde_json::to_string(&verdict).unwrap_or_default();
                        let _ = c.save_judge_with_key(
                            // Ignore — cache save is best-effort (non-critical)
                            &judge_key,
                            &gc.comment,
                            &finding.message,
                            &verdict_json,
                        );
                    }
                    verdicts.push(verdict);
                }
                Err(e) => warn!("Judge call failed after retries: {e}"),
            }
        }
    }

    verdicts
}

/// Run the original single-agent evaluation with finding collection.
/// (private) used by evaluate_pr
#[doc(hidden)]
#[allow(trivial_casts)]
async fn evaluate_pr_single_agent(
    pr: &GoldenCommentEntry,
    client: &openai::Client,
    model: &str,
    judge: &Agent<ResponsesCompletionModel>,
    diff: &str,
    linter_findings: Vec<Finding>,
    rules_preamble: Option<&str>,
    prompt_lib: &PromptLibrary,
    cache: Option<Arc<LlmCache>>,
    cost_tracker: Arc<crate::cost::CostTracker>,
    dashboard_tx: Option<&broadcast::Sender<RunEvent>>,
    additional_params: Option<serde_json::Value>,
) -> Result<(Vec<Finding>, Vec<crb_judge::JudgeVerdict>)> {
    let cache_keys = compute_cache_keys(diff, rules_preamble);

    // ── Phase 1: spawn one agent per role ─────────────────────────────────
    let mut agent_set = tokio::task::JoinSet::new();
    let dashboard_tx_owned = dashboard_tx.map(|t| t.clone());
    let prompt_lib_arc = Arc::new(prompt_lib.clone());
    let diff_arc = Arc::new(diff.to_string());
    let model_arc = Arc::new(model.to_string());
    let diff_hash = cache_keys.diff_hash.clone();
    let rules_hash = cache_keys.rules_hash.clone();
    let rules_preamble_owned = rules_preamble.map(String::from);
    for role in prompt_lib.roles() {
        let cache_arc: Option<Arc<dyn CacheBackend>> =
            cache.clone().map(|c| c as Arc<dyn CacheBackend>);
        agent_set.spawn(spawn_agent_task(
            role.to_string(),
            client.clone(),
            Arc::clone(&model_arc),
            Arc::clone(&diff_arc),
            diff_hash.clone(),
            rules_hash.clone(),
            rules_preamble_owned.clone(),
            Arc::clone(&prompt_lib_arc),
            cache_arc,
            cost_tracker.clone(),
            dashboard_tx_owned.clone(),
            additional_params.clone(),
        ));
    }

    // ── Phase 2: collect agent findings ──────────────────────────────────
    let mut all_findings: Vec<Finding> = linter_findings;
    while let Some(res) = agent_set.join_next().await {
        match res {
            Ok(Ok(mut findings)) => all_findings.append(&mut findings),
            Ok(Err(e)) => warn!("Agent failed: {e}"),
            Err(e) => warn!("Agent join error: {e}"),
        }
    }

    // ── Phase 3: judge evaluation ────────────────────────────────────────
    let verdicts =
        run_judge_evaluation(&all_findings, pr, judge, &cache_keys, cache, &cost_tracker).await;

    Ok((all_findings, verdicts))
}

#[allow(trivial_casts)]
async fn evaluate_pr(
    pr: &GoldenCommentEntry,
    client: &openai::Client,
    model: &Model,
    judge: &Agent<ResponsesCompletionModel>,
    diff: &Diff,
    linter_findings: Vec<Finding>,
    rules_preamble: Option<&str>,
    selected_agents: &[&AgentEntry],
    max_findings: usize,
    cache: Option<Arc<LlmCache>>,
    cost_tracker: Arc<CostTracker>,
    reasoning_effort: ReasoningEffort,
    dashboard_tx: Option<&broadcast::Sender<RunEvent>>,
) -> Result<(Vec<Finding>, Vec<crb_judge::JudgeVerdict>)> {
    if diff.is_empty() {
        info!("Diff is empty, returning empty result");
        return Ok((Vec::new(), Vec::new()));
    }

    let effective_roles = get_roles_for_diff(diff, selected_agents);
    let additional_params =
        model_capabilities::reasoning_to_additional_params(model, reasoning_effort);
    let template_vars = get_template_vars(pr, diff);

    let (result, agent_usage, judge_usage, agent_api_calls, judge_api_calls, judge_cache_hits) =
        evaluate_pr_with_consensus(
            pr,
            diff,
            client,
            model,
            judge,
            rules_preamble,
            template_vars,
            &effective_roles,
            max_findings,
            cache.clone().map(|c| c as Arc<dyn CacheBackend>),
            &diff_hash,
            &prompt_hash,
            &rules_hash,
            &judge_prompt_hash,
            judge_model,
            tool_preamble.as_deref(),
            workdir,
            additional_params,
            dashboard_tx.map(|t| t.clone()),
        )
        .await?;

    todo!()
}

fn get_template_vars(
    pr: &GoldenCommentEntry,
    diff: &Diff,
) -> Option<&'static HashMap<String, serde_json::Value>> {
    cfg_select! {
      feature = "exp14_template_vars" => {
        let language = language_detector::detect_primary_language(diff);
        let repo_name = language_detector::extract_repo_name(&pr.url);
        let lang_ref: &'static str = Box::leak(language.into_boxed_str());
        let repo_ref: &'static str = Box::leak(repo_name.into_boxed_str());
        let map: HashMap<String, serde_json::Value> = HashMap::from([
            (
                "language".to_string(),
                serde_json::Value::String(lang_ref.to_string()),
            ),
            (
                "repo".to_string(),
                serde_json::Value::String(repo_ref.to_string()),
            ),
            ("role".to_string(), serde_json::Value::String(String::new())),
        ]);
        Some(&*Box::leak(Box::new(map)))
      },
      _ => None
    }
}

#[doc(hidden)]
#[allow(trivial_casts)]
async fn evaluate_pr_consensus(
    pr: &GoldenCommentEntry,
    client: &openai::Client,
    model: &str,
    judge: &Agent<ResponsesCompletionModel>,
    diff: &str,
    linter_findings: Vec<Finding>,
    rules_preamble: Option<&str>,
    prompt_lib: &PromptLibrary,
    roles: &str,
    max_findings: usize,
    cache: Option<Arc<LlmCache>>,
    cost_tracker: Arc<crate::cost::CostTracker>,
    workdir: Option<&str>,
    reasoning_effort: Option<&str>,
    dashboard_tx: Option<&broadcast::Sender<RunEvent>>,
) -> Result<(Vec<Finding>, Vec<crb_judge::JudgeVerdict>)> {
    // Parse comma-separated roles
    // let parsed_roles: Vec<&str> = roles
    //     .split(',')
    //     .map(|r| r.trim())
    //     .filter(|r| !r.is_empty())
    //     .collect();

    // ── Adaptive agent dispatch (EXP-016) ──────────────────────────────
    // NOTE: This experimental feature is intentionally disabled because it
    // overrides user-selected roles with a single GEN agent, which:
    //   (a) violates user role selection expectations, and
    //   (b) prevents ARCH/AR agents from appearing in the results.
    // Feature flag is kept to avoid breaking builds that enable it,
    // but the override is suppressed to respect user-selected roles.
    // #[cfg(feature = "exp16_adaptive_agents")]
    // let parsed_roles: Vec<&str> = {
    //     // Only apply adaptive dispatch when user has explicitly opted in
    //     // by selecting only a single role; otherwise respect user's choice.

    //     use crb_consensus::adaptive::should_use_single_agent;
    //     if parsed_roles.len() == 1 && should_use_single_agent(diff, 3, 200) {
    //         info!("EXP-016 adaptive dispatch: small PR, using single GEN agent");
    //         vec!["GEN"]
    //     } else {
    //         parsed_roles
    //     }
    // };

    // if diff.is_empty() {
    //     info!("No diff - returning empty result");
    //     return Ok((Vec::new(), Vec::new()));
    // }

    // ── Pre-compute content-addressed cache key components ──────────────
    let first_role = parsed_roles.first().copied().unwrap_or("SA");
    let prompt_hash = crb_shared::cache::sha256_hex(prompt_lib.get(first_role).unwrap_or(""));
    let rules_hash = crb_shared::cache::sha256_hex(rules_preamble.unwrap_or(""));
    let judge_prompt_hash = crb_shared::cache::sha256_hex(crb_judge::JUDGE_PROMPT);
    let diff_hash = crb_shared::cache::sha256_hex(diff);
    let judge_model = "";

    // Compute tool preamble only when workdir is provided
    let tool_preamble = workdir.map(|_| {
        let default_budget = crb_tools::budget::ToolCallBudget::default();
        crb_tools::tool_prompt_section(first_role, &default_budget, &[])
    });

    info!(
        "Consensus pipeline: {} agent role(s), max {} findings per role",
        parsed_roles.len(),
        max_findings,
    );

    // ── Convert reasoning_effort to additional_params ──────────────────
    // let additional_params =
    //     model_capabilities::reasoning_to_additional_params(model, reasoning_effort);
    // if additional_params.is_some() {
    //     info!(
    //         "Reasoning effort enabled: {:?}",
    //         reasoning_effort.unwrap_or("medium")
    //     );
    // }

    // ── Build template variables from diff and PR context (EXP-014) ──
    // #[cfg(feature = "exp14_template_vars")]
    // let template_vars: Option<&'static HashMap<String, serde_json::Value>> = {
    //     let language = language_detector::detect_primary_language(diff);
    //     let repo_name = language_detector::extract_repo_name(&pr.url);
    //     let lang_ref: &'static str = Box::leak(language.into_boxed_str());
    //     let repo_ref: &'static str = Box::leak(repo_name.into_boxed_str());
    //     let map: HashMap<String, serde_json::Value> = HashMap::from([
    //         (
    //             "language".to_string(),
    //             serde_json::Value::String(lang_ref.to_string()),
    //         ),
    //         (
    //             "repo".to_string(),
    //             serde_json::Value::String(repo_ref.to_string()),
    //         ),
    //         ("role".to_string(), serde_json::Value::String(String::new())),
    //     ]);
    //     Some(&*Box::leak(Box::new(map)))
    // };

    // #[cfg(not(feature = "exp14_template_vars"))]
    // let template_vars = None;

    let (result, agent_usage, judge_usage, agent_api_calls, judge_api_calls, judge_cache_hits) =
        evaluate_pr_with_consensus(
            pr,
            diff,
            client,
            model,
            judge,
            rules_preamble,
            template_vars,
            &parsed_roles,
            max_findings,
            cache.clone().map(|c| c as Arc<dyn CacheBackend>),
            &diff_hash,
            &prompt_hash,
            &rules_hash,
            &judge_prompt_hash,
            judge_model,
            tool_preamble.as_deref(),
            workdir,
            additional_params,
            dashboard_tx.map(|t| t.clone()),
        )
        .await?;

    let role_count = parsed_roles.len();
    if role_count > 0 {
        let per_agent = Usage {
            input_tokens: agent_usage.input_tokens / role_count as u64,
            output_tokens: agent_usage.output_tokens / role_count as u64,
            total_tokens: agent_usage.total_tokens / role_count as u64,
            cached_input_tokens: agent_usage.cached_input_tokens / role_count as u64,
            cache_creation_input_tokens: agent_usage.cache_creation_input_tokens
                / role_count as u64,
            reasoning_tokens: agent_usage.reasoning_tokens / role_count as u64,
            tool_use_prompt_tokens: agent_usage.tool_use_prompt_tokens / role_count as u64,
        };
        // First agent_api_calls are cache misses, the rest are cache hits
        for i in 0..role_count {
            let cache_hit = i >= agent_api_calls;
            cost_tracker.record_agent(&per_agent, cache_hit);
        }
    }

    // Judge usage: only cache misses have real usage data
    let judge_total = judge_api_calls + judge_cache_hits;
    if judge_total > 0 {
        let per_judge = if judge_api_calls > 0 {
            Usage {
                input_tokens: judge_usage.input_tokens / judge_api_calls as u64,
                output_tokens: judge_usage.output_tokens / judge_api_calls as u64,
                total_tokens: judge_usage.total_tokens / judge_api_calls as u64,
                cached_input_tokens: judge_usage.cached_input_tokens / judge_api_calls as u64,
                cache_creation_input_tokens: judge_usage.cache_creation_input_tokens
                    / judge_api_calls as u64,
                reasoning_tokens: judge_usage.reasoning_tokens / judge_api_calls as u64,
                tool_use_prompt_tokens: judge_usage.tool_use_prompt_tokens / judge_api_calls as u64,
            }
        } else {
            Usage::new()
        };
        for _ in 0..judge_api_calls {
            cost_tracker.record_judge(&per_judge, false);
        }
        // Cache hits have zero usage (no stored data)
        for _ in 0..judge_cache_hits {
            cost_tracker.record_judge_empty(true);
        }
    }

    info!(
        "Consensus pipeline: {} agent findings, {} linter findings, {} goldens",
        result.findings_count,
        linter_findings.len(),
        result.golden_count
    );

    // The consensus crate's PrResult contains the actual findings count.
    // We still need to return `all_findings` for post-processing compat,
    // but note that all_findings is empty when linters are skipped -
    // the findings_count will be derived from verdicts in the caller.
    let all_findings: Vec<Finding> = Vec::new();
    Ok((all_findings, result.verdicts))
}

/// Post-process findings through aggregator dedup and auditor severity checks.
#[doc(hidden)]
pub fn post_process_findings(findings: &[Finding]) -> Vec<Finding> {
    if findings.is_empty() {
        return findings.to_vec();
    }

    let deduped = semantic_dedup(findings.to_vec());
    let audited = apply_severity_auditor(deduped);
    let capped = {
        let max = 20;
        if audited.len() > max {
            info!("capping {} findings to {} candidates", audited.len(), max);
            audited.into_iter().take(max).collect()
        } else {
            audited
        }
    };

    capped
}

/// Load the diff for a PR from pre-extracted cached diff files.
///
/// Tries the persistent worktree first, then falls back to cached diff files
/// at `{benchmark_dir}/diffs/{owner}_{repo}_{pr_num}.diff`.
pub async fn load_pr_diff(pr: &GoldenCommentEntry, benchmark_dir: &Path) -> Result<String> {
    match parse_github_url(&pr.url) {
        Ok((owner, repo, pr_num)) => {
            let worktree_path = benchmark_dir
                .join("worktrees")
                .join(format!("{owner}_{repo}_{pr_num}"));
            if worktree_path.join(".git").exists() {
                info!(
                    "Using persistent worktree at {} for PR #{}",
                    worktree_path.display(),
                    pr_num
                );
            }

            let d = load_cached_diff(benchmark_dir, &owner, &repo, pr_num).unwrap_or_default();
            if d.is_empty() {
                warn!("Empty diff for PR: {} (url: {})", pr.pr_title, pr.url);
            } else {
                info!("Loaded diff ({} bytes) for PR: {}", d.len(), pr.pr_title);
            }
            Ok(d)
        }
        Err(_) => {
            warn!(
                "Could not extract PR info from URL '{}'. Using empty diff.",
                pr.url
            );
            Ok(String::new())
        }
    }
}

/// Unified evaluation of a single PR.
///
/// This function runs the steps:
/// - diff preprocessing
/// - linter collection
/// - strategy dispatch
/// - post-processing (dedup / severity auditor / capping)
/// - metrics computation
/// - dashboard events
/// - metadata
/// - caching
pub async fn evaluate_pr(
    pr: &GoldenCommentEntry,
    diff: &str,
    config: &EvalConfig,
) -> Result<PrResult> {
    let bench_dir = config
        .benchmark_dir
        .as_deref()
        .unwrap_or_else(|| Path::new("."));

    let cache: Option<Arc<LlmCache>> = if let Some(ref cache_dir) = config.cache_dir {
        let pr_key = sanitize_filename(&pr.pr_title);
        let c = Arc::new(
            LlmCache::new(cache_dir, &pr_key).expect("Failed to create LLM cache directory"),
        );
        info!(
            "LLM cache enabled for PR '{}' at {}",
            pr.pr_title,
            c.dir().display()
        );
        Some(c)
    } else {
        info!("LLM cache disabled for PR '{}'", pr.pr_title);
        None
    };

    let diff = crate::preprocess_diff(diff);

    let mut linter_findings: Vec<Finding> = Vec::new();
    if let Some(ref configs) = config.linter_configs {
        let host_repo_path = bench_dir.to_string_lossy().to_string();
        let mut linter_set = tokio::task::JoinSet::new();
        for (_name, lconfig) in configs {
            let tool = create_linter_tool(lconfig);
            let args = LinterArgs {
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
                Ok(Err(e)) => warn!("Linter failed: {e}"),
                Err(e) => warn!("Linter join error: {e}"),
            }
        }

        info!(
            "Found {} linter finding(s) for PR: {}",
            linter_findings.len(),
            pr.pr_title
        );
    }

    if config.linters_only {
        return Ok(PrResult {
            pr_title: pr.pr_title.clone(),
            url: pr.url.clone(),
            findings_count: linter_findings.len(),
            golden_count: pr.comments.len(),
            metrics: crb_judge::Metrics::default(),
            verdicts: vec![],
            cost: Some(config.cost_tracker.to_summary()),
        });
    }

    let rules_preamble = config.ruleset.as_ref().map(|rs| rs.format_preamble(&[]));

    let pr_key = sanitize_filename(&pr.pr_title);
    if let Some(ref tx) = config.dashboard_tx {
        for role in config
            .roles
            .split(',')
            .map(|r| r.trim())
            .filter(|r| !r.is_empty())
        {
            let _ = tx.send(RunEvent::AgentStarted {
                pr_key: pr_key.clone(),
                role: role.to_string(),
            });
        }
    }

    let (all_findings, verdicts) = match config.strategy {
        EvalStrategy::SingleAgent => {
            let reasoning_params = config
                .reasoning_effort
                .as_ref()
                .map(|re| {
                    model_capabilities::reasoning_to_additional_params(
                        &config.model,
                        Some(re.as_str()),
                    )
                })
                .flatten();
            evaluate_pr_single_agent(
                pr,
                &config.client,
                &config.model,
                &config.judge,
                &diff,
                linter_findings,
                rules_preamble.as_deref(),
                &config.prompt_lib,
                cache.clone(),
                config.cost_tracker.clone(),
                config.dashboard_tx.as_ref(),
                reasoning_params,
            )
            .await?
        }
        EvalStrategy::Consensus => {
            let reasoning = config
                .reasoning_effort
                .as_deref()
                .filter(|re| !re.is_empty() && *re != "none");
            evaluate_pr_consensus(
                pr,
                &config.client,
                &config.model,
                &config.judge,
                &diff,
                linter_findings,
                rules_preamble.as_deref(),
                &config.prompt_lib,
                &config.roles,
                config.max_findings,
                cache.clone(),
                config.cost_tracker.clone(),
                config.workdir.as_deref(),
                reasoning,
                config.dashboard_tx.as_ref(),
            )
            .await?
        }
    };

    let processed_findings = post_process_findings(&all_findings);

    if let Some(ref tx) = config.dashboard_tx {
        for (_i, role) in ["SA", "CL", "AR", "SEC"].iter().enumerate() {
            let _ = tx.send(RunEvent::AgentFinished {
                role: role.to_string(),
                findings: processed_findings.len() / 4,
                success: true,
            });
        }
    }

    // ── Phase 10: Metrics ────────────────────────────────────────────────
    let metrics = compute_metrics(&verdicts, pr.comments.len());

    // ── Phase 11: Dashboard PrCompleted event ──────────────────────────
    if let Some(ref tx) = config.dashboard_tx {
        let tokens = config.cost_tracker.total_tokens();
        let total_tokens = tokens.0 + tokens.1;
        let cost_usd = config.cost_tracker.total_cost_usd();
        let total_agent_calls = 4;
        let _ = tx.send(RunEvent::PrCompleted {
            pr_key,
            metrics: crb_types::MetricsData {
                true_positives: metrics.true_positives,
                false_positives: metrics.false_positives,
                false_negatives: metrics.false_negatives,
                precision: metrics.precision,
                recall: metrics.recall,
                f1: metrics.f1,
            },
            cost: cost_usd,
            total_tokens,
            agent_calls: total_agent_calls,
            findings_count: verdicts.len(),
        });
    }

    // ── Phase 12: Cache metadata ────────────────────────────────────────
    let metadata = serde_json::json!({
        "pr_title": pr.pr_title,
        "url": pr.url,
        "model": config.model,
        "strategy": format!("{:?}", config.strategy),
        "timestamp": format!("{:?}", std::time::SystemTime::now()),
        "findings_count": verdicts.len(),
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
    if let Some(ref cache) = cache {
        if let Err(e) = cache.save_metadata(&metadata) {
            warn!("Failed to write cache metadata: {e}");
        }
    }

    // ── Phase 13: Return result ─────────────────────────────────────────
    Ok(PrResult {
        pr_title: pr.pr_title.clone(),
        url: pr.url.clone(),
        findings_count: verdicts.len(),
        golden_count: pr.comments.len(),
        metrics,
        verdicts,
        cost: Some(config.cost_tracker.to_summary()),
    })
}

/// Append a run history entry to the runs file in the cache directory.
fn append_run_history(cache_dir: &Path, entry: &RunHistoryEntry) -> Result<()> {
    let path = cache_dir.join(crate::paths::RUNS_FILE);
    let mut runs: Vec<RunHistoryEntry> = if path.exists() {
        let content = fs::read_to_string(&path).unwrap_or_else(|_| "[]".to_string());
        serde_json::from_str(&content).unwrap_or_else(|_| Vec::new())
    } else {
        Vec::new()
    };
    runs.push(entry.clone());
    fs::write(&path, serde_json::to_string_pretty(&runs)?)?;
    info!("Appended run history to: {}", path.display());
    Ok(())
}

/// Write the `_summary.json` aggregate statistics file to the cache directory.
#[doc(hidden)]
pub fn write_summary(
    cache_dir: &PathBuf,
    model: &str,
    judge_model: &str,
    results: &[PrResult],
    duration: Duration,
) -> Result<()> {
    let total_llm_calls: usize = results.iter().map(|r| r.findings_count).sum();
    let total_judge_calls: usize = results.iter().map(|r| r.verdicts.len()).sum();

    let total_tokens: usize = results
        .iter()
        .filter_map(|r| r.cost.as_ref())
        .map(|c| c.agent_tokens_in + c.agent_tokens_out + c.judge_tokens_in + c.judge_tokens_out)
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
        let avg_f1 = results.iter().map(|r| r.metrics.f1).sum::<f64>() / results.len() as f64;
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
        "model": model,
        "judge_model": judge_model,
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

    let summary_path = cache_dir.join(crate::paths::SUMMARY_FILE);
    fs::write(&summary_path, serde_json::to_string_pretty(&summary)?)?;
    info!("Cache summary written to: {}", summary_path.display());

    let run_entry = RunHistoryEntry {
        run_id: summary["run_id"].as_str().unwrap_or("").to_string(),
        timestamp: format!("{:?}", std::time::SystemTime::now()),
        model: model.to_string(),
        judge_model: judge_model.to_string(),
        total_prs: results.len(),
        duration_secs: duration.as_secs_f64(),
        total_cost_usd,
        total_tokens,
        agent_cache_hit_rate: avg_agent_cache_hit_rate,
        judge_cache_hit_rate: avg_judge_cache_hit_rate,
    };
    append_run_history(cache_dir, &run_entry)?;

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
        let pr_label = parse_github_url(&result.url)
            .map(|(owner, repo, num)| format!("{owner}/{repo}/{num}"))
            .unwrap_or_else(|_| result.pr_title.clone());

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
pub async fn run_validate(workspace_root: &Path, version: &str) -> Result<()> {
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
    let mut entries: Vec<_> = fs::read_dir(&results_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "json"))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in &entries {
        let path = entry.path();
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read result: {}", path.display()))?;
        match serde_json::from_str::<crb_judge::Metrics>(&content) {
            Ok(metrics) => loaded_results.push(metrics),
            Err(e) => {
                warn!(
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
        info!("Validation PASSED - all metrics within baseline thresholds");
        Ok(())
    } else {
        Err(anyhow!(
            "Validation FAILED - metrics exceed baseline thresholds"
        ))
    }
}
