use leptos::prelude::*;

/// Form section with a visible title header and a fields container.
#[component]
pub fn FormSection(title: impl Into<String>, children: Children) -> impl IntoView {
    let title = title.into();
    view! {
        <section class="form-section">
            <h2 class="form-section__title">{title}</h2>
            <div class="form-section__fields">{children()}</div>
        </section>
    }
}
