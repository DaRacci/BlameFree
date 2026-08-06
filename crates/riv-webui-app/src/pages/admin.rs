use leptos::html;
use leptos::prelude::*;
use lucide_leptos::{ArrowDownToLine, ClipboardList, TriangleAlert};
use riv_webui_shared::admin::LogsResponse;

use crate::components::log_text::LogText;
#[cfg(target_arch = "wasm32")]
use riv_webui_shared::routes::API_ADMIN_LOGS_STREAM;

#[cfg(target_arch = "wasm32")]
use {crate::sse, futures::StreamExt};

#[server]
async fn read_admin_logs() -> Result<LogsResponse, ServerFnError> {
    let services = use_context::<crate::AppServices>()
        .ok_or_else(|| ServerFnError::new("missing app services"))?;

    (services.read_admin_logs)(())
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
            <span>"Loading logs..."</span>
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

#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
pub fn AdminPage() -> impl IntoView {
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
        <div class="admin-page">
            <div class="page-header">
                <h1 class="page-header__title">"Admin"</h1>
            </div>

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
        </div>
    }
}
