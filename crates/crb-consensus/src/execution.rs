use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crb_agents::{build_agent, prompts::PromptLibrary};
use crb_cache::sha256::sha256_hex;
use crb_shared::finding::Finding;
use crb_types::wrappers::Model;
use rig_core::completion::{AssistantContent, Message, PromptError, Usage};
use rig_core::providers::openai;
use rig_core::streaming::StreamingPrompt;
use rig_core::tool::server::ToolServerHandle;
use serde::{Deserialize, Serialize};
use tokio::task::JoinSet;
use tracing::{info, warn};

use crate::agent::TurnBudgetHook;
use crate::{Role, extract_last_assistant_text, parse_findings_from_response};
use crb_cache::traits::CacheBackend;

/// Compute a content-addressed cache key for an agent review call.
#[deprecated = "Use new cache system instead."]
fn compute_agent_cache_key(
    prompt_hash: &str,
    diff_hash: &str,
    model: &str,
    role: &str,
    rules_hash: &str,
) -> String {
    sha256_hex(&format!(
        "{prompt_hash}{diff_hash}{model}{role}{rules_hash}"
    ))
}

/// Serializable snapshot of agent cache data.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[deprecated = "Use new cache system instead."]
struct CachedAgentData {
    response: String,
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    total_tokens: u64,
    #[serde(default)]
    cached_input_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
    #[serde(default)]
    reasoning_tokens: u64,
    #[serde(default)]
    tool_use_prompt_tokens: u64,
}

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
#[deprecated = "needs full rewrite to use new cache, agent builder, and apis."]
pub async fn run_reviewers(
    configs: Vec<(Role, String, usize)>,
    diff: &str,
    diff_hash: &str,
    client: &openai::Client,
    rules_preamble: Option<&str>,
    template_vars: Option<&std::collections::HashMap<String, serde_json::Value>>,
    cache: Option<Arc<dyn CacheBackend>>,
    prompt_hash: &str,
    rules_hash: &str,
    tool_preamble: Option<&str>,
    additional_params: Option<serde_json::Value>,
    dashboard_tx: Option<tokio::sync::broadcast::Sender<crb_types::RunEvent>>,
    tool_server_handle: ToolServerHandle,
) -> (Vec<(Role, Vec<Finding>)>, usize, Usage) {
    let mut set = JoinSet::new();
    let agent_api_calls = Arc::new(AtomicUsize::new(0));
    let aggregate_usage = Arc::new(Mutex::new(Usage::new()));

    for (role, model, max_findings) in configs {
        let client = client.clone();
        let diff = diff.to_string();
        let diff_hash = diff_hash.to_string();
        let preamble = rules_preamble.map(String::from);
        let tool_preamble = tool_preamble.map(String::from);
        let agent = build_agent(
            &client,
            &Model(model.clone()),
            PromptLibrary::get_instance().config(role.as_str()).unwrap(),
            preamble.as_deref(),
            template_vars,
            tool_preamble.as_deref(),
            additional_params.clone(),
            tool_server_handle.clone(),
        );

        let cache = cache.clone();
        let prompt_hash = prompt_hash.to_string();
        let rules_hash = rules_hash.to_string();
        let model_name = model.clone();
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

            if let Some(ref cache) = cache {
                let raw = cache.load_raw(&cache_key);
                if !raw.is_empty() {
                    if let Ok(cached) = serde_json::from_str::<CachedAgentData>(&raw) {
                        info!("CACHE HIT for role {:?} (key={})", role, &cache_key[..12]);
                        if cached.input_tokens > 0 || cached.output_tokens > 0 {
                            if let Ok(mut agg) = aggregate_usage.lock() {
                                agg.input_tokens += cached.input_tokens;
                                agg.output_tokens += cached.output_tokens;
                                agg.total_tokens += cached.total_tokens;
                                agg.cached_input_tokens += cached.cached_input_tokens;
                                agg.cache_creation_input_tokens +=
                                    cached.cache_creation_input_tokens;
                                agg.reasoning_tokens += cached.reasoning_tokens;
                                agg.tool_use_prompt_tokens += cached.tool_use_prompt_tokens;
                            }
                        }

                        let response = &cached.response;
                        let preview_len = std::cmp::min(500, response.len());
                        info!(
                            "Agent cached response (first 500 chars): {}",
                            &response[..preview_len]
                        );

                        let mut findings: Vec<Finding> =
                            parse_findings_from_response(response, &role, "CACHED");
                        if findings.len() > max_findings {
                            warn!(
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
            }

            agent_api_calls.fetch_add(1, Ordering::SeqCst);
            info!("CACHE MISS for role {:?} (key={})", role, &cache_key[..12]);

            let turn_budget_hook = TurnBudgetHook::new(6);

            let role_async = role.clone();
            let outcome = tokio::time::timeout(Duration::from_secs(900), async {
                use futures::StreamExt;
                use rig_core::agent::MultiTurnStreamItem;

                let role = role_async;
                let mut stream = agent
                    .build()
                    .stream_prompt(&diff)
                    .with_hook(turn_budget_hook)
                    .await;
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
                                // Ignore; receiver may have disconnected
                                let _ = tx.send(crb_types::RunEvent::AgentChunk {
                                    identifier: role.to_string(),
                                    chunk: chunk.clone(),
                                });
                            }
                        }

                        MultiTurnStreamItem::CompletionCall(call) => {
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
                            let final_usage = final_resp.usage();
                            if usage.input_tokens == 0 && usage.output_tokens == 0 {
                                usage = final_usage;
                            }

                            if response.is_empty() {
                                response = final_resp.response().to_string();
                            }

                            reasoning_text = final_resp.history().and_then(|msgs| {
                                let mut reasoning = String::new();
                                for msg in msgs {
                                    if let Message::Assistant { content, .. } = msg {
                                        for item in content.iter() {
                                            if let AssistantContent::Reasoning(r) = item {
                                                use std::fmt::Write;
                                                let _ = write!(reasoning, "{}", r.display_text()); // Ignore; write! to String is infallible
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

                if let (Some(cache), Some(reasoning)) = (&cache, &reasoning_text) {
                    let reasoning_key = format!("{}_reasoning", cache_key);
                    cache.store_raw(&reasoning_key, reasoning);
                }

                if let Ok(mut agg) = aggregate_usage.lock() {
                    agg.input_tokens += usage.input_tokens;
                    agg.output_tokens += usage.output_tokens;
                    agg.total_tokens += usage.total_tokens;
                    agg.cached_input_tokens += usage.cached_input_tokens;
                    agg.cache_creation_input_tokens += usage.cache_creation_input_tokens;
                    agg.reasoning_tokens += usage.reasoning_tokens;
                    agg.tool_use_prompt_tokens += usage.tool_use_prompt_tokens;
                }

                if let Some(ref cache) = cache {
                    let cached = CachedAgentData {
                        response: response.clone(),
                        input_tokens: usage.input_tokens,
                        output_tokens: usage.output_tokens,
                        total_tokens: usage.total_tokens,
                        cached_input_tokens: usage.cached_input_tokens,
                        cache_creation_input_tokens: usage.cache_creation_input_tokens,
                        reasoning_tokens: usage.reasoning_tokens,
                        tool_use_prompt_tokens: usage.tool_use_prompt_tokens,
                    };
                    if let Ok(json) = serde_json::to_string(&cached) {
                        cache.store_raw(&cache_key, &json);
                    }
                }

                let preview_len = std::cmp::min(500, response.len());
                info!(
                    "Agent raw response (first 500 chars): {}",
                    &response[..preview_len]
                );

                let mut findings: Vec<Finding> = parse_findings_from_response(&response, &role, "");
                if findings.len() > max_findings {
                    warn!(
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
                    // Check for MaxTurnsError, the model may have produced text findings that were cut off by the turn limit.
                    if let Some(PromptError::MaxTurnsError { chat_history, .. }) =
                        e.downcast_ref::<PromptError>()
                    {
                        if let Some(text) = extract_last_assistant_text(chat_history) {
                            info!(
                                "Role {:?} hit MaxTurnsError but chat_history contains text \
                                 - attempting to recover findings",
                                role,
                            );
                            let preview_len = std::cmp::min(500, text.len());
                            info!("Recovered text (first 500 chars): {}", &text[..preview_len]);

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
                    warn!("Role {:?} agent failed: {e}", role);
                    (role, Vec::new())
                }
                Err(_) => {
                    warn!("Role {:?} timed out after 300s", role);
                    (role, Vec::new())
                }
            }
        });
    }

    let mut results: Vec<(Role, Vec<Finding>)> = Vec::new();
    while let Some(res) = set.join_next().await {
        match res {
            Ok(pair) => results.push(pair),
            Err(e) => warn!("Agent join error: {e}"),
        }
    }

    // Sort by role for deterministic ordering
    // returns tasks in completion order, which is non-deterministic.
    results.sort_by(|a, b| a.0.cmp(&b.0));
    let aggregate_usage = *aggregate_usage.lock().unwrap_or_else(|e| e.into_inner());
    (
        results,
        agent_api_calls.load(Ordering::SeqCst),
        aggregate_usage,
    )
}
