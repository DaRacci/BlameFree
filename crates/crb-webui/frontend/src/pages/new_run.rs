use crate::{api_url, AppConfig, DatasetInfo, NewRunRequest, NewRunResponse};
use leptos::*;
use leptos_router::*;

#[component]
pub fn NewRunPage() -> impl IntoView {
    let (config, set_config) = create_signal::<Option<AppConfig>>(None);
    let (config_loading, set_config_loading) = create_signal(true);
    let (config_error, set_config_error) = create_signal::<Option<String>>(None);

    let (datasets, set_datasets) = create_signal::<Vec<DatasetInfo>>(Vec::new());
    let (datasets_loading, set_datasets_loading) = create_signal(true);

    let (model, set_model) = create_signal(String::new());
    let (dataset, set_dataset) = create_signal(String::new());
    let (roles, set_roles) = create_signal::<Vec<String>>(Vec::new());
    let (pr_filter, set_pr_filter) = create_signal(String::new());
    let (submitting, set_submitting) = create_signal(false);
    let (submit_error, set_submit_error) = create_signal::<Option<String>>(None);
    let (submit_result, set_submit_result) = create_signal::<Option<String>>(None);

    let navigator = use_navigate();

    // Fetch config
    let _fetch_config = create_local_resource(
        || (),
        move |_| {
            let set_config = set_config.clone();
            let set_loading = set_config_loading.clone();
            let set_error = set_config_error.clone();
            let set_model = set_model.clone();
            let set_dataset = set_dataset.clone();
            let set_datasets = set_datasets.clone();
            let set_datasets_loading = set_datasets_loading.clone();
            async move {
                set_loading.set(true);
                set_datasets_loading.set(true);
                // Fetch config first
                match get_config().await {
                    Ok(cfg) => {
                        if let Some(m) = cfg.models.first() {
                            set_model.set(m.clone());
                        }
                        if let Some(d) = cfg.datasets.first() {
                            set_dataset.set(d.clone());
                        }
                        set_config.set(Some(cfg));
                        set_loading.set(false);
                    }
                    Err(e) => {
                        set_error.set(Some(e));
                        set_loading.set(false);
                    }
                }
                // Then fetch dataset details
                match get_datasets().await {
                    Ok(ds) => {
                        set_datasets.set(ds);
                        set_datasets_loading.set(false);
                    }
                    Err(_) => {
                        set_datasets_loading.set(false);
                    }
                }
            }
        },
    );

    let toggle_role = move |role: &str| {
        let r = role.to_string();
        set_roles.update(|roles| {
            if let Some(pos) = roles.iter().position(|x| x == &r) {
                roles.remove(pos);
            } else {
                roles.push(r);
            }
        });
    };

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_submitting.set(true);
        set_submit_error.set(None);
        set_submit_result.set(None);

        let req = NewRunRequest {
            model: model.get(),
            dataset: dataset.get(),
            roles: roles.get(),
            pr_filter: {
                let val = pr_filter.get();
                if val.is_empty() { None } else { Some(val) }
            },
        };

        let navigator = navigator.clone();
        spawn_local(async move {
            match create_run(req).await {
                Ok(resp) => {
                    set_submitting.set(false);
                    set_submit_result.set(Some(resp.run_id.clone()));
                    // Navigate to run detail after a brief delay
                    navigator(&format!("/runs/{}", resp.run_id), Default::default());
                }
                Err(e) => {
                    set_submitting.set(false);
                    set_submit_error.set(Some(e));
                }
            }
        });
    };

    view! {
        <div class="new-run-page">
            <div class="page-header">
                <h1 class="page-header__title">"New Benchmark Run"</h1>
                <div class="page-header__actions">
                    <a href="/" class="btn btn--ghost">"Cancel"</a>
                </div>
            </div>

            // Config loading state
            {move || {
                if config_loading.get() || datasets_loading.get() {
                    view! {
                        <div style="display: flex; align-items: center; gap: 12px; color: var(--text-secondary, #8b949e); padding: 24px 0;">
                            <div class="skeleton skeleton--text" style="width: 200px;"></div>
                        </div>
                    }.into_view()
                } else { view! { <span></span> }.into_view() }
            }}

            // Config error
            {move || {
                if let Some(e) = config_error.get() {
                    view! {
                        <div class="card" style="margin-bottom: var(--spacing-lg, 16px);">
                            <div class="card__body">
                                <p style="color: var(--accent-red, #f85149);">{format!("Failed to load config: {}", e)}</p>
                                <p style="color: var(--text-secondary, #8b949e); font-size: var(--text-sm, 14px);">"You can still fill in the form manually."</p>
                            </div>
                        </div>
                    }.into_view()
                } else { view! { <span></span> }.into_view() }
            }}

            <form on:submit=on_submit>
                // ─── Configuration Section ───────────────────────────
                <section class="form-section">
                    <h2 class="form-section__title">"Configuration"</h2>
                    <div class="form-section__fields">
                        <div class="form-field">
                            <label class="form-field__label" for="model">"Model"</label>
                            <select id="model" class="input select" prop:value=model.get() on:change=move |ev| {
                                set_model.set(event_target_value(&ev));
                            }>
                                {move || {
                                    let cfg = config.get();
                                    let models = if let Some(ref c) = cfg {
                                        c.models.clone()
                                    } else {
                                        vec!["deepseek/deepseek-v4-flash".into(), "deepseek/deepseek-v4-pro".into()]
                                    };
                                    models.into_iter().map(|m| {
                                        let is_selected = model.get() == m;
                                        view! { <option value=&m selected=is_selected>{&m}</option> }
                                    }).collect::<Vec<_>>()
                                }}
                            </select>
                            <p class="form-field__helper">"The model used for review agents"</p>
                        </div>

                        <div class="form-field">
                            <label class="form-field__label" for="dataset">"Dataset"</label>
                            <select id="dataset" class="input select" prop:value=dataset.get() on:change=move |ev| {
                                set_dataset.set(event_target_value(&ev));
                            }>
                                {move || {
                                    let ds = datasets.get();
                                    if !ds.is_empty() {
                                        ds.into_iter().map(|d| {
                                            let is_selected = dataset.get() == d.id;
                                            let label = format!("{} ({} PRs)", d.id, d.pr_count);
                                            view! { <option value=&d.id selected=is_selected>{label}</option> }
                                        }).collect::<Vec<_>>()
                                    } else {
                                        let cfg = config.get();
                                        let datasets = if let Some(ref c) = cfg {
                                            c.datasets.clone()
                                        } else {
                                            vec!["golden_comments".into()]
                                        };
                                        datasets.into_iter().map(|d| {
                                            let is_selected = dataset.get() == d;
                                            view! { <option value=&d selected=is_selected>{&d}</option> }
                                        }).collect::<Vec<_>>()
                                    }
                                }}
                            </select>
                            <p class="form-field__helper">"The dataset used for evaluation"</p>
                        </div>
                    </div>
                </section>

                // ─── Execution Section ──────────────────────────────
                <section class="form-section">
                    <h2 class="form-section__title">"Execution"</h2>
                    <div class="form-section__fields">
                        {move || {
                            let cfg = config.get();
                            let roles_list: Vec<String> = if let Some(ref c) = cfg {
                                c.roles.clone()
                            } else {
                                vec!["reviewer".into(), "summarizer".into(), "tester".into(), "analyst".into()]
                            };
                            view! {
                                <div class="form-field">
                                    <label class="form-field__label">"Roles / Agents"</label>
                                    <div class="checkbox-group">
                                        {roles_list.into_iter().map(|r| {
                                            let checked = roles.get().contains(&r);
                                            view! {
                                                <label class="checkbox-label">
                                                    <input
                                                        type="checkbox"
                                                        prop:checked=checked
                                                        on:click={
                                                            let r_clone = r.clone();
                                                            move |_| toggle_role(&r_clone)
                                                        }
                                                    />
                                                    {r}
                                                </label>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </div>
                                    <p class="form-field__helper">"Select at least one role for this run."</p>
                                </div>
                            }
                        }}
                    </div>
                </section>

                // ─── PR Filter Section ─────────────────────────────
                <section class="form-section">
                    <h2 class="form-section__title">"Filtering"</h2>
                    <div class="form-section__fields">
                        <div class="form-field">
                            <label class="form-field__label" for="pr_filter">"PR Filter (optional)"</label>
                            <input
                                id="pr_filter"
                                class="input"
                                type="text"
                                prop:value=pr_filter.get()
                                on:input=move |ev| { set_pr_filter.set(event_target_value(&ev)); }
                                placeholder="discourse-7,calcom-11059"
                            />
                            <p class="form-field__helper">"Filter to specific PRs: discourse-7,calcom-11059. Leave empty for all PRs."</p>
                        </div>
                    </div>
                </section>

                // ─── Form Actions ──────────────────────────────────────
                <div class="form-actions">
                    <button
                        type="submit"
                        class="btn btn--primary btn--lg btn--full"
                        disabled=move || submitting.get() || roles.get().is_empty()
                    >
                        {move || {
                            if submitting.get() {
                                "Creating..."
                            } else {
                                "🚀 Start Benchmark"
                            }
                        }}
                    </button>
                </div>

                // Submit error
                {move || {
                    if let Some(e) = submit_error.get() {
                        view! {
                            <div class="error-state" role="alert" style="padding: var(--spacing-lg, 16px);">
                                <p style="color: var(--accent-red, #f85149); font-size: var(--text-sm, 14px);">{format!("Error: {}", e)}</p>
                            </div>
                        }.into_view()
                    } else {
                        view! { <span></span> }.into_view()
                    }
                }}
            </form>
        </div>
    }
}

async fn get_config() -> Result<AppConfig, String> {
    let url = api_url("/api/config");
    let response = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !response.ok() {
        // Return sensible defaults
        return Ok(AppConfig {
            models: vec![
                "deepseek/deepseek-v4-flash".into(),
                "deepseek/deepseek-v4-pro".into(),
            ],
            datasets: vec!["swir-bench".into(), "code-review-bench".into()],
            roles: vec![
                "reviewer".into(),
                "summarizer".into(),
                "tester".into(),
                "analyst".into(),
            ],
        });
    }

    let data: AppConfig = response
        .json()
        .await
        .map_err(|e| format!("Parse error: {}", e))?;

    Ok(data)
}

async fn create_run(req: NewRunRequest) -> Result<NewRunResponse, String> {
    let url = api_url("/api/runs");
    let body = serde_json::to_string(&req).map_err(|e| format!("Serialize error: {e}"))?;

    let response = gloo_net::http::Request::post(&url)
        .header("Content-Type", "application/json")
        .body(&body)
        .map_err(|e| format!("Body error: {e}"))?
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if !response.ok() {
        let text = response.text().await.unwrap_or_default();
        return Err(format!("Server returned {}: {}", response.status(), text));
    }

    let data: NewRunResponse = response
        .json()
        .await
        .map_err(|e| format!("Parse error: {e}"))?;

    Ok(data)
}

async fn get_datasets() -> Result<Vec<DatasetInfo>, String> {
    let url = api_url("/api/config/datasets");
    let response = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if !response.ok() {
        return Ok(Vec::new());
    }

    let data: Vec<DatasetInfo> = response
        .json()
        .await
        .map_err(|e| format!("Parse error: {e}"))?;

    Ok(data)
}
