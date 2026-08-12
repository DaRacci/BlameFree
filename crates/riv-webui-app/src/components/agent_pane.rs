use leptos::prelude::*;
use lucide_leptos::{Check, Circle, CirclePlay, X};
use riv_types::review::ReviewStatus;

use crate::components::logs::log_text::LogText;

/// Live agent status pane.
///
/// Shows a single review agent's name, status, streaming response,
/// and its live reasoning/thinking.
#[component]
pub fn AgentPane(
    name: String,
    status: Signal<ReviewStatus>,
    response: Signal<Option<String>>,
    #[prop(optional)] thinking: Signal<Option<String>>,
    current_pr: Signal<Option<String>>,
) -> impl IntoView {
    let pane_class = move || -> &'static str {
        match status.get() {
            ReviewStatus::Running => "agent-pane--running",
            ReviewStatus::Completed => "agent-pane--completed",
            ReviewStatus::Failed => "agent-pane--failed",
            ReviewStatus::Pending => "agent-pane--pending",
            ReviewStatus::Cancelled => "agent-pane--cancelled",
        }
    };

    let status_icon = move || -> AnyView {
        match status.get() {
            ReviewStatus::Running => view! { <CirclePlay size=16 /> }.into_any(),
            ReviewStatus::Completed => view! { <Check size=16 /> }.into_any(),
            ReviewStatus::Failed => view! { <X size=16 /> }.into_any(),
            ReviewStatus::Pending | ReviewStatus::Cancelled => {
                view! { <Circle size=16 /> }.into_any()
            }
        }
    };

    let status_text = move || -> &'static str { status.get().into() };

    view! {
        <div class=move || format!("agent-pane {}", pane_class())>
            <div class="agent-pane__header">
                <span>{status_icon()}</span>
                <span class="agent-pane__role">{name}</span>
                <span class="agent-pane__status">{status_text()}</span>
            </div>

            <div class="agent-pane__content">
                <AgentPaneContent status=status response=response thinking=thinking />
            </div>

            <div class="agent-pane__footer">
                {move || {
                    current_pr.get().map(|pr| {
                        view! {
                            <span class="agent-pane__findings">{format!("PR: {}", pr)}</span>
                        }
                    })
                }}
            </div>
        </div>
    }
}

/// Renders the agent body: live reasoning/thinking and the streamed response.
#[component]
fn AgentPaneContent(
    status: Signal<ReviewStatus>,
    response: Signal<Option<String>>,
    #[prop(optional)] thinking: Signal<Option<String>>,
) -> impl IntoView {
    let thinking = thinking.get().filter(|text| !text.trim().is_empty());
    if let Some(think) = thinking {
        return view! {
            <details class="agent-pane__thinking">
                <summary class="agent-pane__thinking-summary">"Reasoning"</summary>
                <LogText text=think />
            </details>
        }
        .into_any();
    }

    if let Some(resp) = response.get().filter(|text| !text.is_empty()) {
        return view! { <LogText text=resp /> }.into_any();
    }

    let message = match status.get() {
        ReviewStatus::Pending => "Waiting for task...",
        ReviewStatus::Running => "Processing...",
        _ => "No response yet",
    };

    view! { <span class="text-tertiary text-italic">{message}</span> }.into_any()
}
