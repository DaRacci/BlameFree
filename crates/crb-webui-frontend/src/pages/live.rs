use crate::components::agent_pane::AgentPane;
use crate::components::metrics_card::MetricsCard;
use crate::components::progress_bar::ProgressBar;
use crate::sse;
use crate::AppConfig;
use crb_types::RunEvent;
use crb_webui_shared::config::RoleInfo;
use leptos::{
    IntoView, ReadSignal, SignalGet, SignalGetUntracked, SignalSet, SignalUpdate, WriteSignal,
    component, create_signal, spawn_local, view,
};
use leptos_router::use_params_map;
use std::collections::HashMap;

/// State for a single agent (role) within a single PR.
#[derive(Debug, Clone)]
struct PerAgentState {
    status: String,   // "pending", "reviewing", "done", "failed"
    response: String, // accumulated response chunks
    findings: Option<usize>,
}

impl PerAgentState {
    fn new() -> Self {
        Self {
            status: "pending".into(),
            response: String::new(),
            findings: None,
        }
    }
}

/// State for a single PR, containing the state of all agents (roles) working on it.
#[derive(Debug, Clone)]
struct PrState {
    agents: HashMap<String, PerAgentState>,
    completed: bool,
}

impl PrState {
    fn new(roles: &[String]) -> Self {
        let mut agents = HashMap::new();
        for role in roles {
            agents.insert(role.clone(), PerAgentState::new());
        }
        Self {
            agents,
            completed: false,
        }
    }

    fn all_completed(&self) -> bool {
        self.agents
            .values()
            .all(|a| a.status == "done" || a.status == "failed")
    }
}

#[component]
pub fn LivePage() -> impl IntoView {
    let params = use_params_map();
    let run_id = move || params.get().get("id").cloned().unwrap_or_default();

    let (pr_states, set_pr_states) = create_signal::<HashMap<String, PrState>>(HashMap::new());
    let (pr_order, set_pr_order) = create_signal::<Vec<String>>(Vec::new());
    let (selected_pr, set_selected_pr) = create_signal::<Option<String>>(None);
    let (role_current_pr, set_role_current_pr) =
        create_signal::<HashMap<String, String>>(HashMap::new());

    let (available_role_infos, set_available_role_infos) =
        create_signal::<Vec<RoleInfo>>(Vec::new());

    let (progress_done, set_progress_done) = create_signal(0usize);
    let (progress_total, set_progress_total) = create_signal(0usize);
    let (status, set_status) = create_signal::<String>("connecting".into());
    let (_connected, set_connected) = create_signal(false);

    // Fetch available roles on mount
    spawn_local(async move {
        let url = "/api/config";
        if let Ok(resp) = gloo_net::http::Request::get(&url).send().await {
            if let Ok(config) = resp.json::<AppConfig>().await {
                set_available_role_infos.set(config.roles);
            }
        }
    });

    {
        let id = run_id();
        let set_states = set_pr_states;
        let set_order = set_pr_order;
        let set_selected = set_selected_pr;
        let set_role_pr = set_role_current_pr;
        let set_done = set_progress_done;
        let set_total = set_progress_total;
        let set_stat = set_status;
        let set_conn = set_connected;

        let role_pr = role_current_pr;
        let state_pr = pr_states;
        let roles = available_role_infos;

        spawn_local(async move {
            if id.is_empty() {
                set_stat.update(|s| *s = "no_run_id".into());
                return;
            }

            let url = format!("/api/runs/{}/live", id);

            match sse::connect_sse(&url).await {
                Ok(mut rx) => {
                    set_conn.set(true);
                    set_stat.update(|s| *s = "running".into());
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
                                    &set_done,
                                    &set_total,
                                    &set_stat,
                                    &current_roles,
                                );
                            }
                            Err(e) => {
                                log::warn!("Failed to parse SSE event: {}", e);
                            }
                        }
                    }
                    set_stat.update(|s| *s = "complete".into());
                }
                Err(e) => {
                    set_stat.update(|s| *s = format!("error: {}", e));
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
            .filter_map(|key| states.get(key).map(|s| (key.clone(), s.completed)))
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
    let _is_complete = move || status.get() == "complete";

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
                            match s.as_str() {
                                "connecting" => format!("Live: {}", run_id()),
                                "running" => format!("Live: {}", run_id()),
                                "complete" => format!("{} (completed)", run_id()),
                                s => format!("{}: {}", s, run_id()),
                            }
                        }}
                    </span>
                </div>
                <div class="page-header__actions">
                    <a href={format!("/runs/{}", run_id())} class="btn btn--ghost">"< Back"</a>
                </div>
            </div>

            {move || {
                let s = status.get();
                if s == "connecting" {
                    view! {
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
                    }.into_view()
                } else if s.starts_with("error") || s == "no_run_id" {
                    view! {
                        <div class="error-state" role="alert">
                            <div class="error-state__icon">"!"</div>
                            <h3 class="error-state__heading">"Connection lost"</h3>
                            <p class="error-state__message">{format!("Status: {}", s)}</p>
                            <div class="error-state__action">
                                <button class="btn btn--primary">"Reconnect"</button>
                            </div>
                        </div>
                    }.into_view()
                } else {
                    view! {
                            <MetricsCard value={format!("{}/{}", done(), total())} label="Progress" />
                            <MetricsCard value={status.get().clone()} label="Status" />
                            <MetricsCard value={format!("{}%", pct())} label="Completed" />
                            {move || {
                                let t = total();
                                if t > 0 {
                                    view! {
                                        <MetricsCard value={format!("{}", pr_order.get().len())} label="Active PRs" />
                                    }.into_view()
                                } else {
                                    view! { <span></span> }.into_view()
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
                                        let completed = states.get(&key).map(|s| s.completed).unwrap_or(false);
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
                                                {if completed { "✓ " } else { "" }}
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
                                let role_lookup: HashMap<&str, &RoleInfo> = roles
                                    .iter()
                                    .map(|ri| (ri.abbreviation.as_str(), ri))
                                    .collect();
                                if let Some(state) = pr_state {
                                    roles.iter().map(|ri| {
                                        let agent_ref = state.agents.get(&ri.abbreviation);
                                        let status_val = agent_ref.map(|a| a.status.clone()).unwrap_or_else(|| "pending".into());
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
                                    Vec::<leptos::View>::new()
                                }
                            }}
                        </div>

                        <div class="bottom-bar" style="margin-top: var(--spacing-xl, 24px); padding: var(--spacing-md, 12px); background: var(--bg-surface, #161b22); border: 1px solid var(--border-default, #30363d); border-radius: var(--radius-lg, 8px);">
                            {move || {
                                if total() > 0 {
                                    view! {
                                        <ProgressBar value=done() as u32 max=total() as u32 label=format!("{} / {} PRs ({}%)", done(), total(), pct()) />
                                        <div class="bottom-bar__info" style="display: flex; justify-content: space-between; align-items: center; margin-top: var(--spacing-sm, 8px); font-size: var(--text-sm, 14px); color: var(--text-secondary, #8b949e);">
                                            <span>{format!("PRs loaded: {}", pr_order.get().len())}</span>
                                        </div>
                                    }.into_view()
                                } else {
                                    view! {
                                        <ProgressBar value=0 max=1 label="Waiting for data...".to_string() />
                                    }.into_view()
                                }
                            }}
                        </div>
                    }.into_view()
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
    set_done: &WriteSignal<usize>,
    set_total: &WriteSignal<usize>,
    set_stat: &WriteSignal<String>,
    roles: &[RoleInfo],
) {
    match ev {
        RunEvent::AgentStarted { identifier, agent } => {
            // Ensure PR state exists
            set_states.update(|states| {
                if !states.contains_key(&identifier) {
                    let role_abbrs: Vec<String> =
                        roles.iter().map(|r| r.abbreviation.clone()).collect();
                    states.insert(identifier.clone(), PrState::new(&role_abbrs));
                }
                if let Some(pr) = states.get_mut(&identifier) {
                    // Dynamically add agent if it doesn't exist yet (roles may have been
                    // empty when the PrState was created, e.g. during the async roles fetch)
                    if !pr.agents.contains_key(&agent) {
                        pr.agents.insert(agent.clone(), PerAgentState::new());
                    }
                    if let Some(agent) = pr.agents.get_mut(&agent) {
                        agent.status = "reviewing".into();
                    }
                }
            });
            // Track which PR this role is working on
            set_role_pr.update(|rp| {
                rp.insert(agent, identifier.clone());
            });
            // Add to order list if new
            set_order.update(|order| {
                if !order.contains(&identifier) {
                    order.push(identifier.clone());
                }
            });
            // Auto-select: pick the first PR, or switch to this PR if the
            // currently selected PR is already completed.
            set_selected.update(|sel| match sel {
                None => *sel = Some(identifier.clone()),
                Some(current) => {
                    let should_switch = pr_states
                        .get()
                        .get(current)
                        .map(|s| s.completed)
                        .unwrap_or(true);
                    if should_switch {
                        *sel = Some(identifier.clone());
                    }
                }
            });
        }

        RunEvent::AgentChunk {
            identifier: role,
            chunk,
        } => {
            with_role_pr(role_current_pr, set_states, &role, |agent| {
                agent.response.push_str(&chunk);
            });
        }

        RunEvent::AgentFinished {
            identifier: role,
            findings,
            success,
        } => {
            with_role_pr(role_current_pr, set_states, &role, |agent| {
                agent.status = if success {
                    "done".into()
                } else {
                    "failed".into()
                };
                agent.findings = Some(findings);
            });
            // Check if all agents for this PR are done
            if let Some(pr_key) = role_current_pr.get().get(&role).cloned() {
                set_states.update(|states| {
                    if let Some(pr) = states.get_mut(&pr_key) {
                        if pr.all_completed() {
                            pr.completed = true;
                        }
                    }
                });
            }
        }

        RunEvent::ReviewStarted { .. } => {
            // Review started — progress signal, nothing to do on the dashboard
        }

        RunEvent::RunProgress {
            completed_prs,
            total_prs,
            current_pr,
            ..
        } => {
            set_done.set(completed_prs);
            set_total.set(total_prs);
            if let Some(pr_key) = current_pr {
                // Ensure this PR is tracked
                set_states.update(|states| {
                    if !states.contains_key(&pr_key) {
                        let role_abbrs: Vec<String> =
                            roles.iter().map(|r| r.abbreviation.clone()).collect();
                        states.insert(pr_key.clone(), PrState::new(&role_abbrs));
                    }
                });
                set_order.update(|order| {
                    if !order.contains(&pr_key) {
                        order.push(pr_key.clone());
                    }
                });
                // Get first PR key for auto-selection (we can't read WriteSignal inside another update closure)
                set_selected.update(|sel| {
                    if sel.is_none() {
                        *sel = Some(pr_key.clone());
                    }
                });
            }
        }

        RunEvent::ReviewCompleted {
            identifier: pr_key, ..
        } => {
            set_states.update(|states| {
                if let Some(pr) = states.get_mut(&pr_key) {
                    pr.completed = true;
                }
            });
        }

        RunEvent::RunFinished { .. } => {
            set_stat.update(|s| *s = "complete".into());
        }
    }
}
