use leptos::prelude::*;

/// Labelled text input with helper text.
#[component]
pub fn TextField(
    id: &'static str,
    label: &'static str,
    helper: &'static str,
    value: ReadSignal<String>,
    set_value: WriteSignal<String>,

    /// Placeholder text for the input field.
    #[prop(optional)]
    placeholder: Option<&'static str>,

    /// If the input field should be unavailable for interaction.
    #[prop(optional)]
    disabled: bool,
) -> impl IntoView {
    view! {
        <div class="form-field">
            <label class="form-field__label" for=id>{label}</label>
            <input
                id=id
                class="input"
                type="text"
                placeholder=placeholder.unwrap_or_default()
                disabled=disabled
                prop:value=move || value.get()
                on:input=move |ev| set_value.set(event_target_value(&ev))
            />
            <p class="form-field__helper">{helper}</p>
        </div>
    }
}
