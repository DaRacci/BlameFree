use leptos::*;
use crate::RunSummary;
use crate::api_url;
use std::time::Duration;

#[component]
pub fn HomePage() -> impl IntoView {
    let (runs, set_runs) = create_signal::<Vec<RunSummary>>(Vec::new());
    let (loading, set_loading) = create_signal(true);
    let (error, set_error) = create_signal::<Option<String>>(None);
    let (has_active, set_has_active) = create_signal(false);

    // Fetch runs
    let fetch = move |set_runs: WriteSignal<Vec<RunSummary>>,
                       set_loading: WriteSignal<bool>,
                       set_error: WriteSignal<Option<String>>,
                       set_has_active: WriteSignal<bool>| {
        spawn_local(async move {
            match get_runs().await {
                Ok(data) => {
                    let active = data.iter().any(|r| r.status == "running");
                    set_has_active.set(active);
                    set_runs.set(data);
                    set_loading.set(false);
                }
                Err(e) => {
                    set_error.set(Some(e));
                    set_loading.set(false);
                }
            }
        });
    };

    // Initial fetch
    fetch(set_runs, set_loading, set_error, set_has_active);

    // Auto-refresh every 5 seconds when there are active runs
    let set_runs_clone = set_runs.clone();
    let set_loading_clone = set_loading.clone();
    let set_error_clone = set_error.clone();
    let set_has_active_clone = set_has_active.clone();
    create_effect(move |_| {
        if has_active.get() {
            let s_runs = set_runs_clone.clone();
            let s_loading = set_loading_clone.clone();
            let s_error = set_error_clone.clone();
            let s_active = set_has_active_clone.clone();
            set_interval(move || {
                spawn_local(async move {
                    match get_runs().await {
                        Ok(data) => {
                            let active = data.iter().any(|r| r.status == "running");
                            s_active.set(active);
                            s_runs.set(data);
                        }
                        Err(e) => {
                            s_error.set(Some(e));
                        }
                    }
                });
            }, Duration::from_secs(5));
        }
    });

    view! {
        <div class="home-page">
            // ─── Page Header ──────────────────────────────────────────
            <div class="page-header">
                <h1 class="page-header__title">"Dashboard"</h1>
                <div class="page-header__actions">
                    <a href="/new" class="btn btn--primary">
                        <span class="btn__icon">"🆕"</span>
                        <span class="btn__label">"New Benchmark"</span>
                    </a>
                </div>
            </div>

            // ─── Content ──────────────────────────────────────────────
            {move || {
                if loading.get() {
                    view! {
                        <div class="content-grid content-grid--metrics">
                            <div class="skeleton skeleton--metric"></div>
                            <div class="skeleton skeleton--metric"></div>
                            <div class="skeleton skeleton--metric"></div>
                            <div class="skeleton skeleton--metric"></div>
                        </div>
                        <div class="content-grid content-grid--cards">
                            <div class="card skeleton skeleton--card" role="status" aria-label="Loading..."></div>
                            <div class="card skeleton skeleton--card" role="status" aria-label="Loading..."></div>
                            <div class="card skeleton skeleton--card" role="status" aria-label="Loading..."></div>
                        </div>
                    }.into_view()
                } else if let Some(e) = error.get() {
                    view! {
                        <div class="error-state" role="alert">
                            <div class="error-state__icon">"⚠️"</div>
                            <h3 class="error-state__heading">"Failed to load runs"</h3>
                            <p class="error-state__message">{format!("Something went wrong while fetching benchmark runs: {}", e)}</p>
                            <div class="error-state__action">
                                <button class="btn btn--primary" on:click=move |_| {
                                    set_loading.set(true);
                                    fetch(set_runs, set_loading, set_error, set_has_active);
                                }>"🔄 Retry"</button>
                            </div>
                        </div>
                    }.into_view()
                } else if runs.get().is_empty() {
                    view! {
                        <div class="empty-state">
                            <div class="empty-state__icon">"📂"</div>
                            <h3 class="empty-state__heading">"No benchmark runs yet"</h3>
                            <p class="empty-state__message">"Run your first benchmark to see results here."</p>
                            <div class="empty-state__action">
                                <a href="/new" class="btn btn--primary">
                                    <span class="btn__icon">"🚀"</span>
                                    <span class="btn__label">"Start Your First Run"</span>
                                </a>
                            </div>
                        </div>
                    }.into_view()
                } else {
                    let all_runs = runs.get();
                    let active_runs: Vec<RunSummary> = all_runs.clone().into_iter().filter(|r| r.status == "running").collect();
                    let completed_runs: Vec<RunSummary> = all_runs.into_iter().filter(|r| r.status != "running").collect();

                    // Metrics — include both active and completed
                    let total_runs = completed_runs.len() + active_runs.len();
                    let avg_f1 = {
                        let vals: Vec<f64> = completed_runs.iter().filter_map(|r| r.avg_f1).collect();
                        if vals.is_empty() { 0.0 } else { vals.iter().sum::<f64>() / vals.len() as f64 }
                    };
                    let total_cost: f64 = completed_runs.iter().filter_map(|r| r.total_cost).sum();

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
                                <p class="metric-card__label">"Total Cost"</p>
                                <p class="metric-card__value">{format!("${:.2}", total_cost)}</p>
                            </div>
                            <div class="metric-card">
                                <p class="metric-card__label">"Total PRs"</p>
                                <p class="metric-card__value">{completed_runs.iter().map(|r| r.pr_count).sum::<u32>().to_string()}</p>
                            </div>
                        </div>

                        // ─── Active Runs Section ──────────────────────
                        {if !active_runs.is_empty() {
                            view! {
                                <div class="section-header">
                                    <h2 class="section-header__title">
                                        <span class="active-runs-indicator"></span>
                                        "Active Runs"
                                    </h2>
                                    <span class="active-runs-count">{format!("{} running", active_runs.len())}</span>
                                </div>
                                <div class="content-grid content-grid--cards">
                                    {active_runs.into_iter().map(|run| {
                                        let live_path = format!("/runs/{}/live", run.id);
                                        let detail_path = format!("/runs/{}", run.id);
                                        let elapsed = run.duration_secs
                                            .map(format_elapsed)
                                            .unwrap_or_else(|| "Just started".into());

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
                                                        <span>{format!("{} PRs", run.pr_count)}</span>
                                                        <span>{elapsed}</span>
                                                    </div>
                                                    <div class="home-page__active-actions">
                                                        "🔴 View live..."
                                                    </div>
                                                </div>
                                                <div class="card__footer">
                                                    <a href=detail_path class="btn btn--ghost btn--sm">"Details →"</a>
                                                </div>
                                            </a>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                            }.into_view()
                        } else {
                            view! {}.into_view()
                        }}

                        // ─── Past Runs Section ────────────────────────────
                        {if !completed_runs.is_empty() {
                            view! {
                                <div class="section-header">
                                    <h2 class="section-header__title">"Past Runs"</h2>
                                </div>
                                <div class="content-grid content-grid--cards">
                                    {completed_runs.into_iter().map(|run| {
                                        let detail_path = format!("/runs/{}", run.id);
                                        let f1_str = run.avg_f1.map(|v| format!("{:.3}", v)).unwrap_or_else(|| "—".into());
                                        let cost_str = run.total_cost.map(|v| format!("${:.4}", v)).unwrap_or_else(|| "—".into());
                                        let dur_str = run.duration_secs.map(|d| format!("{:.0}s", d)).unwrap_or_else(|| "—".into());

                                        let badge_variant = match run.status.as_str() {
                                            "completed" | "done" => "badge--success",
                                            "failed" => "badge--danger",
                                            _ => "badge--neutral",
                                        };

                                        view! {
                                            <a href=detail_path class="card card--interactive" style="display: block; text-decoration: none;">
                                                <div class="card__header">
                                                    <h3 class="card__title">{&run.name}</h3>
                                                    <span class=format!("badge {}", badge_variant)>
                                                        <span class="badge__dot"></span>
                                                        <span class="badge__label">{&run.status}</span>
                                                    </span>
                                                </div>
                                                <div class="card__body">
                                                    <div class="home-page__meta-row" style="display: flex; gap: var(--spacing-lg, 16px); font-size: var(--text-sm, 14px); color: var(--text-secondary, #8b949e);">
                                                        <span>{format!("{} PRs", run.pr_count)}</span>
                                                        <span>{cost_str}</span>
                                                    </div>
                                                    <p class="home-page__f1-value" style="font-family: var(--font-mono, monospace); font-size: var(--text-2xl, 24px); font-weight: var(--weight-semibold, 600); color: var(--text-primary, #c9d1d9);">{f1_str}</p>
                                                </div>
                                                <div class="card__footer">
                                                    <span class="card__meta">{dur_str}</span>
                                                </div>
                                            </a>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                            }.into_view()
                        } else {
                            view! {}.into_view()
                        }}
                    }.into_view()
                }
            }}
        </div>
    }
}

fn format_elapsed(secs: f64) -> String {
    let total = secs as u64;
    let mins = total / 60;
    let secs_rem = total % 60;
    format!("{:02}:{:02} elapsed", mins, secs_rem)
}

async fn get_runs() -> Result<Vec<RunSummary>, String> {
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
