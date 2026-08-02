use leptos::prelude::*;

use super::checkbox_group::{CheckboxGroup, CheckboxOption};

/// Trait for items that can be displayed in the PR selection list.
pub trait PrItem {
    /// Unique key used as the selection identifier. Use [`PrMeta::number`] when available.
    fn pr_key(&self) -> &str;

    /// Display label shown next to the checkbox.
    fn pr_label(&self) -> String;
}

/// PR checkbox list with select-all / deselect-all and count summary.
///
/// Thin wrapper over `CheckboxGroup`. Handles loading, empty, and populated states.
#[component]
pub fn PrSelection<T: PrItem + Clone + Send + Sync + 'static>(
    prs_loading: ReadSignal<bool>,
    available_prs: ReadSignal<Vec<T>>,
    selected_prs: ReadSignal<Vec<String>>,
    set_selected_prs: WriteSignal<Vec<String>>,
    empty_message: &'static str,
    helper_text: &'static str,
) -> impl IntoView {
    let options = Signal::derive(move || {
        available_prs
            .get()
            .iter()
            .map(|pr| CheckboxOption {
                key: pr.pr_key().to_string(),
                label: pr.pr_label(),
                disabled: false,
                tooltip: None,
            })
            .collect::<Vec<_>>()
    });

    view! {
        <section class="form-section">
            <h2 class="form-section__title">"PR Selection"</h2>
            <div class="form-section__fields">
                <div class="form-field">
                    <label class="form-field__label">"Select PRs to evaluate"</label>
                    {move || -> AnyView {
                        if prs_loading.get() {
                            return view! {
                                <div class="pr-status-msg">"Loading PRs..."</div>
                            }.into_any();
                        }
                        if available_prs.get().is_empty() {
                            return view! {
                                <div class="pr-status-msg">{empty_message}</div>
                            }.into_any();
                        }
                        let total = available_prs.get().len();
                        let checked = selected_prs.get().len();
                        view! {
                            <div class="pr-select-bar">
                                <span class="pr-select-bar__count">
                                    {format!("{} / {} PRs selected", checked, total)}
                                </span>
                                <button
                                    type="button"
                                    class="btn btn--ghost btn--sm"
                                    on:click=move |_| {
                                        let all: Vec<String> = available_prs
                                            .get()
                                            .iter()
                                            .map(|p| p.pr_key().to_string())
                                            .collect();
                                        set_selected_prs.set(all);
                                    }
                                >
                                    "Select All"
                                </button>
                                <button
                                    type="button"
                                    class="btn btn--ghost btn--sm"
                                    on:click=move |_| set_selected_prs.set(Vec::new())
                                >
                                    "Deselect All"
                                </button>
                            </div>
                            <div class="pr-list">
                                <CheckboxGroup
                                    options=options
                                    selected=selected_prs
                                    set_selected=set_selected_prs
                                />
                            </div>
                            <p class="form-field__helper">{helper_text}</p>
                        }.into_any()
                    }}
                </div>
            </div>
        </section>
    }
}
