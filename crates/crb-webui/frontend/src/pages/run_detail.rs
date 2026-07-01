use crate::components::log_viewer::LogViewer;
use crate::components::metrics_card::MetricsCard;
use crate::components::progress_bar::ProgressBar;
use crate::components::replay_overlay::ReplayOverlay;
use crate::{api_url, LogsListResponse, PrResult, RunDetail};
use leptos::*;
use leptos_router::*;

#[component]
pub fn RunDetailPage() -> impl IntoView {
    let params = use_params_map();
    let run_id = move || params.get().get("id").cloned().unwrap_or_default();

    let (run, set_run) = create_signal::<Option<RunDetail>>(None);
    let (loading, set_loading) = create_signal(true);
    let (error, set_error) = create_signal::<Option<String>>(None);

    let _fetch = create_local_resource(
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

    // ─── Replay state signals ──────────────────────────────
    let (show_replay, set_show_replay) = create_signal(false);
    let (replay_loading, set_replay_loading) = create_signal(false);

    let run_replay = move |id: String| {
        set_replay_loading.set(true);
        let rl = set_replay_loading.clone();
        let sr = set_show_replay.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let url = api_url(&format!("/api/runs/{}/replay", id));
            match gloo_net::http::Request::post(&url).send().await {
                Ok(r) if r.ok() => {
                    sr.set(true);
                }
                Ok(r) => {
                    log::error!("Replay start returned status: {}", r.status());
                }
                Err(e) => {
                    log::error!("Replay start error: {}", e);
                }
            }
            rl.set(false);
        });
    };

    view! {
        <div class="run-detail-page">
            // ─── Back Link ───────────────────────────────────────────
            <A href=move || "/".to_string()>"< Dashboard"</A>

            {move || {
                if loading.get() {
                    view! {
                        <><div class="content-grid content-grid--metrics">
                            <div class="skeleton skeleton--metric"></div>
                            <div class="skeleton skeleton--metric"></div>
                            <div class="skeleton skeleton--metric"></div>
                            <div class="skeleton skeleton--metric"></div>
                            <div class="skeleton skeleton--metric"></div>
                        </div></>
                    }
                } else if loading.get() {
                            view! {
                                <><div style="text-align: center; padding: 2rem; color: var(--text-secondary, #64748b); font-style: italic;">
                                    "Loading run details..."
                                </div></>
                            }
                        } else if let Some(e) = error.get() {
                            view! {
                                <><div class="error-state" role="alert">
                                    <div class="error-state__icon">"!"</div>
                                    <h3 class="error-state__heading">"Failed to load run details"</h3>
                                    <p class="error-state__message">{format!("Error: {e}")}</p>
                                    <div class="error-state__action">
                                        <button class="btn btn--primary" on:click=move |_| set_loading.set(true)>"Retry"</button>
                                    </div>
                                </div></>
                            }
                } else if let Some(detail) = run.get() {
                    let detail_clone = detail.clone();
                    let _detail_clone2 = detail.clone();
                    let detail_id = detail.id.clone();
                    let detail_id_replay = detail.id.clone();
                    let detail4_id_replay = detail.id.clone();
                    let results_clone = detail.results.clone();
                    let results_clone2 = detail.results.clone();

                    let badge_variant = match detail.status.as_str() {
                        "completed" | "done" => "badge--success",
                        "failed" => "badge--danger",
                        "running" => "badge--warning",
                        _ => "badge--neutral",
                    };

                    let is_running = detail.status == "running" || detail.status == "pending";
                    let _has_results = !detail.results.is_empty();

                    let live_url = format!("/runs/{}/live", detail.id);

                    view! {
                        // ─── Page Header ──────────────────────────────
                        <div class="page-header">
                            <div>
                                <h1 class="page-header__title">{&detail.name}</h1>
                                <div style="display: flex; align-items: center; gap: 8px; margin-top: 4px;">
                                    <span class=format!("badge {}", badge_variant)>
                                        <span class="badge__dot"></span>
                                        <span class="badge__label">{&detail.status}</span>
                                    </span>
                                    <span style="font-size: var(--text-sm, 14px); color: var(--text-secondary, #8b949e);">
                                        {format!("Model: ")}<span class="code">{&detail.model}</span>
                                    </span>
                                </div>
                            </div>
                            <div class="page-header__actions">
                                {move || {
                                    if is_running {
                                        view! {
                                            <a href=&live_url class="btn btn--success">
                                                <span class="btn__icon">""</span>
                                                <span class="btn__label">"Live View"</span>
                                            </a>
                                        }.into_view()
                                    } else {
                                        view! { <span></span> }.into_view()
                                    }
                                }}
                            </div>
                        </div>

                        // ─── Progress ──────────────────────────────────
                        {move || {
                            let total = results_clone.len() as u32;
                            let done = results_clone.iter().filter(|r| r.status.as_deref() == Some("done")).count() as u32;
                            if total > 0 && is_running {
                                let pct = if total > 0 { (done as f64 / total as f64 * 100.0) as u32 } else { 0 };
                                view! {
                                    <div class="card" style="margin-bottom: var(--spacing-lg, 16px);">
                                        <div class="card__body">
                                            <h3 class="card__title">"Progress"</h3>
                                            <ProgressBar value=done max=total label=format!("{} / {} PRs ({}%)", done, total, pct) />
                                        </div>
                                    </div>
                                }.into_view()
                            } else {
                                view! { <span></span> }.into_view()
                            }
                        }}

                        // ─── Metrics ──────────────────────────────────
                        <div class="content-grid content-grid--metrics">
                            {move || {
                                if let Some(ref agg) = detail_clone.aggregate {
                                    view! {
                                        <div class="metric-card">
                                            <p class="metric-card__label">"F1 Score"</p>
                                            <p class="metric-card__value" style="color: var(--accent-blue, #58a6ff);">{format!("{:.3}", agg.avg_f1)}</p>
                                        </div>
                                        <div class="metric-card">
                                            <p class="metric-card__label">"Precision"</p>
                                            <p class="metric-card__value" style="color: var(--accent-green, #3fb950);">{format!("{:.3}", agg.avg_precision)}</p>
                                        </div>
                                        <div class="metric-card">
                                            <p class="metric-card__label">"Recall"</p>
                                            <p class="metric-card__value" style="color: var(--accent-orange, #f0883e);">{format!("{:.3}", agg.avg_recall)}</p>
                                        </div>
                                        <div class="metric-card">
                                            <p class="metric-card__label">"Total Cost"</p>
                                            <p class="metric-card__value">{format!("${:.4}", agg.total_cost)}</p>
                                        </div>
                                        <div class="metric-card">
                                            <p class="metric-card__label">"Duration"</p>
                                            <p class="metric-card__value">{format!("{:.0}s", agg.duration_secs)}</p>
                                        </div>
                                    }.into_view()
                                } else {
                                    let cost_str = detail_clone.total_cost.map(|c| format!("${:.4}", c)).unwrap_or_else(|| "-".into());
                                    let dur_str = detail_clone.duration_secs.map(|d| format!("{:.0}s", d)).unwrap_or_else(|| "-".into());
                                    view! {
                                        <div class="metric-card">
                                            <p class="metric-card__label">"F1 Score"</p>
                                            <p class="metric-card__value">"-"</p>
                                        </div>
                                        <div class="metric-card">
                                            <p class="metric-card__label">"Precision"</p>
                                            <p class="metric-card__value">"-"</p>
                                        </div>
                                        <div class="metric-card">
                                            <p class="metric-card__label">"Recall"</p>
                                            <p class="metric-card__value">"-"</p>
                                        </div>
                                        <div class="metric-card">
                                            <p class="metric-card__label">"Total Cost"</p>
                                            <p class="metric-card__value">{cost_str}</p>
                                        </div>
                                        <div class="metric-card">
                                            <p class="metric-card__label">"Duration"</p>
                                            <p class="metric-card__value">{dur_str}</p>
                                        </div>
                                    }.into_view()
                                }
                            }}
                        </div>

                        // ─── Per-PR Results ────────────────────────────
                        <div class="section-header">
                            <h2 class="section-header__title">"Per-PR Results"</h2>
                        </div>

                        <div class="table-wrapper">
                            <table class="table">
                                <thead>
                                    <tr>
                                        <th class="table__th table__th--sortable">"# " <span class="table__sort-icon">""</span></th>
                                        <th class="table__th table__th--sortable">"Title " <span class="table__sort-icon">""</span></th>
                                        <th class="table__th table__th--sortable">"F1 " <span class="table__sort-icon">""</span></th>
                                        <th class="table__th table__th--sortable">"Prec " <span class="table__sort-icon">""</span></th>
                                        <th class="table__th table__th--sortable">"Rec " <span class="table__sort-icon">""</span></th>
                                        <th class="table__th table__th--sortable">"Cost " <span class="table__sort-icon">""</span></th>
                                        <th class="table__th">"Status"</th>
                                        <th class="table__th">"Details"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {move || {
                                        let results_clone2 = results_clone2.clone();
                                        results_clone2.iter().map(|pr: &PrResult| {
                                        let pr_number = pr.pr_number;
                                        let pr_title = pr.title.clone();
                                        let f1 = pr.f1;
                                        let precision = pr.precision;
                                        let recall = pr.recall;
                                        let cost = pr.cost;
                                        let status = pr.status.clone();
                                        let run_id = detail_id.clone();
                                        let has_agents = pr.has_agents;
                                        let pr_key = pr.pr_key.clone();

                                        let pr_badge = match status.as_deref() {
                                            Some("done") => "badge--success",
                                            Some("failed") => "badge--danger",
                                            Some("reviewing") => "badge--warning",
                                            _ => "badge--neutral",
                                        };
                                        let status_text = status.unwrap_or_else(|| "pending".into());
                                        view! {
                                            <tr class="table__row">
                                                <td class="table__td" style="font-weight: var(--weight-semibold, 600);">{format!("#{}", pr_number)}</td>
                                                <td class="table__td">{&pr_title}</td>
                                                <td class="table__td" style="font-family: var(--font-mono, monospace);">{f1.map(|v| format!("{:.3}", v)).unwrap_or_else(|| "-".into())}</td>
                                                <td class="table__td" style="font-family: var(--font-mono, monospace);">{precision.map(|v| format!("{:.3}", v)).unwrap_or_else(|| "-".into())}</td>
                                                <td class="table__td" style="font-family: var(--font-mono, monospace);">{recall.map(|v| format!("{:.3}", v)).unwrap_or_else(|| "-".into())}</td>
                                                <td class="table__td" style="font-family: var(--font-mono, monospace);">{cost.map(|v| format!("${:.4}", v)).unwrap_or_else(|| "-".into())}</td>
                                                <td class="table__td">
                                                    <span class=format!("badge {}", pr_badge)>
                                                        <span class="badge__dot"></span>
                                                        <span class="badge__label">{status_text}</span>
                                                    </span>
                                                </td>
                                                <td class="table__td">
                                                    {if has_agents {
                                                        view! {
                                                            <span style="border: 1px solid #475569; border-radius: 4px; display: inline-block;">
                                                                <A
                                                                    href=move || format!("/runs/{}/prs/{}", run_id.clone(), pr_key.clone())
                                                                    attr:style="padding: 0.25rem 0.5rem; border: 0; background: transparent; color: #94a3b8; font-size: 0.8rem; text-decoration: none; display: inline-block; cursor: pointer;"
                                                                >
                                                                    "Logs"
                                                                </A>
                                                            </span>
                                                        }.into_view()
                                                    } else {
                                                        view! {
                                                            <span
                                                                style="padding: 0.25rem 0.5rem; border: 1px solid #334155; border-radius: 4px; cursor: not-allowed; background: transparent; color: #475569; font-size: 0.8rem; display: inline-block;"
                                                                title="No cached logs available"
                                                            >
                                                                "Logs"
                                                            </span>
                                                        }.into_view()
                                                    }}
                                                </td>
                                            </tr>
                                        }
                                    }).collect::<Vec<_>>()}
                                    }
                                </tbody>
                            </table>
                        </div>

                        // ─── Replay Section ────────────────────────────
                        {
                            let replay_id = detail_id_replay.clone();
                            move || {
                                let run = run.get();
                                let is_completed = run.as_ref().map(|r| {
                                    r.status == "completed" || r.status == "done"
                                }).unwrap_or(false);
                                if is_completed {
                                    let replay_id = replay_id.clone();
                                    view! {
                                        <div class="section-header" style="margin-top: 24px;">
                                            <h2 class="section-header__title">"Replay"</h2>
                                        </div>
                                        <div style="display: flex; gap: 8px; margin-bottom: 12px;">
                                            <button
                                                class="btn btn--primary"
                                                disabled=move || replay_loading.get()
                                                on:click=move |_| run_replay(replay_id.clone())
                                            >
                                                {move || if replay_loading.get() {
                                                    "Starting replay..."
                                                } else {
                                                    "Replay from Cache"
                                                }}
                                            </button>
                                        </div>
                                    }.into_view()
                                } else {
                                    view! { <span></span> }.into_view()
                                }
                            }
                        }

                        // ─── Replay Overlay ───────────────────────────
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
                } else {
                    view! { <><p>"No data."</p></> }
                }
            }}
        </div>
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
