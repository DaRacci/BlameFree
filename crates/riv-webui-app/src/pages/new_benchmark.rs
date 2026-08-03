use leptos::prelude::*;
use leptos::task::spawn_local;
use riv_types::{
    benchmark::golden::GoldenCommentEntry, capabilities::ReasoningEffort, review::Review,
};
use riv_webui_shared::config::{AgentInfo, DatasetInfo};

use super::form_support::{
    dataset_options, model_options, parse_reasoning_effort, reasoning_options, reasoning_value,
};
#[cfg(feature = "ssr")]
use super::form_support::{
    placeholder_dataset_prs, placeholder_datasets, placeholder_models,
    placeholder_reasoning_efforts, placeholder_roles,
};
use crate::components::{
    error_state::ErrorState,
    form_page::FormPage,
    form_section::FormSection,
    loading_state::{LoadingState, LoadingVariant},
    pr_selection::PrSelection,
    role_selector::RoleSelector,
    select_field::SelectField,
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
    let models = placeholder_models();
    let default_model = models.first().cloned().unwrap_or_default();
    Ok((
        models,
        placeholder_roles(),
        placeholder_datasets(),
        placeholder_reasoning_efforts(&default_model),
    ))
}

#[server]
async fn read_placeholder_dataset_prs(
    dataset_id: String,
) -> Result<Vec<GoldenCommentEntry>, ServerFnError> {
    if dataset_id.trim().is_empty() {
        return Ok(Vec::new());
    }

    Ok(placeholder_dataset_prs(dataset_id.trim()))
}

#[server]
async fn read_new_benchmark_reasoning_efforts(
    model: String,
) -> Result<Vec<ReasoningEffort>, ServerFnError> {
    Ok(placeholder_reasoning_efforts(&model))
}

#[server]
async fn submit_new_benchmark_placeholder(
    dataset_id: String,
    pr_urls: Vec<String>,
    model: String,
    roles: Vec<String>,
    reasoning_effort: Option<ReasoningEffort>,
) -> Result<Review, ServerFnError> {
    let _ = (dataset_id, pr_urls, model, roles, reasoning_effort);
    Err(ServerFnError::new(
        "Placeholder: benchmark launch backend/service layer not implemented yet",
    ))
}

#[component]
pub fn NewBenchmarkPage() -> impl IntoView {
    let bootstrap = Resource::new(|| (), |_| async { read_new_benchmark_bootstrap().await });

    view! {
        <Suspense fallback=move || view! { <LoadingState variant=LoadingVariant::SkeletonCards /> }>
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
        </Suspense>
    }
}

#[component]
fn NewBenchmarkForm(
    models: Vec<String>,
    roles: Vec<AgentInfo>,
    datasets: Vec<DatasetInfo>,
    reasoning_levels: Vec<ReasoningEffort>,
) -> impl IntoView {
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
        .map(reasoning_value)
        .unwrap_or("none")
        .to_string();

    let (model, set_model) = signal(initial_model);
    let reasoning_levels = Resource::new(
        move || model.get(),
        |selected_model| async move { read_new_benchmark_reasoning_efforts(selected_model).await },
    );
    let (dataset, set_dataset) = signal(initial_dataset);
    let (reasoning_effort, set_reasoning_effort) = signal(default_reasoning);
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
            match read_placeholder_dataset_prs(dataset_id).await {
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
        let reasoning_value = parse_reasoning_effort(&reasoning_effort.get_untracked());

        spawn_local(async move {
            match submit_new_benchmark_placeholder(
                dataset_value,
                selected_pr_urls,
                model_value,
                roles_value,
                reasoning_value,
            )
            .await
            {
                Ok(_) => {
                    set_submit_error.set(Some(
                        "Placeholder launch returned unexpected success. Detail route still pending."
                            .to_string(),
                    ));
                }
                Err(err) => {
                    set_submit_error.set(Some(err.to_string()));
                }
            }
            set_submitting.set(false);
        });
    };

    let model_select_options = model_options(&models);
    let dataset_select_options = dataset_options(&datasets);

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
                        "Placeholder bootstrap active. Real dataset/model discovery + launch path still pending."
                    </p>
                </div>
            </div>

            <FormSection title="Configuration">
                <SelectField
                    id="benchmark-model"
                    label_text="Model"
                    options=model_select_options
                    value=model
                    set_value=set_model
                    helper="Default comes from riv_shared::DEFAULT_MODEL placeholder bootstrap."
                />
                <SelectField
                    id="benchmark-dataset"
                    label_text="Dataset"
                    options=dataset_select_options
                    value=dataset
                    set_value=set_dataset
                    helper="Placeholder datasets stand in for future backend discovery."
                />
                {move || {
                    let options = match reasoning_levels.get() {
                        Some(Ok(levels)) => reasoning_options(&levels),
                        _ => Vec::new(),
                    };
                    view! {
                        <SelectField
                            id="benchmark-reasoning"
                            label_text="Reasoning Effort"
                            options=options
                            value=reasoning_effort
                            set_value=set_reasoning_effort
                            helper="Placeholder capability discovery currently returns all effort levels."
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
                        "Placeholder server fn returns sample dataset PR entries for selected dataset."
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
                empty_message="Load placeholder dataset PRs to populate selection list."
                helper_text="Placeholder flow selects all dataset PRs by default; uncheck rows to trim run scope."
            />
        </FormPage>
    }
}
