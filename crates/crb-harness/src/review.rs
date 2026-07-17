use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use crb_reporting::cost::SessionUsageProvider;
use crb_shared::diff::Diff;
use crb_types::RunEvent;
use crb_types::agent::{AgentChunk, ToolByte};
use crb_types::cost::SessionUsage;
use crb_types::finding::Finding;
use crb_types::wrappers::WrappedData;
use futures::StreamExt;
use mti::prelude::MagicTypeId;
use rig_core::agent::{Agent, MultiTurnStreamItem, PromptHook};
use rig_core::completion::{CompletionModel, GetTokenUsage};
use rig_core::message::{AssistantContent, ToolResultContent};
use rig_core::streaming::{
    StreamedAssistantContent, StreamedUserContent, StreamingPrompt, ToolCallDeltaContent,
};
use serde::de::DeserializeOwned;
use tracing::{error, info};

use crate::eval::EvalConfig;
use crate::{pipeline, send_event};

pub async fn stream_agent<Output, M, P>(
    config: Arc<EvalConfig>,
    agent_id: &MagicTypeId,
    agent: &Agent<M, P>,
    prompt: &str,
) -> Result<Output>
where
    M: CompletionModel + 'static,
    P: PromptHook<M> + 'static,
    Output: DeserializeOwned,
{
    let analytics_trcker = &config.cost_tracker;
    let mut stream = agent.stream_prompt(prompt).await;
    while let Some(chunk) = stream.next().await {
        let review_id = config.review_id.clone();
        let agent_id = agent_id.clone();
        match chunk? {
            MultiTurnStreamItem::StreamAssistantItem(assistant) => match assistant {
                StreamedAssistantContent::Text(text) => {
                    send_event!(
                        config,
                        RunEvent::AgentChunk {
                            review_id,
                            chunk: AgentChunk::Output {
                                id: agent_id,
                                content: text.text,
                                last: false,
                            }
                        }
                    )
                }
                StreamedAssistantContent::ReasoningDelta { reasoning, .. } => {
                    send_event!(
                        config,
                        RunEvent::AgentChunk {
                            review_id,
                            chunk: AgentChunk::Thinking {
                                id: agent_id,
                                content: reasoning,
                                last: false,
                            }
                        }
                    )
                }
                StreamedAssistantContent::Final(r) => {
                    let analytics = r.token_usage().get_usage();
                    analytics_trcker.record(&agent_id, analytics, false).await;
                    send_event!(
                        config,
                        RunEvent::AgentFinished {
                            review_id,
                            agent_id,
                            analytics,
                        }
                    )
                }
                StreamedAssistantContent::ToolCall {
                    internal_call_id, ..
                } => {
                    let mut usage = SessionUsage::default();
                    usage.tool_use_count += 1;
                    analytics_trcker.record(&review_id, usage, false).await;

                    send_event!(
                        config,
                        RunEvent::AgentChunk {
                            review_id,
                            chunk: AgentChunk::Tool {
                                id: agent_id,
                                invocation_id: internal_call_id,
                                byte: ToolByte::End,
                                last: true
                            }
                        }
                    )
                }
                StreamedAssistantContent::ToolCallDelta {
                    internal_call_id,
                    content,
                    ..
                } => match content {
                    ToolCallDeltaContent::Name(name) => {
                        send_event!(
                            config,
                            RunEvent::AgentChunk {
                                review_id,
                                chunk: AgentChunk::Tool {
                                    id: agent_id,
                                    invocation_id: internal_call_id,
                                    byte: ToolByte::Begin(name),
                                    last: false
                                }
                            }
                        )
                    }
                    ToolCallDeltaContent::Delta(delta) => {
                        send_event!(
                            config,
                            RunEvent::AgentChunk {
                                review_id,
                                chunk: AgentChunk::Tool {
                                    id: agent_id,
                                    invocation_id: internal_call_id,
                                    byte: ToolByte::Bit(delta),
                                    last: false
                                }
                            }
                        );
                    }
                },
                StreamedAssistantContent::Reasoning(_) => {
                    send_event!(
                        config,
                        RunEvent::AgentChunk {
                            review_id,
                            chunk: AgentChunk::Thinking {
                                id: agent_id,
                                // We don't have a delta here, so we send an empty string.
                                // We already sent the reasoning delta from the `StreamedAssistantContent::ReasoningDelta` variant,
                                // so this is just to indicate that the reasoning is complete.
                                content: "".to_string(),
                                last: true,
                            }
                        }
                    )
                }
            },
            MultiTurnStreamItem::StreamUserItem(StreamedUserContent::ToolResult {
                internal_call_id,
                tool_result,
            }) => {
                send_event!(
                    config,
                    RunEvent::AgentChunk {
                        review_id,
                        chunk: AgentChunk::Tool {
                            id: agent_id,
                            invocation_id: internal_call_id,
                            byte: ToolByte::Result(
                                tool_result
                                    .content
                                    .into_iter()
                                    .filter_map(|r| match r {
                                        ToolResultContent::Text(text) => Some(text.text),
                                        ToolResultContent::Image(_) => {
                                            error!("Image tool results are not supported in the review pipeline");
                                            None
                                        }
                                    })
                                    .collect()
                            ),
                            last: true,
                        }
                    }
                )
            }
            MultiTurnStreamItem::FinalResponse(response) => {
                // I think all we need is the last text ? or will the others contain more outputs, idk, needs testing.
                let raw_content = response
                    .content()
                    .iter()
                    .filter(|message| matches!(message, AssistantContent::Text(_)))
                    .last();
                let Some(AssistantContent::Text(text)) = raw_content else {
                    error!(
                        "No text content found in final response for agent {} during review {}",
                        agent_id, config.review_id
                    );
                    continue;
                };
                let serde = serde_json::from_str::<Output>(&text.text);
                match serde {
                    Ok(output) => return Ok(output),
                    Err(e) => {
                        error!(
                            "Failed to deserialize final response for agent {} during review {}: {}",
                            agent_id, config.review_id, e
                        );
                        continue;
                    }
                }
            }
            _ => {}
        }
    }

    Err(anyhow!(
        "Stream ended without final response for agent {} during review {}",
        agent_id,
        config.review_id
    ))
}

/// Review a PR diff.
pub async fn review_pr(diff: Diff, config: &EvalConfig) -> Result<Vec<Finding>> {
    info!(
        "Reviewing diff ({} bytes, {} sections) with {} agents, model={}",
        diff.raw.len(),
        diff.sections.len(),
        config.agents.len(),
        config.model.get(),
    );

    let findings = pipeline::evaluate(diff, config)
        .await
        .context("Pipeline evaluation failed")?;

    info!("Review complete: {} findings", findings.len());
    Ok(findings)
}

/// Build an `EvalConfig` from `ReviewArgs` for a one-shot review.
#[cfg(feature = "binary")]
pub fn build_review_config(args: &crate::config::ReviewArgs) -> Result<EvalConfig> {
    use crb_agents::AgentEntry;
    use crb_reporting::cost::AnalyticsTracker;
    use crb_types::wrappers::Model;
    use rig_core::agent::Agent;
    use rig_core::client::{CompletionClient, ProviderClient};
    use rig_core::providers::openrouter;
    use rig_core::providers::openrouter::responses_api::ResponsesCompletionModel;
    use rig_core::tool::server::ToolServer;

    let client = Arc::new(client);

    let tool_server = ToolServer::new().run();

    let agents: Vec<&'static AgentEntry> = match &args.roles {
        Some(abbrevs) => {
            let lib = crb_agents::prompts::PromptLibrary::get_instance();
            abbrevs
                .iter()
                .filter_map(|a| lib.config(a.trim()))
                .collect()
        }
        None => crb_agents::prompts::PromptLibrary::get_instance()
            .agents()
            .into_iter()
            .collect(),
    };
    // Leak to get 'static lifetime required by EvalConfig.agents
    let agents: &'static [&'static AgentEntry] = Box::leak(agents.into_boxed_slice());

    let cost_tracker = Arc::new(AnalyticsTracker::new());

    let model = Model(args.model.clone());

    Ok(EvalConfig {
        strategy: crate::eval::EvalStrategy::Panel,
        identifier: "review-cli".to_string(),
        model,
        reasoning_effort: None,
        client,
        cache: None,
        cost_tracker,
        tool_handle: tool_server,
        dashboard_tx: None,
        agents,
        repo_root: args.path.clone(),
        max_findings: args.max_findings,
        linter_configs: None,
        ruleset: None,
        template_vars: None,
    })
}
