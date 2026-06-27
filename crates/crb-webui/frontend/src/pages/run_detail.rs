use leptos::*;
use leptos_router::*;
use crate::{RunDetail, PrResult, LogsListResponse, api_url};
use crate::components::metrics_card::MetricsCard;
use crate::components::progress_bar::ProgressBar;
use crate::components::log_viewer::LogViewer;
use crate::components::replay_overlay::ReplayOverlay;

#[component]
pub fn RunDetailPage() -> impl IntoView {
    let params = use_params_map();
    let run_id = move || params.get().get("id").cloned().unwrap_or_default();

    let (run, set_run) = create_signal::<Option<RunDetail>>(None);
    let (loading, set_loading) = create_signal(true);
    let (error, set_error) = create_signal::<Option<String>>(None);

    let fetch = create_local_resource(
        move || run_id(),
        move |id| {
            let set_run = set_run.clone();
            let set_loading = set_loading.clone();
            let set_error = set_error.clone();
            async move {
                set_loading.set(true);
                set_error.set(None);
                match get_run_detail(&id).await {
                    Ok(detail) => {
                        set_run.set(Some(detail));
                        set_loading.set(false);
                    }
                    Err(e) => {
                        set_error.set(Some(e));
                        set_loading.set(false);
                    }
                }
            }
        },
    );

    view! {
        <div class="container">
            <a href="/" style="color: #94a3b8; text-decoration: none; display: inline-block; margin-bottom: 1rem;">
                "← Back to Runs"
            </a>

            {move || {
                if loading.get() {
                    view! {
                        <div class="loading-state">
                            <div class="spinner"></div>
                            <span>"Loading run details..."</span>
                        </div>
                    }.into_view()
                } else if let Some(e) = error.get() {
                    view! { <div class="card"><p style="color: #ef4444;">{format!("Error: {e}")}</p></div> }.into_view()
                } else if let Some(detail) = run.get() {
                    view! { <RunDetailView detail=detail.clone() /> }.into_view()
                } else {
                    view! { <p>"No data."</p> }.into_view()
                }
            }}
        </div>
    }
}

#[component]
fn RunDetailView(detail: RunDetail) -> impl IntoView {
    let id_clone = detail.id.clone();
    let live_url = format!("/runs/{}/live", detail.id);
    let detail2 = detail.clone();
    let detail3 = detail.clone();
    let detail4 = detail.clone();
    let detail4_id = detail4.id.clone();
    let detail4_id_replay = detail4_id.clone();

    // Tab state
    let (active_tab, set_active_tab) = create_signal("results".to_string());
    // Replay overlay visibility
    let (show_replay, set_show_replay) = create_signal(false);

    let status_badge_class = match detail.status.as_str() {
        "completed" | "done" => "badge badge-done",
        "failed" => "badge badge-failed",
        "running" | "pending" => "badge badge-running",
        _ => "badge badge-pending",
    };

    // Fetch logs list (for Logs tab)
    let (logs_list, set_logs_list) = create_signal::<Option<LogsListResponse>>(None);
    let (logs_loading, set_logs_loading) = create_signal(false);
    let (logs_fetched, set_logs_fetched) = create_signal(false);

    // Fetch logs only when Logs tab is activated
    let fetch_logs = move || {
        if !logs_fetched.get() && !logs_loading.get() {
            set_logs_loading.set(true);
            let run_id = id_clone.clone();
            let set_logs = set_logs_list.clone();
            let set_loading = set_logs_loading.clone();
            let set_fetched = set_logs_fetched.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let url = api_url(&format!("/api/runs/{}/logs", run_id));
                match gloo_net::http::Request::get(&url).send().await {
                    Ok(r) if r.ok() => {
                        match r.json::<LogsListResponse>().await {
                            Ok(logs) => {
                                set_logs.set(Some(logs));
                            }
                            Err(e) => {
                                log::error!("Failed to parse logs list: {}", e);
                            }
                        }
                    }
                    Ok(r) => {
                        log::error!("Logs list fetch returned status: {}", r.status());
                    }
                    Err(e) => {
                        log::error!("Logs list fetch error: {}", e);
                    }
                }
                set_loading.set(false);
                set_fetched.set(true);
            });
        }
    };

    let tab_style = |tab_name: &str| -> String {
        let is_active = active_tab.with(|t| t == tab_name);
        let base = "padding: 0.5rem 1.25rem; border: none; cursor: pointer; font-weight: 600; font-size: 0.9rem; border-radius: 6px 6px 0 0;";
        if is_active {
            format!("{} background: #334155; color: #e2e8f0;", base)
        } else {
            format!("{} background: transparent; color: #64748b;", base)
        }
    };

    view! {
        // ─── Page header ──────────────────────────────────────────────
        <div style="display: flex; justify-content: space-between; align-items: flex-start; margin-bottom: 1rem;">
            <div>
                <h1 style="margin: 0;">{&detail.name}</h1>
                <p style="color: #64748b; margin: 0.25rem 0 0 0;">
                    {format!("Model: ")}<span class="code">{&detail.model}</span>
                    {format!(" — ")}
                    <span class=status_badge_class>{&detail.status}</span>
                </p>
            </div>
            <div style="display: flex; gap: 0.5rem;">
                {move || {
                    if detail.status == "running" || detail.status == "pending" {
                        view! {
                            <a href=&live_url class="btn btn-green" target="_blank">
                                "Live View"
                            </a>
                        }.into_view()
                    } else {
                        view! { <span></span> }.into_view()
                    }
                }}
                <button
                    style="padding: 0.5rem 1.25rem; border: none; border-radius: 6px; cursor: pointer; background: #3b82f6; color: white; font-weight: 600; font-size: 0.9rem;"
                    on:click=move |_| set_show_replay.set(true)
                >
                    "▶ Replay Run"
                </button>
            </div>
        </div>

        // ─── Status progress ─────────────────────────────────────────
        {move || {
            let total = detail.results.len() as u32;
            let done = detail.results.iter().filter(|r| r.status.as_deref() == Some("done")).count() as u32;
            if total > 0 {
                let pct = if total > 0 { (done as f64 / total as f64 * 100.0) as u32 } else { 0 };
                view! {
                    <div class="card">
                        <h3>"Progress"</h3>
                        <ProgressBar value=done max=total label=format!("{} / {} PRs ({})", done, total, pct) />
                    </div>
                }.into_view()
            } else {
                view! { <span></span> }.into_view()
            }
        }}

        // ─── Metrics ─────────────────────────────────────────────────
        <div style="display: flex; gap: 1rem; flex-wrap: wrap; margin-bottom: 1rem;">
            {move || {
                if let Some(ref agg) = detail2.aggregate {
                    view! {
                        <MetricsCard value={format!("{:.3}", agg.avg_f1)} label="Avg F1" />
                        <MetricsCard value={format!("{:.3}", agg.avg_precision)} label="Avg Precision" />
                        <MetricsCard value={format!("{:.3}", agg.avg_recall)} label="Avg Recall" />
                        <MetricsCard value={format!("${:.4}", agg.total_cost)} label="Total Cost" />
                        <MetricsCard value={format!("{:.1}s", agg.duration_secs)} label="Duration" />
                        <MetricsCard value={format!("{}", agg.total_prs)} label="Total PRs" />
                    }.into_view()
                } else {
                    let cost_str = detail2.total_cost.map(|c| format!("${:.4}", c)).unwrap_or_else(|| "—".into());
                    let dur_str = detail2.duration_secs.map(|d| format!("{:.1}s", d)).unwrap_or_else(|| "—".into());
                    view! {
                        <MetricsCard value="—" label="Avg F1" />
                        <MetricsCard value="—" label="Avg Precision" />
                        <MetricsCard value="—" label="Avg Recall" />
                        <MetricsCard value=cost_str label="Total Cost" />
                        <MetricsCard value=dur_str label="Duration" />
                        <MetricsCard value={format!("{}", detail2.results.len())} label="Total PRs" />
                    }.into_view()
                }
            }}
        </div>

        // ─── Tab bar ──────────────────────────────────────────────────
        <div style="display: flex; gap: 0; border-bottom: 2px solid #334155; margin-bottom: 0;">
            <button
                style=tab_style("results")
                on:click=move |_| set_active_tab.set("results".to_string())
            >
                "Results"
            </button>
            <button
                style=tab_style("logs")
                on:click=move |_| {
                    set_active_tab.set("logs".to_string());
                    fetch_logs();
                }
            >
                "Logs"
            </button>
        </div>

        // ─── Tab content ──────────────────────────────────────────────
        <div style="background: #1e2938; border-radius: 0 0 8px 8px; padding: 1rem; border: 1px solid #334155; border-top: none;">
            {move || {
                let tab = active_tab.get();
                if tab == "results" {
                    // ─── Results tab ──────────────────────────────────
                    view! {
                        <div class="card" style="background: transparent; border: none; padding: 0;">
                            <h3>"PR Results"</h3>
                            <table>
                                <thead>
                                    <tr>
                                        <th>"#"</th>
                                        <th>"Title"</th>
                                        <th>"F1"</th>
                                        <th>"Precision"</th>
                                        <th>"Recall"</th>
                                        <th>"Cost"</th>
                                        <th>"Status"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {detail3.results.iter().map(|pr| {
                                        let status_class = match pr.status.as_deref() {
                                            Some("done") => "badge badge-done",
                                            Some("failed") => "badge badge-failed",
                                            Some("reviewing") => "badge badge-running",
                                            _ => "badge badge-pending",
                                        };
                                        let status_text = pr.status.clone().unwrap_or_else(|| "pending".into());
                                        view! {
                                            <tr>
                                                <td style="font-weight: 600;">{format!("#{}", pr.pr_number)}</td>
                                                <td>{&pr.title}</td>
                                                <td style="font-family: monospace;">{pr.f1.map(|v| format!("{:.3}", v)).unwrap_or_else(|| "—".into())}</td>
                                                <td style="font-family: monospace;">{pr.precision.map(|v| format!("{:.3}", v)).unwrap_or_else(|| "—".into())}</td>
                                                <td style="font-family: monospace;">{pr.recall.map(|v| format!("{:.3}", v)).unwrap_or_else(|| "—".into())}</td>
                                                <td style="font-family: monospace;">{pr.cost.map(|v| format!("${:.4}", v)).unwrap_or_else(|| "—".into())}</td>
                                                <td><span class=status_class>{status_text}</span></td>
                                            </tr>
                                        }
                                    }).collect::<Vec<_>>()}
                                </tbody>
                            </table>
                        </div>
                    }.into_view()
                } else {
                    // ─── Logs tab ────────────────────────────────────
                    let detail4_id = detail4_id.clone();
                    view! {
                        <div>
                            <h3 style="color: #e2e8f0; margin: 0 0 1rem 0;">"Agent Logs"</h3>
                            {move || {
                                if logs_loading.get() {
                                    view! {
                                        <p style="color: #94a3b8; font-style: italic;">
                                            "Loading logs..."
                                        </p>
                                    }.into_view()
                                } else if let Some(ref logs) = logs_list.get() {
                                    let run_id = detail4_id.clone();
                                    view! {
                                        <LogViewer logs=logs.clone() run_id=run_id />
                                    }.into_view()
                                } else {
                                    view! {
                                        <p style="color: #64748b; font-style: italic;">
                                            "Click the Logs tab to load agent logs."
                                        </p>
                                    }.into_view()
                                }
                            }}
                        </div>
                    }.into_view()
                }
            }}
        </div>

        // ─── Replay Overlay ───────────────────────────────────────────
        {move || {
            let run_id = detail4_id_replay.clone();
            view! {
                <ReplayOverlay
                    visible=show_replay.get()
                    on_close=move || set_show_replay.set(false)
                    run_id=run_id
                />
            }
        }}
    }
}

async fn get_run_detail(id: &str) -> Result<RunDetail, String> {
    let url = api_url(&format!("/api/runs/{}", id));
    let response = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !response.ok() {
        return Err(format!("Server returned {}", response.status()));
    }

    let data: RunDetail = response
        .json()
        .await
        .map_err(|e| format!("Parse error: {}", e))?;

    Ok(data)
}
