//! Agent orchestration and prompt library for the code review benchmark harness.
//!
//! This crate builds LLM reviewer agents with configurable tools and prompts:
//! - [`build_agent`] builds a rig [`Agent<CompletionModel>`] for a given agent,
//!   optionally injecting rules preamble, template variables, and filesystem tools.
//! - [`PromptLibrary`] loads and renders agent prompts from embedded Handlebars templates.

use crb_types::wrappers::{Model, WrappedData};
use rig_core::agent::{AgentBuilder, WithToolServerHandle};
use rig_core::client::CompletionClient;
use rig_core::providers::openrouter;
use rig_core::providers::openrouter::CompletionModel;
use rig_core::tool::server::ToolServerHandle;

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

pub mod agent;
pub mod prompts;
pub mod templates;

use crate::prompts::PromptLibrary;

pub use crate::agent::AgentEntry;

const DEFAULT_TEMPERATURE: f64 = 0.3;
const DEFAULT_MAX_TURNS: usize = 6;

static EMPTY_HASHMAP: LazyLock<HashMap<String, serde_json::Value>> = LazyLock::new(HashMap::new);

pub struct AgentConfig<'l> {
    pub client: &'l openrouter::Client,
    pub model: &'l Model,

    pub template_vars: Option<&'l HashMap<String, serde_json::Value>>,
    pub additional_params: Option<&'l serde_json::Value>,
}

pub trait AgentConfigProvider {
    fn get_agent_config(&self) -> AgentConfig<'_>;
}

/// Build a rig agent for the given agent using the embedded prompt library.
pub fn build_agent<P>(
    config: Arc<P>,
    agent: &AgentEntry,
    tool_server_handle: ToolServerHandle,
) -> AgentBuilder<CompletionModel, (), WithToolServerHandle>
where
    P: AgentConfigProvider + Send + Sync,
{
    let prompt_lib = PromptLibrary::get_instance();
    let config = config.get_agent_config();

    let vars = config
        .template_vars
        .clone()
        .unwrap_or_else(|| &*EMPTY_HASHMAP);
    let agent_preamble = prompt_lib.render(agent, vars.clone());

    let mut builder = config
        .client
        .agent(config.model.get())
        .name(&agent.role_name)
        .preamble(&agent_preamble)
        .description(&agent.role_domain)
        .temperature(DEFAULT_TEMPERATURE)
        .default_max_turns(DEFAULT_MAX_TURNS)
        .tool_server_handle(tool_server_handle);

    if let Some(params) = config.additional_params {
        builder = builder.additional_params(params.clone());
    }

    builder
}
