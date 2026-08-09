use leptos::ev::MouseEvent;
use leptos::prelude::*;
use leptos::{component, view};
use leptos_router::hooks::use_location;
use lucide_leptos::{LayoutDashboard, Menu, Settings, X};

use crate::AuthContext;

const ICON_SIZE: usize = 18;

#[component]
pub fn Sidebar() -> impl IntoView {
    let initial_collapsed = cfg_select! {
      target_arch = "wasm32" => {
        web_sys::window()
            .and_then(|w| w.inner_width().ok())
            .and_then(|v| v.as_f64())
            .map(|w| w < 1200.0)
            .unwrap_or(false)
      },
      _ => false,
    };

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

    view! {
        <SidebarButton
            mobile_open=mobile_open
            toggle_mobile=toggle_mobile
        />

        <SidebarOverlay
            mobile_open=mobile_open
            set_mobile_open=set_mobile_open
        />

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
                <SidebarItem
                    href="/"
                    label="Dashboard"
                    icon=move || view! { <LayoutDashboard size=ICON_SIZE /> }
                    close_mobile=close_mobile
                />
                <SidebarItem
                    href="/admin"
                    label="Admin"
                    icon=move || view! { <Settings size=ICON_SIZE /> }
                    close_mobile=close_mobile
                />
            </ul>

            <SidebarAuth
                close_mobile=close_mobile
            />

            <div class="sidebar__footer">
                <span class="sidebar__version">{env!("CARGO_PKG_VERSION")}</span>
            </div>
        </nav>
    }
}

#[component]
fn SidebarButton(
    mobile_open: ReadSignal<bool>,
    toggle_mobile: impl Fn(MouseEvent) + 'static,
) -> impl IntoView {
    let icon = move || {
        if mobile_open.get() {
            view! { <X size=24 /> }.into_any()
        } else {
            view! { <Menu size=24 /> }.into_any()
        }
    };

    view! {
        <button
            class="sidebar__hamburger btn btn--ghost"
            aria-label="Toggle navigation menu"
            on:click=toggle_mobile
        >
            {icon}
        </button>
    }
}

#[component]
fn SidebarOverlay(
    mobile_open: ReadSignal<bool>,
    set_mobile_open: WriteSignal<bool>,
) -> impl IntoView {
    move || {
        if mobile_open.get() {
            view! {
                <div
                    class="sidebar-overlay sidebar-overlay--open"
                    on:click=move |_| set_mobile_open.set(false)
                ></div>
            }
            .into_any()
        } else {
            view! { <span></span> }.into_any()
        }
    }
}

#[component]
fn SidebarItem(
    href: &'static str,
    label: &'static str,
    icon: impl IntoView + 'static,
    close_mobile: impl Fn(MouseEvent) + 'static,
) -> impl IntoView {
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

    view! {
        <li>
            <a
                href=href
                class=move || format!("sidebar__item {}", is_active(href))
                on:click=close_mobile
            >
                <span class="sidebar__icon">{icon}</span>
                <span class="sidebar__label">{label}</span>
            </a>
        </li>
    }
}

#[component]
fn SidebarAuth(close_mobile: impl Fn(MouseEvent) + 'static) -> impl IntoView {
    let auth_ctx = use_context::<AuthContext>();
    let auth_ctx2 = auth_ctx.clone();
    let auth_enabled = move || {
        auth_ctx
            .as_ref()
            .map(|ctx| ctx.auth_enabled.get())
            .unwrap_or(false)
    };
    let user = move || auth_ctx2.as_ref().and_then(|ctx| ctx.user.get());

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
        }
        .into_any();
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
    }
    .into_any()
}
