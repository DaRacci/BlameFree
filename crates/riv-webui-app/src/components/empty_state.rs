use leptos::prelude::*;

/// Empty-state placeholder with message and optional CTA children.
#[component]
pub fn EmptyState(
    message: impl Into<String>,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    let message = message.into();
    view! {
        <div class="empty-state py-xl">
            <p class="empty-state__message" style="margin: 0;">{message}</p>
            {children.map(|c| c())}
        </div>
    }
}
