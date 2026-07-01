use leptos::*;
use crate::{RunSummary, AdhocRunSummary, api_url};
use std::time::Duration;

// ─── Home Page Component ─────────────────────────────────────────────────────

#[component]
pub fn HomePage() -> impl IntoView {
    let (bench_runs, set_bench_runs) = create_signal::<Vec<RunSummary>>(Vec::new());
    let (adhoc_runs, set_adhoc_runs) = create_signal::<Vec<AdhocRunSummary>>(Vec::new());
    let (loading, set_loading) = create_signal(true);
    let (error, set_error) = create_signal::<Option<String>>(None);
    let (has_active, set_has_active) = create_signal(false);

    // ─── Fetch all data ──────────────────────────────────────────────────
    let fetch = move |sb: WriteSignal<Vec<RunSummary>>,
                       sa: WriteSignal<Vec<AdhocRunSummary>>,
                       sl: WriteSignal<bool>,
                       se: WriteSignal<Option<String>>,
                       sha: WriteSignal<bool>| {
        spawn_local(async move {
            let mut active = false;

            match get_bench_runs().await {
                Ok(data) => {
                    if data.iter().any(|r| r.status == "running") {
                        active = true;
                    }
                    sb.set(data);
                }
                Err(e) => se.set(Some(e)),
            }

            match get_adhoc_runs().await {
                Ok(data) => {
                    if data.iter().any(|r| r.status == "running") {
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

    // Initial load
    fetch(
        set_bench_runs,
        set_adhoc_runs,
        set_loading,
        set_error,
        set_has_active,
    );

    // ─── Auto-refresh when active runs exist ────────────────────────────
    let s_bench = set_bench_runs.clone();
    let s_adhoc = set_adhoc_runs.clone();
    let s_loading = set_loading.clone();
    let s_error = set_error.clone();
    let s_has_active = set_has_active.clone();
    create_effect(move |_| {
        if has_active.get() {
            let sb = s_bench.clone();
            let sa = s_adhoc.clone();
            let sl = s_loading.clone();
            let se = s_error.clone();
            let sha = s_has_active.clone();
            set_interval(
                move || {
                    fetch(sb.clone(), sa.clone(), sl.clone(), se.clone(), sha.clone());
                },
                Duration::from_secs(5),
            );
        }
    });

    // ─── View ────────────────────────────────────────────────────────────
    view! {
        <div class="home-page">
            // ─── Page Header ──────────────────────────────────────────
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

            // ─── Content ──────────────────────────────────────────────────
            {move || {
                if loading.get() {
                    view! {
                        <div class="content-grid content-grid--metrics">
                            <div class="skeleton skeleton--metric"></div>
                            <div class="skeleton skeleton--metric"></div>
                            <div class="skeleton skeleton--metric"></div>
                        </div>
                        <div style="margin-top: var(--spacing-xl);">
                            <div class="skeleton skeleton--card" style="height: 180px; margin-bottom: var(--spacing-lg);"></div>
                            <div class="skeleton skeleton--card" style="height: 300px;"></div>
                        </div>
                    }.into_view()
                } else if let Some(e) = error.get() {
                    view! {
                        <div class="error-state" role="alert">
                            <div class="error-state__icon">"!"</div>
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
                    }.into_view()
                } else {
                    let bench = bench_runs.get();
                    let adhoc = adhoc_runs.get();

                    // ── Compute stats ─────────────────────────────────
                    let completed_bench: Vec<&RunSummary> = bench.iter()
                        .filter(|r| r.status != "running" && r.status != "pending")
                        .collect();
                    let completed_adhoc: Vec<&AdhocRunSummary> = adhoc.iter()
                        .filter(|r| r.status != "running" && r.status != "pending")
                        .collect();

                    let total_runs = completed_bench.len() + completed_adhoc.len();
                    let total_prs: u32 = completed_bench.iter().map(|r| r.pr_count).sum();
                    let avg_f1 = {
                        let vals: Vec<f64> = completed_bench.iter().filter_map(|r| r.avg_f1).collect();
                        if vals.is_empty() { 0.0 } else { vals.iter().sum::<f64>() / vals.len() as f64 }
                    };

                    // ── Active runs (benchmark + ad-hoc) ─────────────
                    let active_bench: Vec<&RunSummary> = bench.iter()
                        .filter(|r| r.status == "running" || r.status == "pending")
                        .collect();
                    let active_adhoc: Vec<&AdhocRunSummary> = adhoc.iter()
                        .filter(|r| r.status == "running" || r.status == "pending")
                        .collect();
                    let has_any_active = !active_bench.is_empty() || !active_adhoc.is_empty();

                    // ── Merged recent runs (sorted by created_at) ────
                    let mut merged: Vec<RecentRunItem> = Vec::new();
                    for r in bench.iter() {
                        merged.push(RecentRunItem::Benchmark(r.clone()));
                    }
                    for r in adhoc.iter() {
                        merged.push(RecentRunItem::Adhoc(r.clone()));
                    }
                    merged.sort_by(|a, b| b.created_at().cmp(a.created_at()));
                    merged.truncate(10);

                    view! {
                        // ─── Summary Metrics ──────────────────────────
                        <div class="content-grid content-grid--metrics">
                            <div class="metric-card">
                                <p class="metric-card__label">"Total Runs"</p>
                                <p class="metric-card__value">{total_runs.to_string()}</p>
                            </div>
                            <div class="metric-card">
                                <p class="metric-card__label">"Avg F1"</p>
                                <p class="metric-card__value">{format!("{:.2}", avg_f1)}</p>
                            </div>
                            <div class="metric-card">
                                <p class="metric-card__label">"PRs Reviewed"</p>
                                <p class="metric-card__value">{total_prs.to_string()}</p>
                            </div>
                        </div>

                        // ─── Quick Actions ────────────────────────────
                        <div class="quick-actions">
                            <a href="/new" class="btn btn--primary btn--lg quick-actions__btn">
                                "New Benchmark"
                            </a>
                            <a href="/adhoc/new" class="btn btn--secondary btn--lg quick-actions__btn">
                                "Ad-hoc Review"
                            </a>
                        </div>

                        // ─── Running Reviews ──────────────────────────
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
                                        let detail_path = format!("/runs/{}", run.id);
                                        let detail_path2 = detail_path.clone();
                                        let elapsed = run.duration_secs
                                            .map(format_elapsed)
                                            .unwrap_or_else(|| "Just started".into());
                                        let pr_progress = if run.pr_count > 0 {
                                            format!("{} PRs", run.pr_count)
                                        } else {
                                            String::new()
                                        };

                                        view! {
                                            <a href=live_path class="card card--interactive card--active-run" style="display: block; text-decoration: none;">
                                                <div class="card__header">
                                                    <h3 class="card__title">{&run.name}</h3>
                                                    <span class="badge badge--running">
                                                        <span class="badge__dot badge__dot--pulse"></span>
                                                        <span class="badge__label">"Running"</span>
                                                    </span>
                                                </div>
                                                <div class="card__body">
                                                    <div class="home-page__meta-row" style="display: flex; gap: var(--spacing-lg, 16px); font-size: var(--text-sm, 14px); color: var(--text-secondary, #8b949e);">
                                                        {if !pr_progress.is_empty() {
                                                            view! { <span>{pr_progress}</span> }.into_view()
                                                        } else {
                                                            view! { <span></span> }.into_view()
                                                        }}
                                                        <span>{elapsed}</span>
                                                    </div>
                                                    {run.model.as_ref().map(|m| {
                                                        view! { <span class="card__meta" style="font-size: var(--text-sm); color: var(--text-secondary);">{format!("Model: {}", m)}</span> }
                                                    })}
                                                </div>
                                                <div class="card__footer">
                                                    <a href=detail_path2 class="btn btn--ghost btn--sm">"Details >"</a>
                                                </div>
                                            </a>
                                        }
                                    }).collect::<Vec<_>>()}

                                    {active_adhoc.into_iter().map(|run| {
                                        let detail_path = format!("/adhoc/runs/{}", run.id);
                                        let detail_path2 = detail_path.clone();

                                        view! {
                                            <a href=detail_path class="card card--interactive card--active-run" style="display: block; text-decoration: none;">
                                                <div class="card__header">
                                                    <h3 class="card__title">{&run.pr_title}</h3>
                                                    <span class="badge badge--running">
                                                        <span class="badge__dot badge__dot--pulse"></span>
                                                        <span class="badge__label">"Running"</span>
                                                    </span>
                                                </div>
                                                <div class="card__body">
                                                    <div class="home-page__meta-row" style="display: flex; gap: var(--spacing-lg, 16px); font-size: var(--text-sm, 14px); color: var(--text-secondary, #8b949e);">
                                                        <span>{format!("Model: {}", run.model)}</span>
                                                        <span>"Ad-hoc"</span>
                                                    </div>
                                                    {if !run.roles.is_empty() {
                                                        view! { <span class="card__meta" style="font-size: var(--text-sm); color: var(--text-secondary);">{format!("Roles: {}", run.roles.join(", "))}</span> }
                                                    } else {
                                                        view! { <span></span> }
                                                    }}
                                                </div>
                                                <div class="card__footer">
                                                    <a href=detail_path2 class="btn btn--ghost btn--sm">"Details >"</a>
                                                </div>
                                            </a>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                            }.into_view()
                        } else {
                            view! {
                                <div class="section-header">
                                    <h2 class="section-header__title">"Running Reviews"</h2>
                                </div>
                                <div class="empty-state" style="padding: var(--spacing-xl);">
                                    <p class="empty-state__message" style="margin: 0;">"No active reviews"</p>
                                </div>
                            }.into_view()
                        }}

                        // ─── Recent Runs ────────────────────────────────
                        <div class="section-header">
                            <h2 class="section-header__title">"Recent Runs"</h2>
                        </div>
                        {if merged.is_empty() {
                            view! {
                                <div class="empty-state" style="padding: var(--spacing-xl);">
                                    <p class="empty-state__message" style="margin: 0;">"No runs yet"</p>
                                </div>
                            }.into_view()
                        } else {
                            view! {
                                <div class="home-page__recent-list">
                                    {merged.into_iter().map(|item| {
                                        let display_name = item.display_name();
                                        let status = item.status().to_string();
                                        let created = item.created_at().to_string();
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
                                            <a href=detail_path class="card card--interactive home-page__recent-row" style="display: block; text-decoration: none;">
                                                <div class="card__header">
                                                    <h3 class="card__title">{display_name}</h3>
                                                    <div style="display: flex; gap: var(--spacing-sm); align-items: center;">
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
                            }.into_view()
                        }}
                    }.into_view()
                }
            }}
        </div>
    }
}

// ─── Helper Types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum RecentRunItem {
    Benchmark(RunSummary),
    Adhoc(AdhocRunSummary),
}

impl RecentRunItem {
    fn id(&self) -> &str {
        match self {
            RecentRunItem::Benchmark(r) => &r.id,
            RecentRunItem::Adhoc(r) => &r.id,
        }
    }

    fn status(&self) -> &str {
        match self {
            RecentRunItem::Benchmark(r) => &r.status,
            RecentRunItem::Adhoc(r) => &r.status,
        }
    }

    fn created_at(&self) -> &str {
        match self {
            RecentRunItem::Benchmark(r) => &r.created_at,
            RecentRunItem::Adhoc(r) => &r.created_at,
        }
    }

    fn display_name(&self) -> String {
        match self {
            RecentRunItem::Benchmark(r) => r.name.clone(),
            RecentRunItem::Adhoc(r) => r.pr_title.clone(),
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
            RecentRunItem::Benchmark(r) => format!("/runs/{}", r.id),
            RecentRunItem::Adhoc(r) => format!("/adhoc/runs/{}", r.id),
        }
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn format_elapsed(secs: f64) -> String {
    let total = secs as u64;
    let mins = total / 60;
    let secs_rem = total % 60;
    format!("{:02}:{:02} elapsed", mins, secs_rem)
}

async fn get_bench_runs() -> Result<Vec<RunSummary>, String> {
    let url = api_url("/api/runs");
    let response = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !response.ok() {
        return Err(format!("Server returned {}", response.status()));
    }

    let data: Vec<RunSummary> = response
        .json()
        .await
        .map_err(|e| format!("Parse error: {}", e))?;

    Ok(data)
}

async fn get_adhoc_runs() -> Result<Vec<AdhocRunSummary>, String> {
    let url = api_url("/api/adhoc/runs");
    let response = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !response.ok() {
        return Err(format!("Server returned {}", response.status()));
    }

    let data: Vec<AdhocRunSummary> = response
        .json()
        .await
        .map_err(|e| format!("Parse error: {}", e))?;

    Ok(data)
}
