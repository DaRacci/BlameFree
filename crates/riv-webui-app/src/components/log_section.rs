use leptos::prelude::*;

use super::log_text::LogText;

#[component]
pub fn LogSection(label: &'static str, text: String) -> impl IntoView {
    if text.trim().is_empty() {
        return view! { <span></span> }.into_any();
    }

    let line_count = text.lines().count();

    view! {
        <section class="form-field">
            <label class="form-field__label">{format!("{} ({} lines)", label, line_count)}</label>
            <LogText text=text />
        </section>
    }
    .into_any()
}
