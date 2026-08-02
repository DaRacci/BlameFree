use leptos::prelude::*;

/// Page-level header with title and optional action buttons.
///
/// Children slot is rendered inside `page-header__actions`.
#[component]
pub fn PageHeader(
    title: impl Into<String>,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    let title = title.into();
    view! {
        <div class="page-header">
            <h1 class="page-header__title">{title}</h1>
            {children.map(|c| view! { <div class="page-header__actions">{c()}</div> })}
        </div>
    }
}
