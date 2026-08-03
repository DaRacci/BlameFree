use leptos::prelude::*;
use leptos::task::spawn_local;
use riv_types::{capabilities::ReasoningEffort, review::Review, vcs::pr::PrMeta};
use riv_webui_shared::config::AgentInfo;

use super::form_support::{
    model_options, parse_reasoning_effort, pr_options, reasoning_options, reasoning_value,
};
#[cfg(feature = "ssr")]
use super::form_support::{
    placeholder_models, placeholder_reasoning_efforts, placeholder_repo_prs, placeholder_roles,
};
use crate::components::{
    error_state::ErrorState,
    form_page::FormPage,
    form_section::FormSection,
    loading_state::{LoadingState, LoadingVariant},
    role_selector::RoleSelector,
    select_field::SelectField,
    text_field::TextField,
};

#[server]
async fn read_new_review_bootstrap()
-> Result<(Vec<String>, Vec<AgentInfo>, Vec<ReasoningEffort>), ServerFnError> {
    let models = placeholder_models();
    let default_model = models.first().cloned().unwrap_or_default();
    Ok((
        models,
        placeholder_roles(),
        placeholder_reasoning_efforts(&default_model),
    ))
}

#[server]
async fn read_placeholder_repo_prs(
    owner: String,
    repo: String,
) -> Result<Vec<PrMeta>, ServerFnError> {
    if owner.trim().is_empty() || repo.trim().is_empty() {
        return Ok(Vec::new());
    }

    Ok(placeholder_repo_prs(owner.trim(), repo.trim()))
}

#[server]
async fn read_new_review_reasoning_efforts(
    model: String,
) -> Result<Vec<ReasoningEffort>, ServerFnError> {
    Ok(placeholder_reasoning_efforts(&model))
}

#[server]
async fn submit_new_review_placeholder(
    url: String,
    owner: String,
    repo: String,
    pr_number: u32,
    model: String,
    roles: Vec<String>,
    reasoning_effort: Option<ReasoningEffort>,
) -> Result<Review, ServerFnError> {
    let _ = (url, owner, repo, pr_number, model, roles, reasoning_effort);
    Err(ServerFnError::new(
        "Placeholder: review launch backend/service layer not implemented yet",
    ))
}

#[component]
pub fn NewReviewPage() -> impl IntoView {
    let bootstrap = Resource::new(|| (), |_| async { read_new_review_bootstrap().await });

    view! {
        <Suspense fallback=move || view! { <LoadingState variant=LoadingVariant::SkeletonCards /> }>
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
        </Suspense>
    }
}

#[component]
fn NewReviewForm(
    models: Vec<String>,
    roles: Vec<AgentInfo>,
    reasoning_levels: Vec<ReasoningEffort>,
) -> impl IntoView {
    let initial_model = models.first().cloned().unwrap_or_default();
    let default_reasoning = reasoning_levels
        .iter()
        .find(|level| matches!(level, ReasoningEffort::Medium))
        .copied()
        .or_else(|| reasoning_levels.first().copied())
        .map(reasoning_value)
        .unwrap_or("none")
        .to_string();

    let (owner, set_owner) = signal(String::new());
    let (repo, set_repo) = signal(String::new());
    let (model, set_model) = signal(initial_model);
    let reasoning_levels = Resource::new(
        move || model.get(),
        |selected_model| async move { read_new_review_reasoning_efforts(selected_model).await },
    );
    let (reasoning_effort, set_reasoning_effort) = signal(default_reasoning);
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
            match read_placeholder_repo_prs(owner_value, repo_value).await {
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
        let reasoning_value = parse_reasoning_effort(&reasoning_effort.get_untracked());
        let selected_pr = available_prs
            .get_untracked()
            .into_iter()
            .find(|pr| pr.url == selected_url);

        let Some(pr) = selected_pr else {
            set_submit_error.set(Some("Select placeholder PR first.".to_string()));
            set_submitting.set(false);
            return;
        };

        spawn_local(async move {
            match submit_new_review_placeholder(
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
                        "Placeholder bootstrap active. Real backend discovery + launch path still pending."
                    </p>
                </div>
            </div>

            <FormSection title="Repository">
                <TextField
                    id="review-owner"
                    label="Owner"
                    helper="Git provider repo owner/org. Placeholder lookup uses this to generate mock PRs."
                    value=owner
                    set_value=set_owner
                    placeholder="example"
                />
                <TextField
                    id="review-repo"
                    label="Repository"
                    helper="Repository slug. Placeholder lookup uses this to generate mock PRs."
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
                        "Placeholder server fn returns sample PRs for entered repo."
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
                    let options = pr_options(&prs);
                    let disabled = prs_loading.get() || prs.is_empty();
                    view! {
                        <SelectField
                            id="review-pr"
                            label_text="Pull Request"
                            options=options
                            value=selected_pr_url
                            set_value=set_selected_pr_url
                            helper="Pick one PR from placeholder lookup results."
                            loading=prs_loading.get()
                            disabled=disabled
                        />
                    }
                    .into_any()
                }}
            </FormSection>

            <FormSection title="Configuration">
                <SelectField
                    id="review-model"
                    label_text="Model"
                    options=model_select_options
                    value=model
                    set_value=set_model
                    helper="Default comes from riv_shared::DEFAULT_MODEL placeholder bootstrap."
                />
                {move || {
                    let options = match reasoning_levels.get() {
                        Some(Ok(levels)) => reasoning_options(&levels),
                        _ => Vec::new(),
                    };
                    view! {
                        <SelectField
                            id="review-reasoning"
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
        </FormPage>
    }
}
