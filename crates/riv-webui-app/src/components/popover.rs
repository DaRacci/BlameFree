use leptos::html;
use leptos::prelude::*;

/// Approximate popover height plus its gap so it doesn't clip off the top edge.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const FLIP_THRESHOLD_PX: f64 = 320.0;

/// Hover/focus popover.
///
/// Renders the trigger `children` and the popover `content` inside a `.popover-host` wrapper.
#[component]
pub fn Popover<F, IV>(
    /// Popover body rendered inside the tooltip box. Passed as a render prop.
    content: F,

    /// Trigger element that reveals the popover on hover/focus.
    children: Children,
) -> impl IntoView
where
    F: Fn() -> IV + 'static,
    IV: IntoView,
{
    let host_ref: NodeRef<html::Div> = NodeRef::new();
    #[allow(unused_variables)]
    let (flip, set_flip) = signal(false);

    let measure_and_flip = move || {
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(host) = host_ref.get() {
                let el: &web_sys::Element = host.as_ref();
                let top = el.get_bounding_client_rect().top();
                set_flip.set(top < FLIP_THRESHOLD_PX);
            }
        }
    };

    // Re-measure on every reveal so scroll position changes are respected.
    let on_mouse_enter = move |_: leptos::ev::MouseEvent| measure_and_flip();
    let on_focus_in = move |_: leptos::ev::FocusEvent| measure_and_flip();

    view! {
        <div
            class="popover-host"
            class:popover-host--flip=move || flip.get()
        >
            <div node_ref=host_ref on:mouseenter=on_mouse_enter on:focusin=on_focus_in>
                {children()}
            </div>
            <div class="popover" role="tooltip">
                {content()}
            </div>
        </div>
    }
}
