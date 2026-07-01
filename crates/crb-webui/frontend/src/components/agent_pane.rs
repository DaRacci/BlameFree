use leptos::*;

#[component]
pub fn AgentPane(
    name: String,
    status: impl Fn() -> String + 'static,
    response: impl Fn() -> Option<String> + 'static,
    current_pr: impl Fn() -> Option<String> + 'static,
) -> impl IntoView {
    let status = Signal::derive(status);
    let response = Signal::derive(response);
    let current_pr = Signal::derive(current_pr);

    let pane_class = move || -> &'static str {
        match status.get().as_str() {
            "reviewing" | "running" => "agent-pane--running",
            "done" | "completed" => "agent-pane--completed",
            "failed" => "agent-pane--failed",
            _ => "agent-pane--pending",
        }
    };

    let status_icon = move || -> &'static str {
        match status.get().as_str() {
            "reviewing" | "running" => "●",
            "done" | "completed" => "✓",
            "failed" => "✗",
            _ => "||",
        }
    };

    let status_text = move || -> &'static str {
        match status.get().as_str() {
            "reviewing" | "running" => "reviewing...",
            "done" | "completed" => "completed",
            "failed" => "failed",
            _ => "pending",
        }
    };

    // Generate a short code from name
    let short_name = {
        let name = name.clone();
        move || name.chars().take(2).collect::<String>().to_uppercase()
    };
    let _short = short_name();

    view! {
        <div class=move || format!("agent-pane {}", pane_class())>
            <div class="agent-pane__header">
                <span>{status_icon()}</span>
                <span class="agent-pane__role">{name}</span>
                <span class="agent-pane__status">{status_text()}</span>
            </div>

            <div class="agent-pane__content">
                {move || {
                    match (status.get(), response.get()) {
                        (_s, Some(resp)) if !resp.is_empty() => {
                            view! {
                                <pre style="white-space: pre-wrap; word-break: break-word; margin: 0; font-size: var(--text-sm, 13px); line-height: 1.4;">{resp}</pre>
                            }.into_view()
                        }
                        (s, _) if s == "pending" => {
                            view! {
                                <span style="color: var(--text-tertiary, #6e7681); font-style: italic;">"Waiting for task..."</span>
                            }.into_view()
                        }
                        (s, _) if s == "reviewing" || s == "running" => {
                            view! {
                                <span style="color: var(--text-tertiary, #6e7681); font-style: italic;">"Processing..."</span>
                            }.into_view()
                        }
                        (_, _) => {
                            view! {
                                <span style="color: var(--text-tertiary, #6e7681); font-style: italic;">"No response yet"</span>
                            }.into_view()
                        }
                    }
                }}
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
