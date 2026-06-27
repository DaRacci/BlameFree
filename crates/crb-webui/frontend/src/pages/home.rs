use leptos::*;
use crate::RunSummary;
use crate::api_url;
use crate::components::run_table::RunTable;

#[component]
pub fn HomePage() -> impl IntoView {
    let (runs, set_runs) = create_signal::<Vec<RunSummary>>(Vec::new());
    let (loading, set_loading) = create_signal(true);
    let (error, set_error) = create_signal::<Option<String>>(None);

    let fetch_runs = create_local_resource(
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
        <div class="container">
            <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 1rem;">
                <h1>"Review Runs"</h1>
                <a href="/new" class="btn btn-primary">"+ New Run"</a>
            </div>

            {move || {
                if loading.get() {
                    view! { <p>"Loading runs..."</p> }.into_view()
                } else if let Some(e) = error.get() {
                    view! { <div class="card"><p style="color: #ef4444;">{format!("Error: {}", e)}</p></div> }.into_view()
                } else if runs.get().is_empty() {
                    view! {
                        <div class="card" style="text-align: center; padding: 3rem;">
                            <h2>"No runs yet"</h2>
                            <p style="color: #64748b;">"Create your first benchmark run to get started."</p>
                            <a href="/new" class="btn btn-primary" style="margin-top: 1rem;">"Create Run"</a>
                        </div>
                    }.into_view()
                } else {
                    view! { <RunTable runs=runs.get() /> }.into_view()
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
