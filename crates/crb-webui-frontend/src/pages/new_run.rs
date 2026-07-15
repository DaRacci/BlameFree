use crate::components::role_selector::RoleSelector;
use crate::{AppConfig, NewRunRequest, NewRunResponse};
use crb_shared::{DEFAULT_MODEL, DEFAULT_MODEL_PRO};
use crb_webui_shared::config::{DatasetInfo, PrEntry};
use leptos::{
    IntoView, ReadSignal, SignalGet, SignalSet, SignalUpdate, WriteSignal, component,
    create_local_resource, create_signal, event_target_value, spawn_local, view,
};
use leptos_router::use_navigate;

// ─── Signal Declarations ────────────────────────────────────────────────────

struct NewRunSignals {
    config: ReadSignal<Option<AppConfig>>,
    set_config: WriteSignal<Option<AppConfig>>,
    config_loading: ReadSignal<bool>,
    set_config_loading: WriteSignal<bool>,
    config_error: ReadSignal<Option<String>>,
    set_config_error: WriteSignal<Option<String>>,
    datasets: ReadSignal<Vec<DatasetInfo>>,
    set_datasets: WriteSignal<Vec<DatasetInfo>>,
    datasets_loading: ReadSignal<bool>,
    set_datasets_loading: WriteSignal<bool>,
    model: ReadSignal<String>,
    set_model: WriteSignal<String>,
    dataset: ReadSignal<String>,
    set_dataset: WriteSignal<String>,
    set_roles: WriteSignal<Vec<String>>,
    available_prs: ReadSignal<Vec<PrEntry>>,
    set_available_prs: WriteSignal<Vec<PrEntry>>,
    selected_prs: ReadSignal<Vec<String>>,
    set_selected_prs: WriteSignal<Vec<String>>,
    prs_loading: ReadSignal<bool>,
    set_prs_loading: WriteSignal<bool>,
    concurrency: ReadSignal<String>,
    set_concurrency: WriteSignal<String>,
    max_findings: ReadSignal<String>,
    set_max_findings: WriteSignal<String>,
    use_cache: ReadSignal<bool>,
    set_use_cache: WriteSignal<bool>,
    reasoning_effort: ReadSignal<Option<String>>,
    set_reasoning_effort: WriteSignal<Option<String>>,
    submitting: ReadSignal<bool>,
    set_submitting: WriteSignal<bool>,
    submit_error: ReadSignal<Option<String>>,
    set_submit_error: WriteSignal<Option<String>>,
    set_submit_result: WriteSignal<Option<String>>,
    effort_levels: ReadSignal<Vec<String>>,
    set_effort_levels: WriteSignal<Vec<String>>,
    effort_loading: ReadSignal<bool>,
    set_effort_loading: WriteSignal<bool>,
    roles: ReadSignal<Vec<String>>,
}

fn create_form_signals() -> NewRunSignals {
    let (config, set_config) = create_signal::<Option<AppConfig>>(None);
    let (config_loading, set_config_loading) = create_signal(true);
    let (config_error, set_config_error) = create_signal::<Option<String>>(None);
    let (datasets, set_datasets) = create_signal::<Vec<DatasetInfo>>(Vec::new());
    let (datasets_loading, set_datasets_loading) = create_signal(true);
    let (model, set_model) = create_signal(String::new());
    let (dataset, set_dataset) = create_signal(String::new());
    let (roles, set_roles) = create_signal::<Vec<String>>(Vec::new());
    let (available_prs, set_available_prs) = create_signal::<Vec<PrEntry>>(Vec::new());
    let (selected_prs, set_selected_prs) = create_signal::<Vec<String>>(Vec::new());
    let (prs_loading, set_prs_loading) = create_signal(false);
    let (concurrency, set_concurrency) = create_signal(String::new());
    let (max_findings, set_max_findings) = create_signal(String::new());
    let (use_cache, set_use_cache) = create_signal(true);
    let (reasoning_effort, set_reasoning_effort) =
        create_signal::<Option<String>>(Some("medium".to_string()));
    let (submitting, set_submitting) = create_signal(false);
    let (submit_error, set_submit_error) = create_signal::<Option<String>>(None);
    let (_submit_result, set_submit_result) = create_signal::<Option<String>>(None);
    let (effort_levels, set_effort_levels) = create_signal::<Vec<String>>(Vec::new());
    let (effort_loading, set_effort_loading) = create_signal(true);

    NewRunSignals {
        config,
        set_config,
        config_loading,
        set_config_loading,
        config_error,
        set_config_error,
        datasets,
        set_datasets,
        datasets_loading,
        set_datasets_loading,
        model,
        set_model,
        dataset,
        set_dataset,
        set_roles,
        available_prs,
        set_available_prs,
        selected_prs,
        set_selected_prs,
        prs_loading,
        set_prs_loading,
        concurrency,
        set_concurrency,
        max_findings,
        set_max_findings,
        use_cache,
        set_use_cache,
        reasoning_effort,
        set_reasoning_effort,
        submitting,
        set_submitting,
        submit_error,
        set_submit_error,
        set_submit_result,
        effort_levels,
        set_effort_levels,
        effort_loading,
        set_effort_loading,
        roles,
    }
}

// ─── Data Fetching ──────────────────────────────────────────────────────────

fn create_fetch_prs_handler(
    set_available_prs: WriteSignal<Vec<PrEntry>>,
    set_selected_prs: WriteSignal<Vec<String>>,
    set_prs_loading: WriteSignal<bool>,
) -> impl Fn(String) {
    move |ds_id: String| {
        if ds_id.is_empty() {
            set_available_prs.set(Vec::new());
            set_selected_prs.set(Vec::new());
            return;
        }
        set_prs_loading.set(true);
        let set_available = set_available_prs;
        let set_selected = set_selected_prs;
        let set_loading = set_prs_loading;
        spawn_local(async move {
            match get_dataset_prs(&ds_id).await {
                Ok(prs) => {
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
    }
}

fn create_dataset_change_handler(
    set_dataset: WriteSignal<String>,
    set_model: WriteSignal<String>,
    datasets: ReadSignal<Vec<DatasetInfo>>,
    set_concurrency: WriteSignal<String>,
    set_max_findings: WriteSignal<String>,
    set_roles: WriteSignal<Vec<String>>,
    fetch_prs: std::sync::Arc<dyn Fn(String) + 'static>,
) -> impl Fn(leptos::ev::Event) {
    move |ev: leptos::ev::Event| {
        let new_ds = event_target_value(&ev);
        set_dataset.set(new_ds.clone());

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

        fetch_prs(new_ds);
    }
}

fn create_config_resource(
    set_config: WriteSignal<Option<AppConfig>>,
    set_config_loading: WriteSignal<bool>,
    set_config_error: WriteSignal<Option<String>>,
    set_model: WriteSignal<String>,
    dataset: ReadSignal<String>,
    set_dataset: WriteSignal<String>,
    set_datasets: WriteSignal<Vec<DatasetInfo>>,
    set_datasets_loading: WriteSignal<bool>,
    set_concurrency: WriteSignal<String>,
    set_max_findings: WriteSignal<String>,
    set_roles: WriteSignal<Vec<String>>,
    set_effort_levels: WriteSignal<Vec<String>>,
    set_effort_loading: WriteSignal<bool>,
    reasoning_effort: ReadSignal<Option<String>>,
    set_reasoning_effort: WriteSignal<Option<String>>,
    fetch_prs: std::sync::Arc<dyn Fn(String) + 'static>,
) {
    let _ = create_local_resource( // Ignore — triggered for side-effect; returns LocalResource handle
        || (),
        move |_| {
            let set_config = set_config;
            let set_loading = set_config_loading;
            let set_error = set_config_error;
            let set_model = set_model;
            let dataset = dataset;
            let set_dataset = set_dataset;
            let set_datasets = set_datasets;
            let set_datasets_loading = set_datasets_loading;
            let set_concurrency = set_concurrency;
            let set_max_findings = set_max_findings;
            let set_roles = set_roles;
            let set_effort_levels = set_effort_levels;
            let set_effort_loading = set_effort_loading;
            let reasoning_effort = reasoning_effort;
            let set_reasoning_effort = set_reasoning_effort;
            let fetch_prs = fetch_prs.clone();
            async move {
                set_loading.set(true);
                set_datasets_loading.set(true);
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

                match get_datasets().await {
                    Ok(ds) => {
                        if let Some(first) = ds.first() {
                            let current_ds = dataset.get();
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

                let initial_ds = dataset.get();
                if !initial_ds.is_empty() {
                    fetch_prs(initial_ds);
                }

                match get_reasoning_efforts().await {
                    Ok(levels) => {
                        let has_medium = levels.contains(&"medium".to_string());
                        let current = reasoning_effort.get();
                        if current == Some("medium".to_string()) && !has_medium {
                            let first = levels.first().cloned();
                            if let Some(l) = first {
                                set_reasoning_effort.set(Some(l));
                            }
                        }
                        set_effort_levels.set(levels);
                    }
                    Err(_) => {
                        set_effort_levels.set(vec![
                            "low".into(),
                            "medium".into(),
                            "high".into(),
                            "max".into(),
                        ]);
                    }
                }
                set_effort_loading.set(false);
            }
        },
    );
}

fn create_submit_handler(
    available_prs: ReadSignal<Vec<PrEntry>>,
    selected_prs: ReadSignal<Vec<String>>,
    model: ReadSignal<String>,
    dataset: ReadSignal<String>,
    roles: ReadSignal<Vec<String>>,
    use_cache: ReadSignal<bool>,
    reasoning_effort: ReadSignal<Option<String>>,
    set_submitting: WriteSignal<bool>,
    set_submit_error: WriteSignal<Option<String>>,
    set_submit_result: WriteSignal<Option<String>>,
    navigator: impl Fn(&str, leptos_router::NavigateOptions) + Clone + 'static,
) -> impl Fn(leptos::ev::SubmitEvent) {
    move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_submitting.set(true);
        set_submit_error.set(None);
        set_submit_result.set(None);

        let total_keys = available_prs.get().len();
        let selected = selected_prs.get();
        let pr_filter = if selected.len() == total_keys || selected.is_empty() {
            None
        } else {
            Some(selected.join(","))
        };

        let req = NewRunRequest {
            model: model.get(),
            dataset: dataset.get(),
            roles: roles.get(),
            pr_filter,
            use_cache: use_cache.get(),
            reasoning_effort: reasoning_effort.get(),
        };

        let navigator = navigator.clone();
        spawn_local(async move {
            match create_run(req).await {
                Ok(resp) => {
                    set_submitting.set(false);
                    set_submit_result.set(Some(resp.run_id.clone()));
                    navigator(&format!("/runs/{}", resp.run_id), Default::default());
                }
                Err(e) => {
                    set_submitting.set(false);
                    set_submit_error.set(Some(e));
                }
            }
        });
    }
}

// ─── View Section Helpers ───────────────────────────────────────────────────

fn render_page_header() -> impl IntoView {
    view! {
        <div class="page-header">
            <h1 class="page-header__title">"New Benchmark Run"</h1>
            <div class="page-header__actions">
                <a href="/" class="btn btn--ghost">"Cancel"</a>
            </div>
        </div>
    }
}

fn render_loading_indicator(
    config_loading: ReadSignal<bool>,
    datasets_loading: ReadSignal<bool>,
) -> impl IntoView {
    move || {
        if config_loading.get() || datasets_loading.get() {
            view! {
                <div style="display: flex; align-items: center; gap: 12px; color: var(--text-secondary, #8b949e); padding: 24px 0;">
                    <div class="skeleton skeleton--text" style="width: 200px;"></div>
                </div>
            }
            .into_view()
        } else {
            view! { <span></span> }.into_view()
        }
    }
}

fn render_config_error_view(config_error: ReadSignal<Option<String>>) -> impl IntoView {
    move || {
        if let Some(e) = config_error.get() {
            view! {
                <div class="card" style="margin-bottom: var(--spacing-lg, 16px);">
                    <div class="card__body">
                        <p style="color: var(--accent-red, #f85149);">{format!("Failed to load config: {}", e)}</p>
                        <p style="color: var(--text-secondary, #8b949e); font-size: var(--text-sm, 14px);">"You can still fill in the form manually."</p>
                    </div>
                </div>
            }
            .into_view()
        } else {
            view! { <span></span> }.into_view()
        }
    }
}

fn render_reduce_diff_badge(config: ReadSignal<Option<AppConfig>>) -> impl IntoView {
    move || {
        config.get().map(|cfg| {
            if cfg.reduce_diff_enabled {
                view! {
                    <div style="margin-bottom: var(--spacing-lg, 16px);">
                        <span class="badge badge--green">"Reduce Diff: ON (-U1 + stripped)"</span>
                    </div>
                }
                .into_view()
            } else {
                view! {
                    <div style="margin-bottom: var(--spacing-lg, 16px);">
                        <span class="badge badge--muted">"Reduce Diff: OFF (full diff)"</span>
                    </div>
                }
                .into_view()
            }
        }).unwrap_or_else(|| view! { <span></span> }.into_view())
    }
}

fn render_config_section(
    config: ReadSignal<Option<AppConfig>>,
    datasets: ReadSignal<Vec<DatasetInfo>>,
    dataset: ReadSignal<String>,
    model: ReadSignal<String>,
    set_model: WriteSignal<String>,
    on_dataset_change: impl Fn(leptos::ev::Event) + 'static,
) -> impl IntoView {
    view! {
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
                                vec![DEFAULT_MODEL.into(), DEFAULT_MODEL_PRO.into()]
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
    }
}

fn render_execution_section(
    config: ReadSignal<Option<AppConfig>>,
    roles: ReadSignal<Vec<String>>,
    set_roles: WriteSignal<Vec<String>>,
) -> impl IntoView {
    view! {
        <section class="form-section">
            <h2 class="form-section__title">"Execution"</h2>
            <div class="form-section__fields">
                {move || {
                    let cfg = config.get();
                    if let Some(ref c) = cfg {
                        let role_infos = c.roles.clone();
                        view! {
                        <div class="form-field">
                            <label class="form-field__label">"Roles / Agents"</label>
                            <div class="checkbox-group">
                                <RoleSelector available_roles=role_infos selected_roles=roles set_selected_roles=set_roles />
                            </div>
                            <p class="form-field__helper">"Select at least one role for this run."</p>
                        </div>
                        }.into_view()
                    } else {
                        view! { <span></span> }.into_view()
                    }
                }}
            </div>
        </section>
    }
}

fn render_pr_selection_section(
    prs_loading: ReadSignal<bool>,
    available_prs: ReadSignal<Vec<PrEntry>>,
    selected_prs: ReadSignal<Vec<String>>,
    set_selected_prs: WriteSignal<Vec<String>>,
) -> impl IntoView {
    view! {
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
                                            let label = format!("{} - {}", pr.repo, pr.title);
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
    }
}

fn render_advanced_section(
    concurrency: ReadSignal<String>,
    set_concurrency: WriteSignal<String>,
    max_findings: ReadSignal<String>,
    set_max_findings: WriteSignal<String>,
    use_cache: ReadSignal<bool>,
    set_use_cache: WriteSignal<bool>,
    reasoning_effort: ReadSignal<Option<String>>,
    set_reasoning_effort: WriteSignal<Option<String>>,
    effort_levels: ReadSignal<Vec<String>>,
    effort_loading: ReadSignal<bool>,
) -> impl IntoView {
    view! {
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
                <div class="form-field">
                    <label class="checkbox-label">
                        <input
                            type="checkbox"
                            prop:checked=use_cache.get()
                            on:click=move |_| set_use_cache.update(|v| *v = !*v)
                        />
                        "Use cache (reuse LLM responses from previous runs)"
                    </label>
                    <p class="form-field__helper">"Check to cache LLM responses. Uncheck to force fresh API calls."</p>
                </div>
                <div class="form-field">
                    <label class="form-field__label" for="reasoning_effort">"Reasoning Effort"</label>
                    <select
                        id="reasoning_effort"
                        class="input select"
                        on:change=move |ev| {
                            let val = event_target_value(&ev);
                            if val == "none" {
                                set_reasoning_effort.set(None);
                            } else {
                                set_reasoning_effort.set(Some(val));
                            }
                        }
                    >
                        {move || {
                            let current = reasoning_effort.get();
                            let levels = effort_levels.get();
                            let loading = effort_loading.get();
                            let mut options: Vec<_> = Vec::new();
                            options.push(view! { <option value="none">"None (disable reasoning)"</option> });
                            if loading {
                                options.push(view! { <option value="loading" disabled>"Loading..."</option> });
                            } else {
                                for level in &levels {
                                    let val = level.clone();
                                    let label = level[..1].to_uppercase() + &level[1..];
                                    let is_selected = match &current {
                                        Some(s) if s == &val => true,
                                        None if val == "medium" => true,
                                        _ => false,
                                    };
                                    options.push(view! { <option value=val selected=is_selected>{label}</option> });
                                }
                            }
                            options
                        }}
                    </select>
                    <p class="form-field__helper">"Set reasoning/thinking effort for compatible models (DeepSeek, OpenAI o-series, etc.)"</p>
                </div>
            </div>
        </section>
    }
}

fn render_submit_button(
    submitting: ReadSignal<bool>,
    roles: ReadSignal<Vec<String>>,
) -> impl IntoView {
    view! {
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
                        "Start Benchmark"
                    }
                }}
            </button>
        </div>
    }
}

fn render_submit_error(submit_error: ReadSignal<Option<String>>) -> impl IntoView {
    move || {
        if let Some(e) = submit_error.get() {
            view! {
                <div class="error-state" role="alert" style="padding: var(--spacing-lg, 16px);">
                    <p style="color: var(--accent-red, #f85149); font-size: var(--text-sm, 14px);">{format!("Error: {}", e)}</p>
                </div>
            }
            .into_view()
        } else {
            view! { <span></span> }.into_view()
        }
    }
}

// ─── Main Component ─────────────────────────────────────────────────────────

#[component]
pub fn NewRunPage() -> impl IntoView {
    let s = create_form_signals();
    let navigator = use_navigate();

    let fetch_prs = std::sync::Arc::new(create_fetch_prs_handler(
        s.set_available_prs,
        s.set_selected_prs,
        s.set_prs_loading,
    ));

    let on_dataset_change = create_dataset_change_handler(
        s.set_dataset,
        s.set_model,
        s.datasets,
        s.set_concurrency,
        s.set_max_findings,
        s.set_roles,
        fetch_prs.clone(),
    );

    create_config_resource(
        s.set_config,
        s.set_config_loading,
        s.set_config_error,
        s.set_model,
        s.dataset,
        s.set_dataset,
        s.set_datasets,
        s.set_datasets_loading,
        s.set_concurrency,
        s.set_max_findings,
        s.set_roles,
        s.set_effort_levels,
        s.set_effort_loading,
        s.reasoning_effort,
        s.set_reasoning_effort,
        fetch_prs,
    );

    let on_submit = create_submit_handler(
        s.available_prs,
        s.selected_prs,
        s.model,
        s.dataset,
        s.roles,
        s.use_cache,
        s.reasoning_effort,
        s.set_submitting,
        s.set_submit_error,
        s.set_submit_result,
        navigator,
    );

    view! {
        <div class="new-run-page">
            {render_page_header()}
            {render_loading_indicator(s.config_loading, s.datasets_loading)}
            {render_config_error_view(s.config_error)}
            {render_reduce_diff_badge(s.config)}
            <form on:submit=on_submit>
                {render_config_section(
                    s.config, s.datasets, s.dataset, s.model, s.set_model, on_dataset_change,
                )}
                {render_execution_section(s.config, s.roles, s.set_roles)}
                {render_pr_selection_section(
                    s.prs_loading, s.available_prs, s.selected_prs, s.set_selected_prs,
                )}
                {render_advanced_section(
                    s.concurrency, s.set_concurrency,
                    s.max_findings, s.set_max_findings,
                    s.use_cache, s.set_use_cache,
                    s.reasoning_effort, s.set_reasoning_effort,
                    s.effort_levels, s.effort_loading,
                )}
                {render_submit_button(s.submitting, s.roles)}
                {render_submit_error(s.submit_error)}
            </form>
        </div>
    }
}

// ─── API Call Helpers ───────────────────────────────────────────────────────

async fn get_config() -> Result<AppConfig, String> {
    crate::fetch_json("/api/config").await
}

async fn create_run(req: NewRunRequest) -> Result<NewRunResponse, String> {
    let url = "/api/runs";
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
    let data: Vec<DatasetInfo> = crate::fetch_json("/api/config/datasets").await?;
    Ok(data)
}

async fn get_dataset_prs(id: &str) -> Result<Vec<PrEntry>, String> {
    let url = format!("/api/datasets/{}/prs", id);
    crate::fetch_json(&url).await
}

async fn get_reasoning_efforts() -> Result<Vec<String>, String> {
    #[derive(serde::Deserialize)]
    struct Wrapper {
        levels: Vec<String>,
    }
    let wrapper: Wrapper = crate::fetch_json("/api/config/reasoning-efforts").await?;
    Ok(wrapper.levels)
}
