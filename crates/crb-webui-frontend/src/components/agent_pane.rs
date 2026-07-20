use crb_webui_shared::review::ReviewStatus;
use leptos::either::{Either, EitherOf4};
use leptos::prelude::*;
use lucide_leptos::{Check, Circle, CirclePlay, X};

#[component]
pub fn AgentPane(
    name: String,
    status: impl Fn() -> ReviewStatus + Send + Sync + 'static,
    response: impl Fn() -> Option<String> + Send + Sync + 'static,
    current_pr: impl Fn() -> Option<String> + Send + Sync + 'static,
) -> impl IntoView {
    let status = Signal::derive(status);
    let response = Signal::derive(response);
    let current_pr = Signal::derive(current_pr);

    let pane_class = move || -> &'static str {
        match status.get() {
            ReviewStatus::Running => "agent-pane--running",
            ReviewStatus::Completed => "agent-pane--completed",
            ReviewStatus::Failed => "agent-pane--failed",
            ReviewStatus::Pending => "agent-pane--pending",
            ReviewStatus::Cancelled => "agent-pane--cancelled",
        }
    };

    let status_icon = move || -> EitherOf4<_, _, _, _> {
        match status.get() {
            ReviewStatus::Running => EitherOf4::A(view! { <CirclePlay size=16 /> }),
            ReviewStatus::Completed => EitherOf4::B(view! { <Check size=16 /> }),
            ReviewStatus::Failed => EitherOf4::C(view! { <X size=16 /> }),
            ReviewStatus::Pending => EitherOf4::D(view! { <Circle size=16 /> }),
            ReviewStatus::Cancelled => EitherOf4::D(view! { <Circle size=16 /> }),
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
                                    <pre style="margin: 0; line-height: 1.4;">{resp}</pre>
                                }
                            )))
                        }
                        (s, _) if s == ReviewStatus::Pending => {
                            Either::Left(Either::Left(Either::Right(
                                view! {
                                    <span class="text-tertiary text-italic">"Waiting for task..."</span>
                                }
                            )))
                        }
                        (s, _) if s == ReviewStatus::Running => {
                            Either::Left(Either::Right(
                                view! {
                                    <span class="text-tertiary text-italic">"Processing..."</span>
                                }
                            ))
                        }
                        (_, _) => {
                            Either::Right(
                                view! {
                                    <span class="text-tertiary text-italic">"No response yet"</span>
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
