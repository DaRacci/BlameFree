use leptos::prelude::*;

/// Trait for PR items that can be displayed in the selection list.
///
/// Implement this for each PR type used with `PrSelection`.
/// The trait is public so callers can impl it for their local types.
pub trait PrItem {
    /// Unique key for the checkbox (used as the selection identifier).
    fn pr_key(&self) -> &str;
    /// Display label shown next to the checkbox.
    fn pr_label(&self) -> String;
}

/// A reusable checkbox list for selecting PRs.
///
/// Handles loading, empty, and populated states. Provides Select All / Deselect All
/// buttons and a selected-count summary.
#[component]
pub fn PrSelection<T: PrItem + Clone + 'static>(
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
    view! {
        <section class="form-section">
            <h2 class="form-section__title">"PR Selection"</h2>
            <div class="form-section__fields">
                <div class="form-field">
                    <label class="form-field__label">"Select PRs to evaluate"</label>
                    {move || -> AnyView {
                        if prs_loading.get() {
                            return view! {
                                <div class="pr-status-msg">
                                    "Loading PRs..."
                                </div>
                            }.into_view().into_any();
                        }
                        let prs = available_prs.get();
                        if prs.is_empty() {
                            return view! {
                                <div class="pr-status-msg">
                                    {empty_message}
                                </div>
                            }.into_view().into_any();
                        }
                        let sel = selected_prs.get();
                        let total = prs.len();
                        let checked = sel.len();
                        view! {
                            <div class="pr-select-bar">
                                <span class="pr-select-bar__count">
                                    {format!("{} / {} PRs selected", checked, total)}
                                </span>
                                <button type="button" class="btn btn--ghost btn--sm"
                                    on:click=move |_| {
                                        let all_keys: Vec<String> = available_prs.get().iter().map(|p| p.pr_key().to_string()).collect();
                                        set_selected_prs.set(all_keys);
                                    }
                                >"Select All"</button>
                                <button type="button" class="btn btn--ghost btn--sm"
                                    on:click=move |_| {
                                        set_selected_prs.set(Vec::new());
                                    }
                                >"Deselect All"</button>
                            </div>
                            <div class="pr-list">
                                {prs.into_iter().map(|pr| {
                                    let key = pr.pr_key().to_string();
                                    let is_checked = sel.contains(&key);
                                    let label = pr.pr_label();
                                    view! {
                                        <label class="checkbox-label pr-list__checkbox">
                                            <input type="checkbox" prop:checked=is_checked
                                                on:click={
                                                    let key = key.clone();
                                                    move |_| {
                                                        set_selected_prs.update(|sel| {
                                                            if let Some(pos) = sel.iter().position(|k| k == &key) {
                                                                sel.remove(pos);
                                                            } else {
                                                                sel.push(key.clone());
                                                            }
                                                        });
                                                    }
                                                }
                                            />
                                            <span class="text-sm">{label}</span>
                                        </label>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                            <p class="form-field__helper">{helper_text}</p>
                        }.into_view().into_any()
                    }}
                </div>
            </div>
        </section>
    }
}
