use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

// Ensure the dependency is linked even when unused by the app code itself.
#[allow(unused_imports)]
use riv_webui_shared as _;

/// Renders the full HTML shell around the app, for server-side rendering only.
///
/// This is a plain function (not a component) called directly from the server
/// integration in `riv-webui-backend`. It is only compiled for the native
/// `ssr` target.
#[cfg(feature = "ssr")]
pub fn shell(options: leptos::config::LeptosOptions) -> impl IntoView {
    let leptos_options = options.clone();

    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <AutoReload options=leptos_options.clone()/>
                <HydrationScripts options=leptos_options/>
                <MetaTags/>
                <Title text="BlameFree"/>
                <Stylesheet id="leptos" href="/pkg/blamefree.css"/>
            </head>
            <body>
                <App/>
                <script type="module">
                    "import init from '/pkg/riv-webui-frontend.js'; init().catch(console.error);"
                </script>
            </body>
        </html>
    }
}

/// Root application component.
///
/// SSR-safe: uses only `leptos` primitives, `leptos_meta`, and
/// `leptos_router`. No browser-only APIs, so it compiles for both the native
/// `ssr` build and the wasm `hydrate` build.
#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Router>
            <Routes fallback=|| view! { "Page not found" }>
                <Route path=path!("/") view=HomePage/>
            </Routes>
        </Router>
    }
}

/// Minimal placeholder home page confirming SSR works.
#[component]
fn HomePage() -> impl IntoView {
    view! {
        <h1>"BlameFree"</h1>
        <p>"Server-side rendering is working."</p>
    }
}
