use leptos::*;
use crate::AgentEvent;

#[component]
pub fn AgentPane(
    name: &'static str,
    status: impl Fn() -> String + 'static,
    response: impl Fn() -> Option<String> + 'static,
    current_pr: impl Fn() -> Option<u32> + 'static,
) -> impl IntoView {
    let status = Signal::derive(status);
    let response = Signal::derive(response);
    let current_pr = Signal::derive(current_pr);
    let status_class = move || -> &'static str {
        match status.get().as_str() {
            "reviewing" => "status-reviewing",
            "done" => "status-done",
            "failed" => "status-failed",
            _ => "status-pending",
        }
    };

    let status_icon = move || -> &'static str {
        match status.get().as_str() {
            "reviewing" => "⏳",
            "done" => "✅",
            "failed" => "❌",
            _ => "⏸",
        }
    };

    view! {
        <div class="agent-pane">
            <div class="header">
                <span>{status_icon()}</span>
                <span>{name}</span>
                <span class=status_class() style="margin-left: auto; font-size: 0.8rem;">
                    {move || status.get()}
                </span>
            </div>

            <div style="margin-bottom: 0.25rem;">
                {move || {
                    current_pr.get().map(|pr| {
                        view! {
                            <span style="color: #94a3b8; font-size: 0.8rem;">
                                {format!("PR #{}", pr)}
                            </span>
                        }
                    })
                }}
            </div>

            <div>
                {move || {
                    match (status.get(), response.get()) {
                        (s, Some(resp)) if !resp.is_empty() => {
                            view! {
                                <div class="response-line">{resp}</div>
                            }.into_view()
                        }
                        (s, _) if s == "pending" => {
                            view! {
                                <div style="color: #64748b; font-size: 0.8rem; font-style: italic;">
                                    "Waiting for task..."
                                </div>
                            }.into_view()
                        }
                        (s, _) if s == "reviewing" => {
                            view! {
                                <div style="color: #94a3b8; font-size: 0.8rem; font-style: italic;">
                                    "Processing..."
                                </div>
                            }.into_view()
                        }
                        (_, _) => {
                            view! {
                                <div style="color: #64748b; font-size: 0.8rem; font-style: italic;">
                                    "No response yet"
                                </div>
                            }.into_view()
                        }
                    }
                }}
            </div>
        </div>
    }
}
