use leptos::html;
use leptos::prelude::*;

/// Max lines to render in the log viewer.
/// Prevents browser freeze on huge log dumps.
const MAX_LOG_LINES: usize = 2000;

/// Scrollable monospace text block.
///
/// Renders a `log-viewer__content` container with a `log-viewer__pre` element.
/// Accepts an optional `NodeRef` on the container for auto-scroll.
/// Truncates to last [`MAX_LOG_LINES`] lines to prevent DOM bloat.
#[component]
pub fn LogText(
    text: String,
    #[prop(optional)] container_ref: Option<NodeRef<html::Div>>,
) -> impl IntoView {
    use leptos::either::Either;

    let lines: Vec<&str> = text.lines().collect();
    let truncated = if lines.len() > MAX_LOG_LINES {
        let skipped = lines.len() - MAX_LOG_LINES;
        let keep: Vec<&str> = lines.into_iter().skip(skipped).collect();
        format!("... ({} lines truncated)\n{}", skipped, keep.join("\n"))
    } else {
        text
    };

    if let Some(r) = container_ref {
        Either::Left(view! {
            <div class="log-viewer__content" node_ref=r>
                <pre class="log-viewer__pre">{truncated}</pre>
            </div>
        })
    } else {
        Either::Right(view! {
            <div class="log-viewer__content">
                <pre class="log-viewer__pre">{truncated}</pre>
            </div>
        })
    }
}
