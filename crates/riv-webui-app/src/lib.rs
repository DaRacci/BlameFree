use std::{pin::Pin, sync::Arc};

use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::hooks::use_location;
use leptos_router::path;
use lucide_leptos::{LayoutDashboard, Menu, Settings};
use riv_types::review::Review;
#[cfg(target_arch = "wasm32")]
use riv_webui_shared::routes::API_CONFIG;
use riv_webui_shared::{admin::LogsResponse, auth::AuthUser};

pub mod async_resource;
pub mod components;
pub mod pages;

#[cfg(target_arch = "wasm32")]
pub mod sse;

pub use riv_webui_shared::config::AppConfig;

pub type AppServiceFuture<T> = Pin<Box<dyn Future<Output = Result<T, String>> + Send>>;
pub type AppServiceFn<T> = Arc<dyn Fn() -> AppServiceFuture<T> + Send + Sync>;

#[derive(Clone)]
pub struct AppServices {
    pub list_reviews: AppServiceFn<Vec<Review>>,
    pub read_admin_logs: AppServiceFn<LogsResponse>,
}

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
            </body>
        </html>
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(serde::Deserialize)]
struct ConfigResponse {
    oauth: Option<serde_json::Value>,
}

#[derive(Clone)]
pub struct AuthContext {
    pub user: RwSignal<Option<AuthUser>>,
    pub auth_enabled: RwSignal<bool>,
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    let auth_ctx = AuthContext {
        user: RwSignal::new(None),
        auth_enabled: RwSignal::new(false),
    };
    provide_context(auth_ctx.clone());

    #[cfg(target_arch = "wasm32")]
    leptos::task::spawn_local(async move {
        use gloo_net::http::Request;

        let resp = Request::get(API_CONFIG).send().await;
        if let Ok(resp) = resp {
            if let Ok(config) = resp.json::<ConfigResponse>().await {
                let enabled = config.oauth.is_some();
                auth_ctx.auth_enabled.set(enabled);
                if enabled {
                    let user_resp = Request::get("/auth/me").send().await;
                    if let Ok(user_resp) = user_resp {
                        if user_resp.ok() {
                            if let Ok(user) = user_resp.json::<AuthUser>().await {
                                auth_ctx.user.set(Some(user));
                            }
                        }
                    }
                }
            }
        }
    });

    use pages::admin::AdminPage;
    use pages::four_zero_four::FourZeroFourPage;
    use pages::home::HomePage;
    use pages::new_benchmark::NewBenchmarkPage;
    use pages::new_review::NewReviewPage;
    use pages::review_detail::ReviewDetailPage;
    use pages::review_live::ReviewLivePage;

    view! {
        <Html attr:lang="en" attr:dir="ltr" />
        <Router>
            <div class="app-shell">
                <Sidebar />
                <main class="main-content">
                    <div class="content-container">
                        <Routes fallback=|| view! { <FourZeroFourPage /> }>
                            <Route path=path!("/") view=HomePage />
                            <Route path=path!("/reviews/new") view=NewReviewPage />
                            <Route path=path!("/reviews/:id/live") view=ReviewLivePage />
                            <Route path=path!("/reviews/:id") view=ReviewDetailPage />
                            <Route path=path!("/benchmarks/new") view=NewBenchmarkPage />
                            <Route path=path!("/admin") view=AdminPage />
                        </Routes>
                    </div>
                </main>
            </div>
        </Router>
    }
}

const ICON_SIZE: usize = 18;

#[component]
fn Sidebar() -> impl IntoView {
    #[cfg(target_arch = "wasm32")]
    let initial_collapsed = web_sys::window()
        .and_then(|w| w.inner_width().ok())
        .and_then(|v| v.as_f64())
        .map(|w| w < 1200.0)
        .unwrap_or(false);
    #[cfg(not(target_arch = "wasm32"))]
    let initial_collapsed = false;

    let (collapsed, set_collapsed) = signal(initial_collapsed);
    let (mobile_open, set_mobile_open) = signal(false);

    let toggle_collapsed = move |_| set_collapsed.update(|v| *v = !*v);
    let toggle_mobile = move |_| set_mobile_open.update(|v| *v = !*v);
    let close_mobile = move |_| set_mobile_open.set(false);

    let sidebar_class = move || {
        let mut cls = "sidebar".to_string();
        if collapsed.get() {
            cls.push_str(" sidebar--collapsed");
        }
        if mobile_open.get() {
            cls.push_str(" sidebar--mobile-open");
        }
        cls
    };

    let loc = use_location();
    let is_active = move |path: &'static str| -> &'static str {
        if loc.pathname.get().starts_with(path) {
            "sidebar__item--active"
        } else {
            ""
        }
    };

    let auth_ctx = use_context::<AuthContext>();
    let auth_ctx2 = auth_ctx.clone();
    let auth_enabled = move || {
        auth_ctx
            .as_ref()
            .map(|ctx| ctx.auth_enabled.get())
            .unwrap_or(false)
    };
    let user = move || auth_ctx2.as_ref().and_then(|ctx| ctx.user.get());

    view! {
        <button
            class="sidebar__hamburger btn btn--ghost"
            aria-label="Toggle navigation menu"
            on:click=toggle_mobile
        >
            <Menu size=24 />
        </button>

        {move || {
            if mobile_open.get() {
                view! {
                    <div
                        class="sidebar-overlay sidebar-overlay--open"
                        on:click=move |_| set_mobile_open.set(false)
                    ></div>
                }.into_any()
            } else {
                view! { <span></span> }.into_any()
            }
        }}

        <nav class=sidebar_class aria-label="Main navigation">
            <div class="sidebar__header">
                <button
                    class="sidebar__toggle"
                    on:click=toggle_collapsed
                    aria-label="Toggle sidebar"
                >
                    <Menu size=24 />
                </button>
                <span class="sidebar__brand">"Review Harness"</span>
            </div>

            <ul class="sidebar__nav">
                <li>
                    <a
                        href="/"
                        class=move || format!("sidebar__item {}", is_active("/"))
                        on:click=close_mobile
                    >
                        <span class="sidebar__icon"><LayoutDashboard size=ICON_SIZE /></span>
                        <span class="sidebar__label">"Dashboard"</span>
                    </a>
                </li>
                <li>
                    <a
                        href="/admin"
                        class=move || format!("sidebar__item {}", is_active("/admin"))
                        on:click=close_mobile
                    >
                        <span class="sidebar__icon"><Settings size=ICON_SIZE /></span>
                        <span class="sidebar__label">"Admin"</span>
                    </a>
                </li>
            </ul>

            {move || {
                if !auth_enabled() {
                    return view! { <span></span> }.into_any();
                }
                let Some(u) = user() else {
                    return view! {
                        <div class="sidebar__auth">
                            <a
                                href="/auth/login"
                                class="btn btn--primary sidebar__login"
                                on:click=close_mobile
                            >
                                "Log in"
                            </a>
                        </div>
                    }.into_any();
                };
                let username = u.name.clone().unwrap_or(u.login.clone());
                let avatar = u.avatar_url.clone().map(|url| {
                    view! { <img src=url alt="Avatar" class="sidebar__avatar" /> }
                });
                view! {
                    <div class="sidebar__auth">
                        <div class="sidebar__user">
                            {avatar}
                            <span class="sidebar__username">{username}</span>
                        </div>
                        <a
                            href="/auth/logout"
                            class="btn btn--ghost sidebar__logout"
                            on:click=close_mobile
                        >
                            "Log out"
                        </a>
                    </div>
                }.into_any()
            }}

            <div class="sidebar__footer">
                <span class="sidebar__version">{env!("CARGO_PKG_VERSION")}</span>
            </div>
        </nav>
    }
}
