//! Agent construction — building reviewer agents with turn-budget hooks.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use rig_core::agent::{Agent, HookAction, PromptHook, ToolCallHookAction};
use rig_core::completion::{CompletionModel, CompletionResponse, Message};
use rig_core::providers::openai;
use rig_core::providers::openai::responses_api::ResponsesCompletionModel;
use rig_core::tool::server::ToolServerHandle;

use crate::ReviewerConfig;

use crb_agents::build_agent;
use crb_agents::prompts::PromptLibrary;

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
pub struct TurnBudgetHook {
    max_turns: usize,
    completion_count: Arc<AtomicUsize>,
}

impl TurnBudgetHook {
    pub fn new(max_turns: usize) -> Self {
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
    template_vars: Option<&HashMap<String, serde_json::Value>>,
    tool_preamble: Option<&str>,
    additional_params: Option<serde_json::Value>,
    tool_server_handle: ToolServerHandle,
) -> Agent<ResponsesCompletionModel> {
    build_agent(
        client,
        &config.model,
        config.role.as_str(),
        rules_preamble,
        template_vars,
        tool_preamble,
        additional_params,
        tool_server_handle,
    )
}
