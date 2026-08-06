use leptos::prelude::*;

use super::checkbox_group::{CheckboxGroup, CheckboxOption};
use super::checkbox_select_bar::CheckboxSelectBar;

/// Trait for items that can be displayed in the PR selection list.
pub trait PrItem {
    /// Unique key used as the selection identifier.
    ///
    /// Stable for one fetch cycle,
    /// but do not assume PR-number or title semantics.
    fn pr_key(&self) -> &str;

    /// Display label shown next to the checkbox.
    fn pr_label(&self) -> String;
}

/// PR checkbox list with select-all / deselect-all and count summary.
///
/// Thin wrapper over [`CheckboxGroup`].
/// Handles loading, empty, and populated states.
#[component]
pub fn PrSelection<T: PrItem + Clone + Send + Sync + 'static>(
    /// Signal indicating PRs are still being fetched.
    prs_loading: ReadSignal<bool>,

    /// Signal containing the available PRs to display.
    available_prs: ReadSignal<Vec<T>>,

    /// Signal containing the currently selected PR keys.
    selected_prs: ReadSignal<Vec<String>>,

    /// Write-signal to update the selected PR keys.
    set_selected_prs: WriteSignal<Vec<String>>,

    /// Text shown when no PRs are available.
    empty_message: &'static str,

    /// Text shown below the checkbox list as supplementary help.
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
                        let count_label: fn(usize, usize) -> String =
                            |checked: usize, total: usize| {
                                format!("{} / {} PRs selected", checked, total)
                            };
                        view! {
                            <CheckboxSelectBar
                                options=options
                                selected=selected_prs
                                set_selected=set_selected_prs
                                count_label=count_label
                            />
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
