use riv_types::capabilities::ReasoningEffort;
use leptos::prelude::*;

/// A reusable dropdown for selecting reasoning effort level.
///
/// Displays available effort levels fetched from the API, with a loading state and a "None (disable reasoning)" option.
#[component]
pub fn ReasoningEffortSelector(
    reasoning_effort: ReadSignal<Option<ReasoningEffort>>,
    set_reasoning_effort: WriteSignal<Option<ReasoningEffort>>,
    effort_levels: ReadSignal<Vec<ReasoningEffort>>,
    effort_loading: ReadSignal<bool>,
) -> impl IntoView {
    view! {
        <div class="form-field">
            <label class="form-field__label" for="reasoning_effort">"Reasoning Effort"</label>
            <select
                id="reasoning_effort"
                class="input select"
                on:change=move |ev| {
                    let val = event_target_value(&ev);
                    if val == "none" {
                        set_reasoning_effort.set(None);
                    } else {
                        set_reasoning_effort.set(Some(ReasoningEffort::try_from(val.as_str()).unwrap_or(ReasoningEffort::Medium)));
                    }
                }
            >
                {move || {
                    let current = reasoning_effort.get();
                    let levels = effort_levels.get();
                    let loading = effort_loading.get();
                    let mut options: Vec<AnyView> = Vec::new();
                    options.push(view! { <option value="none">"None (disable reasoning)"</option> }.into_view().into_any());
                    if loading {
                        options.push(view! { <option value="loading" disabled>"Loading..."</option> }.into_view().into_any());
                    } else {
                        for level in &levels {
                            let val = level.clone().to_string();
                            let label = val[..1].to_uppercase() + &val[1..];
                            let is_selected = match &current {
                                Some(curr) if curr == level => true,
                                None if level == &ReasoningEffort::Medium => true,
                                _ => false,
                            };
                            options.push(view! { <option value=val selected=is_selected>{label}</option> }.into_view().into_any());
                        }
                    }
                    options
                }}
            </select>
            <p class="form-field__helper">"Set reasoning/thinking effort for compatible models (DeepSeek, OpenAI o-series, etc.)"</p>
        </div>
    }
}
