use gloo_timers::callback::Interval;
use leptos::either::EitherOf3;
use leptos::prelude::*;
use leptos::task::spawn_local;
use riv_types::review::{Review, ReviewStatus};
use riv_webui_shared::routes::{API_ADHOC_RUNS, API_RUNS};
use serde::Deserialize;

use crate::fetch_json;
use crate::{components::metrics_card::MetricsCard, signal_struct};
use lucide_leptos::{ArrowRight, TriangleAlert};

/// Response type matching the old `RunSummary` JSON format from the API.
/// The API still nests a `Review` under a `meta` field alongside
/// benchmark-only fields we no longer consume.
#[derive(Debug, Clone, Deserialize)]
#[deprecated]
struct BenchmarkRunResponse {
    meta: Review,
}

signal_struct! {
  struct HomeSignals {
    loading: bool = true,
    error: Option<String> = None,
    reviews: Vec<Review> = Vec::new(),
  }
}

#[component]
pub fn HomePage() -> impl IntoView {
    let signals = HomeSignals::new();

    let fetch = move |sb: WriteSignal<Vec<Review>>,
                      sa: WriteSignal<Vec<Review>>,
                      sl: WriteSignal<bool>,
                      se: WriteSignal<Option<String>>,
                      sha: WriteSignal<bool>| {
        spawn_local(async move {
            let mut active = false;

            match fetch_json::<Vec<Review>>(API_RUNS).await {
                Ok(data) => {
                    let reviews: Vec<Review> = data.into_iter().map(|r| r.meta).collect();
                    if reviews.iter().any(|r| r.status == ReviewStatus::Running) {
                        active = true;
                    }
                    sb.set(reviews);
                }
                Err(e) => se.set(Some(e)),
            }

            match fetch_json::<Vec<Review>>(API_ADHOC_RUNS).await {
                Ok(data) => {
                    if data.iter().any(|r| r.status == ReviewStatus::Running) {
                        active = true;
                    }
                    sa.set(data);
                }
                Err(e) => se.set(Some(e)),
            }

            sha.set(active);
            sl.set(false);
        });
    };

    fetch(
        set_bench_runs,
        set_adhoc_runs,
        set_loading,
        set_error,
        set_has_active,
    );

    // Poll every 5s while there are active runs
    Effect::new(move || {
        if has_active.get() {
            let interval = Interval::new(5_000, move || {
                fetch(
                    set_bench_runs,
                    set_adhoc_runs,
                    set_loading,
                    set_error,
                    set_has_active,
                );
            });
            interval.forget();
        }
    });

    view! {
        <div class="home-page">
            <div class="page-header">
                <h1 class="page-header__title">"Overview"</h1>
                <div class="page-header__actions">
                    <a href="/new" class="btn btn--primary">
                        "New Benchmark"
                    </a>
                    <a href="/adhoc/new" class="btn btn--secondary">
                        "Ad-hoc Review"
                    </a>
                </div>
            </div>

            {move || {
                if loading.get() {
                    EitherOf3::A(
                        view! {
                            <div class="content-grid content-grid--metrics">
                                <div class="skeleton skeleton--metric"></div>
                                <div class="skeleton skeleton--metric"></div>
                                <div class="skeleton skeleton--metric"></div>
                            </div>
                            <div class="mt-xl">
                                <div class="skeleton skeleton--card mb-lg" style="height: 180px;"></div>
                                <div class="skeleton skeleton--card" style="height: 300px;"></div>
                            </div>
                        }
                    )
                } else if let Some(e) = error.get() {
                    EitherOf3::B(
                        view! {
                            <div class="error-state" role="alert">
                                <div class="error-state__icon"><TriangleAlert size=24 /></div>
                                <h3 class="error-state__heading">"Failed to load data"</h3>
                                <p class="error-state__message">{format!("Something went wrong: {}", e)}</p>
                                <div class="error-state__action">
                                    <button class="btn btn--primary" on:click=move |_| {
                                        set_loading.set(true);
                                        set_error.set(None);
                                        fetch(
                                            set_bench_runs,
                                            set_adhoc_runs,
                                            set_loading,
                                            set_error,
                                            set_has_active,
                                        );
                                    }>"Retry"</button>
                                </div>
                            </div>
                        }
                    )
                } else {
                    let bench = bench_runs.get();
                    let adhoc = adhoc_runs.get();

                    let completed_bench: Vec<&Review> = bench.iter()
                        .filter(|r| r.status != ReviewStatus::Running && r.status != ReviewStatus::Pending)
                        .collect();
                    let completed_adhoc: Vec<&Review> = adhoc.iter()
                        .filter(|r| r.status != ReviewStatus::Running && r.status != ReviewStatus::Pending)
                        .collect();

                    let total_runs = completed_bench.len() + completed_adhoc.len();
                    let total_prs: usize = 0; // REMOVED: results_len was benchmark-specific
                    let avg_f1 = 0.0; // REMOVED: metrics was benchmark-specific

                    let active_bench: Vec<&Review> = bench.iter()
                        .filter(|r| r.status == ReviewStatus::Running || r.status == ReviewStatus::Pending)
                        .collect();
                    let active_adhoc: Vec<&Review> = adhoc.iter()
                        .filter(|r| r.status == ReviewStatus::Running || r.status == ReviewStatus::Pending)
                        .collect();
                    let has_any_active = !active_bench.is_empty() || !active_adhoc.is_empty();

                    let mut merged: Vec<RecentRunItem> = Vec::new();
                    for r in bench.iter() {
                        merged.push(RecentRunItem::Benchmark(r.clone()));
                    }
                    for r in adhoc.iter() {
                        merged.push(RecentRunItem::Adhoc(r.clone()));
                    }
                    merged.sort_by(|a, b| b.sort_key().cmp(&a.sort_key()));
                    merged.truncate(10);

                    EitherOf3::C(
                        view! {
                            <div class="content-grid content-grid--metrics">
                                <MetricsCard value={total_runs.to_string()} label="Total Runs" />
                                <MetricsCard value={if avg_f1 > 0.0 { format!("{:.2}", avg_f1) } else { "N/A".into() }} label="Avg F1" />
                                <MetricsCard value={total_prs.to_string()} label="PRs Reviewed" />
                            </div>

                            <div class="quick-actions">
                                <a href="/new" class="btn btn--primary btn--lg quick-actions__btn">
                                    "New Benchmark"
                                </a>
                                <a href="/adhoc/new" class="btn btn--secondary btn--lg quick-actions__btn">
                                    "Ad-hoc Review"
                                </a>
                            </div>

                            {if has_any_active {
                                view! {
                                    <div class="section-header">
                                        <h2 class="section-header__title">
                                            <span class="active-runs-indicator"></span>
                                            "Running Reviews"
                                        </h2>
                                        <span class="active-runs-count">{format!("{} running", active_bench.len() + active_adhoc.len())}</span>
                                    </div>
                                    <div class="content-grid content-grid--cards">
                                        {active_bench.into_iter().map(|run| {
                                            let live_path = format!("/runs/{}/live", run.id);
                                            let detail_path = format!("/runs/{}/", run.id);
                                            let detail_path2 = detail_path.clone();
                                            let elapsed = run.duration
                                                .map(|d| format_elapsed(d.as_secs_f64()))
                                                .unwrap_or_else(|| "Just started".into());

                                            view! {
                                                <a href=live_path class="card card--interactive card--active-run card--block-link">
                                                    <div class="card__header">
                                                        <h3 class="card__title">{run.id.to_string()}</h3>
                                                        <span class="badge badge--running">
                                                            <span class="badge__dot badge__dot--pulse"></span>
                                                            <span class="badge__label">"Running"</span>
                                                        </span>
                                                    </div>
                                                    <div class="card__body">
                                                        <div class="home-page__meta-row flex-row gap-lg text-sm text-secondary">
                                                            <span>{elapsed}</span>
                                                        </div>
                                                    </div>
                                                    <div class="card__footer">
                                                        <a href=detail_path2 class="btn btn--ghost btn--sm"><ArrowRight size=16 /></a>
                                                    </div>
                                                </a>
                                            }
                                        }).collect::<Vec<_>>()}

                                        {active_adhoc.into_iter().map(|run| {
                                            let id_str = run.id.to_string();
                                            let detail_path = format!("/adhoc/runs/{}", id_str);
                                            let detail_path2 = detail_path.clone();
                                            let title = "placeholder"; // TODO

                                            view! {
                                                <a href=detail_path class="card card--interactive card--active-run card--block-link">
                                                    <div class="card__header">
                                                        <h3 class="card__title">{title}</h3>
                                                        <span class="badge badge--running">
                                                            <span class="badge__dot badge__dot--pulse"></span>
                                                            <span class="badge__label">"Running"</span>
                                                        </span>
                                                    </div>
                                                    <div class="card__body">
                                                        <div class="home-page__meta-row flex-row gap-lg text-sm text-secondary">
                                                            <span>{format!("Model: placeholder")}</span>
                                                            <span>"Ad-hoc"</span>
                                                        </div>
                                                    </div>
                                                    <div class="card__footer">
                                                        <a href=detail_path2 class="btn btn--ghost btn--sm"><ArrowRight size=16 /></a>
                                                    </div>
                                                </a>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <div class="section-header">
                                        <h2 class="section-header__title">"Running Reviews"</h2>
                                    </div>
                                    <div class="empty-state py-xl">
                                        <p class="empty-state__message" style="margin: 0;">"No active reviews"</p>
                                    </div>
                                }.into_any()
                            }}

                            <div class="section-header">
                                <h2 class="section-header__title">"Recent Runs"</h2>
                            </div>
                            {if merged.is_empty() {
                                view! {
                                    <div class="empty-state py-xl">
                                        <p class="empty-state__message" style="margin: 0;">"No runs yet"</p>
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <div class="home-page__recent-list">
                                        {merged.into_iter().map(|item| {
                                            let display_name = item.display_name();
                                            let status = item.status().to_string();
                                            let created = item.created_at();
                                            let detail_path = item.detail_path();
                                            let run_type = item.run_type_label();
                                            let type_badge_class = match run_type {
                                                "benchmark" => "badge--info",
                                                _ => "badge--neutral",
                                            };
                                            let status_badge_class = match status.as_str() {
                                                "running" | "pending" => "badge--warning",
                                                "completed" | "done" => "badge--success",
                                                "failed" => "badge--danger",
                                                _ => "badge--neutral",
                                            };

                                            view! {
                                                <a href=detail_path class="card card--interactive home-page__recent-row card--block-link">
                                                    <div class="card__header">
                                                        <h3 class="card__title">{display_name}</h3>
                                                        <div class="flex-row gap-sm items-center">
                                                            <span class=format!("badge {}", type_badge_class)>
                                                                <span class="badge__dot"></span>
                                                                <span class="badge__label">{run_type}</span>
                                                            </span>
                                                            <span class=format!("badge {}", status_badge_class)>
                                                                <span class="badge__dot"></span>
                                                                <span class="badge__label">{status}</span>
                                                            </span>
                                                        </div>
                                                    </div>
                                                    <div class="card__body" style="padding-top: var(--spacing-md);">
                                                        <span class="card__meta">{created}</span>
                                                    </div>
                                                </a>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </div>
                                }.into_any()
                            }}
                        }
                    )
                }
            }}
        </div>
    }
}

#[derive(Clone)]
enum RecentRunItem {
    Benchmark(Review),
    Adhoc(Review),
}

impl RecentRunItem {
    fn status(&self) -> &ReviewStatus {
        match self {
            RecentRunItem::Benchmark(r) => &r.status,
            RecentRunItem::Adhoc(r) => &r.status,
        }
    }

    fn sort_key(&self) -> String {
        match self {
            RecentRunItem::Benchmark(r) => r.id.to_string(),
            RecentRunItem::Adhoc(r) => r.id.to_string(),
        }
    }

    fn created_at(&self) -> String {
        match self {
            RecentRunItem::Benchmark(r) => r.id.to_string(),
            RecentRunItem::Adhoc(r) => r.id.to_string(),
        }
    }

    fn display_name(&self) -> String {
        match self {
            RecentRunItem::Benchmark(r) => r.id.to_string(),
            RecentRunItem::Adhoc(r) => "placeholder".to_string(), // TODO
        }
    }

    fn run_type_label(&self) -> &'static str {
        match self {
            RecentRunItem::Benchmark(_) => "benchmark",
            RecentRunItem::Adhoc(_) => "ad-hoc",
        }
    }

    fn detail_path(&self) -> String {
        match self {
            RecentRunItem::Benchmark(r) => format!("/runs/{}/", r.id),
            RecentRunItem::Adhoc(r) => format!("/adhoc/runs/{}/", r.id),
        }
    }
}

fn format_elapsed(secs: f64) -> String {
    let total = secs as u64;
    let mins = total / 60;
    let secs_rem = total % 60;
    format!("{:02}:{:02} elapsed", mins, secs_rem)
}
