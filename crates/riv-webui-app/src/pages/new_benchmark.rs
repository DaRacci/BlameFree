use std::str::FromStr;

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;
use riv_types::{
    benchmark::golden::GoldenCommentEntry, capabilities::ReasoningEffort, review::Review,
};
use riv_webui_shared::config::{AgentInfo, DatasetInfo};

use super::form_support::{dataset_radio_options, model_radio_options, reasoning_radio_options};
use crate::components::{
    error_state::ErrorState,
    form_page::FormPage,
    form_section::FormSection,
    loading_state::{LoadingState, LoadingVariant},
    pr_selection::PrSelection,
    radio_group::RadioGroup,
    role_selector::RoleSelector,
};

#[server]
async fn read_new_benchmark_bootstrap() -> Result<
    (
        Vec<String>,
        Vec<AgentInfo>,
        Vec<DatasetInfo>,
        Vec<ReasoningEffort>,
    ),
    ServerFnError,
> {
    let services = use_context::<crate::AppServices>()
        .ok_or_else(|| ServerFnError::new("missing app services"))?;

    let models = (services.list_models)(())
        .await
        .map_err(ServerFnError::new)?
        .into_iter()
        .map(|model| model.0)
        .collect::<Vec<_>>();
    let default_model = models.first().cloned().unwrap_or_default();
    let roles = (services.list_agents)(())
        .await
        .map_err(ServerFnError::new)?;
    let datasets = (services.list_datasets)(())
        .await
        .map_err(ServerFnError::new)?;
    let reasoning_levels = (services.list_reasoning_efforts)((default_model,))
        .await
        .map_err(ServerFnError::new)?;

    Ok((models, roles, datasets, reasoning_levels))
}

#[server]
async fn read_dataset_prs(dataset_id: String) -> Result<Vec<GoldenCommentEntry>, ServerFnError> {
    if dataset_id.trim().is_empty() {
        return Ok(Vec::new());
    }

    let services = use_context::<crate::AppServices>()
        .ok_or_else(|| ServerFnError::new("missing app services"))?;

    (services.list_dataset_prs)((dataset_id.trim().to_string(),))
        .await
        .map_err(ServerFnError::new)
}

#[server]
async fn read_new_benchmark_reasoning_efforts(
    model: String,
) -> Result<Vec<ReasoningEffort>, ServerFnError> {
    let services = use_context::<crate::AppServices>()
        .ok_or_else(|| ServerFnError::new("missing app services"))?;

    (services.list_reasoning_efforts)((model,))
        .await
        .map_err(ServerFnError::new)
}

#[server]
async fn submit_new_benchmark(
    dataset_id: String,
    pr_urls: Vec<String>,
    model: String,
    roles: Vec<String>,
    reasoning_effort: Option<ReasoningEffort>,
) -> Result<Review, ServerFnError> {
    let services = use_context::<crate::AppServices>()
        .ok_or_else(|| ServerFnError::new("missing app services"))?;

    (services.start_benchmark)((dataset_id, pr_urls, model, roles, reasoning_effort))
        .await
        .map_err(ServerFnError::new)
}

#[component]
pub fn NewBenchmarkPage() -> impl IntoView {
    let bootstrap = Resource::new(|| (), |_| async { read_new_benchmark_bootstrap().await });

    view! {
        <Transition fallback=move || view! { <LoadingState variant=LoadingVariant::SkeletonCards /> }>
            {move || {
                bootstrap.get().map(|result| match result {
                    Ok((models, roles, datasets, reasoning_levels)) => view! {
                        <NewBenchmarkForm
                            models=models
                            roles=roles
                            datasets=datasets
                            reasoning_levels=reasoning_levels
                        />
                    }
                    .into_any(),
                    Err(err) => view! {
                        <ErrorState
                            heading="Failed to load new benchmark form"
                            message=err.to_string()
                        />
                    }
                    .into_any(),
                })
            }}
        </Transition>
    }
}

#[component]
fn NewBenchmarkForm(
    models: Vec<String>,
    roles: Vec<AgentInfo>,
    datasets: Vec<DatasetInfo>,
    reasoning_levels: Vec<ReasoningEffort>,
) -> impl IntoView {
    let navigate = use_navigate();
    let initial_model = models.first().cloned().unwrap_or_default();
    let initial_dataset = datasets
        .first()
        .map(|dataset| dataset.id.clone())
        .unwrap_or_default();
    let default_reasoning = reasoning_levels
        .iter()
        .find(|level| matches!(level, ReasoningEffort::Medium))
        .copied()
        .or_else(|| reasoning_levels.first().copied())
        .map(|v| v.to_string())
        .unwrap_or("none".to_string())
        .to_string();

    let (model, set_model) = signal(initial_model);
    let initial_reasoning_options = reasoning_radio_options(&reasoning_levels);
    let reasoning_levels = Resource::new(
        move || model.get(),
        |selected_model| async move { read_new_benchmark_reasoning_efforts(selected_model).await },
    );
    let (dataset, set_dataset) = signal(initial_dataset);
    let (reasoning_effort, set_reasoning_effort) = signal(default_reasoning);
    let (reasoning_options, set_reasoning_options) = signal(initial_reasoning_options);
    create_effect(move |_| {
        if let Some(Ok(levels)) = reasoning_levels.get() {
            set_reasoning_options.set(reasoning_radio_options(&levels));
        }
    });
    let (selected_roles, set_selected_roles) = signal(Vec::<String>::new());
    let (available_prs, set_available_prs) = signal(Vec::<GoldenCommentEntry>::new());
    let (selected_prs, set_selected_prs) = signal(Vec::<String>::new());
    let (prs_loading, set_prs_loading) = signal(false);
    let (prs_error, set_prs_error) = signal::<Option<String>>(None);
    let (submitting, set_submitting) = signal(false);
    let (submit_error, set_submit_error) = signal::<Option<String>>(None);
    let (config_loading, _set_config_loading) = signal(false);
    let (config_error, _set_config_error) = signal::<Option<String>>(None);

    let load_dataset_prs = move |_| {
        let dataset_id = dataset.get_untracked();

        if dataset_id.trim().is_empty() {
            set_prs_error.set(Some("Select dataset first.".to_string()));
            return;
        }

        set_prs_loading.set(true);
        set_prs_error.set(None);
        set_available_prs.set(Vec::new());
        set_selected_prs.set(Vec::new());

        spawn_local(async move {
            match read_dataset_prs(dataset_id).await {
                Ok(prs) => {
                    let selected = prs.iter().map(|pr| pr.url.clone()).collect::<Vec<_>>();
                    set_selected_prs.set(selected);
                    set_available_prs.set(prs);
                }
                Err(err) => {
                    set_prs_error.set(Some(err.to_string()));
                }
            }
            set_prs_loading.set(false);
        });
    };

    let submit_disabled = move || {
        dataset.get().trim().is_empty()
            || model.get().trim().is_empty()
            || selected_roles.get().is_empty()
            || selected_prs.get().is_empty()
    };

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_submitting.set(true);
        set_submit_error.set(None);

        let dataset_value = dataset.get_untracked();
        let model_value = model.get_untracked();
        let roles_value = selected_roles.get_untracked();
        let selected_pr_urls = selected_prs.get_untracked();
        let reasoning_value = ReasoningEffort::from_str(&reasoning_effort.get_untracked()).ok();
        let navigate = navigate.clone();

        spawn_local(async move {
            match submit_new_benchmark(
                dataset_value,
                selected_pr_urls,
                model_value,
                roles_value,
                reasoning_value,
            )
            .await
            {
                Ok(review) => {
                    let path = format!("/reviews/{}", review.id);
                    navigate(&path, Default::default());
                }
                Err(err) => {
                    set_submit_error.set(Some(err.to_string()));
                }
            }
            set_submitting.set(false);
        });
    };

    let model_radio_options = model_radio_options(&models);
    let dataset_radio_options = dataset_radio_options(&datasets);

    view! {
        <FormPage
            title="New Benchmark"
            submit_label="Launch Benchmark"
            config_loading=config_loading
            config_error=config_error
            submitting=submitting
            submit_error=submit_error
            on_submit=on_submit
            submit_disabled=submit_disabled
        >
            <div class="card form-error-banner">
                <div class="card__body">
                    <p class="text-secondary">
                        "Launch runs async. Current benchmark flow stores raw findings; judge scoring follow-up still pending."
                    </p>
                </div>
            </div>

            <FormSection title="Configuration">
                <RadioGroup
                    id="benchmark-model"
                    label_text="Model"
                    options=model_radio_options
                    value=model
                    set_value=set_model
                    helper="Models come from OpenRouter discovery with fallback defaults."
                />
                <RadioGroup
                    id="benchmark-dataset"
                    label_text="Dataset"
                    options=dataset_radio_options
                    value=dataset
                    set_value=set_dataset
                    helper="Datasets come from server-side scan of configured dataset dir."
                />
                {move || {
                    let options = reasoning_options.get();
                    view! {
                        <RadioGroup
                            id="benchmark-reasoning"
                            label_text="Reasoning Effort"
                            options=options
                            value=reasoning_effort
                            set_value=set_reasoning_effort
                            helper="Shown only for reasoning-capable models."
                            include_none=true
                            loading=reasoning_levels.get().is_none()
                        />
                    }
                    .into_any()
                }}
            </FormSection>

            <FormSection title="Agents">
                <RoleSelector
                    available_roles=roles
                    selected_roles=selected_roles
                    set_selected_roles=set_selected_roles
                />
            </FormSection>

            <FormSection title="Dataset PRs">
                <div class="form-field">
                    <label class="form-field__label">"Dataset PR lookup"</label>
                    <button
                        type="button"
                        class="btn btn--secondary"
                        on:click=load_dataset_prs
                        disabled=move || prs_loading.get()
                    >
                        {move || if prs_loading.get() { "Loading..." } else { "Load Dataset PRs" }}
                    </button>
                    <p class="form-field__helper">
                        "Loads PR entries from selected dataset directory on server."
                    </p>
                </div>
                {move || {
                    prs_error.get().map(|error| {
                        view! {
                            <p class="text-error text-sm">{error}</p>
                        }
                    })
                }}
            </FormSection>

            <PrSelection<GoldenCommentEntry>
                prs_loading=prs_loading
                available_prs=available_prs
                selected_prs=selected_prs
                set_selected_prs=set_selected_prs
                empty_message="Load dataset PRs to populate selection list."
                helper_text="All dataset PRs auto-select after load; uncheck rows to trim scope."
            />
        </FormPage>
    }
}
