use gloo_net::http::Request;
use leptos::mount::mount_to_body;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::{Html, provide_meta_context};
use leptos_router::components::{Route, Router, Routes};
use leptos_router::hooks::use_location;
use leptos_router::path;
use lucide_leptos::{LayoutDashboard, Menu, Settings};
use riv_webui_shared::auth::AuthUser;
use riv_webui_shared::routes::API_CONFIG;
use wasm_bindgen::prelude::wasm_bindgen;

use crate::pages::{admin::AdminPage, four_zero_four::FourZeroFourPage, home::HomePage};

/// Minimal config shape matching backend GET /api/config.
/// Only fields consumed by the frontend are parsed.
#[derive(serde::Deserialize)]
struct ConfigResponse {
    oauth: Option<serde_json::Value>,
}

const ICON_SIZE: usize = 18;

/// Context value shared across components.
#[derive(Clone)]
pub struct AuthContext {
    pub user: RwSignal<Option<AuthUser>>,
    pub auth_enabled: RwSignal<bool>,
}

#[wasm_bindgen(start)]
pub fn main() {
    _ = console_log::init_with_level(log::Level::Debug);
    console_error_panic_hook::set_once();
    mount_to_body(|| view! { <App/> });
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    let auth_ctx = AuthContext {
        user: RwSignal::new(None),
        auth_enabled: RwSignal::new(false),
    };
    provide_context(auth_ctx.clone());

    spawn_local(async move {
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

    view! {
        <Html attr:lang="en" attr:dir="ltr" />
        <Router>
            <div class="app-shell">
                <Sidebar />
                <main class="main-content">
                    <div class="content-container">
                        <Routes fallback=|| view! { <div class="state-container"><h2>"404"</h2></div> }>
                            <Route path=path!("/") view=|| view! { <HomePage /> } />
                            <Route path=path!("/admin") view=|| view! { <AdminPage /> } />
                            <Route path=path!("/*") view=|| view! { <FourZeroFourPage /> } />
                        </Routes>
                    </div>
                </main>
            </div>
        </Router>
    }
}

#[component]
fn Sidebar() -> impl IntoView {
    let initial_collapsed = web_sys::window()
        .unwrap()
        .inner_width()
        .ok()
        .and_then(|v| v.as_f64())
        .map(|w| w < 1200.0)
        .unwrap_or(false);
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
                    <div class="sidebar-overlay sidebar-overlay--open" on:click=move |_| set_mobile_open.set(false)></div>
                }.into_any()
            } else {
                view! { <span></span> }.into_any()
            }
        }}

        <nav class=sidebar_class aria-label="Main navigation">
            <div class="sidebar__header">
                <button class="sidebar__toggle" on:click=toggle_collapsed aria-label="Toggle sidebar">
                    <Menu size=24 />
                </button>
                <span class="sidebar__brand">"Review Harness"</span>
            </div>

            <ul class="sidebar__nav">
                <li>
                    <a href="/" class=move || format!("sidebar__item {}", is_active("/")) on:click=close_mobile>
                        <span class="sidebar__icon"><LayoutDashboard size=ICON_SIZE /></span>
                        <span class="sidebar__label">"Dashboard"</span>
                    </a>
                </li>
                <li>
                    <a href="/admin" class=move || format!("sidebar__item {}", is_active("/admin")) on:click=close_mobile>
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
                            <a href="/auth/login" class="btn btn--primary sidebar__login" on:click=close_mobile>
                                "Log in"
                            </a>
                        </div>
                    }.into_any();
                };
                let username = u.name.clone().unwrap_or(u.login.clone());
                let avatar = u.avatar_url.clone().map(|url| {
                    view! {
                        <img src=url alt="Avatar" class="sidebar__avatar" />
                    }
                });
                view! {
                    <div class="sidebar__auth">
                        <div class="sidebar__user">
                            {avatar}
                            <span class="sidebar__username">{username}</span>
                        </div>
                        <a href="/auth/logout" class="btn btn--ghost sidebar__logout" on:click=close_mobile>
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
