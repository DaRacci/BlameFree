use riv_webui_shared::review::Review;
use riv_webui_shared::routes::API_ADHOC_RUNS;
use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
pub fn AdhocRunsPage() -> impl IntoView {
    let (runs, set_runs) = signal::<Vec<Review>>(Vec::new());
    let (loading, set_loading) = signal(true);
    let (error, set_error) = signal::<Option<String>>(None);

    spawn_local(async move {
        set_loading.set(true);
        set_error.set(None);
        match gloo_net::http::Request::get(API_ADHOC_RUNS).send().await {
            Ok(resp) => {
                if resp.ok() {
                    match resp.json::<Vec<Review>>().await {
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
    });

    view! {
        <div class="adhoc-runs-page">
            <div class="page-header">
                <h1>"Ad-hoc Reviews"</h1>
                <a href="/adhoc/new" class="btn btn--primary">"New Review"</a>
            </div>

            {move || {
                if loading.get() {
                    return view! { <div class="state-container"><p>"Loading..."</p></div> }.into_any();
                }
                if let Some(e) = error.get() {
                    return view! { <div class="state-container error-message">{e}</div> }.into_any();
                }
                let items = runs.get();
                if items.is_empty() {
                    return view! {
                        <div class="state-container">
                            <p>"No ad-hoc reviews yet."</p>
                            <a href="/adhoc/new" class="btn btn--primary">"Start your first review"</a>
                        </div>
                    }.into_any();
                }
                view! {
                    <table class="data-table">
                        <thead>
                            <tr>
                                <th>"PR Title"</th>
                                <th>"Status"</th>
                            </tr>
                        </thead>
                        <tbody>
                            {items.into_iter().map(|run| {
                                let id_str = run.id.to_string();
                                let title = "placeholder"; // TODO
                                let status = run.status.to_string();
                                view! {
                                    <tr>
                                        <td>
                                            <a href=format!("/adhoc/runs/{}", id_str)>{title}</a>
                                        </td>
                                        <td>
                                            <span class=format!("status-badge status-badge--{}", run.status)>
                                                {status}
                                            </span>
                                        </td>
                                    </tr>
                                }
                            }).collect::<Vec<_>>()}
                        </tbody>
                    </table>
                }.into_any()
            }}
        </div>
    }
}
