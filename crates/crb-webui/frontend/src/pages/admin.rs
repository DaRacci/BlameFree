use crate::api_url;
use leptos::*;
use leptos_router::*;
use serde::Deserialize;

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
/// - Server console log viewer (refreshable)
///
/// Structured as a series of `.admin-section` divs so future admin features
/// (cache management, config editor, system stats, etc.) can be added as
/// sibling sections with minimal changes.
#[component]
pub fn AdminPage() -> impl IntoView {
    // ─── Log viewer state ──────────────────────────────────────
    let (logs, set_logs) = create_signal::<Option<LogsResponse>>(None);
    let (loading, set_loading) = create_signal(true);
    let (error, set_error) = create_signal::<Option<String>>(None);

    /// Fetch logs from the server.
    let fetch_logs = {
        let set_logs = set_logs.clone();
        let set_loading = set_loading.clone();
        let set_error = set_error.clone();
        move || {
            set_loading.set(true);
            set_error.set(None);
            let logs_url = api_url("/api/admin/logs");
            spawn_local(async move {
                match gloo_net::http::Request::get(&logs_url).send().await {
                    Ok(resp) => {
                        if resp.ok() {
                            match resp.json::<LogsResponse>().await {
                                Ok(data) => {
                                    set_logs.set(Some(data));
                                }
                                Err(e) => {
                                    set_error.set(Some(format!("Failed to parse response: {e}")));
                                }
                            }
                        } else {
                            let status = resp.status();
                            let text = resp.text().await.unwrap_or_default();
                            set_error
                                .set(Some(format!("Server error ({status}): {text}")));
                        }
                    }
                    Err(e) => {
                        set_error.set(Some(format!("Network error: {e}")));
                    }
                }
                set_loading.set(false);
            });
        }
    };

    // Fetch logs on mount
    let _ = {
        let fetch = fetch_logs.clone();
        create_effect(move |_| {
            // Only run once on mount
            fetch();
        })
    };

    // ─── Scroll-to-bottom ref for auto-scroll ──────────────────
    let log_container_ref = create_node_ref::<html::Div>();

    // Auto-scroll to bottom when logs update
    create_effect(move |_| {
        if !loading.get() {
            if let Some(container) = log_container_ref.get() {
                container.set_scroll_top(container.scroll_height());
            }
        }
    });

    view! {
        <div class="admin-page">
            // ─── Page Header ──────────────────────────────────────
            <div class="page-header">
                <h1 class="page-header__title">"Admin"</h1>
            </div>

            // ─── Log Viewer Section ───────────────────────────────
            // This is a modular section. Future admin features (cache
            // management, config editor, etc.) would be added as sibling
            // <div class="admin-section"> blocks after this one.
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
                                <button
                                    class="btn btn--primary"
                                    on:click=move |_| fetch_logs()
                                >
                                    "Retry"
                                </button>
                            </div>
                        }.into_view();
                    }

                    if let Some(ref resp) = logs.get() {
                        if !resp.available {
                            return view! {
                                <div class="admin-empty">
                                    <div class="admin-empty__icon">"📋"</div>
                                    <div class="admin-empty__title">"Log File Not Configured"</div>
                                    <div class="admin-empty__desc">
                                        {resp.message.clone().unwrap_or_default()}
                                    </div>
                                </div>
                            }.into_view();
                        }

                        return view! {
                            <div class="log-viewer">
                                <div class="log-viewer__toolbar">
                                    <span class="log-viewer__toolbar-label">
                                        {format!("{} lines", resp.logs.lines().count())}
                                    </span>
                                    <button
                                        class="btn btn--ghost"
                                        on:click=move |_| fetch_logs()
                                    >
                                        "Refresh"
                                    </button>
                                </div>
                                <div class="log-viewer__content" node_ref=log_container_ref>
                                    <pre class="log-viewer__pre">{resp.logs.clone()}</pre>
                                </div>
                            </div>
                        }.into_view();
                    }

                    // Initial state (should not be reached due to fetch on mount)
                    view! { <div class="admin-loading"><span>"Loading..."</span></div> }.into_view()
                }}
            </div>
        </div>
    }
}
