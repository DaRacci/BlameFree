use std::collections::HashMap;

use crb_webui_shared::runs::{AgentLogResponse, PrAgentEntry, PrAgentsResponse};
use leptos::{
    component, create_local_resource, create_signal, view, DynAttrs, IntoView, SignalGet, SignalSet,
};
use leptos_router::{use_params_map, A};

#[component]
pub fn PrDetailPage() -> impl IntoView {
    let params = use_params_map();
    let run_id = move || params.get().get("id").cloned().unwrap_or_default();
    let pr_key = move || params.get().get("pr_key").cloned().unwrap_or_default();

    let (pr_title, set_pr_title) = create_signal(String::new());
    let (agents, set_agents) = create_signal::<Vec<PrAgentEntry>>(vec![]);
    let (loading, set_loading) = create_signal(true);
    let (error, set_error) = create_signal::<Option<String>>(None);

    let (agent_logs, set_agent_logs) =
        create_signal::<HashMap<String, AgentLogResponse>>(HashMap::new());
    let (logs_loading, set_logs_loading) = create_signal(false);

    let fetch_pr = move || {
        let rid = run_id();
        let pk = pr_key();
        if rid.is_empty() || pk.is_empty() {
            return;
        }
        set_loading.set(true);
        set_error.set(None);
        let rid_clone = rid.clone();
        let pk_clone = pk.clone();
        let set_title = set_pr_title;
        let set_agents = set_agents;
        let set_loading = set_loading;
        let set_error = set_error;
        let set_logs = set_agent_logs;
        let set_logs_loading = set_logs_loading;
        wasm_bindgen_futures::spawn_local(async move {
            let url = format!("/api/runs/{}/prs/{}", rid_clone, pk_clone);
            match gloo_net::http::Request::get(&url).send().await {
                Ok(r) if r.ok() => match r.json::<PrAgentsResponse>().await {
                    Ok(pr) => {
                        set_title.set(pr.pr_title);
                        set_agents.set(pr.agents.clone());
                        set_loading.set(false);

                        set_logs_loading.set(true);
                        let roles: Vec<String> = pr.agents.iter().map(|a| a.role.clone()).collect();
                        if roles.is_empty() {
                            set_logs_loading.set(false);
                            return;
                        }
                        let rid2 = rid_clone.clone();
                        let pk2 = pk_clone.clone();
                        let set_logs = set_logs;
                        let set_logs_loading = set_logs_loading;
                        wasm_bindgen_futures::spawn_local(async move {
                            let mut results = HashMap::new();
                            for role in &roles {
                                let log_url = format!("/api/runs/{}/logs/{}/{}", rid2, pk2, role);
                                if let Ok(resp) =
                                    gloo_net::http::Request::get(&log_url).send().await
                                {
                                    if let Ok(log) = resp.json::<AgentLogResponse>().await {
                                        results.insert(role.clone(), log);
                                    }
                                }
                            }
                            set_logs.set(results);
                            set_logs_loading.set(false);
                        });
                    }
                    Err(e) => {
                        set_error.set(Some(format!("Failed to parse PR data: {}", e)));
                        set_loading.set(false);
                    }
                },
                Ok(r) => {
                    set_error.set(Some(format!("Server returned {}", r.status())));
                    set_loading.set(false);
                }
                Err(e) => {
                    set_error.set(Some(format!("Network error: {}", e)));
                    set_loading.set(false);
                }
            }
        });
    };

    // Trigger fetch on mount
    let _ = create_local_resource(
        move || (run_id(), pr_key()),
        move |_| {
            fetch_pr();
            async move {}
        },
    );

    let role_display = |role: &str| -> &'static str {
        match role {
            "SA" => "Security Auditor (SA)",
            "CL" => "Code Logician (CL)",
            "AR" | "ARCH" => "Architecture Reviewer (ARCH)",
            "SEC" => "Security Evaluator (SEC)",
            _ => "Unknown Agent",
        }
    };

    let role_color = |role: &str| -> &'static str {
        match role {
            "SA" => "#3b82f6",
            "CL" => "#22c55e",
            "AR" | "ARCH" => "#f59e0b",
            "SEC" => "#ef4444",
            _ => "#8b5cf6",
        }
    };

    view! {
        <div class="pr-detail-page">
            <div style="display: flex; align-items: center; gap: 8px; margin-bottom: 16px; font-size: 14px; color: var(--text-secondary, #8b949e);">
                <A href=move || "/".to_string()>"Home"</A>
                <span>"/"</span>
                <A href=move || format!("/runs/{}", run_id())>
                    {move || run_id()}
                </A>
                <span>"/"</span>
                <span>{move || pr_key()}</span>
            </div>

            <div class="page-header">
                <div>
                    <h1 class="page-header__title">{move || pr_title.get()}</h1>
                    <div style="display: flex; align-items: center; gap: 8px; margin-top: 4px;">
                        <span class="badge badge--neutral">
                            <span class="badge__label">{move || format!("PR #{}", pr_key())}</span>
                        </span>
                        <span style="font-size: var(--text-sm, 14px); color: var(--text-secondary, #8b949e);">
                            {move || format!("Run: {}", run_id())}
                        </span>
                    </div>
                </div>
                <div class="page-header__actions">
                    <A
                        href=move || format!("/runs/{}", run_id())
                        attr:class="btn btn--primary"
                    >
                        "< Back to Run"
                    </A>
                </div>
            </div>

            {move || {
                if loading.get() {
                    view! {
                        <div style="text-align: center; padding: 2rem; color: var(--text-secondary, #64748b); font-style: italic;">
                            "Loading PR details..."
                        </div>
                    }.into_view()
                } else if let Some(ref e) = error.get() {
                    view! {
                        <div class="error-state" role="alert">
                            <div class="error-state__icon">"!"</div>
                            <h3 class="error-state__heading">"Failed to load PR details"</h3>
                            <p class="error-state__message">{e}</p>
                            <div class="error-state__action">
                                <button class="btn btn--primary" on:click=move |_| fetch_pr()>
                                    "Retry"
                                </button>
                            </div>
                        </div>
                    }.into_view()
                } else {
                    let agents_list = agents.get();
                    if agents_list.is_empty() {
                        view! {
                            <div style="text-align: center; padding: 3rem; color: var(--text-secondary, #64748b);">
                                <p style="font-size: 1.1rem; margin-bottom: 0.5rem;">"No cached agent logs available for this PR."</p>
                                <p style="font-size: 0.9rem;">"Agent logs are only available when the run was executed with caching enabled and the cache is still present."</p>
                                <A href=move || format!("/runs/{}", run_id())
                                    attr:class="btn btn--primary"
                                    attr:style="margin-top: 1rem; display: inline-block;"
                                >
                                    "< Back to Run"
                                </A>
                            </div>
                        }.into_view()
                    } else {
                        let logs = agent_logs.get();
                        let logs_loading_val = logs_loading.get();

                        view! {
                            <div style="margin-bottom: 1rem;">
                                <div style="display: flex; justify-content: space-between; align-items: center;">
                                    <h2 style="color: #e2e8f0; margin: 0;">"Agent Logs"</h2>
                                    {if logs_loading_val {
                                        view! {
                                            <span style="color: #64748b; font-style: italic; font-size: 0.85rem;">
                                                "Loading agent logs..."
                                            </span>
                                        }.into_view()
                                    } else {
                                        view! { <span></span> }.into_view()
                                    }}
                                </div>
                            </div>

                            <div style="display: grid; grid-template-columns: repeat(auto-fit, minmax(450px, 1fr)); gap: 16px;">
                                {agents_list.iter().map(|agent| {
                                    let role = agent.role.clone();
                                    let role_display_name = role_display(&role);
                                    let color = role_color(&role);
                                    let log = logs.get(&role);

                                    let (has_prompt, has_response, has_reasoning) = log.as_ref().map(|l| {
                                        (l.prompt.is_some(), l.response.is_some(), l.reasoning.as_ref().filter(|r| !r.is_empty()).is_some())
                                    }).unwrap_or((false, false, false));

                                    let prompt_content = log.as_ref().and_then(|l| l.prompt.clone());
                                    let response_content = log.as_ref().and_then(|l| l.response.clone());
                                    let reasoning_content = log.as_ref().and_then(|l| l.reasoning.clone());

                                    view! {
                                        <div style=format!("background: #1e2938; border-radius: 8px; border-left: 2px solid {color}; overflow: hidden;")>
                                            // Agent header
                                            <div style="padding: 12px 16px; background: #0f172a; border-bottom: 1px solid #334155;">
                                                <div style="display: flex; justify-content: space-between; align-items: center;">
                                                    <h3 style="margin: 0; color: #e2e8f0; font-size: 0.95rem; display: flex; align-items: center; gap: 8px;">
                                                        <span style="width: 10px; height: 10px; border-radius: 50%; background: {color}; display: inline-block;"></span>
                                                        {role_display_name}
                                                    </h3>
                                                    <div style="display: flex; gap: 6px; font-size: 0.75rem;">
                                                        {if has_prompt {
                                                            view! { <span style="color: #22c55e;">"✓ Prompt"</span> }.into_view()
                                                        } else {
                                                            view! { <span style="color: #64748b;">"✗ Prompt"</span> }.into_view()
                                                        }}
                                                        {if has_response {
                                                            view! { <span style="color: #22c55e;">"✓ Response"</span> }.into_view()
                                                        } else {
                                                            view! { <span style="color: #64748b;">"✗ Response"</span> }.into_view()
                                                        }}
                                                        {if has_reasoning {
                                                            view! { <span style="color: #22c55e;">"✓ Reasoning"</span> }.into_view()
                                                        } else {
                                                            view! { <span></span> }.into_view()
                                                        }}
                                                    </div>
                                                </div>
                                            </div>

                                            // Agent log content with tabs
                                            <div style="padding: 12px;">
                                                {if logs_loading_val {
                                                    view! {
                                                        <p style="color: #64748b; font-style: italic; font-size: 0.85rem; text-align: center; padding: 1rem;">
                                                            "Loading..."
                                                        </p>
                                                    }.into_view()
                                                } else if log.is_some() && (has_prompt || has_response) {
                                                    // Use details/summary for Prompt/Response/Reasoning sections
                                                    view! {
                                                        <>
                                                            {if has_prompt {
                                                                view! {
                                                                    <details style="margin-bottom: 8px;" open=true>
                                                                        <summary style="cursor: pointer; color: #94a3b8; font-size: 0.8rem; font-weight: 600; padding: 4px 0;">
                                                                            "Prompt"
                                                                        </summary>
                                                                        <pre style="background: #0f172a; padding: 0.75rem; border-radius: 4px; font-size: 0.75rem; overflow-x: auto; max-height: 400px; overflow-y: auto; white-space: pre-wrap; word-break: break-word; line-height: 1.4; color: #cbd5e1; margin: 4px 0 0 0;">
                                                                            {prompt_content.unwrap_or_default()}
                                                                        </pre>
                                                                    </details>
                                                                }.into_view()
                                                            } else {
                                                                view! { <span></span> }.into_view()
                                                            }}
                                                            {if has_response {
                                                                view! {
                                                                    <details style="margin-bottom: 8px;" open=true>
                                                                        <summary style="cursor: pointer; color: #94a3b8; font-size: 0.8rem; font-weight: 600; padding: 4px 0;">
                                                                            "Response"
                                                                        </summary>
                                                                        <pre style="background: #0f172a; padding: 0.75rem; border-radius: 4px; font-size: 0.75rem; overflow-x: auto; max-height: 400px; overflow-y: auto; white-space: pre-wrap; word-break: break-word; line-height: 1.4; color: #cbd5e1; margin: 4px 0 0 0;">
                                                                            {response_content.unwrap_or_default()}
                                                                        </pre>
                                                                    </details>
                                                                }.into_view()
                                                            } else {
                                                                view! { <span></span> }.into_view()
                                                            }}
                                                            {if has_reasoning {
                                                                view! {
                                                                    <details style="margin-bottom: 0px;">
                                                                        <summary style="cursor: pointer; color: #94a3b8; font-size: 0.8rem; font-weight: 600; padding: 4px 0;">
                                                                            "Reasoning"
                                                                        </summary>
                                                                        <pre style="background: #0f172a; padding: 0.75rem; border-radius: 4px; font-size: 0.75rem; overflow-x: auto; max-height: 400px; overflow-y: auto; white-space: pre-wrap; word-break: break-word; line-height: 1.4; color: #cbd5e1; margin: 4px 0 0 0;">
                                                                            {reasoning_content.unwrap_or_default()}
                                                                        </pre>
                                                                    </details>
                                                                }.into_view()
                                                            } else {
                                                                view! { <span></span> }.into_view()
                                                            }}
                                                        </>
                                                    }.into_view()
                                                } else {
                                                    view! {
                                                        <p style="color: #64748b; font-style: italic; font-size: 0.85rem; text-align: center; padding: 1rem;">
                                                            "No log data available."
                                                        </p>
                                                    }.into_view()
                                                }}
                                            </div>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        }.into_view()
                    }
                }
            }}
        </div>
    }
}
