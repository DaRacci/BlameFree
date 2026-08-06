use leptos::prelude::*;

use super::checkbox_group::CheckboxOption;

/// Select-all / deselect-all control bar for a [`CheckboxGroup`].
///
/// Composable with any [`CheckboxGroup`], both are driven by the same
/// `options` / `selected` / `set_selected` signals, so they stay in sync.
///
/// Renders a selected-count summary plus **Select All** and **Deselect All** buttons.
/// Select All only selects the currently *enabled* options.
#[component]
pub fn CheckboxSelectBar(
    /// Reactive signal for the available options.
    options: Signal<Vec<CheckboxOption>>,

    /// Reactive signal for the currently selected keys.
    selected: ReadSignal<Vec<String>>,

    /// Write-signal to update the selected keys.
    set_selected: WriteSignal<Vec<String>>,

    /// Formatting function for the count summary.
    #[prop(optional)]
    count_label: Option<fn(usize, usize) -> String>,
) -> impl IntoView {
    let total = move || options.get().len();
    let checked = move || selected.get().len();

    let count_fn: fn(usize, usize) -> String =
        count_label.unwrap_or(|c, t| format!("{} / {} selected", c, t));

    view! {
        <div class="checkbox-select-bar">
            <span class="checkbox-select-bar__count">
                {count_fn(checked(), total())}
            </span>
            <button
                type="button"
                class="btn btn--ghost btn--sm"
                on:click=move |_| {
                    let all: Vec<String> = options
                        .get()
                        .into_iter()
                        .filter(|opt| !opt.disabled)
                        .map(|opt| opt.key)
                        .collect();
                    set_selected.set(all);
                }
            >
                "Select All"
            </button>
            <button
                type="button"
                class="btn btn--ghost btn--sm"
                on:click=move |_| set_selected.set(Vec::new())
            >
                "Deselect All"
            </button>
        </div>
    }
}
