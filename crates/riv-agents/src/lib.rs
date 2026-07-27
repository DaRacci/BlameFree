//! Agent orchestration and prompt library.

use anyhow::{Result, anyhow};
use riv_reporting::cost::{AnalyticsTracker, SessionUsageProvider};
use riv_types::RunEvent;
use riv_types::agent::{AgentChunk, ToolByte};
use riv_types::cost::SessionUsage;
use riv_types::wrappers::{Model, WrappedData};
use futures::StreamExt;
use mti::prelude::MagicTypeId;
use rig_core::agent::{Agent, AgentBuilder, MultiTurnStreamItem, WithToolServerHandle};
use rig_core::client::{Client, CompletionClient};
use rig_core::completion::{CompletionModel, GetTokenUsage};
use rig_core::message::{AssistantContent, ToolResultContent};
use rig_core::providers::openrouter;
use rig_core::streaming::{
    StreamedAssistantContent, StreamedUserContent, StreamingPrompt, ToolCallDeltaContent,
};
use rig_core::tool::server::ToolServerHandle;
use serde::de::DeserializeOwned;
use tokio::sync::broadcast::Sender;
use tracing::error;

use std::collections::HashMap;
use std::sync::Arc;

pub mod agent;
pub mod prompts;
pub mod templates;

pub use crate::agent::AgentEntry;

pub const DEFAULT_TEMPERATURE: f64 = 0.3;
pub const DEFAULT_MAX_TURNS: usize = 6;

// Helper macro to send events to the dashboard if the channel is available.
// `$config` must be an expression that yields an [`RuntimeProvider`].
#[macro_export]
macro_rules! send_event {
    ($event:expr) => {
        $crate::send_event!(config, $event)
    };
    ($provider:expr, $event:expr) => {
        if let Some(tx) = $crate::RuntimeProvider::get_dashboard_tx(&*$provider) {
            let _ = tx.send($event);
        }
    };
}

pub struct AgentConfig<'l> {
    pub client: &'l openrouter::Client,
    pub model: &'l Model,

    pub template_vars: Option<&'l HashMap<String, serde_json::Value>>,
    pub additional_params: Option<&'l serde_json::Value>,
}

pub trait AgentConfigProvider {
    fn get_agent_config(&self) -> AgentConfig<'_>;
}

pub trait AgentDetailsProvider {
    fn get_name(&self) -> &str;
    fn get_prompt(&self, vars: HashMap<String, serde_json::Value>) -> String;
    fn get_description(&self) -> &str;
}

pub trait RuntimeProvider<A>: Send + Sync
where
    A: CompletionModel + Send + Sync + 'static,
{
    fn get_id(&self) -> &MagicTypeId;
    fn get_client(&self) -> Arc<Client<A>>;
    fn get_analytics(&self) -> Arc<AnalyticsTracker>;
    fn get_dashboard_tx(&self) -> Option<Sender<RunEvent>>;
}

/// Build a rig agent for the given [`AgentDetailsProvider`] and [`AgentConfigProvider`].
pub fn build_agent<P>(
    config: Arc<P>,
    agent: &impl AgentDetailsProvider,
    tool_server_handle: ToolServerHandle,
) -> AgentBuilder<impl CompletionModel, (), WithToolServerHandle>
where
    P: AgentConfigProvider + Send + Sync,
{
    let config = config.get_agent_config();
    let vars = config.template_vars.cloned().unwrap_or_else(HashMap::new);
    let agent_preamble = agent.get_prompt(vars);

    let mut builder = config
        .client
        .agent(config.model.get())
        .name(&agent.get_name())
        .preamble(&agent_preamble)
        .description(&agent.get_description())
        .temperature(DEFAULT_TEMPERATURE)
        .default_max_turns(DEFAULT_MAX_TURNS)
        .tool_server_handle(tool_server_handle);

    if let Some(params) = config.additional_params {
        builder = builder.additional_params(params.clone());
    }

    builder
}

pub async fn stream_agent<Output, R, A>(
    config: Arc<R>,
    agent_id: &MagicTypeId,
    prompt: &str,
) -> Result<Output>
where
    Output: DeserializeOwned,
    R: RuntimeProvider<A> + AgentConfigProvider + Send + Sync + 'static,
    A: CompletionModel + Send + Sync + 'static,
{
    let cfg = config.get_agent_config();
    let agent = cfg
        .client
        .agent(cfg.model.get())
        .build();
    let analytics_trcker = &config.get_analytics();
    let mut stream = agent.stream_prompt(prompt).await;
    while let Some(chunk) = stream.next().await {
        let review_id = config.get_id().clone();
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
                        agent_id,
                        config.get_id()
                    );
                    continue;
                };
                let serde = serde_json::from_str::<Output>(&text.text);
                match serde {
                    Ok(output) => return Ok(output),
                    Err(e) => {
                        error!(
                            "Failed to deserialize final response for agent {} during review {}: {}",
                            agent_id,
                            config.get_id(),
                            e
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
        config.get_id()
    ))
}
