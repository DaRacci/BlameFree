use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;
use lucide_leptos::{ArrowDownToLine, ClipboardList, TriangleAlert};
use riv_webui_shared::admin::LogsResponse;
use riv_webui_shared::config::{AvailableModel, CatalogPage};

use crate::components::logs::log_text::LogText;
use crate::components::popovers::model_popover::ModelPopover;
use crate::components::state::empty_state::EmptyState;
#[cfg(target_arch = "wasm32")]
use riv_webui_shared::routes::API_ADMIN_LOGS_STREAM;

#[cfg(target_arch = "wasm32")]
use {crate::sse, futures::StreamExt};

/// Catalog page size for the admin model picker (server-side pagination).
const CATALOG_PER_PAGE: usize = 20;

#[server]
async fn read_admin_logs() -> Result<LogsResponse, ServerFnError> {
    let services = use_context::<crate::AppServices>()
        .ok_or_else(|| ServerFnError::new("missing app services"))?;

    (services.read_admin_logs)(())
        .await
        .map_err(ServerFnError::new)
}

#[server]
async fn read_allowed_models() -> Result<Vec<String>, ServerFnError> {
    let services = use_context::<crate::AppServices>()
        .ok_or_else(|| ServerFnError::new("missing app services"))?;

    (services.list_allowed_models)(())
        .await
        .map_err(ServerFnError::new)
}

#[server]
async fn save_allowed_models(ids: Vec<String>) -> Result<Vec<String>, ServerFnError> {
    let services = use_context::<crate::AppServices>()
        .ok_or_else(|| ServerFnError::new("missing app services"))?;

    (services.set_allowed_models)(ids)
        .await
        .map_err(ServerFnError::new)
}

#[server]
async fn read_model_catalog(
    page: usize,
    per_page: usize,
    query: String,
) -> Result<CatalogPage, ServerFnError> {
    let services = use_context::<crate::AppServices>()
        .ok_or_else(|| ServerFnError::new("missing app services"))?;

    (services.list_model_catalog)((page, per_page, query))
        .await
        .map_err(ServerFnError::new)
}

fn status_class(status: &str) -> &'static str {
    match status {
        "connected" => "log-viewer__status-dot log-viewer__status-dot--connected",
        "connecting" => "log-viewer__status-dot log-viewer__status-dot--connecting",
        _ => "log-viewer__status-dot log-viewer__status-dot--disconnected",
    }
}

fn status_label(status: &str) -> String {
    if status == "connected" {
        "Connected".to_string()
    } else if status == "connecting" {
        "Connecting...".to_string()
    } else if status.starts_with("error:") {
        status.to_string()
    } else {
        "Disconnected".to_string()
    }
}

fn render_loading() -> AnyView {
    view! {
        <div class="admin-loading">
            <span>"Loading..."</span>
        </div>
    }
    .into_any()
}

fn render_error(error: String) -> AnyView {
    view! {
        <div class="admin-empty">
            <div class="admin-empty__icon"><TriangleAlert size=24 /></div>
            <div class="admin-empty__title">"Error"</div>
            <div class="admin-empty__desc">{error}</div>
        </div>
    }
    .into_any()
}

fn render_unavailable(message: Option<String>) -> AnyView {
    view! {
        <div class="admin-empty">
            <div class="admin-empty__icon"><ClipboardList size=24 /></div>
            <div class="admin-empty__title">"Log File Not Configured"</div>
            <div class="admin-empty__desc">{message.unwrap_or_default()}</div>
        </div>
    }
    .into_any()
}

fn render_log_view(
    logs: String,
    connection_status: String,
    log_container_ref: NodeRef<html::Div>,
    follow: ReadSignal<bool>,
    toggle_follow: Callback<()>,
) -> AnyView {
    let line_count = logs.lines().count();
    let status_label = status_label(&connection_status);
    let follow_class = if follow.get() {
        "log-viewer__follow-btn log-viewer__follow-btn--active"
    } else {
        "log-viewer__follow-btn"
    };
    view! {
        <div class="log-viewer">
            <div class="log-viewer__toolbar">
                <span class="log-viewer__toolbar-label">{format!("{} lines", line_count)}</span>
                <span class="log-viewer__status">
                    <span class=status_class(&connection_status) title=status_label.clone()></span>
                    <span class="log-viewer__status-text">{status_label}</span>
                </span>
                <button
                    class=follow_class
                    title=if follow.get() {
                        "Stop following the latest log output"
                    } else {
                        "Jump to the latest log output"
                    }
                    on:click=move |_| toggle_follow.run(())
                >
                    <ArrowDownToLine size=14 />
                    {if follow.get() { "Following" } else { "Follow" }}
                </button>
            </div>
            <LogText text=logs container_ref=log_container_ref />
        </div>
    }
    .into_any()
}

/// Server console logs section (unchanged behavior, extracted for reuse).
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
pub fn LogsSection() -> impl IntoView {
    let (logs, set_logs) = signal(String::new());
    let (loading, set_loading) = signal(true);
    let (error, set_error) = signal::<Option<String>>(None);
    let (available, set_available) = signal(true);
    let (status_msg, set_status_msg) = signal::<Option<String>>(None);
    let (connection_status, set_connection_status) = signal("connecting".to_string());
    let (live_line_count, set_live_line_count) = signal(0usize);
    let (follow, set_follow) = signal(true);
    let toggle_follow = Callback::new(move |_: ()| set_follow.update(|f| *f = !*f));
    let initial_logs = Resource::new(|| (), |_| async { read_admin_logs().await });
    let log_container_ref: NodeRef<html::Div> = NodeRef::new();

    #[cfg(target_arch = "wasm32")]
    Effect::new(move || {
        if !loading.get() {
            return;
        }

        if let Some(result) = initial_logs.get() {
            match result {
                Ok(data) => {
                    set_available.set(data.available);
                    set_status_msg.set(data.message.clone());
                    if data.available {
                        let initial_count = data.logs.lines().count();
                        set_live_line_count.set(initial_count);
                        set_logs.set(data.logs);
                    }
                    set_loading.set(false);
                }
                Err(err) => {
                    set_error.set(Some(err.to_string()));
                    set_loading.set(false);
                }
            }
        }
    });

    #[cfg(target_arch = "wasm32")]
    {
        let sse_logs = set_logs;
        let sse_lines = set_live_line_count;
        let sse_conn = set_connection_status;
        leptos::task::spawn_local(async move {
            match sse::connect_sse_with_status(&API_ADMIN_LOGS_STREAM, sse_conn).await {
                Ok(mut rx) => {
                    while let Some(chunk) = rx.next().await {
                        sse_logs.update(|s| {
                            if !s.is_empty() {
                                s.push('\n');
                            }
                            s.push_str(&chunk);
                        });
                        sse_lines.update(|n| *n += chunk.lines().count());
                    }
                    sse_conn.set("disconnected".into());
                }
                Err(e) => {
                    sse_conn.set(format!("error: {e}"));
                }
            }
        });
    }

    #[cfg(target_arch = "wasm32")]
    Effect::new(move || {
        let _ = logs.get();
        let _ = live_line_count.get();
        if follow.get() && !loading.get() {
            if let Some(container) = log_container_ref.get() {
                container.set_scroll_top(container.scroll_height());
            }
        }
    });

    view! {
        <div class="admin-section">
            <div class="admin-section__header">
                <h2 class="admin-section__title">"Server Logs"</h2>
                <span class="admin-section__badge">"console"</span>
            </div>

            <Suspense fallback=render_loading>
                {move || {
                    if !loading.get() {
                        if let Some(err) = error.get() {
                            return Some(render_error(err));
                        }

                        if !available.get() {
                            return Some(render_unavailable(status_msg.get()));
                        }

                        return Some(render_log_view(
                            logs.get(),
                            connection_status.get(),
                            log_container_ref.clone(),
                            follow,
                            toggle_follow,
                        ));
                    }

                    initial_logs.get().map(|result| match result {
                        Ok(data) => {
                            if data.available {
                                render_log_view(
                                    data.logs,
                                    "connecting".to_string(),
                                    log_container_ref.clone(),
                                    follow,
                                    toggle_follow,
                                )
                            } else {
                                render_unavailable(data.message)
                            }
                        }
                        Err(err) => render_error(err.to_string()),
                    })
                }}
            </Suspense>
        </div>
    }
}

/// Admin model allowlist picker.
///
/// Loads the current allowlist plus a server-side paginated slice of the live
/// OpenRouter catalog. Checkboxes toggle allowlist membership; Save replaces the
/// whole allowlist. Prev/Next re-fetch the server-side page.
#[component]
pub fn ModelsSection() -> impl IntoView {
    let (selected, set_selected) = signal::<Vec<String>>(Vec::new());
    let (_, set_catalog) = signal::<Vec<AvailableModel>>(Vec::new());
    let (page, set_page) = signal(1usize);
    let (query, set_query) = signal(String::new());
    let (total, set_total) = signal(0usize);
    let (loading, set_loading) = signal(true);
    let (saving, set_saving) = signal(false);
    let (error, set_error) = signal::<Option<String>>(None);
    let (status, set_status) = signal::<Option<String>>(None);

    let load_allowlist = Resource::new(|| (), |_| async { read_allowed_models().await });
    let catalog_page = Resource::new(
        move || (page.get(), query.get()),
        move |(p, q)| async move { read_model_catalog(p, CATALOG_PER_PAGE, q).await },
    );
    let total_pages = move || total.get().div_ceil(CATALOG_PER_PAGE).max(1);

    // Apply the loaded allowlist once available.
    Effect::new(move || {
        if let Some(result) = load_allowlist.get() {
            match result {
                Ok(ids) => {
                    set_selected.set(ids);
                }
                Err(err) => set_error.set(Some(err.to_string())),
            }
        }
    });

    // Reflect the server-side catalog page into local state.
    Effect::new(move || {
        if let Some(result) = catalog_page.get() {
            match result {
                Ok(resp) => {
                    set_catalog.set(resp.models);
                    set_total.set(resp.total);
                    set_error.set(None);
                    set_loading.set(false);
                }
                Err(err) => {
                    set_error.set(Some(err.to_string()));
                    set_loading.set(false);
                }
            }
        }
    });

    let save = move |_| {
        set_saving.set(true);
        set_status.set(None);
        let ids = selected.get_untracked();
        spawn_local(async move {
            match save_allowed_models(ids).await {
                Ok(ids) => {
                    set_selected.set(ids);
                    set_status.set(Some("Allowlist saved.".to_string()));
                }
                Err(err) => {
                    set_status.set(Some(format!("Save failed: {err}")));
                }
            }
            set_saving.set(false);
        });
    };

    let selected_count = move || selected.get().len();

    view! {
        <div class="admin-section">
            <div class="admin-section__header">
                <h2 class="admin-section__title">"Models"</h2>
                <span class="admin-section__badge">"allowlist"</span>
            </div>

            <div class="admin-section__body">
                <p class="text-secondary">
                    "Enable the models reviewers may use. Changes take effect immediately; review and benchmark forms only show enabled models."
                </p>

                {move || {
                    if let Some(err) = error.get() {
                        render_error(err)
                    } else {
                        view! {
                            <div class="admin-models">
                                <div class="admin-models__searchbar">
                                    <input
                                        type="search"
                                        class="input"
                                        placeholder="Search models by name, id, or capability..."
                                        prop:value=query
                                        on:input=move |ev| {
                                            let v = event_target_value(&ev);
                                            set_page.set(1);
                                            set_query.set(v);
                                        }
                                    />
                                </div>
                                <div class="admin-models__toolbar">
                                    <span class="admin-models__count">
                                        {move || format!("{} enabled / {} total", selected_count(), total.get())}
                                    </span>
                                    <span class="admin-models__pager">
                                        <button
                                            type="button"
                                            class="btn btn--secondary"
                                            disabled=move || loading.get() || page.get() <= 1
                                            on:click=move |_| set_page.update(|p| *p = p.saturating_sub(1).max(1))
                                        >
                                            "Prev"
                                        </button>
                                        <span class="admin-models__page">
                                            {move || format!("Page {} / {}", page.get(), total_pages())}
                                        </span>
                                        <button
                                            type="button"
                                            class="btn btn--secondary"
                                            disabled=move || loading.get() || (page.get() >= total_pages())
                                            on:click=move |_| set_page.update(|p| *p += 1)
                                        >
                                            "Next"
                                        </button>
                                    </span>
                                </div>

                                <Suspense fallback=move || render_loading()>
                                    {move || {
                                        match catalog_page.get() {
                                            Some(Ok(resp)) => {
                                                let models = resp.models;
                                                if models.is_empty() {
                                                    view! {
                                                        <EmptyState message="No models in the catalog. Check the OpenRouter API connection." />
                                                    }
                                                    .into_any()
                                                } else {
                                                    view! {
                                                        <div class="admin-models__grid">
                                                            {models.into_iter().map(|m| {
                                                                let id = m.id.clone();
                                                                let reasoning = m.reasoning;
                                                                let checked_id = id.clone();
                                                                let click_id = id.clone();
                                                                let display_id = id.clone();
                                                                view! {
                                                                    <div class="admin-model-card">
                                                                        <ModelPopover model=m>
                                                                            <label class="admin-model-card__row">
                                                                                <input
                                                                                    type="checkbox"
                                                                                    prop:checked=move || selected.get().contains(&checked_id)
                                                                                    on:click=move |_| {
                                                                                        set_selected.update(|s| {
                                                                                            if let Some(pos) = s.iter().position(|k| k == &click_id) {
                                                                                                s.remove(pos);
                                                                                            } else {
                                                                                                s.push(click_id.clone());
                                                                                            }
                                                                                        });
                                                                                    }
                                                                                />
                                                                                <span class="admin-model-card__id">{display_id}</span>
                                                                                {reasoning.then(|| {
                                                                                    view! { <span class="admin-model-card__badge">"reasoning"</span> }
                                                                                })}
                                                                            </label>
                                                                        </ModelPopover>
                                                                    </div>
                                                                }
                                                            }).collect::<Vec<_>>()}
                                                        </div>
                                                    }
                                                    .into_any()
                                                }
                                            }
                                            Some(Err(err)) => render_error(err.to_string()),
                                            None => render_loading(),
                                        }
                                    }}
                                </Suspense>

                                <div class="admin-models__footer">
                                    {move || {
                                        status.get().map(|msg| {
                                            view! { <p class="admin-models__status">{msg}</p> }
                                        })
                                    }}
                                    <button
                                        type="button"
                                        class="btn btn--primary"
                                        disabled=move || saving.get()
                                        on:click=save
                                    >
                                        {move || if saving.get() { "Saving..." } else { "Save Allowlist" }}
                                    </button>
                                </div>
                            </div>
                        }
                        .into_any()
                    }
                }}
            </div>
        </div>
    }
}

#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
pub fn AdminPage() -> impl IntoView {
    view! {
        <div class="admin-page">
            <div class="page-header">
                <h1 class="page-header__title">"Admin"</h1>
            </div>

            <LogsSection />
            <ModelsSection />
        </div>
    }
}
