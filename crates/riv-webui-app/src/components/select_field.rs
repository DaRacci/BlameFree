use leptos::prelude::*;

/// A single option in a select dropdown.
#[derive(Debug, Clone)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

/// Generic single-select dropdown.
///
/// Callers build `Vec<SelectOption>` from their domain data
/// and handle any value-type conversion before passing in.
#[component]
pub fn SelectField(
    id: &'static str,
    label_text: &'static str,
    options: Vec<SelectOption>,
    value: ReadSignal<String>,
    set_value: WriteSignal<String>,

    /// Optional helper text to display below the select field.
    #[prop(optional)]
    helper: Option<&'static str>,

    /// Prepend a "none" sentinel option (value = "none")
    #[prop(optional)]
    include_none: bool,

    /// If a "loading" option should be displayed while the options are being fetched.
    #[prop(optional)]
    loading: bool,

    /// If the select field should be unavailable for interaction.
    #[prop(optional)]
    disabled: bool,
) -> impl IntoView {
    let loading = match loading {
        true => Some(view! { <option value="loading" disabled>"Loading..."</option> }),
        false => None,
    };
    let none_opt = match include_none {
        true => Some(view! { <option value="none">"None"</option> }),
        false => None,
    };
    let options = move || {
        let current = value.get();
        options
            .iter()
            .map(|opt| {
                let is_selected = opt.value == current;
                view! {
                    <option value=opt.value.clone() selected=is_selected>
                        {opt.label.clone()}
                    </option>
                }
            })
            .collect::<Vec<_>>()
    };
    let helper = helper.map(|h| view! { <p class="form-field__helper">{h}</p> });

    view! {
        <div class="form-field">
            <label class="form-field__label" for=id>{label_text}</label>
            <select
                id=id
                class="input select"
                disabled=disabled
                on:change=move |ev| set_value.set(event_target_value(&ev))
            >
                {none_opt}
                {loading}
                {options}
            </select>
            {helper}
        </div>
    }
}
