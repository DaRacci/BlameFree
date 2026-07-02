use crate::api_url;
use futures::channel::mpsc;
use futures::StreamExt;
use leptos::*;
use serde::Deserialize;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::MessageEvent;

/// Response from GET /api/admin/logs.
#[derive(Debug, Clone, Deserialize)]
struct LogsResponse {
    pub logs: String,
    pub available: bool,
    #[serde(default)]
    pub message: Option<String>,
}

/// Admin page component.
///
/// Provides:
/// - Server console log viewer (live streaming via SSE)
///
/// Structured as a series of `.admin-section` divs so future admin features
/// (cache management, config editor, system stats, etc.) can be added as
/// sibling sections with minimal changes.
#[component]
pub fn AdminPage() -> impl IntoView {
    // ─── Log viewer state ──────────────────────────────────────
    let (logs, set_logs) = create_signal::<String>(String::new());
    let (loading, set_loading) = create_signal(true);
    let (error, set_error) = create_signal::<Option<String>>(None);
    let (available, set_available) = create_signal(true);
    let (status_msg, set_status_msg) = create_signal::<Option<String>>(None);
    // Connection status: "connecting", "connected", "disconnected", "error: ..."
    let (connection_status, set_connection_status) = create_signal("connecting".to_string());
    // Number of live-streamed lines received
    let (live_line_count, set_live_line_count) = create_signal(0usize);

    // ─── Initial fetch via GET /api/admin/logs ─────────────────
    let logs_url = api_url("/api/admin/logs");
    spawn_local(async move {
        match gloo_net::http::Request::get(&logs_url).send().await {
            Ok(resp) => {
                if resp.ok() {
                    match resp.json::<LogsResponse>().await {
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
                    }
                } else {
                    let status = resp.status();
                    let text = resp.text().await.unwrap_or_default();
                    set_error.set(Some(format!("Server error ({status}): {text}")));
                }
            }
            Err(e) => {
                set_error.set(Some(format!("Network error: {e}")));
            }
        }
        set_loading.set(false);
    });

    // ─── SSE connection to /api/admin/logs/stream ──────────────
    let stream_url = api_url("/api/admin/logs/stream");
    let sse_logs = set_logs.clone();
    let sse_lines = set_live_line_count.clone();
    let sse_conn = set_connection_status.clone();
    spawn_local(async move {
        match connect_sse(&stream_url, sse_conn.clone()).await {
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
                // Channel closed — stream ended
                sse_conn.set("disconnected".into());
            }
            Err(e) => {
                sse_conn.set(format!("error: {e}"));
            }
        }
    });

    // ─── Scroll-to-bottom ref for auto-scroll ──────────────────
    let log_container_ref = create_node_ref::<html::Div>();

    // Auto-scroll to bottom when logs update (only after initial load)
    create_effect(move |_| {
        // Depend on logs and live_line_count so this fires on new content
        let _ = logs.get();
        let _ = live_line_count.get();
        if !loading.get() {
            if let Some(container) = log_container_ref.get() {
                container.set_scroll_top(container.scroll_height());
            }
        }
    });

    // ─── Connection status indicator class ─────────────────────
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
            // ─── Page Header ──────────────────────────────────────
            <div class="page-header">
                <h1 class="page-header__title">"Admin"</h1>
            </div>

            // ─── Log Viewer Section ───────────────────────────────
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
                        }.into_view();
                    }

                    if let Some(ref err) = error.get() {
                        return view! {
                            <div class="admin-empty">
                                <div class="admin-empty__icon">"⚠"</div>
                                <div class="admin-empty__title">"Error"</div>
                                <div class="admin-empty__desc">{err.clone()}</div>
                            </div>
                        }.into_view();
                    }

                    if !available.get() {
                        return view! {
                            <div class="admin-empty">
                                <div class="admin-empty__icon">"📋"</div>
                                <div class="admin-empty__title">"Log File Not Configured"</div>
                                <div class="admin-empty__desc">
                                    {status_msg.get().unwrap_or_default()}
                                </div>
                            </div>
                        }.into_view();
                    }

                    let line_count = logs.get().lines().count();
                    let _conn_status = connection_status.get();

                    return view! {
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
                    }.into_view();
                }}
            </div>
        </div>
    }
}

// ─── SSE connection via web_sys::EventSource ────────────────────────────────

/// Connect to an SSE endpoint and return a channel receiver that yields
/// event data strings as they arrive.
async fn connect_sse(
    url: &str,
    set_connected: WriteSignal<String>,
) -> Result<mpsc::UnboundedReceiver<String>, String> {
    let (tx, rx) = mpsc::unbounded();

    let es = web_sys::EventSource::new(url)
        .map_err(|e| format!("Failed to construct EventSource: {:?}", e))?;

    // onopen — connection established
    let on_open = set_connected.clone();
    let open_closure = Closure::wrap(Box::new(move || {
        on_open.set("connected".into());
    }) as Box<dyn FnMut()>);
    es.set_onopen(Some(open_closure.as_ref().unchecked_ref()));
    open_closure.forget();

    // onmessage — new event received
    let tx_clone = tx.clone();
    let msg_closure = Closure::wrap(Box::new(move |event: MessageEvent| {
        if let Some(text) = event.data().as_string() {
            let _ = tx_clone.unbounded_send(text);
        } else {
            log::warn!("SSE message with non-string data");
        }
    }) as Box<dyn FnMut(MessageEvent)>);
    es.set_onmessage(Some(msg_closure.as_ref().unchecked_ref()));
    msg_closure.forget();

    // onerror — connection lost or error
    let on_err = set_connected.clone();
    let es_for_err = es.clone();
    let err_closure = Closure::wrap(Box::new(move || {
        // EventSource auto-reconnects, but we update the status
        // readyState: 0=CONNECTING, 1=OPEN, 2=CLOSED
        if es_for_err.ready_state() == 2 {
            on_err.set("disconnected".into());
        } else {
            on_err.set("connecting".into());
        }
    }) as Box<dyn FnMut()>);
    es.set_onerror(Some(err_closure.as_ref().unchecked_ref()));
    err_closure.forget();

    Ok(rx)
}
