use std::str::FromStr;

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;
use riv_types::{capabilities::ReasoningEffort, review::Review, vcs::pr::PrMeta};
use riv_webui_shared::config::AgentInfo;

use super::form_support::{model_radio_options, pr_radio_options, reasoning_radio_options};
use crate::components::{
    error_state::ErrorState,
    form_page::FormPage,
    form_section::FormSection,
    loading_state::{LoadingState, LoadingVariant},
    radio_group::RadioGroup,
    role_selector::RoleSelector,
    text_field::TextField,
};

#[server]
async fn read_new_review_bootstrap()
-> Result<(Vec<String>, Vec<AgentInfo>, Vec<ReasoningEffort>), ServerFnError> {
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
    let reasoning_levels = (services.list_reasoning_efforts)((default_model,))
        .await
        .map_err(ServerFnError::new)?;

    Ok((models, roles, reasoning_levels))
}

#[server]
async fn read_repo_prs(owner: String, repo: String) -> Result<Vec<PrMeta>, ServerFnError> {
    if owner.trim().is_empty() || repo.trim().is_empty() {
        return Ok(Vec::new());
    }

    let services = use_context::<crate::AppServices>()
        .ok_or_else(|| ServerFnError::new("missing app services"))?;

    (services.list_repo_prs)((owner.trim().to_string(), repo.trim().to_string()))
        .await
        .map_err(ServerFnError::new)
}

#[server]
async fn read_new_review_reasoning_efforts(
    model: String,
) -> Result<Vec<ReasoningEffort>, ServerFnError> {
    let services = use_context::<crate::AppServices>()
        .ok_or_else(|| ServerFnError::new("missing app services"))?;

    (services.list_reasoning_efforts)((model,))
        .await
        .map_err(ServerFnError::new)
}

#[server]
async fn submit_new_review(
    url: String,
    _owner: String,
    _repo: String,
    _pr_number: u32,
    model: String,
    roles: Vec<String>,
    reasoning_effort: Option<ReasoningEffort>,
) -> Result<Review, ServerFnError> {
    let services = use_context::<crate::AppServices>()
        .ok_or_else(|| ServerFnError::new("missing app services"))?;

    (services.start_review)((url, model, roles, reasoning_effort))
        .await
        .map_err(ServerFnError::new)
}

#[component]
pub fn NewReviewPage() -> impl IntoView {
    let bootstrap = Resource::new(|| (), |_| async { read_new_review_bootstrap().await });

    view! {
        <Transition fallback=move || view! { <LoadingState variant=LoadingVariant::SkeletonCards /> }>
            {move || {
                bootstrap.get().map(|result| match result {
                    Ok((models, roles, reasoning_levels)) => view! {
                        <NewReviewForm
                            models=models
                            roles=roles
                            reasoning_levels=reasoning_levels
                        />
                    }
                    .into_any(),
                    Err(err) => view! {
                        <ErrorState
                            heading="Failed to load new review form"
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
fn NewReviewForm(
    models: Vec<String>,
    roles: Vec<AgentInfo>,
    reasoning_levels: Vec<ReasoningEffort>,
) -> impl IntoView {
    let navigate = use_navigate();
    let initial_model = models.first().cloned().unwrap_or_default();
    let default_reasoning = reasoning_levels
        .iter()
        .find(|level| matches!(level, ReasoningEffort::Medium))
        .copied()
        .or_else(|| reasoning_levels.first().copied())
        .map(|v| v.to_string())
        .unwrap_or("none".to_string())
        .to_string();

    let (owner, set_owner) = signal(String::new());
    let (repo, set_repo) = signal(String::new());
    let (model, set_model) = signal(initial_model);
    let initial_reasoning_options = reasoning_radio_options(&reasoning_levels);
    let reasoning_levels = Resource::new(
        move || model.get(),
        |selected_model| async move { read_new_review_reasoning_efforts(selected_model).await },
    );
    let (reasoning_effort, set_reasoning_effort) = signal(default_reasoning);
    let (reasoning_options, set_reasoning_options) = signal(initial_reasoning_options);
    create_effect(move |_| {
        if let Some(Ok(levels)) = reasoning_levels.get() {
            set_reasoning_options.set(reasoning_radio_options(&levels));
        }
    });
    let (selected_roles, set_selected_roles) = signal(Vec::<String>::new());
    let (available_prs, set_available_prs) = signal(Vec::<PrMeta>::new());
    let (selected_pr_url, set_selected_pr_url) = signal(String::new());
    let (prs_loading, set_prs_loading) = signal(false);
    let (prs_error, set_prs_error) = signal::<Option<String>>(None);
    let (submitting, set_submitting) = signal(false);
    let (submit_error, set_submit_error) = signal::<Option<String>>(None);
    let (config_loading, _set_config_loading) = signal(false);
    let (config_error, _set_config_error) = signal::<Option<String>>(None);

    let load_prs = move |_| {
        let owner_value = owner.get_untracked();
        let repo_value = repo.get_untracked();

        if owner_value.trim().is_empty() || repo_value.trim().is_empty() {
            set_prs_error.set(Some("Enter repo owner + repo name first.".to_string()));
            return;
        }

        set_prs_loading.set(true);
        set_prs_error.set(None);
        set_available_prs.set(Vec::new());
        set_selected_pr_url.set(String::new());

        spawn_local(async move {
            match read_repo_prs(owner_value, repo_value).await {
                Ok(prs) => {
                    if let Some(first) = prs.first() {
                        set_selected_pr_url.set(first.url.clone());
                    }
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
        owner.get().trim().is_empty()
            || repo.get().trim().is_empty()
            || model.get().trim().is_empty()
            || selected_roles.get().is_empty()
            || selected_pr_url.get().trim().is_empty()
    };

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_submitting.set(true);
        set_submit_error.set(None);

        let owner_value = owner.get_untracked();
        let repo_value = repo.get_untracked();
        let model_value = model.get_untracked();
        let roles_value = selected_roles.get_untracked();
        let selected_url = selected_pr_url.get_untracked();
        let reasoning_value = ReasoningEffort::from_str(&reasoning_effort.get_untracked()).ok();
        let selected_pr = available_prs
            .get_untracked()
            .into_iter()
            .find(|pr| pr.url == selected_url);
        let navigate = navigate.clone();

        let Some(pr) = selected_pr else {
            set_submit_error.set(Some("Select PR first.".to_string()));
            set_submitting.set(false);
            return;
        };

        spawn_local(async move {
            match submit_new_review(
                pr.url,
                owner_value,
                repo_value,
                pr.number,
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

    view! {
        <FormPage
            title="New Review"
            submit_label="Launch Review"
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
                        "Launch runs async. Detail page fills cost/duration from output files after completion."
                    </p>
                </div>
            </div>

            <FormSection title="Repository">
                <TextField
                    id="review-owner"
                    label="Owner"
                    helper="GitHub repo owner or org. Used for live open-PR lookup."
                    value=owner
                    set_value=set_owner
                    placeholder="example"
                />
                <TextField
                    id="review-repo"
                    label="Repository"
                    helper="Repository slug. Used for live open-PR lookup."
                    value=repo
                    set_value=set_repo
                    placeholder="blamefree"
                />
                <div class="form-field">
                    <label class="form-field__label">"Open PR lookup"</label>
                    <button
                        type="button"
                        class="btn btn--secondary"
                        on:click=load_prs
                        disabled=move || prs_loading.get()
                    >
                        {move || if prs_loading.get() { "Loading..." } else { "Load PRs" }}
                    </button>
                    <p class="form-field__helper">
                        "Loads current open pull requests from GitHub for entered repo."
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

            <FormSection title="Review Target">
                {move || {
                    let prs = available_prs.get();
                    let options = pr_radio_options(&prs);
                    let disabled = prs_loading.get() || prs.is_empty();
                    view! {
                        <RadioGroup
                            id="review-pr"
                            label_text="Pull Request"
                            options=options
                            value=selected_pr_url
                            set_value=set_selected_pr_url
                            helper="Pick one PR from live lookup results."
                            loading=prs_loading.get()
                            disabled=disabled
                        />
                    }
                    .into_any()
                }}
            </FormSection>

            <FormSection title="Configuration">
                <RadioGroup
                    id="review-model"
                    label_text="Model"
                    options=model_radio_options
                    value=model
                    set_value=set_model
                    helper="Models come from OpenRouter discovery with fallback defaults."
                />
                {move || {
                    let options = reasoning_options.get();
                    view! {
                        <RadioGroup
                            id="review-reasoning"
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
        </FormPage>
    }
}
