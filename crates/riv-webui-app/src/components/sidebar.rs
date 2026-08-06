use leptos::prelude::*;
use leptos::{component, view};
use leptos_router::hooks::use_location;
use lucide_leptos::{LayoutDashboard, Menu, Settings, X};

use crate::AuthContext;

const ICON_SIZE: usize = 18;

#[component]
pub fn Sidebar() -> impl IntoView {
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
        let p = loc.pathname.get();
        let active = if path == "/" {
            p == "/"
        } else {
            p.starts_with(path)
        };
        if active { "sidebar__item--active" } else { "" }
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
            {move || {
                if mobile_open.get() {
                    view! { <X size=24 /> }.into_any()
                } else {
                    view! { <Menu size=24 /> }.into_any()
                }
            }}
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
