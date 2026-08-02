use leptos::prelude::*;
use lucide_leptos::TriangleAlert;

/// Error state block with icon, heading, message, and optional action children.
///
/// Place retry buttons or other actions as children,
/// they are wrapped in `error-state__action`.
#[component]
pub fn ErrorState(
    heading: impl Into<String>,
    message: impl Into<String>,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    let heading = heading.into();
    let message = message.into();
    view! {
        <div class="error-state" role="alert">
            <div class="error-state__icon"><TriangleAlert size=24 /></div>
            <h3 class="error-state__heading">{heading}</h3>
            <p class="error-state__message">{message}</p>
            {children.map(|c| view! { <div class="error-state__action">{c()}</div> })}
        </div>
    }
}
