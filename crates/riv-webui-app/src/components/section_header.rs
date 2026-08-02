use leptos::prelude::*;

/// Section-level header with title and optional trailing elements.
#[component]
pub fn SectionHeader(
    title: impl Into<String>,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    let title = title.into();
    view! {
        <div class="section-header">
            <h2 class="section-header__title">{title}</h2>
            {children.map(|c| c())}
        </div>
    }
}
