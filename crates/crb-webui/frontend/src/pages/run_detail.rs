use leptos::*;
use leptos_router::*;
use crate::{RunDetail, PrResult, api_url};
use crate::components::metrics_card::MetricsCard;
use crate::components::progress_bar::ProgressBar;

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

    let status_badge_class = match detail.status.as_str() {
        "completed" | "done" => "badge badge-done",
        "failed" => "badge badge-failed",
        "running" | "pending" => "badge badge-running",
        _ => "badge badge-pending",
    };

    view! {
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

        // ─── Results table ───────────────────────────────────────────
        <div class="card">
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
                    {move || detail3.results.iter().map(|pr| {
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
