use crate::{api_url, AdhocReviewResponse, AGENT_ROLES};
use leptos::*;
use leptos_router::*;

#[component]
pub fn AdhocReviewPage() -> impl IntoView {
    let (url, set_url) = create_signal(String::new());
    let (model, set_model) = create_signal("deepseek/deepseek-v4-flash".to_string());
    let (selected_roles, set_selected_roles) = create_signal::<Vec<String>>(Vec::new());
    let (loading, set_loading) = create_signal(false);
    let (error, set_error) = create_signal::<Option<String>>(None);

    let navigator = use_navigate();

    let toggle_role = move |role: &str| {
        let role = role.to_string();
        set_selected_roles.update(|roles| {
            if let Some(pos) = roles.iter().position(|r| r == &role) {
                roles.remove(pos);
            } else {
                roles.push(role);
            }
        });
    };

    let is_role_selected = move |role: &str| -> bool {
        selected_roles.with(|roles| roles.contains(&role.to_string()))
    };

    let submit = move |_| {
        let navigator = navigator.clone();
        let url_val = url.get();
        let model_val = model.get();
        let roles_val = selected_roles.get();

        if url_val.trim().is_empty() {
            set_error.set(Some("Please enter a GitHub PR URL.".to_string()));
            return;
        }

        set_loading.set(true);
        set_error.set(None);

        let body = serde_json::json!({
            "url": url_val,
            "model": model_val,
            "roles": roles_val,
        });

        spawn_local(async move {
            let req = gloo_net::http::Request::post(&api_url("/api/adhoc/review"))
                .header("Content-Type", "application/json");
            let resp = match req.body(body.to_string()) {
                Ok(r) => r.send().await,
                Err(e) => {
                    set_error.set(Some(format!("Request error: {}", e)));
                    set_loading.set(false);
                    return;
                }
            };

            match resp {
                Ok(r) => {
                    if r.ok() {
                        match r.json::<AdhocReviewResponse>().await {
                            Ok(data) => {
                                navigator(&format!("/adhoc/runs/{}", data.run_id), Default::default());
                            }
                            Err(e) => {
                                set_error.set(Some(format!("Failed to parse response: {}", e)));
                                set_loading.set(false);
                            }
                        }
                    } else {
                        let status = r.status();
                        let text = r.text().await.unwrap_or_default();
                        set_error.set(Some(format!("Server error ({}): {}", status, text)));
                        set_loading.set(false);
                    }
                }
                Err(e) => {
                    set_error.set(Some(format!("Network error: {}", e)));
                    set_loading.set(false);
                }
            }
        });
    };

    view! {
        <div class="adhoc-review-page">
            <h1>"Ad-hoc PR Review"</h1>
            <p>"Submit a GitHub PR URL for a one-off review by the agent team."</p>

            <div class="form-group">
                <label for="pr-url">"GitHub PR URL"</label>
                <input
                    id="pr-url"
                    type="text"
                    placeholder="https://github.com/owner/repo/pull/123"
                    prop:value=url
                    on:input=move |ev| set_url.set(event_target_value(&ev))
                    class="form-input"
                />
            </div>

            <div class="form-group">
                <label for="model">"Model"</label>
                <input
                    id="model"
                    type="text"
                    prop:value=model
                    on:input=move |ev| set_model.set(event_target_value(&ev))
                    class="form-input"
                />
            </div>

            <div class="form-group">
                <label>"Roles"</label>
                <div class="checkbox-group">
                    {AGENT_ROLES.iter().map(|role| {
                        let role_str = *role;
                        let checked = is_role_selected(role_str);
                        view! {
                            <label class="checkbox-label">
                                <input
                                    type="checkbox"
                                    checked=checked
                                    on:click=move |_| toggle_role(role_str)
                                />
                                <span>{role_str}</span>
                            </label>
                        }
                    }).collect::<Vec<_>>()}
                </div>
            </div>

            {move || error.get().map(|e| {
                view! { <div class="error-message">{e}</div> }
            })}

            <button
                class="btn btn--primary"
                on:click=submit
                disabled=move || loading.get()
            >
                {move || if loading.get() { "Starting..." } else { "Start Review" }}
            </button>
        </div>
    }
}
