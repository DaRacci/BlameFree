use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use rig_core::agent::{
    AgentBuilder, HookAction, PromptHook, ToolCallHookAction, WithToolServerHandle,
};
use rig_core::completion::{CompletionModel, CompletionResponse, Message};
use rig_core::providers::openai;
use rig_core::providers::openai::responses_api::ResponsesCompletionModel;
use rig_core::tool::server::ToolServerHandle;

/// A [`PromptHook`] that skips tool calls with budget nudge messages when the agent is approaching its turn limit.
///
/// Mechanism:
/// - Counts model-completion calls via [`on_completion_response`].
/// - When ≤2 completions remain, [`on_tool_call`] returns `Skip` with a
///   progressively firmer nudge ("X turns remaining..." -> "LAST TURN: ...").
/// - The skipped reason is fed back to the model as a synthetic tool result,
///   effectively "stripping tools" without requiring internal loop access.
///
/// See arXiv:2510.16786 for the two-tier nudge pattern at ~70 % / ~90 % of the turn budget.
#[derive(Clone)]
pub struct TurnBudgetHook {
    /// The maximum number of turns the agent is allowed to take before it must stop calling tools and output its findings.
    max_turns: usize,

    /// The number of model-completion calls made so far.
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
    async fn on_tool_call(
        &self,
        _tool_name: &str,
        _tool_call_id: Option<String>,
        _internal_call_id: &str,
        _args: &str,
    ) -> ToolCallHookAction {
        let calls_made = self.completion_count.load(Ordering::SeqCst);
        // Total possible completion calls = max_turns + 1
        // the final text-only turn before the error fires at max_turns + 2.
        let total_possible = self.max_turns + 1;
        let remaining = total_possible.saturating_sub(calls_made);

        if remaining <= 1 {
            const LAST_TURN_MSG: &str = "LAST TURN: This is your final opportunity. Do NOT call any more tools. Output your JSON findings directly.";
            return ToolCallHookAction::Skip {
                reason: LAST_TURN_MSG.to_string(),
            };
        }

        if remaining <= 2 {
            return ToolCallHookAction::Skip {
                reason: format!(
                    "You have {} turns remaining. Stop exploring and output your JSON findings.",
                    remaining
                ),
            };
        }

        ToolCallHookAction::cont()
    }
}
