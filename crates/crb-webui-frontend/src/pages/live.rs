use crate::AppConfig;
use crate::components::agent_pane::AgentPane;
use crate::components::metrics_card::MetricsCard;
use crate::components::progress_bar::ProgressBar;
use crate::sse;
use crb_types::RunEvent;
use crb_types::agent::AgentChunk;
use crb_webui_shared::config::AgentInfo;
use crb_webui_shared::review::ReviewStatus;
use crb_webui_shared::route;
use crb_webui_shared::routes::API_CONFIG;
use gloo_net::http::Request;
use leptos::either::{Either, EitherOf3};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_params_map;
use log::{error, warn};
use lucide_leptos::{ArrowLeft, Check, TriangleAlert};
use std::collections::HashMap;

/// State for a single agent within a single PR.
#[derive(Debug, Clone)]
struct PerAgentState {
    status: ReviewStatus,

    /// Acumulated response chunks from the provider.
    response: String,

    findings: Option<usize>,
}

impl PerAgentState {
    fn new() -> Self {
        Self {
            status: ReviewStatus::Pending,
            response: String::new(),
            findings: None,
        }
    }
}

/// State for a single PR, containing the state of all agents working on it.
#[derive(Debug, Clone)]
struct PrState {
    agents: HashMap<String, PerAgentState>,
}

impl PrState {
    fn new(roles: &[String]) -> Self {
        let mut agents = HashMap::new();
        for role in roles {
            agents.insert(role.clone(), PerAgentState::new());
        }
        Self { agents }
    }

    fn all_completed(&self) -> bool {
        self.agents
            .values()
            .all(|a| a.status == ReviewStatus::Completed || a.status == ReviewStatus::Failed)
    }
}

#[component]
pub fn LivePage() -> impl IntoView {
    let params = use_params_map();
    let run_id = move || params.read().get("id").unwrap_or_default();

    let (pr_states, set_pr_states) = signal::<HashMap<String, PrState>>(HashMap::new());
    let (pr_order, set_pr_order) = signal::<Vec<String>>(Vec::new());
    let (selected_pr, set_selected_pr) = signal::<Option<String>>(None);
    let (role_current_pr, set_role_current_pr) = signal::<HashMap<String, String>>(HashMap::new());

    let (available_role_infos, set_available_role_infos) = signal::<Vec<AgentInfo>>(Vec::new());

    let (progress_done, _set_progress_done) = signal(0usize);
    let (progress_total, _set_progress_total) = signal(0usize);
    let (status, set_status) = signal::<ReviewStatus>(ReviewStatus::Pending);
    let (_connected, set_connected) = signal(false);

    // Fetch available roles on mount
    spawn_local(async move {
        if let Ok(resp) = Request::get(&API_CONFIG).send().await {
            if let Ok(config) = resp.json::<AppConfig>().await {
                set_available_role_infos.set(config.agents);
            }
        }
    });

    {
        let id = run_id();
        let set_states = set_pr_states;
        let set_order = set_pr_order;
        let set_selected = set_selected_pr;
        let set_role_pr = set_role_current_pr;
        let set_stat = set_status;
        let set_conn = set_connected;

        let role_pr = role_current_pr;
        let state_pr = pr_states;
        let roles = available_role_infos;

        spawn_local(async move {
            if id.is_empty() {
                set_stat.update(|s| *s = ReviewStatus::Pending);
                return;
            }

            let url = route!(API_RUNS_ID_LIVE, id);

            match sse::connect_sse(&url).await {
                Ok(mut rx) => {
                    set_conn.set(true);
                    set_stat.update(|s| *s = ReviewStatus::Running);
                    while let Ok(event) = rx.recv().await {
                        match serde_json::from_str::<RunEvent>(&event) {
                            Ok(ev) => {
                                let current_roles = roles.get_untracked();
                                handle_event(
                                    ev,
                                    &state_pr,
                                    &set_states,
                                    &set_order,
                                    &set_selected,
                                    &set_role_pr,
                                    &role_pr,
                                    &set_stat,
                                    &current_roles,
                                );
                            }
                            Err(e) => {
                                warn!("Failed to parse SSE event: {}", e);
                            }
                        }
                    }
                    set_stat.update(|s| *s = ReviewStatus::Completed);
                }
                Err(e) => {
                    error!("Failed to connect to SSE: {}", e);
                    set_stat.update(|s| *s = ReviewStatus::Failed);
                }
            }
        });
    };

    // The currently selected PR - auto-select first on initial data
    let _pr_list = move || {
        let order = pr_order.get();
        let states = pr_states.get();
        order
            .iter()
            .filter_map(|key| states.get(key).map(|s| (key.clone(), s.all_completed())))
            .collect::<Vec<_>>()
    };

    // Ensure there's always a selection once PRs arrive
    {
        let set_sel = set_selected_pr;
        let order = pr_order.get();
        let sel = selected_pr.get();
        if !order.is_empty() && sel.is_none() {
            set_sel.set(Some(order[0].clone()));
        }
    };

    let _active_pr_key = move || selected_pr.get();
    let active_pr_state = move || {
        let key = selected_pr.get()?;
        pr_states.get().get(&key).cloned()
    };

    let total = move || progress_total.get();
    let done = move || progress_done.get();
    let pct = move || {
        let t = total();
        if t > 0 {
            (done() as f64 / t as f64 * 100.0) as u32
        } else {
            0
        }
    };

    view! {
        <div class="live-view-page">
            <div class="page-header">
                <div class="page-header__title">
                    <span class="live-header__dot" style="width: 10px; height: 10px; border-radius: 50%; background: var(--accent-red, #f85149); display: inline-block;"></span>
                    <span>
                        {move || {
                            let s = status.get();
                            match s {
                                ReviewStatus::Pending => format!("Live: {}", run_id()),
                                ReviewStatus::Running => format!("Live: {}", run_id()),
                                ReviewStatus::Completed => format!("{} (completed)", run_id()),
                                s => format!("{}: {}", s, run_id()),
                            }
                        }}
                    </span>
                </div>
                <div class="page-header__actions">
                    <a href={format!("/runs/{}", run_id())} class="btn btn--ghost">
                        <ArrowLeft size=16 />
                        " Back"
                    </a>
                </div>
            </div>

            {move || {
                let s = status.get();
                if s == ReviewStatus::Pending {
                    EitherOf3::A(view! {
                        <div class="content-grid content-grid--metrics">
                            <div class="skeleton skeleton--metric"></div>
                            <div class="skeleton skeleton--metric"></div>
                            <div class="skeleton skeleton--metric"></div>
                            <div class="skeleton skeleton--metric"></div>
                        </div>
                        <div class="content-grid content-grid--agent-panes" style="margin-top: var(--spacing-lg, 16px);">
                            <div class="skeleton skeleton--card" style="height: 200px;"></div>
                            <div class="skeleton skeleton--card" style="height: 200px;"></div>
                            <div class="skeleton skeleton--card" style="height: 200px;"></div>
                            <div class="skeleton skeleton--card" style="height: 200px;"></div>
                        </div>
                    })
                } else if s == ReviewStatus::Failed {
                    EitherOf3::B(view! {
                        <div class="error-state" role="alert">
                            <div class="error-state__icon">
                                <TriangleAlert size=48 />
                            </div>
                            <h3 class="error-state__heading">"Connection lost"</h3>
                            <p class="error-state__message">{format!("Status: {}", s)}</p>
                            <div class="error-state__action">
                                <button class="btn btn--primary">"Reconnect"</button>
                            </div>
                        </div>
                    })
                } else {
                    EitherOf3::C(view! {
                            <MetricsCard value={format!("{}/{}", done(), total())} label="Progress" />
                            <MetricsCard value={status.get().to_string()} label="Status" />
                            <MetricsCard value={format!("{}%", pct())} label="Completed" />
                            {move || {
                                let t = total();
                                if t > 0 {
                                    Either::Left(view! {
                                        <MetricsCard value={format!("{}", pr_order.get().len())} label="Active PRs" />
                                    })
                                } else {
                                    Either::Right(view! { <span></span> })
                                }
                            }}
                                        <div class="pr-selector">
                            <div class="pr-selector__tabs">
                                <span class="pr-selector__label">"PR:"</span>
                                {move || {
                                    let order = pr_order.get();
                                    let states = pr_states.get();
                                    let sel = selected_pr.get();
                                    order.into_iter().map(|key| {
                                        let is_sel = sel.as_deref() == Some(&key);
                                        let completed = states.get(&key).map(|s| s.all_completed()).unwrap_or(false);
                                        let click_key = key.clone();
                                        let set_sel = set_selected_pr;
                                        view! {
                                            <button
                                                class=move || {
                                                    let mut cls = "pr-tab".to_string();
                                                    if is_sel { cls.push_str(" pr-tab--active"); }
                                                    if completed { cls.push_str(" pr-tab--completed"); }
                                                    cls
                                                }
                                                on:click=move |_| set_sel.set(Some(click_key.clone()))
                                            >
                                                {if completed {
                                                    Either::Left(view! { <Check size=14 /> " " })
                                                } else {
                                                    Either::Right(view! { <span></span> })
                                                }}
                                                {key.clone()}
                                            </button>
                                        }
                                    }).collect::<Vec<_>>()
                                }}
                            </div>
                        </div>

                        <div class="content-grid content-grid--agent-panes" style="margin-top: var(--spacing-lg, 16px);">
                            {move || {
                                let pr_state = active_pr_state();
                                let sel_key = selected_pr.get().unwrap_or_default();
                                let roles = available_role_infos.get();
                                let role_lookup: HashMap<&str, &AgentInfo> = roles
                                    .iter()
                                    .map(|ri| (ri.abbreviation.as_str(), ri))
                                    .collect();
                                if let Some(state) = pr_state {
                                    roles.iter().map(|ri| {
                                        let agent_ref = state.agents.get(&ri.abbreviation);
                                        let status_val = agent_ref.map(|a| a.status.clone()).unwrap_or_else(|| ReviewStatus::Pending);
                                        let resp_val = agent_ref.and_then(|a| {
                                            if a.response.is_empty() { None } else { Some(a.response.clone()) }
                                        });
                                        let pr_key = sel_key.clone();
                                        let role_name = ri.abbreviation.clone();
                                        let display_name = role_lookup
                                            .get(ri.abbreviation.as_str())
                                            .map(|ri| ri.display_name())
                                            .unwrap_or_else(|| role_name.clone());
                                        view! {
                                            <AgentPane
                                                name=display_name
                                                status=move || status_val.clone()
                                                response=move || resp_val.clone()
                                                current_pr=move || Some(pr_key.clone())
                                            />
                                        }
                                    }).collect::<Vec<_>>()
                                } else {
                                    vec![]
                                }
                            }}
                        </div>

                        <div class="bottom-bar" style="margin-top: var(--spacing-xl, 24px); padding: var(--spacing-md, 12px); background: var(--bg-surface, #161b22); border: 1px solid var(--border-default, #30363d); border-radius: var(--radius-lg, 8px);">
                            {move || {
                                if total() > 0 {
                                    Either::Left(view! {
                                        <ProgressBar value=done() as u32 max=total() as u32 label=format!("{} / {} PRs ({}%)", done(), total(), pct()) />
                                        <div class="bottom-bar__info" style="display: flex; justify-content: space-between; align-items: center; margin-top: var(--spacing-sm, 8px); font-size: var(--text-sm, 14px); color: var(--text-secondary, #8b949e);">
                                            <span>{format!("PRs loaded: {}", pr_order.get().len())}</span>
                                        </div>
                                    })
                                } else {
                                    Either::Right(view! {
                                        <ProgressBar value=0 max=1 label="Waiting for data...".to_string() />
                                    })
                                }
                            }}
                        </div>
                    })
                }
            }}
        </div>
    }
}

/// Look up the PR key for a given role and update agent state within that PR.
/// Helper that avoids duplicating the `role_current_pr` → `set_states` lookup chain.
fn with_role_pr(
    role_current_pr: &ReadSignal<HashMap<String, String>>,
    set_states: &WriteSignal<HashMap<String, PrState>>,
    role: &str,
    f: impl FnOnce(&mut PerAgentState),
) {
    let pr_key = role_current_pr.get().get(role).cloned();
    if let Some(key) = pr_key {
        set_states.update(|states| {
            states
                .get_mut(&key)
                .and_then(|pr| pr.agents.get_mut(role))
                .map(f);
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_event(
    ev: RunEvent,
    pr_states: &ReadSignal<HashMap<String, PrState>>,
    set_states: &WriteSignal<HashMap<String, PrState>>,
    set_order: &WriteSignal<Vec<String>>,
    set_selected: &WriteSignal<Option<String>>,
    set_role_pr: &WriteSignal<HashMap<String, String>>,
    role_current_pr: &ReadSignal<HashMap<String, String>>,
    set_stat: &WriteSignal<ReviewStatus>,
    roles: &[AgentInfo],
) {
    match ev {
        RunEvent::AgentStarted {
            review_id,
            agent_id,
        } => {
            // Ensure PR state exists
            set_states.update(|states| {
                if !states.contains_key(&review_id.to_string()) {
                    let role_abbrs: Vec<String> =
                        roles.iter().map(|r| r.abbreviation.clone()).collect();
                    states.insert(review_id.to_string(), PrState::new(&role_abbrs));
                }
                if let Some(pr) = states.get_mut(&review_id.to_string()) {
                    // Dynamically add agent if it doesn't exist yet (roles may have been
                    // empty when the PrState was created, e.g. during the async roles fetch)
                    if !pr.agents.contains_key(&agent_id.to_string()) {
                        pr.agents.insert(agent_id.to_string(), PerAgentState::new());
                    }
                    if let Some(agent) = pr.agents.get_mut(&agent_id.to_string()) {
                        agent.status = ReviewStatus::Running;
                    }
                }
            });
            // Track which PR this role is working on
            set_role_pr.update(|rp| {
                rp.insert(agent_id.to_string(), review_id.to_string());
            });
            // Add to order list if new
            set_order.update(|order| {
                let id_str = review_id.to_string();
                if !order.contains(&id_str) {
                    order.push(id_str);
                }
            });
            // Auto-select: pick the first PR, or switch to this PR if the
            // currently selected PR is already completed.
            set_selected.update(|sel| match sel {
                None => *sel = Some(review_id.to_string()),
                Some(current) => {
                    let should_switch = pr_states
                        .get()
                        .get(current)
                        .map(|s| s.all_completed())
                        .unwrap_or(true);
                    if should_switch {
                        *sel = Some(review_id.to_string());
                    }
                }
            });
        }

        RunEvent::AgentChunk { review_id, chunk } => {
            let role_str = review_id.to_string();
            with_role_pr(role_current_pr, set_states, &role_str, |agent| {
                agent.response.push_str(&match &chunk {
                    AgentChunk::Thinking { content, .. } | AgentChunk::Output { content, .. } => {
                        content.clone()
                    }
                    _ => String::new(),
                });
            });
        }

        RunEvent::AgentFinished { agent_id, .. } => {
            let role_str = agent_id.to_string();
            with_role_pr(role_current_pr, set_states, &role_str, |agent| {
                agent.status = ReviewStatus::Completed;
                agent.findings = None;
            });
        }

        RunEvent::ReviewStarted { .. } => {}

        RunEvent::ReviewCompleted { .. } => {
            set_stat.update(|s| *s = ReviewStatus::Completed);
        }
    }
}
