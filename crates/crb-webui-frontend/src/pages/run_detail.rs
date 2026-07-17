use crate::components::metrics_card::MetricsCard;
use crate::components::progress_bar::ProgressBar;
use crb_types::benchmark::MetricsProvider;
use crb_webui_shared::{
    route,
    runs::{PrResultRow, RunDetail},
};
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;
use lucide_leptos::{ArrowLeft, Play, TriangleAlert};

#[component]
pub fn RunDetailPage() -> impl IntoView {
    let params = use_params_map();
    let run_id = move || params.get().get("id").unwrap_or_default();

    let (run, set_run) = signal::<Option<RunDetail>>(None);
    let (loading, set_loading) = signal(true);
    let (error, set_error) = signal::<Option<String>>(None);

    let _fetch = LocalResource::new(move || {
        let id = run_id();
        let set_run = set_run;
        let set_loading = set_loading;
        let set_error = set_error;
        async move {
            set_loading.set(true);
            set_error.set(None);
            match crate::fetch_json(&route!(API_RUNS_ID, id)).await {
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
    });

    view! {
        <div class="run-detail-page">
            <A href=move || "/".to_string()>
                <ArrowLeft size=16 />" Dashboard"
            </A>

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
                    }.into_any()
                } else if loading.get() {
                            view! {
                                <><div style="text-align: center; padding: 2rem; color: var(--text-secondary, #64748b); font-style: italic;">
                                    "Loading run details..."
                                </div></>
                            }.into_any()
                        } else if let Some(e) = error.get() {
                            view! {
                                <><div class="error-state" role="alert">
                                    <div class="error-state__icon"><TriangleAlert size=24 /></div>
                                    <h3 class="error-state__heading">"Failed to load run details"</h3>
                                    <p class="error-state__message">{e}</p>
                                    <div class="error-state__action">
                                        <button class="btn btn--primary" on:click=move |_| set_loading.set(true)>"Retry"</button>
                                    </div>
                                </div></>
                            }.into_any()
                } else if let Some(detail) = run.get() {
                    let detail_clone = detail.clone();
                    let _detail_clone2 = detail.clone();
                    let detail_id = detail.meta.id.clone();
                    let results_clone = detail.results.clone();
                    let results_clone2 = detail.results.clone();

                    let badge_variant = match detail.meta.status {
                        crb_webui_shared::runs::RunStatus::Completed => "badge--success",
                        crb_webui_shared::runs::RunStatus::Failed => "badge--danger",
                        crb_webui_shared::runs::RunStatus::Running => "badge--warning",
                        crb_webui_shared::runs::RunStatus::Pending | crb_webui_shared::runs::RunStatus::Cancelled => "badge--neutral",
                    };

                    let is_running = detail.meta.status == crb_webui_shared::runs::RunStatus::Running || detail.meta.status == crb_webui_shared::runs::RunStatus::Pending;
                    let _has_results = !detail.results.is_empty();

                    let live_url = format!("/runs/{}/live", detail.meta.id);

                    let name = detail.meta.name.clone();
                    let status: String = detail.meta.status.to_string();
                    let model = detail.meta.model.clone().unwrap_or_default();

                    view! {
                        <div class="page-header">
                            <div>
                                <h1 class="page-header__title">{name}</h1>
                                <div style="display: flex; align-items: center; gap: 8px; margin-top: 4px;">
                                    <span class=format!("badge {}", badge_variant)>
                                        <span class="badge__dot"></span>
                                        <span class="badge__label">{status}</span>
                                    </span>
                                    <span style="font-size: var(--text-sm, 14px); color: var(--text-secondary, #8b949e);">
                                        {"Model: ".to_string()}<span class="code">{model}</span>
                                    </span>
                                </div>
                            </div>
                            <div class="page-header__actions">
                                {move || {
                                    if is_running {
                                        view! {
                                            <a href=&live_url class="btn btn--success">
                                                <span class="btn__icon"><Play size=16 /></span>
                                                <span class="btn__label">"Live View"</span>
                                            </a>
                                        }.into_any()
                                    } else {
                                        view! { <span></span> }.into_any()
                                    }
                                }}
                            </div>
                        </div>

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
                                }.into_any()
                            } else {
                                view! { <span></span> }.into_any()
                            }
                        }}

                        <div class="content-grid content-grid--metrics">
                            {move || {
                                if let Some(ref agg) = detail_clone.aggregate {
                                    view! {
                                        <MetricsCard value={format!("{:.3}", agg.f1())} label="F1 Score" value_style="color: var(--accent-blue, #58a6ff);"/>
                                        <MetricsCard value={format!("{:.3}", agg.precision())} label="Precision" value_style="color: var(--accent-green, #3fb950);"/>
                                        <MetricsCard value={format!("{:.3}", agg.recall())} label="Recall" value_style="color: var(--accent-orange, #f0883e);"/>
                                        <MetricsCard value={detail_clone.meta.total_cost.map(|c| format!("${:.4}", c)).unwrap_or_else(|| "-".into())} label="Total Cost" />
                                        <MetricsCard value={format!("{:.0}s", agg.duration_secs)} label="Duration" />
                                    }.into_any()
                                } else {
                                    let cost_str = detail_clone.meta.total_cost.map(|c| format!("${:.4}", c)).unwrap_or_else(|| "-".into());
                                    let dur_str = detail_clone.meta.duration_secs.map(|d| format!("{:.0}s", d)).unwrap_or_else(|| "-".into());
                                    view! {
                                        <MetricsCard value={"-".to_string()} label="F1 Score" />
                                        <MetricsCard value={"-".to_string()} label="Precision" />
                                        <MetricsCard value={"-".to_string()} label="Recall" />
                                        <MetricsCard value={cost_str} label="Total Cost" />
                                        <MetricsCard value={dur_str} label="Duration" />
                                    }.into_any()
                                }
                            }}
                        </div>

                        <div class="section-header">
                            <h2 class="section-header__title">"Per-PR Results"</h2>
                        </div>

                        <div class="table-wrapper">
                            <table class="table">
                                <thead>
                                    <tr>
                                        <th class="table__th">"# "</th>
                                        <th class="table__th">"Title "</th>
                                        <th class="table__th">"F1 "</th>
                                        <th class="table__th">"Prec "</th>
                                        <th class="table__th">"Rec "</th>
                                        <th class="table__th">"Cost "</th>
                                        <th class="table__th">"Status"</th>
                                        <th class="table__th">"Details"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {move || {
                                        let results_clone2 = results_clone2.clone();
                                        results_clone2.iter().map(|pr: &PrResultRow| {
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
                                                <td class="table__td">{pr_title}</td>
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
                                                        }.into_any()
                                                    } else {
                                                        view! {
                                                            <span
                                                                style="padding: 0.25rem 0.5rem; border: 1px solid #334155; border-radius: 4px; cursor: not-allowed; background: transparent; color: #475569; font-size: 0.8rem; display: inline-block;"
                                                                title="No cached logs available"
                                                            >
                                                                "Logs"
                                                            </span>
                                                        }.into_any()
                                                    }}
                                                </td>
                                            </tr>
                                        }
                                    }).collect::<Vec<_>>()}
                                    }
                                </tbody>
                            </table>
                        </div>
                    }.into_any()
                } else {
                    view! { <><p>"No data."</p></> }.into_any()
                }
            }}
        </div>
    }
}
