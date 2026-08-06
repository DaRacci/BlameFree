use leptos::prelude::*;

/// A single option in a radio group.
#[derive(Debug, Clone)]
pub struct RadioOption {
    pub value: String,
    pub label: String,
    pub disabled: bool,
    pub tooltip: Option<String>,
}

/// Generic single-select radio list.
///
/// Renders each option as a labelled radio row; picking one writes its `value` to `set_value`.
#[component]
pub fn RadioGroup(
    /// Unique identifier for the radio group.
    id: &'static str,

    /// Label text shown above the list.
    label_text: &'static str,

    /// Reactive signal for the available options to display.
    options: Vec<RadioOption>,

    /// The currently selected value.
    value: ReadSignal<String>,

    /// Write-signal to update the selected value.
    set_value: WriteSignal<String>,

    /// Optional helper text shown below the list.
    #[prop(optional)]
    helper: Option<&'static str>,

    /// Prepend a "None" option whose value is the literal string "none".
    #[prop(optional)]
    include_none: bool,

    /// When true, show a "Loading..." status row instead of the options.
    #[prop(optional)]
    loading: bool,

    /// Disable all options when true.
    #[prop(optional)]
    disabled: bool,
) -> impl IntoView {
    view! {
        <div class="form-field">
            <label class="form-field__label" for=id>{label_text}</label>
            <div class="radio-group">
                {move || {
                    if loading {
                        return view! { <p class="radio-status">"Loading..."</p> }.into_any();
                    }
                    let mut opts = Vec::new();
                    if include_none {
                        opts.push(RadioOption {
                            value: "none".to_string(),
                            label: "None".to_string(),
                            disabled,
                            tooltip: None,
                        });
                    }
                    opts.extend(options.clone());
                    let sel = value.get();
                    opts.into_iter().map(|opt| {
                        let is_checked = sel == opt.value;
                        let is_disabled = opt.disabled || disabled;
                        let label_class = if is_disabled {
                            "radio-label radio-label--disabled"
                        } else {
                            "radio-label"
                        };
                        let tooltip = opt.tooltip.clone().unwrap_or_default();
                        let opt_value = opt.value.clone();
                        let label = opt.label.clone();
                        view! {
                            <label class=label_class>
                                <input
                                    type="radio"
                                    name=id
                                    prop:checked=is_checked
                                    disabled=is_disabled
                                    on:click={
                                        let opt_value = opt_value.clone();
                                        move |_| {
                                            set_value.set(opt_value.clone());
                                        }
                                    }
                                />
                                <span title=tooltip>{label}</span>
                            </label>
                        }.into_any()
                    }).collect::<Vec<_>>().into_any()
                }}
            </div>
            {move || helper.map(|h| view! { <p class="form-field__helper">{h}</p> })}
        </div>
    }
}
