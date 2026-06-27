use leptos::*;
use crate::RunSummary;
use crate::api_url;

#[component]
pub fn HomePage() -> impl IntoView {
    let (runs, set_runs) = create_signal::<Vec<RunSummary>>(Vec::new());
    let (loading, set_loading) = create_signal(true);
    let (error, set_error) = create_signal::<Option<String>>(None);

    let _fetch_runs = create_local_resource(
        || (),
        move |_| {
            let set_runs = set_runs.clone();
            let set_loading = set_loading.clone();
            let set_error = set_error.clone();
            async move {
                set_loading.set(true);
                set_error.set(None);
                match get_runs().await {
                    Ok(data) => {
                        set_runs.set(data);
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
                                <button class="btn btn--primary" on:click=move |_| set_loading.set(true)>"🔄 Retry"</button>
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
                    let total_runs = runs.get().len();
                    let avg_f1 = {
                        let vals: Vec<f64> = runs.get().iter().filter_map(|r| r.avg_f1).collect();
                        if vals.is_empty() { 0.0 } else { vals.iter().sum::<f64>() / vals.len() as f64 }
                    };
                    let total_cost: f64 = runs.get().iter().filter_map(|r| r.total_cost).sum();

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
                                <p class="metric-card__value">{runs.get().iter().map(|r| r.pr_count).sum::<u32>().to_string()}</p>
                            </div>
                        </div>

                        // ─── Section Header ────────────────────────────
                        <div class="section-header">
                            <h2 class="section-header__title">"Past Runs"</h2>
                        </div>

                        // ─── Run Cards Grid ────────────────────────────
                        <div class="content-grid content-grid--cards">
                            {runs.get().into_iter().map(|run| {
                                let detail_path = format!("/runs/{}", run.id);
                                let f1_str = run.avg_f1.map(|v| format!("{:.3}", v)).unwrap_or_else(|| "—".into());
                                let cost_str = run.total_cost.map(|v| format!("${:.4}", v)).unwrap_or_else(|| "—".into());
                                let dur_str = run.duration_secs.map(|d| format!("{:.0}s", d)).unwrap_or_else(|| "—".into());

                                let badge_variant = match run.status.as_str() {
                                    "completed" | "done" => "badge--success",
                                    "failed" => "badge--danger",
                                    "running" => "badge--warning",
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
                }
            }}
        </div>
    }
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
