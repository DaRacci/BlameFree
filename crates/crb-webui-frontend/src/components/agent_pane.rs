use crb_webui_shared::runs::RunStatus;
use leptos::either::{Either, EitherOf4};
use leptos::prelude::*;
use lucide_leptos::{Check, Circle, CirclePlay, X};

#[component]
pub fn AgentPane(
    name: String,
    status: impl Fn() -> RunStatus + Send + Sync + 'static,
    response: impl Fn() -> Option<String> + Send + Sync + 'static,
    current_pr: impl Fn() -> Option<String> + Send + Sync + 'static,
) -> impl IntoView {
    let status = Signal::derive(status);
    let response = Signal::derive(response);
    let current_pr = Signal::derive(current_pr);

    let pane_class = move || -> &'static str {
        match status.get() {
            RunStatus::Running => "agent-pane--running",
            RunStatus::Completed => "agent-pane--completed",
            RunStatus::Failed => "agent-pane--failed",
            RunStatus::Pending => "agent-pane--pending",
            RunStatus::Cancelled => "agent-pane--cancelled",
        }
    };

    let status_icon = move || -> EitherOf4<_, _, _, _> {
        match status.get() {
            RunStatus::Running => EitherOf4::A(view! { <CirclePlay size=16 /> }),
            RunStatus::Completed => EitherOf4::B(view! { <Check size=16 /> }),
            RunStatus::Failed => EitherOf4::C(view! { <X size=16 /> }),
            RunStatus::Pending => EitherOf4::D(view! { <Circle size=16 /> }),
            RunStatus::Cancelled => EitherOf4::D(view! { <Circle size=16 /> }),
        }
    };

    let status_text = move || -> &'static str { status.get().into() };

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
                            Either::Left(Either::Left(Either::Left(
                                view! {
                                    <pre style="white-space: pre-wrap; word-break: break-word; margin: 0; font-size: var(--text-sm, 13px); line-height: 1.4;">{resp}</pre>
                                }
                            )))
                        }
                        (s, _) if s == RunStatus::Pending => {
                            Either::Left(Either::Left(Either::Right(
                                view! {
                                    <span style="color: var(--text-tertiary, #6e7681); font-style: italic;">"Waiting for task..."</span>
                                }
                            )))
                        }
                        (s, _) if s == RunStatus::Running => {
                            Either::Left(Either::Right(
                                view! {
                                    <span style="color: var(--text-tertiary, #6e7681); font-style: italic;">"Processing..."</span>
                                }
                            ))
                        }
                        (_, _) => {
                            Either::Right(
                                view! {
                                    <span style="color: var(--text-tertiary, #6e7681); font-style: italic;">"No response yet"</span>
                                }
                            )
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
