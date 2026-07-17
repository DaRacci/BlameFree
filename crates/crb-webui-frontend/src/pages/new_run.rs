use std::sync::Arc;

use crate::AppConfig;
use crate::components::role_selector::RoleSelector;
use crate::{NewRunRequest, NewRunResponse};
use crb_shared::{DEFAULT_MODEL, DEFAULT_MODEL_PRO};
use crb_types::capabilities::ReasoningEffort;
use crb_webui_shared::config::{DatasetInfo, PrEntry};
use crb_webui_shared::route;
use crb_webui_shared::routes::{API_CONFIG, API_CONFIG_DATASETS, API_CONFIG_REASONING, API_RUNS};
use gloo_net::http::Request;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;

#[derive(Clone, Copy)]
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
    max_findings: ReadSignal<String>,
    set_max_findings: WriteSignal<String>,
    reasoning_effort: ReadSignal<Option<ReasoningEffort>>,
    set_reasoning_effort: WriteSignal<Option<ReasoningEffort>>,
    submitting: ReadSignal<bool>,
    set_submitting: WriteSignal<bool>,
    submit_error: ReadSignal<Option<String>>,
    set_submit_error: WriteSignal<Option<String>>,
    set_submit_result: WriteSignal<Option<String>>,
    effort_levels: ReadSignal<Vec<ReasoningEffort>>,
    set_effort_levels: WriteSignal<Vec<ReasoningEffort>>,
    effort_loading: ReadSignal<bool>,
    set_effort_loading: WriteSignal<bool>,
    roles: ReadSignal<Vec<String>>,
    judge_model: ReadSignal<String>,
    set_judge_model: WriteSignal<String>,
}

fn create_form_signals() -> NewRunSignals {
    let (config, set_config) = signal(None);
    let (config_loading, set_config_loading) = signal(true);
    let (config_error, set_config_error) = signal(None);
    let (datasets, set_datasets) = signal(Vec::new());
    let (datasets_loading, set_datasets_loading) = signal(true);
    let (model, set_model) = signal(String::new());
    let (dataset, set_dataset) = signal(String::new());
    let (roles, set_roles) = signal(Vec::new());
    let (available_prs, set_available_prs) = signal(Vec::new());
    let (selected_prs, set_selected_prs) = signal(Vec::new());
    let (prs_loading, set_prs_loading) = signal(false);
    let (concurrency, set_concurrency) = signal(String::new());
    let (max_findings, set_max_findings) = signal(String::new());
    let (use_cache, set_use_cache) = signal(true);
    let (reasoning_effort, set_reasoning_effort) = signal(Some(ReasoningEffort::Medium));
    let (submitting, set_submitting) = signal(false);
    let (submit_error, set_submit_error) = signal(None);
    let (_submit_result, set_submit_result) = signal(None);
    let (effort_levels, set_effort_levels) = signal(Vec::new());
    let (effort_loading, set_effort_loading) = signal(true);
    let (judge_model, set_judge_model) = signal(String::new());
    let (cache_dir, set_cache_dir) = signal(String::new());
    let (skip_consensus, set_skip_consensus) = signal(false);
    let (linters_only, set_linters_only) = signal(false);

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
        max_findings,
        set_max_findings,
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
        judge_model,
        set_judge_model,
    }
}

fn create_fetch_prs_handler(signals: NewRunSignals) -> impl Fn(String) {
    move |ds_id: String| {
        if ds_id.is_empty() {
            signals.set_available_prs.set(Vec::new());
            signals.set_selected_prs.set(Vec::new());
            return;
        }
        set_prs_loading.set(true);
        spawn_local(async move {
            match crate::fetch_json::<Vec<PrEntry>>(&route!(API_DATASETS_ID_PRS, ds_id)).await {
                Ok(prs) => {
                    let all_keys: Vec<String> = prs.iter().map(|p| p.key.clone()).collect();
                    signals.set_available.set(prs);
                    signals.set_selected.set(all_keys);
                }
                Err(_) => {
                    signals.set_available.set(Vec::new());
                    signals.set_selected.set(Vec::new());
                }
            }
            signals.set_loading.set(false);
        });
    }
}

fn create_dataset_change_handler(
    signals: NewRunSignals,
    fetch_prs: Arc<dyn Fn(String) + 'static>,
) -> impl Fn(leptos::ev::Event) {
    move |ev: leptos::ev::Event| {
        let new_ds = event_target_value(&ev);
        signals.set_dataset.set(new_ds.clone());

        let ds_list = signals.datasets.get();
        if let Some(ds_info) = ds_list.iter().find(|d| d.id == new_ds) {
            if let Some(ref cfg) = ds_info.config {
                let defaults = &cfg.defaults;
                if let Some(ref m) = defaults.model {
                    signals.set_model.set(m.clone());
                }
                if let Some(c) = defaults.concurrency {
                    signals.set_concurrency.set(c.to_string());
                }
                if let Some(mf) = defaults.max_findings {
                    signals.set_max_findings.set(mf.to_string());
                }
                if let Some(ref r) = defaults.roles {
                    let roles_vec: Vec<String> = r.clone();
                    signals.set_roles.set(roles_vec);
                }
            }
        }

        fetch_prs(new_ds);
    }
}

fn init_config_spawn(signals: NewRunSignals, fetch_prs: Arc<dyn Fn(String) + 'static>) {
    spawn_local({
        let signals = signals;
        let fetch_prs = fetch_prs;
        async move {
            signals.set_loading.set(true);
            signals.set_datasets_loading.set(true);
            match async move { crate::fetch_json::<AppConfig>(API_CONFIG).await }.await {
                Ok(cfg) => {
                    if let Some(m) = cfg.models.first() {
                        signals.set_model.set(m.clone());
                    }
                    if let Some(d) = cfg.datasets.first() {
                        signals.set_dataset.set(d.clone());
                    }
                    signals.set_config.set(Some(cfg));
                    signals.set_loading.set(false);
                }
                Err(e) => {
                    signals.set_error.set(Some(e));
                    signals.set_loading.set(false);
                }
            }

            match async move { crate::fetch_json::<Vec<DatasetInfo>>(API_CONFIG_DATASETS).await }
                .await
            {
                Ok(ds) => {
                    if let Some(first) = ds.first() {
                        let current_ds = dataset.get();
                        if first.id == current_ds {
                            if let Some(ref cfg) = first.config {
                                if let Some(ref m) = cfg.defaults.model {
                                    signals.set_model.set(m.clone());
                                }
                                if let Some(c) = cfg.defaults.concurrency {
                                    signals.set_concurrency.set(c.to_string());
                                }
                                if let Some(mf) = cfg.defaults.max_findings {
                                    signals.set_max_findings.set(mf.to_string());
                                }
                                if let Some(ref r) = cfg.defaults.roles {
                                    let roles_vec: Vec<String> = r.clone();
                                    signals.set_roles.set(roles_vec);
                                }
                            }
                        }
                    }
                    signals.set_datasets.set(ds);
                    signals.set_datasets_loading.set(false);
                }
                Err(_) => {
                    signals.set_datasets_loading.set(false);
                }
            }

            let initial_ds = dataset.get();
            if !initial_ds.is_empty() {
                fetch_prs(initial_ds);
            }

            if let Ok(resp) =
                async move { crate::fetch_json::<Vec<ReasoningEffort>>(API_CONFIG_REASONING).await }
                    .await
            {
                signals.set_effort_levels.set(resp.levels);
            }
            signals.set_effort_loading.set(false);
        }
    });
}

fn create_submit_handler(
    signals: NewRunSignals,
    navigator: impl Fn(&str, leptos_router::NavigateOptions) + Clone + 'static,
) -> impl Fn(leptos::ev::SubmitEvent) {
    move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        signals.set_submitting.set(true);
        signals.set_submit_error.set(None);
        signals.set_submit_result.set(None);

        let total_keys = signals.available_prs.get().len();
        let selected = signals.selected_prs.get();
        let pr_filter = if selected.len() == total_keys || selected.is_empty() {
            None
        } else {
            Some(selected.join(","))
        };

        let max_f = signals
            .max_findings
            .get()
            .parse::<usize>()
            .unwrap_or(FINDINGS);
        let cache = cache_dir.get();
        let cache_dir_val = if cache.is_empty() { None } else { Some(cache) };

        let req = NewRunRequest {
            model: model.get(),
            dataset: dataset.get(),
            roles: roles.get(),
            pr_filter,
            reasoning_effort: reasoning_effort.get(),
            judge_model: judge_model.get(),
            max_findings: max_f,
        };

        let navigator = navigator.clone();
        spawn_local(async move {
            match create_run(req).await {
                Ok(resp) => {
                    signals.set_submitting.set(false);
                    signals.set_submit_result.set(Some(resp.run_id.clone()));
                    navigator(&format!("/runs/{}", resp.run_id), Default::default());
                }
                Err(e) => {
                    signals.set_submitting.set(false);
                    signals.set_submit_error.set(Some(e));
                }
            }
        });
    }
}

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
    move || -> AnyView {
        if config_loading.get() || datasets_loading.get() {
            view! {
                <div style="display: flex; align-items: center; gap: 12px; color: var(--text-secondary, #8b949e); padding: 24px 0;">
                    <div class="skeleton skeleton--text" style="width: 200px;"></div>
                </div>
            }.into_view().into_any()
        } else {
            view! { <span></span> }.into_view().into_any()
        }
    }
}

fn render_config_error_view(config_error: ReadSignal<Option<String>>) -> impl IntoView {
    move || -> AnyView {
        if let Some(e) = config_error.get() {
            view! {
                <div class="card" style="margin-bottom: var(--spacing-lg, 16px);">
                    <div class="card__body">
                        <p style="color: var(--accent-red, #f85149);">{format!("Failed to load config: {}", e)}</p>
                        <p style="color: var(--text-secondary, #8b949e); font-size: var(--text-sm, 14px);">"You can still fill in the form manually."</p>
                    </div>
                </div>
            }.into_view().into_any()
        } else {
            view! { <span></span> }.into_view().into_any()
        }
    }
}

fn render_reduce_diff_badge() -> impl IntoView {
    view! { <span></span> }.into_view()
}

fn render_config_section(
    signals: NewRunSignals,
    on_dataset_change: impl Fn(leptos::ev::Event) + 'static,
) -> impl IntoView {
    view! {
        <section class="form-section">
            <h2 class="form-section__title">"Configuration"</h2>
            <div class="form-section__fields">
                <div class="form-field">
                    <label class="form-field__label" for="model">"Model"</label>
                    <select id="model" class="input select" prop:value=signals.model.get() on:change=move |ev| {
                        signals.set_model.set(event_target_value(&ev));
                    }>
                        {move || {
                            let cfg = signals.config.get();
                            let models = if let Some(ref c) = cfg {
                                c.models.clone()
                            } else {
                                vec![DEFAULT_MODEL.into(), DEFAULT_MODEL_PRO.into()]
                            };
                            models.into_iter().map(|m| {
                            let is_selected = signals.model.get() == m;
                            view! { <option value=m.clone() selected=is_selected>{m.clone()}</option> }
                        }).collect::<Vec<_>>()
                        }}
                    </select>
                    <p class="form-field__helper">"The model used for review agents"</p>
                </div>

                <div class="form-field">
                    <label class="form-field__label" for="dataset">"Dataset"</label>
                    <select id="dataset" class="input select" prop:value=signals.dataset.get() on:change=on_dataset_change>
                        {move || {
                            let ds = signals.datasets.get();
                            if !ds.is_empty() {
                                ds.into_iter().map(|d| {
                                    let is_selected = signals.dataset.get() == d.id;
                                    let label = format!("{} ({} PRs)", d.id, d.pr_count);
                                    view! { <option value=d.id.clone() selected=is_selected>{label}</option> }
                                }).collect::<Vec<_>>()
                            } else {
                                let cfg = signals.config.get();
                                let datasets = if let Some(ref c) = cfg {
                                    c.datasets.clone()
                                } else {
                                    vec!["golden_comments".into()]
                                };
                                datasets.into_iter().map(|d| {
                                    let is_selected = signals.dataset.get() == d;
                                    view! { <option value=d.clone() selected=is_selected>{d.clone()}</option> }
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
                {move || -> AnyView {
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
                        }.into_view().into_any()
                    } else {
                        view! { <span></span> }.into_view().into_any()
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
                    {move || -> AnyView {
                        if prs_loading.get() {
                            return view! {
                                <div style="color: var(--text-secondary, #8b949e); padding: 8px 0;">
                                    "Loading PRs..."
                                </div>
                            }.into_view().into_any();
                        }
                        let prs = available_prs.get();
                        if prs.is_empty() {
                            return view! {
                                <div style="color: var(--text-secondary, #8b949e); padding: 8px 0;">
                                    "Select a dataset to see available PRs."
                                </div>
                            }.into_view().into_any();
                        }
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
                        }.into_view().into_any()
                    }}
                </div>
            </div>
        </section>
    }
}

fn render_advanced_section(signals: NewRunSignals) -> impl IntoView {
    view! {
        <section class="form-section">
            <h2 class="form-section__title">"Advanced"</h2>
            <div class="form-section__fields">
                <div class="form-field">
                    <label class="form-field__label" for="judge_model">"Judge Model"</label>
                    <input
                        id="judge_model"
                        class="input"
                        type="text"
                        prop:value=signals.judge_model.get()
                        on:input=move |ev| { signals.set_judge_model.set(event_target_value(&ev)); }
                        placeholder="..."
                    />
                    <p class="form-field__helper">"Model used for judge evaluations"</p>
                </div>
                <div class="form-field">
                    <label class="form-field__label" for="max_findings">"Max Findings per Agent"</label>
                    <input
                        id="max_findings"
                        class="input"
                        type="number"
                        prop:value=signals.max_findings.get()
                        on:input=move |ev| { signals.set_max_findings.set(event_target_value(&ev)); }
                        placeholder="20"
                        min="1"
                    />
                    <p class="form-field__helper">"Maximum number of findings per agent per PR"</p>
                </div>
                <div class="form-field">
                    <label class="form-field__label" for="reasoning_effort">"Reasoning Effort"</label>
                    <select
                        id="reasoning_effort"
                        class="input select"
                        on:change=move |ev| {
                            let val = event_target_value(&ev);
                            if val == "none" {
                                signals.set_reasoning_effort.set(None);
                            } else {
                                signals.set_reasoning_effort.set(Some(ReasoningEffort::try_from(val.as_str()).unwrap_or(ReasoningEffort::Medium)));
                            }
                        }
                    >
                        {move || {
                            let current = signals.reasoning_effort.get();
                            let levels = signals.effort_levels.get();
                            let loading = signals.effort_loading.get();
                            let mut options: Vec<AnyView> = Vec::new();
                            options.push(view! { <option value="none">"None (disable reasoning)"</option> }.into_view().into_any());
                            if loading {
                                options.push(view! { <option value="loading" disabled>"Loading..."</option> }.into_view().into_any());
                            } else {
                                for level in &levels {
                                    let val = level.clone().to_string();
                                    let label = val[..1].to_uppercase() + &val[1..];
                                    let is_selected = match &current {
                                        Some(curr) if curr == level => true,
                                        None if level == &ReasoningEffort::Medium => true,
                                        _ => false,
                                    };
                                    options.push(view! { <option value=val selected=is_selected>{label}</option> }.into_view().into_any());
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
    move || -> AnyView {
        if let Some(e) = submit_error.get() {
            view! {
                <div class="error-state" role="alert" style="padding: var(--spacing-lg, 16px);">
                    <p style="color: var(--accent-red, #f85149); font-size: var(--text-sm, 14px);">{format!("Error: {}", e)}</p>
                </div>
            }.into_view().into_any()
        } else {
            view! { <span></span> }.into_view().into_any()
        }
    }
}

#[component]
pub fn NewRunPage() -> impl IntoView {
    let s = create_form_signals();
    let navigator = use_navigate();
    let fetch_prs = Arc::new(create_fetch_prs_handler(s));
    let on_dataset_change = create_dataset_change_handler(s, fetch_prs.clone());
    let on_submit = create_submit_handler(s, navigator);
    init_config_spawn(s, fetch_prs);

    view! {
        <div class="new-run-page">
            {render_page_header()}
            {render_loading_indicator(s.config_loading, s.datasets_loading)}
            {render_config_error_view(s.config_error)}
            {render_reduce_diff_badge()}
            <form on:submit=on_submit>
                {render_config_section(s, on_dataset_change)}
                {render_execution_section(s.config, s.roles, s.set_roles)}
                {render_pr_selection_section(s.prs_loading, s.available_prs, s.selected_prs, s.set_selected_prs)}
                {render_advanced_section(s)}
                {render_submit_button(s.submitting, s.roles)}
                {render_submit_error(s.submit_error)}
            </form>
        </div>
    }
}

async fn create_run(req: NewRunRequest) -> Result<NewRunResponse, String> {
    let body = serde_json::to_string(&req).map_err(|e| format!("Serialize error: {e}"))?;

    let response = Request::post(&API_RUNS)
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
