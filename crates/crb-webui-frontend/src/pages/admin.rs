use crate::sse;
use crb_webui_shared::admin::LogsResponse;
use crb_webui_shared::routes::API_ADMIN_LOGS;
use crb_webui_shared::routes::API_ADMIN_LOGS_STREAM;
use futures::StreamExt;
use gloo_net::http::Request;
use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;
use lucide_leptos::{ClipboardList, TriangleAlert};

/// Admin page component
#[component]
pub fn AdminPage() -> impl IntoView {
    let (logs, set_logs) = signal::<String>(String::new());
    let (loading, set_loading) = signal(true);
    let (error, set_error) = signal::<Option<String>>(None);
    let (available, set_available) = signal(true);
    let (status_msg, set_status_msg) = signal::<Option<String>>(None);
    let (connection_status, set_connection_status) = signal("connecting".to_string());
    let (live_line_count, set_live_line_count) = signal(0usize);

    spawn_local(async move {
        match Request::get(&API_ADMIN_LOGS).send().await {
            Ok(resp) if resp.ok() => match resp.json::<LogsResponse>().await {
                Ok(data) => {
                    set_available.set(data.available);
                    set_status_msg.set(data.message.clone());
                    if data.available {
                        let initial_count = data.logs.lines().count();
                        set_logs.set(data.logs);
                        set_live_line_count.set(initial_count);
                    }
                }
                Err(e) => {
                    set_error.set(Some(format!("Failed to parse response: {e}")));
                }
            },
            Ok(resp) => {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                set_error.set(Some(format!("Server error ({status}): {text}")));
            }
            Err(e) => {
                set_error.set(Some(format!("Network error: {e}")));
            }
        }
        set_loading.set(false);
    });

    let sse_logs = set_logs;
    let sse_lines = set_live_line_count;
    let sse_conn = set_connection_status;
    spawn_local(async move {
        match sse::connect_sse_with_status(&API_ADMIN_LOGS_STREAM, sse_conn).await {
            Ok(mut rx) => {
                while let Some(line) = rx.next().await {
                    sse_logs.update(|s| {
                        if !s.is_empty() {
                            s.push('\n');
                        }
                        s.push_str(&line);
                    });
                    sse_lines.update(|n| *n += 1);
                }
                sse_conn.set("disconnected".into());
            }
            Err(e) => {
                sse_conn.set(format!("error: {e}"));
            }
        }
    });

    let log_container_ref: NodeRef<html::Div> = NodeRef::new();

    // Auto-scroll to bottom when logs update (only after initial load)
    Effect::new(move || {
        // Depend on logs and live_line_count so this fires on new content
        let _ = logs.get(); // Signal read to create reactivity dependency
        let _ = live_line_count.get(); // Signal read to create reactivity dependency
        if !loading.get() {
            if let Some(container) = log_container_ref.get() {
                container.set_scroll_top(container.scroll_height());
            }
        }
    });

    let status_class = move || {
        let s = connection_status.get();
        match s.as_str() {
            "connected" => "admin-status-dot admin-status-dot--connected",
            "connecting" => "admin-status-dot admin-status-dot--connecting",
            _ => "admin-status-dot admin-status-dot--disconnected",
        }
    };

    let status_label = move || {
        let s = connection_status.get();
        if s == "connected" {
            "Connected"
        } else if s == "connecting" {
            "Connecting..."
        } else if s.starts_with("error:") {
            &s
        } else {
            "Disconnected"
        }
        .to_string()
    };

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

                {move || {
                    if loading.get() {
                        return view! {
                            <div class="admin-loading">
                                <span>"Loading logs..."</span>
                            </div>
                        }.into_any();
                    }

                    if let Some(ref err) = error.get() {
                        return view! {
                            <div class="admin-empty">
                                <div class="admin-empty__icon"><TriangleAlert size=24 /></div>
                                <div class="admin-empty__title">"Error"</div>
                                <div class="admin-empty__desc">{err.clone()}</div>
                            </div>
                        }.into_any();
                    }

                    if !available.get() {
                        return view! {
                            <div class="admin-empty">
                                <div class="admin-empty__icon"><ClipboardList size=24 /></div>
                                <div class="admin-empty__title">"Log File Not Configured"</div>
                                <div class="admin-empty__desc">
                                    {status_msg.get().unwrap_or_default()}
                                </div>
                            </div>
                        }.into_any();
                    }

                    let line_count = logs.get().lines().count();
                    let _conn_status = connection_status.get();

                    view! {
                        <div class="log-viewer">
                            <div class="log-viewer__toolbar">
                                <span class="log-viewer__toolbar-label">
                                    {format!("{} lines", line_count)}
                                </span>
                                <span class={status_class} title={status_label()}></span>
                                <span class="log-viewer__status-text">{status_label()}</span>
                            </div>
                            <div class="log-viewer__content" node_ref=log_container_ref>
                                <pre class="log-viewer__pre">{logs.get()}</pre>
                            </div>
                        </div>
                    }.into_any()
                }}
            </div>
        </div>
    }
}
