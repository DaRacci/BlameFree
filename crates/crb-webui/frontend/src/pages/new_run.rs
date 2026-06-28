use crate::{api_url, AppConfig, DatasetInfo, NewRunRequest, NewRunResponse, PrEntry};
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

    // Multi-select PR checklist state
    let (available_prs, set_available_prs) = create_signal::<Vec<PrEntry>>(Vec::new());
    let (selected_prs, set_selected_prs) = create_signal::<Vec<String>>(Vec::new());
    let (prs_loading, set_prs_loading) = create_signal(false);

    let (concurrency, set_concurrency) = create_signal(String::new());
    let (max_findings, set_max_findings) = create_signal(String::new());
    let (submitting, set_submitting) = create_signal(false);
    let (submit_error, set_submit_error) = create_signal::<Option<String>>(None);
    let (submit_result, set_submit_result) = create_signal::<Option<String>>(None);

    let navigator = use_navigate();

    // Fetch PRs for a given dataset
    let fetch_prs = move |ds_id: String| {
        if ds_id.is_empty() {
            set_available_prs.set(Vec::new());
            set_selected_prs.set(Vec::new());
            return;
        }
        set_prs_loading.set(true);
        let set_available = set_available_prs.clone();
        let set_selected = set_selected_prs.clone();
        let set_loading = set_prs_loading.clone();
        spawn_local(async move {
            match get_dataset_prs(&ds_id).await {
                Ok(prs) => {
                    // Select all PRs by default
                    let all_keys: Vec<String> = prs.iter().map(|p| p.key.clone()).collect();
                    set_available.set(prs);
                    set_selected.set(all_keys);
                }
                Err(_) => {
                    set_available.set(Vec::new());
                    set_selected.set(Vec::new());
                }
            }
            set_loading.set(false);
        });
    };

    // When dataset selection changes, auto-fill from config defaults and fetch PRs
    let on_dataset_change = move |ev: leptos::ev::Event| {
        let new_ds = event_target_value(&ev);
        set_dataset.set(new_ds.clone());

        // Look up the selected dataset in the datasets list
        let ds_list = datasets.get();
        if let Some(ds_info) = ds_list.iter().find(|d| d.id == new_ds) {
            if let Some(ref cfg) = ds_info.config {
                let defaults = &cfg.defaults;
                if let Some(ref m) = defaults.model {
                    set_model.set(m.clone());
                }
                if let Some(c) = defaults.concurrency {
                    set_concurrency.set(c.to_string());
                }
                if let Some(mf) = defaults.max_findings {
                    set_max_findings.set(mf.to_string());
                }
                if let Some(ref r) = defaults.roles {
                    let roles_vec: Vec<String> = r
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    set_roles.set(roles_vec);
                }
            }
        }

        // Fetch PRs for this dataset
        fetch_prs(new_ds);
    };

    // Fetch config
    let _fetch_config = create_local_resource(
        || (),
        move |_| {
            let set_config = set_config.clone();
            let set_loading = set_config_loading.clone();
            let set_error = set_config_error.clone();
            let set_model = set_model.clone();
            let dataset = dataset.clone();
            let set_dataset = set_dataset.clone();
            let set_datasets = set_datasets.clone();
            let set_datasets_loading = set_datasets_loading.clone();
            let set_concurrency = set_concurrency.clone();
            let set_max_findings = set_max_findings.clone();
            let set_roles = set_roles.clone();
            let fetch_prs = fetch_prs.clone();
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
                // Then fetch dataset details (with config)
                match get_datasets().await {
                    Ok(ds) => {
                        // Auto-fill from first dataset's config defaults
                        if let Some(first) = ds.first() {
                            let current_ds = dataset.get();
                            // If the first dataset is already selected, apply its defaults
                            if first.id == current_ds {
                                if let Some(ref cfg) = first.config {
                                    if let Some(ref m) = cfg.defaults.model {
                                        set_model.set(m.clone());
                                    }
                                    if let Some(c) = cfg.defaults.concurrency {
                                        set_concurrency.set(c.to_string());
                                    }
                                    if let Some(mf) = cfg.defaults.max_findings {
                                        set_max_findings.set(mf.to_string());
                                    }
                                    if let Some(ref r) = cfg.defaults.roles {
                                        let roles_vec: Vec<String> = r
                                            .split(',')
                                            .map(|s| s.trim().to_string())
                                            .filter(|s| !s.is_empty())
                                            .collect();
                                        set_roles.set(roles_vec);
                                    }
                                }
                            }
                        }
                        set_datasets.set(ds);
                        set_datasets_loading.set(false);
                    }
                    Err(_) => {
                        set_datasets_loading.set(false);
                    }
                }

                // Fetch PRs for the initially selected dataset
                let initial_ds = dataset.get();
                if !initial_ds.is_empty() {
                    fetch_prs(initial_ds);
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

        // If all PRs are selected, send None (run all); otherwise send comma-joined keys
        let total_keys = available_prs.get().len();
        let selected = selected_prs.get();
        let pr_filter = if selected.len() == total_keys {
            None
        } else if selected.is_empty() {
            None
        } else {
            Some(selected.join(","))
        };

        let req = NewRunRequest {
            model: model.get(),
            dataset: dataset.get(),
            roles: roles.get(),
            pr_filter,
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
                            <select id="dataset" class="input select" prop:value=dataset.get() on:change=on_dataset_change>
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
                    <h2 class="form-section__title">"PR Selection"</h2>
                    <div class="form-section__fields">
                        <div class="form-field">
                            <label class="form-field__label">"Select PRs to evaluate"</label>
                            {move || {
                                if prs_loading.get() {
                                    view! {
                                        <div style="color: var(--text-secondary, #8b949e); padding: 8px 0;">
                                            "Loading PRs..."
                                        </div>
                                    }.into_view()
                                } else {
                                    let prs = available_prs.get();
                                    if prs.is_empty() {
                                        view! {
                                            <div style="color: var(--text-secondary, #8b949e); padding: 8px 0;">
                                                "Select a dataset to see available PRs."
                                            </div>
                                        }.into_view()
                                    } else {
                                        let sel = selected_prs.get();
                                        let total = prs.len();
                                        let checked = sel.len();
                                        view! {
                                            <div style="margin-bottom: 8px; display: flex; gap: 8px; align-items: center;">
                                                <span style="color: var(--text-secondary, #8b949e); font-size: var(--text-sm, 14px);">
                                                    {format!("{} / {} PRs selected", checked, total)}
                                                </span>
                                                <button
                                                    type="button"
                                                    class="btn btn--ghost btn--sm"
                                                    on:click=move |_| {
                                                        let all_keys: Vec<String> = available_prs.get().iter().map(|p| p.key.clone()).collect();
                                                        set_selected_prs.set(all_keys);
                                                    }
                                                >
                                                    "Select All"
                                                </button>
                                                <button
                                                    type="button"
                                                    class="btn btn--ghost btn--sm"
                                                    on:click=move |_| {
                                                        set_selected_prs.set(Vec::new());
                                                    }
                                                >
                                                    "Deselect All"
                                                </button>
                                            </div>
                                            <div class="checkbox-group" style="max-height: 300px; overflow-y: auto; border: 1px solid var(--border, #30363d); border-radius: 6px; padding: 8px;">
                                                {prs.into_iter().map(|pr| {
                                                    let is_checked = sel.contains(&pr.key);
                                                    let label = format!("{} — {}", pr.repo, pr.title);
                                                    view! {
                                                        <label class="checkbox-label" style="padding: 4px 0;">
                                                            <input
                                                                type="checkbox"
                                                                prop:checked=is_checked
                                                                on:click={
                                                                    let key = pr.key.clone();
                                                                    move |_| {
                                                                        set_selected_prs.update(|sel| {
                                                                            if let Some(pos) = sel.iter().position(|k| k == &key) {
                                                                                sel.remove(pos);
                                                                            } else {
                                                                                sel.push(key.clone());
                                                                            }
                                                                        });
                                                                    }
                                                                }
                                                            />
                                                            <span style="font-size: var(--text-sm, 14px);">{label}</span>
                                                        </label>
                                                    }
                                                }).collect::<Vec<_>>()}
                                            </div>
                                            <p class="form-field__helper">"Uncheck PRs you want to skip. All PRs selected = run entire dataset."</p>
                                        }.into_view()
                                    }
                                }
                            }}
                        </div>
                    </div>
                </section>

                // ─── Advanced Section ─────────────────────────────
                <section class="form-section">
                    <h2 class="form-section__title">"Advanced"</h2>
                    <div class="form-section__fields">
                        <div class="form-field">
                            <label class="form-field__label" for="concurrency">"Concurrency"</label>
                            <input
                                id="concurrency"
                                class="input"
                                type="number"
                                prop:value=concurrency.get()
                                on:input=move |ev| { set_concurrency.set(event_target_value(&ev)); }
                                placeholder="4"
                                min="1"
                            />
                            <p class="form-field__helper">"Number of concurrent agent evaluations"</p>
                        </div>
                        <div class="form-field">
                            <label class="form-field__label" for="max_findings">"Max Findings per Agent"</label>
                            <input
                                id="max_findings"
                                class="input"
                                type="number"
                                prop:value=max_findings.get()
                                on:input=move |ev| { set_max_findings.set(event_target_value(&ev)); }
                                placeholder="20"
                                min="1"
                            />
                            <p class="form-field__helper">"Maximum number of findings per agent per PR"</p>
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

async fn get_dataset_prs(id: &str) -> Result<Vec<PrEntry>, String> {
    let url = api_url(&format!("/api/datasets/{}/prs", id));
    let response = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if !response.ok() {
        return Ok(Vec::new());
    }

    let data: Vec<PrEntry> = response
        .json()
        .await
        .map_err(|e| format!("Parse error: {e}"))?;

    Ok(data)
}
