//! Agent orchestration and prompt library for the code review benchmark harness.
//!
//! This crate builds LLM reviewer agents with configurable tools and prompts:
//! - [`build_agent`] builds a rig [`Agent<ResponsesCompletionModel>`] for a given role,
//!   optionally injecting rules preamble, template variables, and filesystem tools.
//! - [`PromptLibrary`] loads and renders role prompts from embedded Handlebars templates.

use rig_core::agent::Agent;
use rig_core::client::CompletionClient;
use rig_core::providers::openai;
use rig_core::providers::openai::responses_api::ResponsesCompletionModel;
use rig_core::tool::server::ToolServerHandle;

use std::collections::HashMap;

pub mod prompts;
pub mod templates;

use crate::prompts::PromptLibrary;

const DEFAULT_TEMPERATURE: f64 = 0.3;
const DEFAULT_MAX_TURNS: usize = 6;

/// Build a rig agent for the given role using the embedded prompt library.
///
/// The role's preamble is resolved through the [`PromptLibrary`], which loads
/// prompts from the embedded `include_dir!` prompts directory at compile time.
///
/// If `rules_preamble` is `Some` and non-empty, it is prepended before the role-specific preamble, separated by a blank line.
/// This allows project-level rules to be injected into the agent's system prompt before its role-specific instructions.
///
/// If `extra_preamble` is `Some`, it is appended after the role preamble.
/// This is used for tool-calling instructions and other supplementary content.
///
/// `template_vars` provides variable substitutions for the prompt template
/// (e.g. `{diff}`, `{role}`, `{file_list}`, `{language}`).
#[allow(clippy::too_many_arguments)]
pub fn build_agent(
    client: &openai::Client,
    model: &str,
    role: &str,
    rules_preamble: Option<&str>,
    template_vars: Option<&HashMap<String, serde_json::Value>>,
    extra_preamble: Option<&str>,
    additional_params: Option<serde_json::Value>,
    tool_server_handle: ToolServerHandle,
) -> Agent<ResponsesCompletionModel> {
    let prompt_lib = PromptLibrary::get_instance();

    let empty_map = HashMap::new();
    let vars: HashMap<String, serde_json::Value> = template_vars.cloned().unwrap_or(empty_map);
    let role_preamble = prompt_lib.render(role, vars);
    let mut full_preamble = match rules_preamble {
        Some(rp) if !rp.is_empty() => format!("{rp}\n\n{role_preamble}"),
        _ => role_preamble.to_string(),
    };

    if let Some(extra) = extra_preamble {
        if !extra.is_empty() {
            full_preamble = format!("{full_preamble}\n\n{extra}");
        }
    }

    let mut builder = client
        .agent(model)
        .preamble(&full_preamble)
        .temperature(DEFAULT_TEMPERATURE)
        .default_max_turns(DEFAULT_MAX_TURNS)
        .tool_server_handle(tool_server_handle);

    if let Some(params) = additional_params {
        builder = builder.additional_params(params.clone());
    }

    builder.build()
}
