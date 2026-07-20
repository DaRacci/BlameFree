use std::sync::Arc;

use crate::components::agent_selector::RoleSelector;
use crate::components::dataset_selector::DatasetSelector;
use crate::components::form_page::FormPage;
use crate::components::model_selector::ModelSelector;
use crate::components::pr_selection::{PrItem, PrSelection};
use crate::components::reasoning_effort_selector::ReasoningEffortSelector;
use crate::{AppConfig, signal_struct};
use crate::{NewRunRequest, NewRunResponse};
use crb_shared::{DEFAULT_MODEL, DEFAULT_MODEL_PRO};
use crb_types::capabilities::ReasoningEffort;
use crb_webui_shared::config::DatasetInfo;
use crb_webui_shared::routes::{API_CONFIG, API_CONFIG_DATASETS, API_CONFIG_REASONING, API_RUNS};
use gloo_net::http::Request;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;
use log::error;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[deprecated]
struct PrEntry {
    key: String,
    // url: String,
    title: String,
    repo: String,
    // pr_number: u32,
}

impl PrItem for PrEntry {
    fn pr_key(&self) -> &str {
        &self.key
    }
    fn pr_label(&self) -> String {
        format!("{} - {}", self.repo, self.title)
    }
}

signal_struct! {
  #[derive(Debug)]
  struct NewRunSignals {
    config: Option<AppConfig> = None,
    config_loading: bool = true,
    config_error: Option<String> = None,

    datasets: Vec<DatasetInfo> = Vec::new(),
    datasets_loading: bool = true,
    dataset: String = String::new(),

    model: String = String::new(),
    roles: Vec<String> = Vec::new(),
    reasoning_effort: Option<ReasoningEffort> = Some(Default::default()),

    available_prs: Vec<PrEntry> = Vec::new(),
    selected_prs: Vec<String> = Vec::new(),
    prs_loading: bool = false,

    effort_levels: Vec<ReasoningEffort> = Vec::new(),
    effort_loading: bool = true,

    judge_model: String = String::new(),
    max_findings: String = String::new(),

    submitting: bool = false,
    submit_error: Option<String> = None,
  }
  write_only {
    set_submit_result: Option<String> = None,
  }
}

#[component]
pub fn NewRunPage() -> impl IntoView {
    let signals = NewRunSignals::new();
    let navigator = use_navigate();
    let fetch_prs = Arc::new(create_fetch_prs_handler(signals));
    let on_dataset_change = create_dataset_change_handler(signals, fetch_prs.clone());
    let on_submit = create_submit_handler(signals, navigator);
    init_config_spawn(signals, fetch_prs);

    view! {
        <FormPage
            title="New Benchmark Run"
            submit_label="Start Benchmark"
            config_loading=signals.config_loading
            config_error=signals.config_error
            submitting=signals.submitting
            submit_error=signals.submit_error
            on_submit=on_submit
            submit_disabled=move || signals.roles.get().is_empty()
        >
            {render_config_section(signals, on_dataset_change)}
            {render_execution_section(signals.config, signals.roles, signals.set_roles)}
            {render_pr_selection_section(signals.prs_loading, signals.available_prs, signals.selected_prs, signals.set_selected_prs)}
            {render_advanced_section(signals)}
        </FormPage>
    }
}

fn init_config_spawn(signals: NewRunSignals, fetch_prs: Arc<dyn Fn(String) + Send + 'static>) {
    spawn_local({
        let signals = signals;
        let fetch_prs = fetch_prs;
        async move {
            signals.set_config_loading.set(true);
            match async move { crate::fetch_json::<AppConfig>(API_CONFIG).await }.await {
                Ok(cfg) => {
                    if let Some(m) = cfg.models.first() {
                        signals.set_model.set(m.clone());
                    }
                    signals.set_config.set(Some(cfg));
                    signals.set_config_loading.set(false);
                }
                Err(e) => {
                    signals.set_config_error.set(Some(e));
                    signals.set_config_loading.set(false);
                }
            }

            match async move { crate::fetch_json::<Vec<DatasetInfo>>(API_CONFIG_DATASETS).await }
                .await
            {
                Ok(ds) => {
                    signals.set_datasets.set(ds);
                    signals.set_datasets_loading.set(false);
                }
                Err(err) => {
                    error!("Failed to fetch datasets, see error:\n{}", err);
                    signals.set_datasets_loading.set(false);
                }
            }

            let initial_ds = signals.dataset.get();
            if !initial_ds.is_empty() {
                fetch_prs(initial_ds);
            }

            if let Ok(resp) =
                async move { crate::fetch_json::<Vec<ReasoningEffort>>(API_CONFIG_REASONING).await }
                    .await
            {
                signals.set_effort_levels.set(resp);
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

        let max_f = signals.max_findings.get().parse::<usize>().unwrap_or(20);

        let req = NewRunRequest {
            model: signals.model.get(),
            dataset: signals.dataset.get(),
            roles: signals.roles.get(),
            pr_filter,
            reasoning_effort: signals.reasoning_effort.get(),
            judge_model: signals.judge_model.get(),
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

fn render_config_section(
    signals: NewRunSignals,
    on_dataset_change: impl Fn(leptos::ev::Event) + 'static,
) -> impl IntoView {
    view! {
        <section class="form-section">
            <h2 class="form-section__title">"Configuration"</h2>
            <div class="form-section__fields">
                <ModelSelector
                    config=signals.config
                    model=signals.model
                    set_model=signals.set_model
                    default_models=vec![DEFAULT_MODEL.to_string(), DEFAULT_MODEL_PRO.to_string()]
                />
                <DatasetSelector
                    config=signals.config
                    datasets=signals.datasets
                    dataset=signals.dataset
                    on_change=on_dataset_change
                    default_datasets=vec!["golden_comments".to_string()]
                />
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
                        let role_infos = c.agents.clone();
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
        <PrSelection<PrEntry>
            prs_loading=prs_loading
            available_prs=available_prs
            selected_prs=selected_prs
            set_selected_prs=set_selected_prs
            empty_message="Select a dataset to see available PRs."
            helper_text="Uncheck PRs you want to skip. All PRs selected = run entire dataset."
        />
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
                <ReasoningEffortSelector
                    reasoning_effort=signals.reasoning_effort
                    set_reasoning_effort=signals.set_reasoning_effort
                    effort_levels=signals.effort_levels
                    effort_loading=signals.effort_loading
                />
            </div>
        </section>
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

fn create_fetch_prs_handler(signals: NewRunSignals) -> impl Fn(String) + Send {
    move |ds_id: String| {
        if ds_id.is_empty() {
            signals.set_available_prs.set(Vec::new());
            signals.set_selected_prs.set(Vec::new());
            return;
        }
        signals.set_prs_loading.set(true);
        spawn_local(async move {
            // TODO: replace with actual API endpoint when available
            // match crate::fetch_json::<Vec<PrEntry>>(&route!(API_DATASETS_ID_PRS, ds_id)).await {
            //     Ok(prs) => {
            //         let all_keys: Vec<String> = prs.iter().map(|p| p.key.clone()).collect();
            //         signals.set_available_prs.set(prs);
            //         signals.set_selected_prs.set(all_keys);
            //     }
            //     Err(_) => {
            //         signals.set_available_prs.set(Vec::new());
            //         signals.set_selected_prs.set(Vec::new());
            //     }
            // }
            // signals.set_prs_loading.set(false);

            // Currently no PR API — just set loading false
            signals.set_prs_loading.set(false);
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

        fetch_prs(new_ds);
    }
}
