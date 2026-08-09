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
#[component]
pub fn CheckboxGroup(
    /// Reactive signal so callers can recompute disabled/tooltip state based on the current selection.
    options: Signal<Vec<CheckboxOption>>,

    /// Reactive signal for the currently selected keys.
    selected: ReadSignal<Vec<String>>,

    /// Write-signal to update the selected keys.
    set_selected: WriteSignal<Vec<String>>,
) -> impl IntoView {
    view! {
        {move || {
            let opts = options.get();
            let sel = selected.get();
            opts.into_iter().map(|opt| {
                view! {
                    <CheckboxElement
                        option=opt.clone()
                        selected=sel.clone()
                        set_selected=set_selected.clone()
                    />
                }
            }).collect::<Vec<_>>()
        }}
    }
}

#[component]
fn CheckboxElement(
    option: CheckboxOption,
    selected: Vec<String>,
    set_selected: WriteSignal<Vec<String>>,
) -> impl IntoView {
    let key = option.key.clone();
    let label = option.label.clone();
    let is_checked = selected.contains(&key);
    let tooltip = option.tooltip.clone().unwrap_or_default();
    let label_class = if option.disabled {
        "checkbox-label checkbox-label--disabled"
    } else {
        "checkbox-label"
    };

    let on_click = {
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
    };

    view! {
        <label class=label_class>
            <input
                type="checkbox"
                prop:checked=is_checked
                disabled=option.disabled
                on:click=on_click
            />
            <span title=tooltip>{label}</span>
        </label>
    }
}
