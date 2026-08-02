use leptos::prelude::*;

/// A single option in a checkbox group.
#[derive(Debug, Clone)]
pub struct CheckboxOption {
    pub key: String,
    pub label: String,
    pub disabled: bool,
    pub tooltip: Option<String>,
}

/// Generic checkbox group primitive.
///
/// Drives the shared row rendering (label + checkbox, checked/disabled state, tooltip).
/// `RoleSelector` and `PrSelection` are thin wrappers over this with their specific policies.
///
/// `options` is a reactive signal so callers can recompute disabled/tooltip state
/// based on the current selection (e.g. incompatibility matrix in `RoleSelector`).
#[component]
pub fn CheckboxGroup(
    options: Signal<Vec<CheckboxOption>>,
    selected: ReadSignal<Vec<String>>,
    set_selected: WriteSignal<Vec<String>>,
) -> impl IntoView {
    view! {
        {move || {
            let opts = options.get();
            let sel = selected.get();
            opts.into_iter().map(|opt| {
                let key = opt.key.clone();
                let is_checked = sel.contains(&key);
                let label = opt.label.clone();
                let tooltip = opt.tooltip.clone().unwrap_or_default();
                let label_class = if opt.disabled {
                    "checkbox-label checkbox-label--disabled"
                } else {
                    "checkbox-label"
                };
                view! {
                    <label class=label_class>
                        <input
                            type="checkbox"
                            prop:checked=is_checked
                            disabled=opt.disabled
                            on:click={
                                let key = key.clone();
                                move |_| {
                                    set_selected.update(|s| {
                                        if let Some(pos) = s.iter().position(|k| k == &key) {
                                            s.remove(pos);
                                        } else {
                                            s.push(key.clone());
                                        }
                                    });
                                }
                            }
                        />
                        <span title=tooltip>{label}</span>
                    </label>
                }
            }).collect::<Vec<_>>()
        }}
    }
}
