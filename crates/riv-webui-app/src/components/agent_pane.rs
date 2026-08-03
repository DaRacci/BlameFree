use leptos::either::{Either, EitherOf4};
use leptos::prelude::*;
use lucide_leptos::{Check, Circle, CirclePlay, X};
use riv_types::review::ReviewStatus;

use super::log_text::LogText;

#[component]
pub fn AgentPane(
    name: String,
    status: Signal<ReviewStatus>,
    response: Signal<Option<String>>,
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

    let status_icon = move || -> EitherOf4<_, _, _, _> {
        match status.get() {
            ReviewStatus::Running => EitherOf4::A(view! { <CirclePlay size=16 /> }),
            ReviewStatus::Completed => EitherOf4::B(view! { <Check size=16 /> }),
            ReviewStatus::Failed => EitherOf4::C(view! { <X size=16 /> }),
            ReviewStatus::Pending | ReviewStatus::Cancelled => {
                EitherOf4::D(view! { <Circle size=16 /> })
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
                {move || {
                    match (status.get(), response.get()) {
                        (_, Some(resp)) if !resp.is_empty() => {
                            Either::Left(Either::Left(Either::Left(
                                view! { <LogText text=resp /> }
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
