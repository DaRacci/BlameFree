use crate::components::metrics_card::MetricsCard;
use crate::components::progress_bar::ProgressBar;
use riv_types::benchmark::metrics::{Metrics, MetricsProvider};
use riv_types::cost::AnalyticsSnapshot;
use riv_types::review::{Review, ReviewStatus};
use riv_types::vcs::pr::PrMeta;
use riv_webui_shared::{review::RunConfig, route};
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;
use lucide_leptos::{ArrowLeft, Play, TriangleAlert};
use serde::Deserialize;

/// Local API response type — matches the old `RunDetail` JSON format.
/// We keep `results` as a local compat type since the API still returns
/// the old PrResultRow format, which has different fields than `PrResult`.
#[derive(Debug, Clone, Deserialize)]
#[deprecated]
struct RunDetailResponse {
    meta: Review,
    results: Vec<PrResultRowEntry>,
    aggregate: Metrics,
    config: Option<RunConfig>,
}

/// Per-PR result entry in the old API format.
/// Mirror of the deprecated `PrResultRow` — needed for JSON compat.
#[derive(Debug, Clone, Deserialize)]
struct PrResultRowEntry {
    meta: PrMeta,
    metrics: Metrics,
    analytics: AnalyticsSnapshot,
    #[serde(default)]
    status: Option<ReviewStatus>,
    #[serde(default)]
    has_agents: bool,
}

#[component]
pub fn RunDetailPage() -> impl IntoView {
    let params = use_params_map();
    let run_id = move || params.get().get("id").unwrap_or_default();

    let (run, set_run) = signal::<Option<RunDetailResponse>>(None);
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
                                <><div class="text-secondary text-italic" style="text-align: center; padding: 2rem;">
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
                        ReviewStatus::Completed => "badge--success",
                        ReviewStatus::Failed => "badge--danger",
                        ReviewStatus::Running => "badge--warning",
                        ReviewStatus::Pending | ReviewStatus::Cancelled => "badge--neutral",
                    };

                    let is_running = detail.meta.status == ReviewStatus::Running || detail.meta.status == ReviewStatus::Pending;
                    let _has_results = !detail.results.is_empty();

                    let live_url = format!("/runs/{}/live", detail.meta.id);

                    let name = detail.meta.id.to_string();
                    let status: String = detail.meta.status.to_string();

                    view! {
                        <div class="page-header">
                            <div>
                                <h1 class="page-header__title">{name}</h1>
                                <div class="flex-row items-center" style="gap: 8px; margin-top: 4px;">
                                    <span class=format!("badge {}", badge_variant)>
                                        <span class="badge__dot"></span>
                                        <span class="badge__label">{status}</span>
                                    </span>
                                    <span class="text-sm text-secondary">
                                        {"Model: ".to_string()}<span class="code">{"placeholder"}</span>
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
                            let done = results_clone.iter().filter(|r| r.status == Some(ReviewStatus::Completed)).count() as u32;
                            if total > 0 && is_running {
                                let pct = if total > 0 { (done as f64 / total as f64 * 100.0) as u32 } else { 0 };
                                view! {
                                    <div class="card mb-lg">
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
                              view! {
                                  <MetricsCard value={format!("{:.3}", detail_clone.aggregate.f1())} label="F1 Score" value_style="color: var(--accent-blue, #58a6ff);"/>
                                  <MetricsCard value={format!("{:.3}", detail_clone.aggregate.precision())} label="Precision" value_style="color: var(--accent-green, #3fb950);"/>
                                  <MetricsCard value={format!("{:.3}", detail_clone.aggregate.recall())} label="Recall" value_style="color: var(--accent-orange, #f0883e);"/>
                                  <MetricsCard value={detail_clone.meta.analytics.as_ref().map(|a| format!("${:.4}", a.total_cost())).unwrap_or_else(|| "-".into())} label="Total Cost" />
                                  <MetricsCard value={format!("{:.0}s", detail_clone.aggregate.duration_secs)} label="Duration" />
                              }.into_any()
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
                                        results_clone2.iter().map(|pr: &PrResultRowEntry| {
                                        let pr_number = pr.meta.number;
                                        let pr_title = pr.meta.title.clone();
                                        let f1 = pr.metrics.f1();
                                        let precision = pr.metrics.precision();
                                        let recall = pr.metrics.recall();
                                        let cost = pr.analytics.total_cost();
                                        let status = pr.status.clone();
                                        let run_id = detail_id.clone();
                                        let has_agents = pr.has_agents;
                                        let pr_key = pr.meta.number.to_string();

                                        let pr_badge = match status {
                                            Some(ReviewStatus::Completed) => "badge--success",
                                            Some(ReviewStatus::Failed) => "badge--danger",
                                            Some(ReviewStatus::Running) => "badge--warning",
                                            _ => "badge--neutral",
                                        };
                                        let status_text = status.unwrap().to_string();
                                        view! {
                                            <tr class="table__row">
                                                <td class="table__td font-semibold">{format!("#{}", pr_number)}</td>
                                                <td class="table__td">{pr_title}</td>
                                                <td class="table__td table__td--mono">{format!("{f1:.3}")}</td>
                                                <td class="table__td table__td--mono">{format!("{precision:.3}")}</td>
                                                <td class="table__td table__td--mono">{format!("{recall:.3}")}</td>
                                                <td class="table__td table__td--mono">{format!("${cost:.4}")}</td>
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
