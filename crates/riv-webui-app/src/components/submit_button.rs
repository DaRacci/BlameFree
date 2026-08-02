use leptos::prelude::*;

/// Submit button with label/loading-label swap and disabled binding.
/// Includes the `form-actions` wrapper div.
#[component]
pub fn SubmitButton(
    /// The label to display when not submitting.
    label: &'static str,

    /// Signal indicating if the form is currently submitting.
    submitting: ReadSignal<bool>,

    // Closure that returns if the submit button should be disabled.
    submit_disabled: impl Fn() -> bool + Send + Sync + 'static,
) -> impl IntoView {
    let button_label = move || match submitting.get() || submit_disabled() {
        true => "Submitting...",
        false => label,
    };
    let submitting = move || match submitting.get() {
        true => "Submitting...",
        false => label,
    };
    view! {
        <div class="form-actions">
            <button
                type="submit"
                class="btn btn--primary btn--lg btn--full"
                disabled=button_label
            >
                {submitting}
            </button>
        </div>
    }
}
