//! Multi-agent consensus orchestration for code review evaluation.
//!
//! Orchestrates multiple LLM reviewer agents concurrently,
//! then aggregates their structured findings via heuristic matching and LLM
//! judge fallback against golden comments.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use anyhow::Result;
use crb_reporting::golden::GoldenCommentEntry;
use crb_shared::finding::Finding;
use crb_shared::jaccard::jaccard_similarity;
use regex::Regex;
use rig_core::agent::{Agent, HookAction, PromptHook, ToolCallHookAction};
use rig_core::completion::{
    AssistantContent, CompletionModel, CompletionResponse, Message, PromptError, Usage,
};
use rig_core::providers::openai;
use rig_core::providers::openai::responses_api::ResponsesCompletionModel;
use rig_core::streaming::StreamingPrompt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::task::JoinSet;

use crb_agents::build_agent;
use crb_agents::prompts::PromptLibrary;
use crb_judge::{JudgeVerdict, run_judge};
use crb_reporting::PrResult;

/// Regex to extract JSON from markdown code blocks.
#[allow(clippy::unwrap_used)]
static RE_CODEBLOCK_JSON: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)```(?:json)?\s*\n(.*?)\n\s*```").unwrap());

/// Regex to find any JSON array in a response.
#[allow(clippy::unwrap_used)]
static RE_JSON_ARRAY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[[\s\S]*\]").unwrap());

/// Attempt to parse findings from an agent response using a 3-strategy
/// fallback:
///
/// 1. Direct JSON parse of the full response
/// 2. Extract JSON from markdown code blocks via [`RE_CODEBLOCK_JSON`]
/// 3. Find any JSON array via [`RE_JSON_ARRAY`]
///
/// If `context` is non-empty, a warning is logged with that context on
/// failure (e.g. `"CACHED"`, `""` for silent failure).
fn parse_findings_from_response(response: &str, role: &Role, context: &str) -> Vec<Finding> {
    serde_json::from_str(response).unwrap_or_else(|_| {
        if let Some(caps) = RE_CODEBLOCK_JSON.captures(response) {
            #[allow(clippy::unwrap_used)]
            let inner = caps.get(1).unwrap().as_str().trim();
            if let Ok(f) = serde_json::from_str::<Vec<Finding>>(inner) {
                return f;
            }
        }
        if let Some(m) = RE_JSON_ARRAY.find(response) {
            if let Ok(f) = serde_json::from_str::<Vec<Finding>>(m.as_str()) {
                return f;
            }
        }
        if !context.is_empty() {
            tracing::warn!(
                "Failed to parse {} findings for role {:?}. Response (truncated): {}",
                context,
                role,
                &response[..std::cmp::min(200, response.len())],
            );
        }
        Vec::new()
    })
}

/// A [`PromptHook`] that skips tool calls with budget nudge messages when the
/// agent is approaching its turn limit.
///
/// Mechanism:
/// - Counts model-completion calls via [`on_completion_response`].
/// - When ≤2 completions remain, [`on_tool_call`] returns `Skip` with a
///   progressively firmer nudge ("X turns remaining…" -> "LAST TURN: …").
/// - The skipped reason is fed back to the model as a synthetic tool result,
///   effectively "stripping tools" without requiring internal loop access.
///
/// See arXiv:2510.16786 for the two-tier nudge pattern at ~70 % / ~90 % of
/// the turn budget.
#[derive(Clone)]
struct TurnBudgetHook {
    max_turns: usize,
    completion_count: Arc<AtomicUsize>,
}

impl TurnBudgetHook {
    fn new(max_turns: usize) -> Self {
        Self {
            max_turns,
            completion_count: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl<M: CompletionModel> PromptHook<M> for TurnBudgetHook {
    /// Increment the completion call counter after each model response.
    async fn on_completion_response(
        &self,
        _prompt: &Message,
        _response: &CompletionResponse<M::Response>,
    ) -> HookAction {
        self.completion_count.fetch_add(1, Ordering::SeqCst);
        HookAction::cont()
    }

    /// Skip tool calls when the agent is close to exhausting its turn budget.
    ///
    /// The 70 % / 90 % pattern from arXiv:2510.16786 translates to:
    /// - ≤2 remaining -> "You have X turns remaining."
    /// - ≤1 remaining -> "LAST TURN: …"
    async fn on_tool_call(
        &self,
        _tool_name: &str,
        _tool_call_id: Option<String>,
        _internal_call_id: &str,
        _args: &str,
    ) -> ToolCallHookAction {
        let calls_made = self.completion_count.load(Ordering::SeqCst);
        // Total possible completion calls = max_turns + 1 (the final text-only
        // turn before the error fires at max_turns + 2).
        let total_possible = self.max_turns + 1;
        let remaining = total_possible.saturating_sub(calls_made);

        if remaining <= 1 {
            ToolCallHookAction::Skip {
                reason: "\
LAST TURN: This is your final opportunity. Do NOT call any more tools. \
Output your JSON findings directly."
                    .to_string(),
            }
        } else if remaining <= 2 {
            ToolCallHookAction::Skip {
                reason: format!(
                    "You have {} turns remaining. Stop exploring and output your JSON findings.",
                    remaining
                ),
            }
        } else {
            ToolCallHookAction::cont()
        }
    }
}

/// Try to extract the last assistant text message from a chat history.
///
/// When [`PromptError::MaxTurnsError`] fires, the agent's accumulated
/// conversation is available in `chat_history`.  This function walks it in
/// reverse to find the most recent `Message::Assistant` whose content includes
/// an `AssistantContent::Text` variant - that text is often a partial or
/// complete JSON findings array the model produced before being cut off.
fn extract_last_assistant_text(history: &[Message]) -> Option<String> {
    for msg in history.iter().rev() {
        if let Message::Assistant { content, .. } = msg {
            for item in content.iter() {
                if let AssistantContent::Text(text) = item {
                    let t = text.text.trim().to_string();
                    if !t.is_empty() {
                        return Some(t);
                    }
                }
            }
        }
    }
    None
}

/// Compute a SHA256 hex digest of the input string.
pub fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Compute a content-addressed cache key for an agent LLM call.
///
/// Components (all SHA256 hex digests or plain strings) are concatenated
/// and hashed again to produce a single deterministic key.
pub fn compute_agent_cache_key(
    prompt_hash: &str,
    diff_hash: &str,
    model_name: &str,
    role: &str,
    rules_hash: &str,
) -> String {
    sha256_hex(&format!(
        "{}:{}:{}:{}:{}",
        prompt_hash, diff_hash, model_name, role, rules_hash
    ))
}

/// Compute a content-addressed cache key for a judge LLM call.
pub fn compute_judge_cache_key(
    judge_prompt_hash: &str,
    finding_message: &str,
    golden_comment: &str,
    judge_model: &str,
) -> String {
    sha256_hex(&format!(
        "{}:{}:{}:{}",
        judge_prompt_hash, finding_message, golden_comment, judge_model
    ))
}

/// Compute a content-addressed cache key for a context gatherer LLM call.
pub fn compute_context_cache_key(
    gatherer_prompt_hash: &str,
    diff_hash: &str,
    repo_state_hash: &str,
    model_name: &str,
) -> String {
    sha256_hex(&format!(
        "{}:{}:{}:{}",
        gatherer_prompt_hash, diff_hash, repo_state_hash, model_name
    ))
}

/// Interface for caching LLM interactions (prompts, responses, judge calls).
///
/// This is a trait so that the harness can inject its own cache implementation
/// without creating a circular dependency between `crb-consensus` and
/// `crb-harness`.
pub trait CacheBackend: Send + Sync {
    /// Save an agent prompt+response pair for the given role.
    fn save_agent(&self, role: &str, prompt: &str, response: &str);

    /// Append a judge call entry (golden comment, finding message, verdict JSON).
    fn save_judge(&self, golden: &str, finding: &str, verdict_json: &str);

    // ── Content-addressed caching methods ─────────────────────────────

    /// Look up a cached agent response by its content-addressed key.
    /// Returns `Some(response_text)` on cache hit, `None` on miss.
    fn lookup_agent_by_key(&self, _cache_key: &str) -> Option<String> {
        None
    }

    /// Look up a cached agent response by its content-addressed key,
    /// also returning the saved API usage data if available.
    /// Returns `Some((response_text, Option<usage>))` on cache hit.
    fn lookup_agent_by_key_with_usage(&self, _cache_key: &str) -> Option<(String, Option<Usage>)> {
        // Default: just return response with no usage
        self.lookup_agent_by_key(_cache_key)
            .map(|resp| (resp, None))
    }

    /// Look up a cached judge verdict by its content-addressed key.
    /// Returns `Some(JudgeVerdict)` on cache hit, `None` on miss.
    fn lookup_judge_by_key(&self, _cache_key: &str) -> Option<JudgeVerdict> {
        None
    }

    /// Save an agent prompt+response pair with a content-addressed cache key.
    fn save_agent_with_key(&self, _cache_key: &str, _role: &str, _prompt: &str, _response: &str) {}

    /// Save an agent prompt+response pair with a content-addressed cache key,
    /// including the API usage data.
    fn save_agent_with_key_and_usage(
        &self,
        _cache_key: &str,
        _role: &str,
        _prompt: &str,
        _response: &str,
        _usage: &Usage,
    ) {
    }

    /// Save agent reasoning/thinking text with a content-addressed cache key.
    fn save_agent_reasoning_with_key(&self, _cache_key: &str, _role: &str, _reasoning: &str) {}

    /// Save a judge verdict with a content-addressed cache key.
    fn save_judge_with_key(
        &self,
        _cache_key: &str,
        _golden: &str,
        _finding: &str,
        _verdict_json: &str,
    ) {
    }

    /// Look up a cached context gatherer response by its content-addressed key.
    fn lookup_context_by_key(&self, _cache_key: &str) -> Option<String> {
        None
    }

    /// Save a context gatherer prompt+response pair with a content-addressed cache key.
    fn save_context_with_key(&self, _cache_key: &str, _prompt: &str, _response: &str) {}
}

// ── Types ────────────────────────────────────────────────────────────────────

/// The role of a reviewer agent.
///
/// This is a dynamic newtype around a string abbreviation.
/// Valid values are loaded at runtime from the agent manifest (`prompts/agents/*.md`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize, Serialize)]
pub struct Role(pub String);

impl Role {
    /// Convert to the string identifier used by `crb_agents::build_agent`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for Role {
    fn from(s: &str) -> Self {
        Role(s.to_uppercase())
    }
}

impl From<String> for Role {
    fn from(s: String) -> Self {
        Role(s.to_uppercase())
    }
}

/// Configuration for a single reviewer agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewerConfig {
    pub role: Role,
    pub model: String,
    pub max_findings: usize,
}

/// A golden (expected) comment against which findings are judged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenComment {
    pub file: String,
    pub line: u32,
    /// Regex pattern matched against `Finding::message`.
    pub message_regex: String,
    pub severity: String,
    /// Which role(s) should catch this: "SA", "CL", "AR", "SEC", or "any".
    pub source: String,
}

impl GoldenComment {
    /// Check whether a candidate finding matches this golden comment's
    /// file and line (exact match on both).
    pub fn matches_candidate(&self, f: &Finding) -> bool {
        f.file.as_deref() == Some(&self.file) && f.line == Some(self.line)
    }
}

/// Result of matching a golden comment against candidate findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchResult {
    /// A candidate finding matches the golden comment.
    TruePositive,
    /// A candidate finding has no matching golden comment.
    FalsePositive,
    /// A golden comment has no matching candidate finding.
    FalseNegative,
}

/// Output of a full consensus run.
#[derive(Debug, Clone, Serialize)]
pub struct ConsensusReport {
    /// Findings from each agent, grouped by role.
    pub agents: Vec<(Role, Vec<Finding>)>,
    /// Goldens that were matched by at least one finding.
    pub true_positives: Vec<(GoldenComment, Finding)>,
    /// Findings that matched no golden.
    pub false_positives: Vec<Finding>,
    /// Goldens that matched no finding.
    pub false_negatives: Vec<GoldenComment>,
    /// TP / (TP + FP)
    pub precision: f64,
    /// TP / (TP + FN)
    pub recall: f64,
    /// F1 = harmonic mean of precision and recall
    pub f1: f64,
    /// Number of agent LLM calls that were cache misses (actual API calls made).
    pub agent_api_calls: usize,
    /// Number of judge LLM calls that were cache misses (actual API calls made).
    pub judge_api_calls: usize,
    /// Number of judge LLM calls that were cache hits (served from cache).
    pub judge_cache_hits: usize,
    /// Aggregate token usage from all agent API calls (real + cached).
    pub agent_usage: Usage,
    /// Aggregate token usage from all judge API calls (real + cached).
    pub judge_usage: Usage,
}

// ── Agent construction ──────────────────────────────────────────────────────

/// Build a reviewer agent for the given role.
///
/// Delegates to [`crb_agents::build_agent`] with the role's string identifier
/// and an optional rules preamble.  The returned agent should be prompted with
/// the diff to produce structured findings (parsed via `serde_json`).
///
/// `prompt_lib` and `template_vars` are forwarded to [`crb_agents::build_agent`]
/// to support file-based prompt loading and template substitution.
#[allow(clippy::too_many_arguments)]
pub fn build_reviewer_agent(
    client: &openai::Client,
    config: &ReviewerConfig,
    rules_preamble: Option<&str>,
    prompt_lib: &PromptLibrary,
    template_vars: Option<&HashMap<String, serde_json::Value>>,
    tool_preamble: Option<&str>,
    workdir: Option<&str>,
    additional_params: Option<serde_json::Value>,
    #[cfg(feature = "exp14_submit_finding")] collector: Option<
        Arc<Mutex<crb_agents::submit_finding::SubmitFindingCollector>>,
    >,
) -> Agent<ResponsesCompletionModel> {
    build_agent(
        client,
        &config.model,
        config.role.as_str(),
        rules_preamble,
        prompt_lib,
        template_vars,
        tool_preamble,
        workdir,
        additional_params,
        #[cfg(feature = "exp14_submit_finding")]
        collector,
    )
}

// ── Concurrent execution ────────────────────────────────────────────────────

/// Spawn all reviewer agents concurrently and collect their findings.
///
/// Each agent is run with a 300-second timeout.  Findings are capped at
/// `config.max_findings`.  Agents that time out or return errors yield an
/// empty finding list with a warning - no hard failure.
///
/// If `cache` is provided, uses content-addressed caching:
/// - Computes cache key from prompt_hash, diff_hash, model, role, rules_hash
/// - On cache hit: skips API call, logs "CACHE HIT", uses cached response
/// - On cache miss: makes API call, saves response, logs "CACHE MISS"
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub async fn run_reviewers(
    configs: Vec<ReviewerConfig>,
    diff: &str,
    diff_hash: &str,
    client: &openai::Client,
    rules_preamble: Option<&str>,
    prompt_lib: &PromptLibrary,
    template_vars: Option<&HashMap<String, serde_json::Value>>,
    cache: Option<Arc<dyn CacheBackend>>,
    prompt_hash: &str,
    rules_hash: &str,
    tool_preamble: Option<&str>,
    workdir: Option<&str>,
    additional_params: Option<serde_json::Value>,
    dashboard_tx: Option<tokio::sync::broadcast::Sender<crb_types::RunEvent>>,
) -> (Vec<(Role, Vec<Finding>)>, usize, Usage) {
    let mut set = JoinSet::new();
    let agent_api_calls = Arc::new(AtomicUsize::new(0));
    let aggregate_usage = Arc::new(Mutex::new(Usage::new()));

    for config in configs {
        let client = client.clone();
        let diff = diff.to_string();
        let diff_hash = diff_hash.to_string();
        let role = config.role.clone();
        let max_findings = config.max_findings;
        let preamble = rules_preamble.map(String::from);
        let tool_preamble = tool_preamble.map(String::from);
        let agent = build_reviewer_agent(
            &client,
            &config,
            preamble.as_deref(),
            prompt_lib,
            template_vars,
            tool_preamble.as_deref(),
            workdir,
            additional_params.clone(),
            #[cfg(feature = "exp14_submit_finding")]
            None,
        );
        let cache = cache.clone();
        let prompt_hash = prompt_hash.to_string();
        let rules_hash = rules_hash.to_string();
        let model_name = config.model.clone();
        let agent_api_calls = Arc::clone(&agent_api_calls);
        let aggregate_usage = Arc::clone(&aggregate_usage);
        let dashboard_tx = dashboard_tx.clone();

        set.spawn(async move {
            let cache_key = compute_agent_cache_key(
                &prompt_hash,
                &diff_hash,
                &model_name,
                role.as_str(),
                &rules_hash,
            );

            // Check cache first
            if let Some(ref cache) = cache {
                if let Some((cached_response, cached_usage_opt)) =
                    cache.lookup_agent_by_key_with_usage(&cache_key)
                {
                    tracing::info!("CACHE HIT for role {:?} (key={})", role, &cache_key[..12]);
                    // Record usage from cache if available
                    if let Some(cached_usage) = cached_usage_opt {
                        if let Ok(mut agg) = aggregate_usage.lock() {
                            agg.input_tokens += cached_usage.input_tokens;
                            agg.output_tokens += cached_usage.output_tokens;
                            agg.total_tokens += cached_usage.total_tokens;
                            agg.cached_input_tokens += cached_usage.cached_input_tokens;
                            agg.cache_creation_input_tokens +=
                                cached_usage.cache_creation_input_tokens;
                            agg.reasoning_tokens += cached_usage.reasoning_tokens;
                            agg.tool_use_prompt_tokens += cached_usage.tool_use_prompt_tokens;
                        }
                    }
                    // Parse findings from cached response
                    let response = cached_response;
                    let preview_len = std::cmp::min(500, response.len());
                    tracing::info!(
                        "Agent cached response (first 500 chars): {}",
                        &response[..preview_len]
                    );

                    let mut findings: Vec<Finding> =
                        parse_findings_from_response(&response, &role, "CACHED");
                    if findings.len() > max_findings {
                        tracing::warn!(
                            "Role {:?} produced {} findings (cached), capping at {}",
                            role,
                            findings.len(),
                            max_findings,
                        );
                        findings.truncate(max_findings);
                    }
                    return (role, findings);
                }
            }

            // Cache miss - make the API call
            agent_api_calls.fetch_add(1, Ordering::SeqCst);
            tracing::info!("CACHE MISS for role {:?} (key={})", role, &cache_key[..12]);

            // Connect the turn-budget hook that nudges the model to stop
            // exploring and produce JSON findings before max_turns is reached.
            let turn_budget_hook = TurnBudgetHook::new(agent.default_max_turns.unwrap_or(6));

            // Clone role for async block capture (Role no longer Copy)
            let role_async = role.clone();
            let outcome = tokio::time::timeout(Duration::from_secs(900), async {
                use futures::StreamExt;
                use rig_core::agent::MultiTurnStreamItem;

                let role = role_async;

                // Start streaming the agent response
                let mut stream = agent.stream_prompt(&diff).with_hook(turn_budget_hook).await;

                let mut response = String::new();
                let mut usage = Usage::new();
                let mut reasoning_text: Option<String> = None;

                while let Some(item) = stream.next().await {
                    match item.map_err(|e| anyhow::anyhow!("{e}"))? {
                        MultiTurnStreamItem::StreamAssistantItem(
                            rig_core::streaming::StreamedAssistantContent::Text(text),
                        ) => {
                            let chunk = text.text;
                            response.push_str(&chunk);
                            if let Some(ref tx) = dashboard_tx {
                                let _ = tx.send(crb_types::RunEvent::AgentChunk {
                                    role: role.to_string(),
                                    chunk: chunk.clone(),
                                });
                            }
                        }
                        MultiTurnStreamItem::CompletionCall(call) => {
                            // Accumulate per-completion-call usage
                            if call.usage.input_tokens > 0 || call.usage.output_tokens > 0 {
                                usage.input_tokens += call.usage.input_tokens;
                                usage.output_tokens += call.usage.output_tokens;
                                usage.total_tokens += call.usage.total_tokens;
                                usage.cached_input_tokens += call.usage.cached_input_tokens;
                                usage.cache_creation_input_tokens +=
                                    call.usage.cache_creation_input_tokens;
                                usage.reasoning_tokens += call.usage.reasoning_tokens;
                                usage.tool_use_prompt_tokens += call.usage.tool_use_prompt_tokens;
                            }
                        }
                        MultiTurnStreamItem::FinalResponse(final_resp) => {
                            // Use aggregated usage from final response
                            let final_usage = final_resp.usage();
                            if usage.input_tokens == 0 && usage.output_tokens == 0 {
                                usage = final_usage;
                            }
                            // If no text was streamed, use final response text
                            if response.is_empty() {
                                response = final_resp.response().to_string();
                            }

                            // Extract reasoning from chat history
                            reasoning_text = final_resp.history().and_then(|msgs| {
                                let mut reasoning = String::new();
                                for msg in msgs {
                                    if let Message::Assistant { content, .. } = msg {
                                        for item in content.iter() {
                                            if let AssistantContent::Reasoning(r) = item {
                                                use std::fmt::Write;
                                                let _ = write!(reasoning, "{}", r.display_text());
                                            }
                                        }
                                    }
                                }
                                if reasoning.is_empty() {
                                    None
                                } else {
                                    Some(reasoning)
                                }
                            });
                        }
                        _ => {}
                    }
                }

                // Save reasoning to cache if available (after stream completes)
                if let (Some(cache), Some(reasoning)) = (&cache, &reasoning_text) {
                    cache.save_agent_reasoning_with_key(&cache_key, role.as_str(), reasoning);
                }

                // Record usage in aggregate
                if let Ok(mut agg) = aggregate_usage.lock() {
                    agg.input_tokens += usage.input_tokens;
                    agg.output_tokens += usage.output_tokens;
                    agg.total_tokens += usage.total_tokens;
                    agg.cached_input_tokens += usage.cached_input_tokens;
                    agg.cache_creation_input_tokens += usage.cache_creation_input_tokens;
                    agg.reasoning_tokens += usage.reasoning_tokens;
                    agg.tool_use_prompt_tokens += usage.tool_use_prompt_tokens;
                }

                // Cache the response + usage if cache is active
                if let Some(ref cache) = cache {
                    cache.save_agent_with_key_and_usage(
                        &cache_key,
                        role.as_str(),
                        &diff,
                        &response,
                        &usage,
                    );
                }

                // Log raw response for debugging
                let preview_len = std::cmp::min(500, response.len());
                tracing::info!(
                    "Agent raw response (first 500 chars): {}",
                    &response[..preview_len]
                );

                let mut findings: Vec<Finding> = parse_findings_from_response(&response, &role, "");
                if findings.len() > max_findings {
                    tracing::warn!(
                        "Role {:?} produced {} findings, capping at {}",
                        role,
                        findings.len(),
                        max_findings,
                    );
                    findings.truncate(max_findings);
                }
                Ok::<_, anyhow::Error>((role, findings))
            })
            .await;

            match outcome {
                Ok(Ok(pair)) => pair,
                Ok(Err(e)) => {
                    // Check for MaxTurnsError - the model may have produced
                    // text findings that were cut off by the turn limit.
                    if let Some(PromptError::MaxTurnsError { chat_history, .. }) =
                        e.downcast_ref::<PromptError>()
                    {
                        if let Some(text) = extract_last_assistant_text(chat_history) {
                            tracing::info!(
                                "Role {:?} hit MaxTurnsError but chat_history contains text \
                                 - attempting to recover findings",
                                role,
                            );
                            let preview_len = std::cmp::min(500, text.len());
                            tracing::info!(
                                "Recovered text (first 500 chars): {}",
                                &text[..preview_len]
                            );

                            // Attempt to parse findings from the recovered text
                            let mut findings: Vec<Finding> =
                                parse_findings_from_response(&text, &role, "");
                            if findings.len() > max_findings {
                                findings.truncate(max_findings);
                            }
                            if !findings.is_empty() {
                                return (role, findings);
                            }
                        }
                    }
                    tracing::warn!("Role {:?} agent failed: {e}", role);
                    (role, Vec::new())
                }
                Err(_) => {
                    tracing::warn!("Role {:?} timed out after 300s", role);
                    (role, Vec::new())
                }
            }
        });
    }

    let mut results: Vec<(Role, Vec<Finding>)> = Vec::new();
    while let Some(res) = set.join_next().await {
        match res {
            Ok(pair) => results.push(pair),
            Err(e) => tracing::warn!("Agent join error: {e}"),
        }
    }
    // Sort by role for deterministic ordering - JoinSet::join_next()
    // returns tasks in completion order, which is non-deterministic.
    results.sort_by(|a, b| a.0.cmp(&b.0));
    let aggregate_usage = *aggregate_usage.lock().unwrap_or_else(|e| e.into_inner());
    (
        results,
        agent_api_calls.load(Ordering::SeqCst),
        aggregate_usage,
    )
}

// ── Judging a golden comment ─────────────────────────────────────────────────

/// Judge a single golden comment against a set of candidate findings using
/// an **LLM-as-judge first, Jaccard word-overlap fallback** pipeline (matching
/// the Python step3_judge_comments.py order).
///
/// **Algorithm:**
/// 1. **Pre-filter** candidates by exact `file` + `line` match (fast, cheap).
/// 2. **LLM judge** — for each pre-filtered candidate, ask the judge agent
///    whether the finding matches the golden.  Uses content-addressed caching
///    (via `cache` / `judge_prompt_hash` / `judge_model`) to avoid redundant
///    API calls.  Returns `TruePositive` on the **first** LLM match.
/// 3. **Jaccard fallback** — if the LLM found no match, run Jaccard word-overlap
///    with threshold **0.3** (matching Python).  Returns `TruePositive` on the
///    **first** candidate scoring ≥ 0.3.
/// 4. **FalseNegative** — no candidate matched.
#[allow(clippy::too_many_arguments, clippy::cognitive_complexity)]
pub async fn judge_comment(
    golden: &GoldenComment,
    candidates: &[Finding],
    judge: &Agent<ResponsesCompletionModel>,
    judge_model: &str,
    cache: Option<Arc<dyn CacheBackend>>,
    judge_prompt_hash: &str,
    judge_api_calls: &mut usize,
    judge_cache_hits: &mut usize,
) -> MatchResult {
    // Step 1: pre-filter candidates by exact file + line match
    let file_matches: Vec<&Finding> = candidates
        .iter()
        .filter(|f| golden.matches_candidate(f))
        .collect();

    if file_matches.is_empty() {
        return MatchResult::FalseNegative;
    }

    // Step 2: LLM judge on each pre-filtered candidate (with cache)
    for finding in &file_matches {
        let judge_key = compute_judge_cache_key(
            judge_prompt_hash,
            &finding.message,
            &golden.message_regex,
            judge_model,
        );

        // Check judge cache first
        if let Some(ref c) = cache {
            if let Some(cached_verdict) = c.lookup_judge_by_key(&judge_key) {
                tracing::info!("CACHE HIT for judge (key={})", &judge_key[..12]);
                *judge_cache_hits += 1;
                if cached_verdict.match_ {
                    return MatchResult::TruePositive;
                }
                // Cache says no match — skip this finding
                continue;
            }
        }

        // Cache miss — make the API call
        tracing::info!("CACHE MISS for judge (key={})", &judge_key[..12]);
        *judge_api_calls += 1;
        match run_judge(judge, &golden.message_regex, &finding.message).await {
            Ok((verdict, _usage)) => {
                // Write-through cache
                if let Some(ref c) = cache {
                    let verdict_json = serde_json::to_string(&verdict).unwrap_or_default();
                    c.save_judge_with_key(
                        &judge_key,
                        &golden.message_regex,
                        &finding.message,
                        &verdict_json,
                    );
                }
                if verdict.match_ {
                    return MatchResult::TruePositive;
                }
            }
            Err(e) => {
                tracing::warn!("Judge call failed: {e}");
            }
        }
    }

    // Step 3: LLM missed — try Jaccard word-overlap fallback (threshold 0.3)
    for finding in &file_matches {
        if jaccard_similarity(&finding.message, &golden.message_regex, false) >= 0.3 {
            return MatchResult::TruePositive;
        }
    }

    // Step 4: no match at all
    MatchResult::FalseNegative
}

// ── Full pipeline ───────────────────────────────────────────────────────────

/// Run the full multi-agent consensus pipeline.
///
/// 1. Concurrently run all reviewer agents via [`run_reviewers`].
/// 2. For each golden comment, attempt heuristic matching ([`judge_comment`])
///    against all findings.
/// 3. Goldens that do not match heuristically fall back to the LLM judge.
/// 4. Remaining unmatched findings are classified as false positives.
/// 5. Compute precision / recall / F1 metrics.
///
/// If `cache` is provided, agent interactions and judge calls are cached
/// using content-addressed keys derived from prompt hashes, diff hash, etc.
#[allow(clippy::too_many_arguments)]
pub async fn run_consensus(
    diff: &str,
    goldens: Vec<GoldenComment>,
    reviewer_configs: Vec<ReviewerConfig>,
    client: &openai::Client,
    judge: &Agent<ResponsesCompletionModel>,
    rules_preamble: Option<&str>,
    prompt_lib: &PromptLibrary,
    template_vars: Option<&HashMap<String, serde_json::Value>>,
    cache: Option<Arc<dyn CacheBackend>>,
    diff_hash: &str,
    prompt_hash: &str,
    rules_hash: &str,
    judge_prompt_hash: &str,
    judge_model: &str,
    tool_preamble: Option<&str>,
    workdir: Option<&str>,
    additional_params: Option<serde_json::Value>,
    dashboard_tx: Option<tokio::sync::broadcast::Sender<crb_types::RunEvent>>,
) -> ConsensusReport {
    // Step 1: run all reviewers concurrently with content-addressed caching
    let (agents, agent_api_calls, agent_usage) = run_reviewers(
        reviewer_configs,
        diff,
        diff_hash,
        client,
        rules_preamble,
        prompt_lib,
        template_vars,
        cache.clone(),
        prompt_hash,
        rules_hash,
        tool_preamble,
        workdir,
        additional_params,
        dashboard_tx,
    )
    .await;

    // Track aggregate judge usage
    let judge_usage = Usage::new();

    // Flatten all findings into a single mutable pool, sorted for determinism
    let mut unmatched: Vec<Finding> = agents
        .iter()
        .flat_map(|(_, findings)| findings.iter())
        .cloned()
        .collect();
    unmatched.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.message.cmp(&b.message))
    });

    let mut true_positives: Vec<(GoldenComment, Finding)> = Vec::new();
    let mut false_negatives: Vec<GoldenComment> = Vec::new();
    let mut judge_api_calls: usize = 0;
    let mut judge_cache_hits: usize = 0;

    // Step 2 & 3: match each golden with LLM → Jaccard pipeline
    for golden in &goldens {
        let result = judge_comment(
            golden,
            &unmatched,
            judge,
            judge_model,
            cache.clone(),
            judge_prompt_hash,
            &mut judge_api_calls,
            &mut judge_cache_hits,
        )
        .await;

        match result {
            MatchResult::TruePositive => {
                // Remove the first file+line matched finding from the pool
                // (judge_comment returns on the first match, so the first
                // candidate in iteration order is the one that was matched).
                if let Some(idx) = unmatched.iter().position(|f| golden.matches_candidate(f)) {
                    let matched = unmatched.remove(idx);
                    true_positives.push((golden.clone(), matched));
                }
            }
            MatchResult::FalseNegative => {
                false_negatives.push(golden.clone());
            }
            MatchResult::FalsePositive => {
                // This variant isn't returned by judge_comment (it checks a golden
                // against candidates, so it only yields TP or FN).  Defensively
                // treat as FN.
                false_negatives.push(golden.clone());
            }
        }
    }

    // Step 4: whatever remains in unmatched are false positives
    let false_positives = unmatched;

    // Step 5: compute metrics
    let tp = true_positives.len();
    let fp = false_positives.len();
    let fn_count = false_negatives.len();

    let precision = if tp + fp > 0 {
        tp as f64 / (tp + fp) as f64
    } else if goldens.is_empty() {
        // No goldens and no findings -> perfect by definition
        1.0
    } else {
        0.0
    };

    let recall = if tp + fn_count > 0 {
        tp as f64 / (tp + fn_count) as f64
    } else {
        1.0
    };

    let f1 = if (precision + recall) > 0.0 {
        2.0 * precision * recall / (precision + recall)
    } else {
        0.0
    };

    ConsensusReport {
        agents,
        true_positives,
        false_positives,
        false_negatives,
        precision,
        recall,
        f1,
        agent_api_calls,
        judge_api_calls,
        judge_cache_hits,
        agent_usage,
        judge_usage,
    }
}

// ── Harness integration ────────────────────────────────────────────────────

/// Convenience function that matches the existing `evaluate_pr()` signature in
/// `crb-harness` but uses the full consensus pipeline internally.
///
/// Bridges between `crb-reporting`'s [`GoldenCommentEntry`] / [`PrResult`] types
/// and the consensus crate's richer golden-comment model so it can serve as a
/// drop-in replacement for the single-agent evaluation.
///
/// Because `crb-reporting::GoldenComment` lacks `file` / `line` fields, the
/// conversion uses an empty file, line 0, and the comment text wrapped in
/// [`regex::escape`] as the message regex.
///
/// If `cache` is provided, agent interactions and judge calls are cached.
#[allow(clippy::too_many_arguments)]
pub async fn evaluate_pr_with_consensus(
    pr: &GoldenCommentEntry,
    diff: &str,
    client: &openai::Client,
    model: &str,
    judge: &Agent<ResponsesCompletionModel>,
    rules_preamble: Option<&str>,
    prompt_lib: &PromptLibrary,
    template_vars: Option<&HashMap<String, serde_json::Value>>,
    roles: &[&str],
    max_findings: usize,
    cache: Option<Arc<dyn CacheBackend>>,
    // Content-addressed cache key components
    diff_hash: &str,
    prompt_hash: &str,
    rules_hash: &str,
    judge_prompt_hash: &str,
    judge_model: &str,
    tool_preamble: Option<&str>,
    workdir: Option<&str>,
    additional_params: Option<serde_json::Value>,
    dashboard_tx: Option<tokio::sync::broadcast::Sender<crb_types::RunEvent>>,
) -> Result<(PrResult, Usage, Usage, usize, usize, usize)> {
    // ── Adaptive agent dispatch (EXP-016) ──────────────────────────────
    #[cfg(feature = "exp16_adaptive_agents")]
    let roles: Vec<&str> = {
        if should_use_single_agent(diff, 3, 200) {
            tracing::info!(
                "EXP-016: adaptive dispatch — small PR detected, using single GEN agent"
            );
            vec!["GEN"]
        } else {
            roles.to_vec()
        }
    };
    #[cfg(not(feature = "exp16_adaptive_agents"))]
    let roles = roles;

    // Build one reviewer config per selected role.
    let reviewer_configs: Vec<ReviewerConfig> = roles
        .iter()
        .map(|role_str| ReviewerConfig {
            role: Role(role_str.to_string()),
            model: model.to_string(),
            max_findings,
        })
        .collect();

    // Convert crb-reporting GoldenComments to consensus GoldenComments.
    // crb-reporting's GoldenComment lacks file/line, so we use
    // empty file + line 0 and escape the comment text as a regex.
    let consensus_goldens: Vec<GoldenComment> = pr
        .comments
        .iter()
        .map(|gc| GoldenComment {
            file: String::new(),
            line: 0,
            message_regex: regex::escape(&gc.comment),
            severity: gc.severity.clone(),
            source: "any".to_string(),
        })
        .collect();

    let report = run_consensus(
        diff,
        consensus_goldens,
        reviewer_configs,
        client,
        judge,
        rules_preamble,
        prompt_lib,
        template_vars,
        cache,
        diff_hash,
        prompt_hash,
        rules_hash,
        judge_prompt_hash,
        judge_model,
        tool_preamble,
        workdir,
        additional_params,
        dashboard_tx,
    )
    .await;
    // Build verdicts for compatibility with crb-reporting::PrResult.
    let mut verdicts = Vec::new();
    for _ in &report.true_positives {
        verdicts.push(JudgeVerdict {
            reasoning: "Matched via heuristic or LLM judge".into(),
            match_: true,
            confidence: 1.0,
        });
    }
    for _ in &report.false_positives {
        verdicts.push(JudgeVerdict {
            reasoning: "No matching golden comment".into(),
            match_: false,
            confidence: 0.0,
        });
    }

    let total_findings: usize = report
        .agents
        .iter()
        .map(|(_, findings)| findings.len())
        .sum();

    Ok((
        PrResult {
            pr_title: pr.pr_title.clone(),
            url: pr.url.clone(),
            findings_count: total_findings,
            golden_count: pr.comments.len(),
            metrics: crb_judge::Metrics {
                true_positives: report.true_positives.len(),
                false_positives: report.false_positives.len(),
                false_negatives: report.false_negatives.len(),
                precision: report.precision,
                recall: report.recall,
                f1: report.f1,
            },
            verdicts,
            cost: None,
        },
        report.agent_usage,
        report.judge_usage,
        report.agent_api_calls,
        report.judge_api_calls,
        report.judge_cache_hits,
    ))
}

// ── Adaptive agent dispatch (EXP-016) ───────────────────────────────────────

/// Languages that always trigger the full 4-agent panel regardless of PR size.
const FULL_PANEL_LANGUAGES: &[&str] = &[
    ".go", ".rs", ".java", ".cpp", ".cc", ".cxx", ".c", ".ts", ".tsx",
];

/// Determine whether the given diff touches any of the full-panel languages.
///
/// Scans each `diff --git` line for file paths ending with one of the
/// [`FULL_PANEL_LANGUAGES`] extensions (Go, Rust, Java, C++, C, TypeScript).
pub fn diff_touches_full_panel_languages(diff: &str) -> bool {
    for line in diff.lines() {
        if line.starts_with("diff --git ") {
            // Format: diff --git a/path b/path
            // We extract the "b/" path
            if let Some(bpath) = line.rsplit(' ').next() {
                let bpath = bpath.trim();
                if let Some(ext_start) = bpath.rfind('.') {
                    let ext = &bpath[ext_start..];
                    if FULL_PANEL_LANGUAGES.contains(&ext) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Parse a unified diff to count the number of changed files.
pub fn count_diff_files(diff: &str) -> usize {
    diff.lines()
        .filter(|line| line.starts_with("diff --git "))
        .count()
}

/// Parse a unified diff to count the total number of changed lines (additions
/// and deletions, excluding `---`/`+++` hunk headers and `diff --git` lines).
pub fn count_diff_lines(diff: &str) -> usize {
    diff.lines()
        .filter(|line| {
            let trimmed = line.trim();
            // Count lines starting with + or - but not +++/---
            (trimmed.starts_with('+') || trimmed.starts_with('-'))
                && !trimmed.starts_with("+++")
                && !trimmed.starts_with("---")
        })
        .count()
}

/// Decide whether a single GEN agent should be used for this diff.
///
/// Returns `true` (single GEN agent) when:
/// - File count ≤ `max_files`
/// - Total changed lines ≤ `max_lines`
/// - The diff does NOT touch any full-panel languages
///
/// Returns `false` (full 4-agent panel) otherwise.
#[allow(clippy::cognitive_complexity)]
pub fn should_use_single_agent(diff: &str, max_files: usize, max_lines: usize) -> bool {
    let file_count = count_diff_files(diff);
    let line_count = count_diff_lines(diff);

    tracing::debug!(
        "Adaptive dispatch: {} files, {} changed lines (threshold: {} files / {} lines)",
        file_count,
        line_count,
        max_files,
        max_lines,
    );

    // Safety override: full panel for complex languages
    if diff_touches_full_panel_languages(diff) {
        tracing::debug!(
            "Adaptive dispatch: full panel forced (diff touches safety-override language)"
        );
        return false;
    }

    // Small PR: single GEN agent
    if file_count <= max_files && line_count <= max_lines {
        tracing::debug!("Adaptive dispatch: using single GEN agent (small PR)");
        return true;
    }

    tracing::debug!("Adaptive dispatch: using full 4-agent panel (complex PR)");
    false
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use crb_shared::jaccard::jaccard_similarity;

    use super::*;

    /// Build a minimal single-hunk diff for the given file path and content.
    /// Content should include the `-` and `+` prefix lines (e.g. "-old\\n+new\\n").
    fn minimal_diff(file_path: &str, content: &str) -> String {
        format!(
            "\
diff --git a/{fp} b/{fp}
--- a/{fp}
+++ b/{fp}
@@ -1 +1 @@
{}",
            fp = file_path,
            content = content
        )
    }

    /// Shorthand for `minimal_diff("src/main.rs", content)`.
    fn diff_main(content: &str) -> String {
        minimal_diff("src/main.rs", content)
    }

    /// Assert that the precision, recall, and F1 metrics of a report
    /// are all equal to the given expected value (within 1e-6).
    fn assert_metrics(report: &ConsensusReport, expected: f64) {
        let eps = 1e-6;
        assert!((report.precision - expected).abs() < eps);
        assert!((report.recall - expected).abs() < eps);
        assert!((report.f1 - expected).abs() < eps);
    }

    #[test]
    fn test_role_as_str() {
        assert_eq!(Role("SA".into()).as_str(), "SA");
        assert_eq!(Role("CL".into()).as_str(), "CL");
        assert_eq!(Role("AR".into()).as_str(), "AR");
        assert_eq!(Role("SEC".into()).as_str(), "SEC");
    }

    #[test]
    fn test_role_variants_are_distinct() {
        assert_ne!(Role("SA".into()), Role("CL".into()));
        assert_ne!(Role("CL".into()), Role("AR".into()));
        assert_ne!(Role("AR".into()), Role("SEC".into()));
    }

    #[test]
    fn test_match_result_serialization() {
        let tp = serde_json::to_value(MatchResult::TruePositive).unwrap();
        let fp = serde_json::to_value(MatchResult::FalsePositive).unwrap();
        let fn_ = serde_json::to_value(MatchResult::FalseNegative).unwrap();
        assert!(tp.is_string());
        assert!(fp.is_string());
        assert!(fn_.is_string());
        assert_ne!(tp, fp);
        assert_ne!(fp, fn_);
    }

    #[test]
    fn test_compute_agent_cache_key_deterministic() {
        let key1 = compute_agent_cache_key("abc", "def", "gpt-4o", "SA", "rules123");
        let key2 = compute_agent_cache_key("abc", "def", "gpt-4o", "SA", "rules123");
        assert_eq!(key1, key2);
        // Different input should produce different key
        let key3 = compute_agent_cache_key("abc", "xyz", "gpt-4o", "SA", "rules123");
        assert_ne!(key1, key3);
    }

    #[test]
    fn test_compute_judge_cache_key_deterministic() {
        let key1 = compute_judge_cache_key("jph", "finding msg", "golden comment", "gpt-4o-mini");
        let key2 = compute_judge_cache_key("jph", "finding msg", "golden comment", "gpt-4o-mini");
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_sha256_hex() {
        let h = sha256_hex("hello");
        assert_eq!(h.len(), 64); // SHA256 hex is 64 chars
    }

    // ── judge_comment tests ─────────────────────────────────────────

    #[test]
    fn test_judge_comment_no_candidates() {
        // Empty candidates → no file+line match → FalseNegative
        let golden = GoldenComment {
            file: "src/main.rs".into(),
            line: 42,
            message_regex: r".*".into(),
            severity: "error".into(),
            source: "SA".into(),
        };
        // We can't call judge_comment directly in unit tests because it requires
        // a real LLM agent.  Instead we test that empty candidates produce FN.
        // The file+line pre-filter returns empty → FalseNegative.
        let candidates: Vec<Finding> = vec![];
        let file_matches: Vec<&Finding> = candidates
            .iter()
            .filter(|f| golden.matches_candidate(f))
            .collect();
        assert!(file_matches.is_empty());
    }

    // ── Jaccard tests (via crb_shared) ───────────────────────────────────

    #[test]
    fn test_jaccard_identical() {
        let score = jaccard_similarity(
            "hardcoded secret in config",
            "hardcoded secret in config",
            false,
        );
        assert!(score > 0.9);
    }

    #[test]
    fn test_jaccard_partial_overlap() {
        let score = jaccard_similarity(
            "hardcoded API key found",
            "hardcoded secret in config",
            false,
        );
        // Intersection: {"hardcoded"}, Union: 7 words → 1/7 ≈ 0.142 < 0.3
        assert!(score < 0.3);
    }

    #[test]
    fn test_jaccard_lower_threshold() {
        let score = jaccard_similarity(
            "hardcoded API key found",
            "hardcoded secret in config",
            false,
        );
        // Raw score ≈ 1/7 ≈ 0.142
        assert!((score - 1.0 / 7.0).abs() < 0.01);
    }

    #[test]
    fn test_jaccard_no_overlap() {
        let score = jaccard_similarity("null pointer check", "SQL injection vulnerability", false);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_jaccard_empty_strings() {
        assert_eq!(jaccard_similarity("", "", false), 0.0);
        assert_eq!(jaccard_similarity("hello", "", false), 0.0);
    }

    #[test]
    fn test_jaccard_case_insensitive() {
        let s1 = jaccard_similarity("SQL Injection", "sql injection", false);
        let s2 = jaccard_similarity("Sql Injection", "sql injection", false);
        assert_eq!(s1, s2);
    }

    #[test]
    fn test_consensus_report_empty() {
        // No goldens, no findings -> perfect metrics
        let report = ConsensusReport {
            agents: vec![],
            true_positives: vec![],
            false_positives: vec![],
            false_negatives: vec![],
            precision: 1.0,
            recall: 1.0,
            f1: 1.0,
            agent_api_calls: 0,
            judge_api_calls: 0,
            judge_cache_hits: 0,
            agent_usage: Usage::new(),
            judge_usage: Usage::new(),
        };
        assert_metrics(&report, 1.0);
    }

    #[test]
    fn test_consensus_report_perfect() {
        // All findings match all goldens
        let report = ConsensusReport {
            agents: vec![],
            true_positives: vec![(
                GoldenComment {
                    file: "a.rs".into(),
                    line: 1,
                    message_regex: "foo".into(),
                    severity: "error".into(),
                    source: "any".into(),
                },
                Finding {
                    file: Some("a.rs".into()),
                    line: Some(1),
                    message: "foo".into(),
                    severity: "error".into(),
                    ..Default::default()
                },
            )],
            false_positives: vec![],
            false_negatives: vec![],
            precision: 1.0,
            recall: 1.0,
            f1: 1.0,
            agent_api_calls: 0,
            judge_api_calls: 0,
            judge_cache_hits: 0,
            agent_usage: Usage::new(),
            judge_usage: Usage::new(),
        };
        assert_eq!(report.true_positives.len(), 1);
        assert_eq!(report.false_positives.len(), 0);
        assert_eq!(report.false_negatives.len(), 0);
        assert_metrics(&report, 1.0);
    }

    #[test]
    fn test_consensus_report_no_matches() {
        let report = ConsensusReport {
            agents: vec![],
            true_positives: vec![],
            false_positives: vec![Finding {
                file: Some("a.rs".into()),
                line: Some(1),
                message: "unexpected".into(),
                severity: "warning".into(),
                severity_audited: false,
                ..Default::default()
            }],
            false_negatives: vec![GoldenComment {
                file: "a.rs".into(),
                line: 1,
                message_regex: "expected".into(),
                severity: "error".into(),
                source: "any".into(),
            }],
            precision: 0.0,
            recall: 0.0,
            f1: 0.0,
            agent_api_calls: 0,
            judge_api_calls: 0,
            judge_cache_hits: 0,
            agent_usage: Usage::new(),
            judge_usage: Usage::new(),
        };
        assert_eq!(report.true_positives.len(), 0);
        assert_eq!(report.false_positives.len(), 1);
        assert_eq!(report.false_negatives.len(), 1);
        assert_metrics(&report, 0.0);
    }

    #[test]
    fn test_reviewer_config_serialization() {
        let config = ReviewerConfig {
            role: Role("SEC".into()),
            model: "gpt-4o".into(),
            max_findings: 15,
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("SEC"));
        assert!(json.contains("gpt-4o"));
        assert!(json.contains("15"));
        let deserialized: ReviewerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.role, Role("SEC".into()));
        assert_eq!(deserialized.model, "gpt-4o");
        assert_eq!(deserialized.max_findings, 15);
    }

    #[test]
    fn test_golden_comment_serialization() {
        let gc = GoldenComment {
            file: "src/lib.rs".into(),
            line: 100,
            message_regex: r"unsafe\s+fn".into(),
            severity: "warning".into(),
            source: "SEC".into(),
        };
        let json = serde_json::to_string(&gc).unwrap();
        let deserialized: GoldenComment = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.file, "src/lib.rs");
        assert_eq!(deserialized.line, 100);
    }

    #[test]
    fn test_count_diff_files_empty() {
        assert_eq!(count_diff_files(""), 0);
    }

    #[test]
    fn test_count_diff_files_single() {
        let diff = "\
diff --git a/src/main.rs b/src/main.rs
index abc..def 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,4 @@
 fn main() {
-    println!(\"hello\");
+    println!(\"hello world\");
 }
";
        assert_eq!(count_diff_files(diff), 1);
    }

    #[test]
    fn test_count_diff_files_multiple() {
        let diff = "\
diff --git a/src/main.rs b/src/main.rs
index a..b
--- a/src/main.rs
+++ b/src/main.rs
@@ -1 +1 @@
-foo
+bar
diff --git a/src/lib.rs b/src/lib.rs
index c..d
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1 +1 @@
-baz
+qux
diff --git a/Cargo.toml b/Cargo.toml
index e..f
--- a/Cargo.toml
+++ b/Cargo.toml
@@ -1 +1 @@
-old
+new
";
        assert_eq!(count_diff_files(diff), 3);
    }

    #[test]
    fn test_count_diff_lines_empty() {
        assert_eq!(count_diff_lines(""), 0);
    }

    #[test]
    fn test_count_diff_lines_counts_additions_and_deletions() {
        let diff = "\
diff --git a/src/main.rs b/src/main.rs
index abc..def 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,5 +1,6 @@
 fn main() {
-    let x = 1;
-    let y = 2;
+    let x = 10;
+    let y = 20;
+    let z = 30;
     println!(\"done\");
 }
";
        assert_eq!(count_diff_lines(diff), 5);
    }

    #[test]
    fn test_count_diff_lines_excludes_headers() {
        let diff = diff_main("-foo\n+bar\n");
        assert_eq!(count_diff_lines(diff), 2);
    }

    #[test]
    fn test_diff_touches_full_panel_languages_no_match() {
        let diff = "\
diff --git a/src/main.py b/src/main.py
diff --git a/README.md b/README.md
";
        assert!(!diff_touches_full_panel_languages(diff));
    }

    #[test]
    fn test_diff_touches_full_panel_languages_rust() {
        let diff = minimal_diff("src/main.rs", "-old\n+new\n");
        assert!(diff_touches_full_panel_languages(diff));
    }

    #[test]
    fn test_diff_touches_full_panel_languages_typescript() {
        let diff = minimal_diff("src/foo.ts", "-old\n+new\n");
        assert!(diff_touches_full_panel_languages(diff));
    }

    #[test]
    fn test_diff_touches_full_panel_languages_go() {
        let diff = minimal_diff("server.go", "-old\n+new\n");
        assert!(diff_touches_full_panel_languages(diff));
    }

    #[test]
    fn test_diff_touches_full_panel_languages_java() {
        let diff = minimal_diff("Main.java", "-old\n+new\n");
        assert!(diff_touches_full_panel_languages(diff));
    }

    #[test]
    fn test_diff_touches_full_panel_languages_cpp() {
        let diff = minimal_diff("main.cpp", "-old\n+new\n");
        assert!(diff_touches_full_panel_languages(diff));
    }

    #[test]
    fn test_should_use_single_agent_small_pr() {
        let diff = minimal_diff("README.md", "-old\n+new\n");
        assert!(should_use_single_agent(diff, 3, 200));
    }

    #[test]
    fn test_should_use_single_agent_too_many_files() {
        let file_count = 4;
        let diff = (0..file_count)
            .map(|i| {
                let fname = format!("a{}.txt", i);
                minimal_diff(&fname, "-old\n+new\n")
            })
            .collect::<Vec<_>>()
            .join("");
        assert!(!should_use_single_agent(diff, 3, 200));
    }

    #[test]
    fn test_should_use_single_agent_too_many_lines() {
        let diff = "\
diff --git a/a.txt b/a.txt
--- a/a.txt
+++ b/a.txt
@@ -1,100 +1,300 @@
"
        .to_string()
            + &(0..250)
                .map(|i| format!("+line_{}\n", i))
                .collect::<String>();
        assert!(!should_use_single_agent(&diff, 3, 200));
    }

    #[test]
    fn test_should_use_single_agent_safety_override_rust() {
        let diff = diff_main("-old\n+new\n");
        assert!(!should_use_single_agent(diff, 3, 200));
    }

    #[test]
    fn test_should_use_single_agent_safety_override_go() {
        let diff = minimal_diff("server.go", "-old\n+new\n");
        assert!(!should_use_single_agent(diff, 3, 200));
    }

    #[test]
    fn test_role_gen_variant() {
        let role = Role("GEN".into());
        assert_eq!(role.as_str(), "GEN");
    }

    #[test]
    fn test_role_gen_serialization() {
        let json = serde_json::to_string(&Role("GEN".into())).unwrap();
        assert_eq!(json, "\"GEN\"");
        let deserialized: Role = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, Role("GEN".into()));
    }
}
