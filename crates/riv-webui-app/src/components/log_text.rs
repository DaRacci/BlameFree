use leptos::html;
use leptos::prelude::*;

/// Scrollable monospace text block.
///
/// Renders a `log-viewer__content` container with a `log-viewer__pre` element.
/// Accepts an optional `NodeRef` on the container for auto-scroll.
#[component]
pub fn LogText(
    text: String,
    #[prop(optional)] container_ref: Option<NodeRef<html::Div>>,
) -> impl IntoView {
    use leptos::either::Either;

    if let Some(r) = container_ref {
        Either::Left(view! {
            <div class="log-viewer__content" node_ref=r>
                <pre class="log-viewer__pre">{text}</pre>
            </div>
        })
    } else {
        Either::Right(view! {
            <div class="log-viewer__content">
                <pre class="log-viewer__pre">{text}</pre>
            </div>
        })
    }
}
