use crate::{api_url, AdhocRunSummary};
use leptos::*;
use leptos_router::*;

#[component]
pub fn AdhocRunsPage() -> impl IntoView {
    let (runs, set_runs) = create_signal::<Vec<AdhocRunSummary>>(Vec::new());
    let (loading, set_loading) = create_signal(true);
    let (error, set_error) = create_signal::<Option<String>>(None);

    let _fetch = create_local_resource(
        move || (),
        move |_| {
            let set_runs = set_runs.clone();
            let set_loading = set_loading.clone();
            let set_error = set_error.clone();
            async move {
                set_loading.set(true);
                set_error.set(None);
                match gloo_net::http::Request::get(&api_url("/api/adhoc/runs"))
                    .send()
                    .await
                {
                    Ok(resp) => {
                        if resp.ok() {
                            match resp.json::<Vec<AdhocRunSummary>>().await {
                                Ok(data) => {
                                    set_runs.set(data);
                                }
                                Err(e) => {
                                    set_error.set(Some(format!("Failed to parse runs: {}", e)));
                                }
                            }
                        } else {
                            let status_code = resp.status();
                            let text = resp.text().await.unwrap_or_default();
                            set_error.set(Some(format!("Server error ({}): {}", status_code, text)));
                        }
                    }
                    Err(e) => {
                        set_error.set(Some(format!("Network error: {}", e)));
                    }
                }
                set_loading.set(false);
            }
        },
    );

    view! {
        <div class="adhoc-runs-page">
            <div class="page-header">
                <h1>"Ad-hoc Reviews"</h1>
                <a href="/adhoc/new" class="btn btn--primary">"New Review"</a>
            </div>

            {move || {
                if loading.get() {
                    return view! { <div class="state-container"><p>"Loading..."</p></div> }.into_view();
                }
                if let Some(e) = error.get() {
                    return view! { <div class="state-container error-message">{e}</div> }.into_view();
                }
                let items = runs.get();
                if items.is_empty() {
                    return view! {
                        <div class="state-container">
                            <p>"No ad-hoc reviews yet."</p>
                            <a href="/adhoc/new" class="btn btn--primary">"Start your first review"</a>
                        </div>
                    }.into_view();
                }
                view! {
                    <table class="data-table">
                        <thead>
                            <tr>
                                <th>"PR Title"</th>
                                <th>"Status"</th>
                                <th>"Model"</th>
                                <th>"Roles"</th>
                                <th>"Findings"</th>
                                <th>"Cost"</th>
                                <th>"Created"</th>
                            </tr>
                        </thead>
                        <tbody>
                            {items.into_iter().map(|run| {
                                let run_status = run.status.clone();
                                view! {
                                    <tr>
                                        <td>
                                            <a href=format!("/adhoc/runs/{}", run.id)>{run.pr_title}</a>
                                        </td>
                                        <td>
                                            <span class=format!("status-badge status-badge--{}", run.status)>
                                                {run_status}
                                            </span>
                                        </td>
                                        <td>{run.model}</td>
                                        <td>{run.roles.join(", ")}</td>
                                        <td>{run.findings_count}</td>
                                        <td>{format!("${:.4}", run.total_cost)}</td>
                                        <td>{run.created_at}</td>
                                    </tr>
                                }
                            }).collect::<Vec<_>>()}
                        </tbody>
                    </table>
                }.into_view()
            }}
        </div>
    }
}
