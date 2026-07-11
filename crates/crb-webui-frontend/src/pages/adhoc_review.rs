use crate::AppConfig;
use crate::components::role_selector::RoleSelector;
use crb_webui_shared::adhoc::{AdhocReviewResponse, GithubPrListItem};
use crb_webui_shared::config::RoleInfo;
use leptos::{
    IntoView, ReadSignal, SignalGet, SignalGetUntracked, SignalSet, WriteSignal, component,
    create_signal, event_target_value, spawn_local, view,
};
use leptos_router::*;

/// Fetch open PRs for a given owner/repo via the backend proxy.
async fn fetch_repo_prs(owner: &str, repo: &str) -> Result<Vec<GithubPrListItem>, String> {
    let url = format!("/api/adhoc/prs/{}/{}", owner, repo);
    let resp = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;
    if !resp.ok() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Server error ({}): {}", status, text));
    }
    resp.json::<Vec<GithubPrListItem>>()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))
}

// ─── State ──────────────────────────────────────────────────────────────────

struct AdhocSignals {
    owner: ReadSignal<String>,
    set_owner: WriteSignal<String>,
    repo: ReadSignal<String>,
    set_repo: WriteSignal<String>,
    model: ReadSignal<String>,
    set_model: WriteSignal<String>,
    selected_roles: ReadSignal<Vec<String>>,
    set_selected_roles: WriteSignal<Vec<String>>,
    available_roles: ReadSignal<Vec<RoleInfo>>,
    set_available_roles: WriteSignal<Vec<RoleInfo>>,
    loading: ReadSignal<bool>,
    set_loading: WriteSignal<bool>,
    error: ReadSignal<Option<String>>,
    set_error: WriteSignal<Option<String>>,
    prs_loading: ReadSignal<bool>,
    set_prs_loading: WriteSignal<bool>,
    open_prs: ReadSignal<Vec<GithubPrListItem>>,
    set_open_prs: WriteSignal<Vec<GithubPrListItem>>,
    pr_mode: ReadSignal<String>,
    set_pr_mode: WriteSignal<String>,
    selected_pr_number: ReadSignal<Option<u32>>,
    set_selected_pr_number: WriteSignal<Option<u32>>,
    manual_pr_number: ReadSignal<String>,
    set_manual_pr_number: WriteSignal<String>,
    prs_error: ReadSignal<Option<String>>,
    set_prs_error: WriteSignal<Option<String>>,
}

fn create_adhoc_signals() -> AdhocSignals {
    let (owner, set_owner) = create_signal(String::new());
    let (repo, set_repo) = create_signal(String::new());
    let (model, set_model) = create_signal(String::new());
    let (selected_roles, set_selected_roles) = create_signal::<Vec<String>>(Vec::new());
    let (available_roles, set_available_roles) = create_signal::<Vec<RoleInfo>>(Vec::new());
    let (loading, set_loading) = create_signal(false);
    let (error, set_error) = create_signal::<Option<String>>(None);
    let (prs_loading, set_prs_loading) = create_signal(false);
    let (open_prs, set_open_prs) = create_signal::<Vec<GithubPrListItem>>(Vec::new());
    let (pr_mode, set_pr_mode) = create_signal::<String>("open".to_string());
    let (selected_pr_number, set_selected_pr_number) = create_signal::<Option<u32>>(None);
    let (manual_pr_number, set_manual_pr_number) = create_signal(String::new());
    let (prs_error, set_prs_error) = create_signal::<Option<String>>(None);

    AdhocSignals {
        owner,
        set_owner,
        repo,
        set_repo,
        model,
        set_model,
        selected_roles,
        set_selected_roles,
        available_roles,
        set_available_roles,
        loading,
        set_loading,
        error,
        set_error,
        prs_loading,
        set_prs_loading,
        open_prs,
        set_open_prs,
        pr_mode,
        set_pr_mode,
        selected_pr_number,
        set_selected_pr_number,
        manual_pr_number,
        set_manual_pr_number,
        prs_error,
        set_prs_error,
    }
}

// ─── Data Fetching ──────────────────────────────────────────────────────────

fn fetch_initial_config(
    set_model: WriteSignal<String>,
    set_available_roles: WriteSignal<Vec<RoleInfo>>,
) {
    spawn_local(async move {
        let url = "/api/config";
        if let Ok(resp) = gloo_net::http::Request::get(&url).send().await {
            if let Ok(config) = resp.json::<AppConfig>().await {
                if let Some(first) = config.models.first() {
                    set_model.set(first.clone());
                }
                set_available_roles.set(config.roles);
            }
        }
    });
}

fn build_load_prs_handler(
    owner: ReadSignal<String>,
    repo: ReadSignal<String>,
    set_prs_loading: WriteSignal<bool>,
    set_prs_error: WriteSignal<Option<String>>,
    set_open_prs: WriteSignal<Vec<GithubPrListItem>>,
    set_selected_pr_number: WriteSignal<Option<u32>>,
) -> impl Fn(leptos::ev::MouseEvent) {
    move |_| {
        let owner_val = owner.get_untracked();
        let repo_val = repo.get_untracked();

        if owner_val.trim().is_empty() || repo_val.trim().is_empty() {
            set_prs_error.set(Some("Please enter both owner and repo.".to_string()));
            return;
        }

        set_prs_loading.set(true);
        set_prs_error.set(None);
        set_open_prs.set(Vec::new());
        set_selected_pr_number.set(None);

        spawn_local(async move {
            match fetch_repo_prs(owner_val.trim(), repo_val.trim()).await {
                Ok(prs) => {
                    set_open_prs.set(prs);
                    set_prs_loading.set(false);
                }
                Err(e) => {
                    set_prs_error.set(Some(e));
                    set_prs_loading.set(false);
                }
            }
        });
    }
}

fn build_submit_handler(
    owner: ReadSignal<String>,
    repo: ReadSignal<String>,
    model: ReadSignal<String>,
    selected_roles: ReadSignal<Vec<String>>,
    pr_mode: ReadSignal<String>,
    selected_pr_number: ReadSignal<Option<u32>>,
    manual_pr_number: ReadSignal<String>,
    set_error: WriteSignal<Option<String>>,
    set_loading: WriteSignal<bool>,
    navigator: impl Fn(&str, leptos_router::NavigateOptions) + Clone + 'static,
) -> impl Fn(leptos::ev::MouseEvent) {
    move |_| {
        let navigator = navigator.clone();
        let owner_val = owner.get();
        let repo_val = repo.get();
        let model_val = model.get();
        let roles_val = selected_roles.get();
        let mode = pr_mode.get();

        if owner_val.trim().is_empty() || repo_val.trim().is_empty() {
            set_error.set(Some("Please enter both owner and repo.".to_string()));
            return;
        }

        let pr_number = if mode == "open" {
            match selected_pr_number.get() {
                Some(n) => n,
                None => {
                    set_error.set(Some(
                        "Please select a PR from the list or switch to manual entry.".to_string(),
                    ));
                    return;
                }
            }
        } else {
            let manual = manual_pr_number.get();
            if manual.trim().is_empty() {
                set_error.set(Some("Please enter a PR number.".to_string()));
                return;
            }
            match manual.trim().parse::<u32>() {
                Ok(n) => n,
                Err(_) => {
                    set_error.set(Some(
                        "Invalid PR number. Please enter a numeric value.".to_string(),
                    ));
                    return;
                }
            }
        };

        let url_val = format!(
            "https://github.com/{}/{}/pull/{}",
            owner_val.trim(),
            repo_val.trim(),
            pr_number
        );

        set_loading.set(true);
        set_error.set(None);

        let body = serde_json::json!({
            "url": url_val,
            "model": model_val,
            "roles": roles_val,
        });

        spawn_local(async move {
            let req = gloo_net::http::Request::post("/api/adhoc/review")
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
                                navigator(
                                    &format!("/adhoc/runs/{}", data.run_id),
                                    Default::default(),
                                );
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
    }
}

// ─── View Sections ──────────────────────────────────────────────────────────

fn render_repo_section(
    owner: ReadSignal<String>,
    set_owner: WriteSignal<String>,
    repo: ReadSignal<String>,
    set_repo: WriteSignal<String>,
    prs_loading: ReadSignal<bool>,
    load_prs: impl Fn(leptos::ev::MouseEvent) + 'static,
) -> impl IntoView {
    view! {
        <section class="form-section">
            <h2 class="form-section__title">"Repository"</h2>
            <div class="form-section__fields">
                <div style="display: flex; gap: var(--spacing-md, 12px); align-items: flex-start;">
                    <div class="form-field" style="flex: 1;">
                        <label class="form-field__label" for="owner">"Owner"</label>
                        <input
                            id="owner"
                            class="input"
                            type="text"
                            placeholder="facebook"
                            prop:value=owner
                            on:input=move |ev| set_owner.set(event_target_value(&ev))
                        />
                    </div>
                    <div class="form-field" style="flex: 1;">
                        <label class="form-field__label" for="repo">"Repo"</label>
                        <input
                            id="repo"
                            class="input"
                            type="text"
                            placeholder="react"
                            prop:value=repo
                            on:input=move |ev| set_repo.set(event_target_value(&ev))
                        />
                    </div>
                    <div class="form-field" style="padding-top: 24px;">
                        <button
                            class="btn btn--primary"
                            on:click=load_prs
                            disabled=move || prs_loading.get()
                        >
                            {move || if prs_loading.get() { "Loading..." } else { "Load PRs" }}
                        </button>
                    </div>
                </div>
            </div>
        </section>
    }
}

fn render_pr_selection_section(
    pr_mode: ReadSignal<String>,
    set_pr_mode: WriteSignal<String>,
    open_prs: ReadSignal<Vec<GithubPrListItem>>,
    prs_loading: ReadSignal<bool>,
    prs_error: ReadSignal<Option<String>>,
    selected_pr_number: ReadSignal<Option<u32>>,
    set_selected_pr_number: WriteSignal<Option<u32>>,
    manual_pr_number: ReadSignal<String>,
    set_manual_pr_number: WriteSignal<String>,
) -> impl IntoView {
    view! {
        <section class="form-section">
            <h2 class="form-section__title">"PR Selection"</h2>
            <div class="form-section__fields">
                <div class="form-field">
                    <label class="form-field__label">"PR Source"</label>
                    <div style="display: flex; gap: var(--spacing-lg, 16px); margin-top: var(--spacing-xs, 4px);">
                        <label class="checkbox-label" style="cursor: pointer;">
                            <input
                                type="radio"
                                name="pr-mode"
                                checked=move || pr_mode.get() == "open"
                                on:click=move |_| set_pr_mode.set("open".to_string())
                            />
                            <span>"Open PRs"</span>
                        </label>
                        <label class="checkbox-label" style="cursor: pointer;">
                            <input
                                type="radio"
                                name="pr-mode"
                                checked=move || pr_mode.get() == "manual"
                                on:click=move |_| set_pr_mode.set("manual".to_string())
                            />
                            <span>"Manual Entry"</span>
                        </label>
                    </div>
                </div>

                {move || {
                    if pr_mode.get() == "open" {
                        let prs = open_prs.get();
                        let loading_prs = prs_loading.get();
                        let prs_err = prs_error.get();

                        if loading_prs {
                            view! {
                                <div class="form-field">
                                    <p style="color: var(--text-secondary, #8b949e);">"Loading PRs..."</p>
                                </div>
                            }.into_view()
                        } else if let Some(err) = prs_err {
                            view! {
                                <div class="form-field">
                                    <p style="color: var(--accent-red, #f85149); font-size: var(--text-sm, 14px);">{err}</p>
                                </div>
                            }.into_view()
                        } else if prs.is_empty() {
                            view! {
                                <div class="form-field">
                                    <p style="color: var(--text-secondary, #8b949e); font-size: var(--text-sm, 14px);">
                                        "Enter owner/repo and click \"Load PRs\" to see open pull requests."
                                    </p>
                                </div>
                            }.into_view()
                        } else {
                            let sel_num = selected_pr_number.get();
                            view! {
                                <div class="form-field">
                                    <label class="form-field__label" for="pr-select">"Select Open PR"</label>
                                    <select
                                        id="pr-select"
                                        class="input select"
                                        prop:value=move || sel_num.map(|n| n.to_string()).unwrap_or_default()
                                        on:change=move |ev| {
                                            let val = event_target_value(&ev);
                                            if val.is_empty() {
                                                set_selected_pr_number.set(None);
                                            } else if let Ok(n) = val.parse::<u32>() {
                                                set_selected_pr_number.set(Some(n));
                                            }
                                        }
                                    >
                                        <option value="">"-- Select a PR --"</option>
                                        {prs.into_iter().map(|pr| {
                                            let label = format!("#{} - {}", pr.number, pr.title);
                                            let val = pr.number.to_string();
                                            let is_selected = sel_num == Some(pr.number);
                                            view! {
                                                <option value=&val selected=is_selected>{&label}</option>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </select>
                                    <p class="form-field__helper">"Select an open PR from the dropdown above."</p>
                                </div>
                            }.into_view()
                        }
                    } else {
                        view! {
                            <div class="form-field">
                                <label class="form-field__label" for="manual-pr">"PR Number"</label>
                                <input
                                    id="manual-pr"
                                    class="input"
                                    type="text"
                                    placeholder="123"
                                    prop:value=manual_pr_number
                                    on:input=move |ev| set_manual_pr_number.set(event_target_value(&ev))
                                />
                                <p class="form-field__helper">"Enter any PR number (open, closed, or merged)."</p>
                            </div>
                        }.into_view()
                    }
                }}
            </div>
        </section>
    }
}

fn render_config_section(
    model: ReadSignal<String>,
    set_model: WriteSignal<String>,
    available_roles: ReadSignal<Vec<RoleInfo>>,
    selected_roles: ReadSignal<Vec<String>>,
    set_selected_roles: WriteSignal<Vec<String>>,
) -> impl IntoView {
    view! {
        <section class="form-section">
            <h2 class="form-section__title">"Configuration"</h2>
            <div class="form-section__fields">
                <div class="form-field">
                    <label class="form-field__label" for="model">"Model"</label>
                    <input
                        id="model"
                        class="input"
                        type="text"
                        prop:value=model
                        on:input=move |ev| set_model.set(event_target_value(&ev))
                    />
                    <p class="form-field__helper">"The model used for review agents."</p>
                </div>

                <div class="form-field">
                    <label class="form-field__label">"Roles / Agents"</label>
                    <div class="checkbox-group">
                        {move || {
                            let roles = available_roles.get();
                            view! { <RoleSelector available_roles=roles selected_roles=selected_roles set_selected_roles=set_selected_roles /> }
                        }}
                    </div>
                    <p class="form-field__helper">"Select at least one role for this review."</p>
                </div>
            </div>
        </section>
    }
}

fn render_error_view(error: ReadSignal<Option<String>>) -> impl IntoView {
    move || error.get().map(|e| {
        view! { <div class="error-message" style="color: var(--accent-red, #f85149); margin-bottom: var(--spacing-lg, 16px);">{e}</div> }
    })
}

fn render_submit_button(
    loading: ReadSignal<bool>,
    submit: impl Fn(leptos::ev::MouseEvent) + 'static,
) -> impl IntoView {
    view! {
        <div class="form-actions">
            <button
                class="btn btn--primary btn--lg"
                on:click=submit
                disabled=move || loading.get()
            >
                {move || if loading.get() { "Starting..." } else { "Start Review" }}
            </button>
        </div>
    }
}

// ─── Main Component ─────────────────────────────────────────────────────────

#[component]
pub fn AdhocReviewPage() -> impl IntoView {
    let s = create_adhoc_signals();
    let navigator = use_navigate();

    fetch_initial_config(s.set_model, s.set_available_roles);

    let load_prs = build_load_prs_handler(
        s.owner,
        s.repo,
        s.set_prs_loading,
        s.set_prs_error,
        s.set_open_prs,
        s.set_selected_pr_number,
    );

    let submit = build_submit_handler(
        s.owner,
        s.repo,
        s.model,
        s.selected_roles,
        s.pr_mode,
        s.selected_pr_number,
        s.manual_pr_number,
        s.set_error,
        s.set_loading,
        navigator,
    );

    view! {
        <div class="adhoc-review-page">
            <div class="page-header">
                <h1 class="page-header__title">"Ad-hoc PR Review"</h1>
                <div class="page-header__actions">
                    <a href="/adhoc" class="btn btn--ghost">"Back to Runs"</a>
                </div>
            </div>

            <p style="color: var(--text-secondary, #8b949e); margin-bottom: var(--spacing-xl, 24px);">
                "Submit a GitHub PR for a one-off review by the agent team."
            </p>

            {render_repo_section(s.owner, s.set_owner, s.repo, s.set_repo, s.prs_loading, load_prs)}

            {render_pr_selection_section(
                s.pr_mode, s.set_pr_mode,
                s.open_prs, s.prs_loading, s.prs_error,
                s.selected_pr_number, s.set_selected_pr_number,
                s.manual_pr_number, s.set_manual_pr_number,
            )}

            {render_config_section(s.model, s.set_model, s.available_roles, s.selected_roles, s.set_selected_roles)}

            {render_error_view(s.error)}

            {render_submit_button(s.loading, submit)}
        </div>
    }
}
