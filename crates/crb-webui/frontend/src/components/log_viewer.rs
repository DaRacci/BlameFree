use leptos::*;
use crate::{LogsListResponse, AgentLogResponse, api_url};

/// Map a role abbreviation to a human-readable display name.
fn role_display_name(role: &str) -> String {
    match role {
        "SA" => "Security Auditor (SA)".to_string(),
        "CL" => "Code Logician (CL)".to_string(),
        "AR" | "ARCH" => "Architecture Reviewer (ARCH)".to_string(),
        "SEC" => "Security Evaluator (SEC)".to_string(),
        _ => role.to_string(),
    }
}

#[component]
pub fn LogViewer(
    logs: LogsListResponse,
    run_id: String,
) -> impl IntoView {
    let style_container = "background: #1e2938; border-radius: 8px; padding: 1rem; margin-top: 0.5rem;";
    let style_pr_header = "cursor: pointer; padding: 0.75rem; background: #334155; border-radius: 6px; margin-bottom: 0.25rem; color: #e2e8f0; font-weight: 600;";
    let style_pr_body = "padding: 0.5rem 0.75rem;";
    let style_agent_header = "cursor: pointer; padding: 0.5rem; background: #1e2938; border-radius: 4px; margin: 0.25rem 0; color: #94a3b8; font-size: 0.9rem;";
    let style_agent_body = "padding: 0.5rem; margin-left: 0.5rem; border-left: 2px solid #3b82f6;";
    let style_pre = "background: #0f172a; padding: 0.75rem; border-radius: 4px; overflow-x: auto; white-space: pre-wrap; word-break: break-word; font-size: 0.8rem; color: #e2e8f0; margin: 0.5rem 0;";
    let style_label = "color: #64748b; font-size: 0.8rem; font-weight: 600; margin-top: 0.5rem; display: block;";
    let style_empty = "color: #64748b; font-style: italic; text-align: center; padding: 2rem;";

    // If no cache available, show empty state
    if !logs.cache_available {
        return view! {
            <div style=style_container>
                <p style=style_empty>"No cache available. Logs are only available when the run was executed with caching enabled."</p>
            </div>
        };
    }

    // If no PRs, show empty state
    if logs.prs.is_empty() {
        return view! {
            <div style=style_container>
                <p style=style_empty>"No PR logs found for this run."</p>
            </div>
        };
    }

    view! {
        <div style=style_container>
            {logs.prs.iter().map(move |pr| {
                let pr_key = pr.pr_key.clone();
                let pr_title = pr.pr_title.clone();
                let agents = pr.agents.clone();
                let run_id_clone = run_id.clone();

                view! {
                    <details style="margin-bottom: 0.5rem;">
                        <summary style=style_pr_header>
                            {format!("PR #{} - {}", pr_key, pr_title)}
                        </summary>
                        <div style=style_pr_body>
                            {agents.iter().map(move |agent_name| {
                                let agent_name = agent_name.clone();
                                let run_id_for_fetch = run_id_clone.clone();
                                let pr_key_for_fetch = pr_key.clone();
                                let role_for_fetch = agent_name.clone();

                                // Signal to hold fetched agent log
                                let (agent_log, set_agent_log) = create_signal::<Option<AgentLogResponse>>(None);
                                let (fetching, set_fetching) = create_signal(false);
                                let (fetched, set_fetched) = create_signal(false);

                                let on_toggle = move || {
                                    if !fetched.get() && !fetching.get() {
                                        set_fetching.set(true);
                                        let run_id = run_id_for_fetch.clone();
                                        let pr_key = pr_key_for_fetch.clone();
                                        let role = role_for_fetch.clone();
                                        let set_log = set_agent_log.clone();
                                        let set_fetch = set_fetching.clone();
                                        let set_fetched = set_fetched.clone();
                                        wasm_bindgen_futures::spawn_local(async move {
                                            let url = api_url(&format!("/api/runs/{}/logs/{}/{}", run_id, pr_key, role));
                                            let resp = gloo_net::http::Request::get(&url).send().await;
                                            match resp {
                                                Ok(r) if r.ok() => {
                                                    match r.json::<AgentLogResponse>().await {
                                                        Ok(log) => {
                                                            set_log.set(Some(log));
                                                        }
                                                        Err(e) => {
                                                            log::error!("Failed to parse agent log: {}", e);
                                                        }
                                                    }
                                                }
                                                Ok(r) => {
                                                    log::error!("Agent log fetch returned status: {}", r.status());
                                                }
                                                Err(e) => {
                                                    log::error!("Agent log fetch error: {}", e);
                                                }
                                            }
                                            set_fetch.set(false);
                                            set_fetched.set(true);
                                        });
                                    }
                                };

                                view! {
                                    <details
                                        on:click=move |_| on_toggle()
                                        style="margin: 0.25rem 0;"
                                    >
                                        <summary style=style_agent_header>
                                            {role_display_name(&agent_name.clone())}
                                        </summary>
                                        <div style=style_agent_body>
                                            {move || {
                                                if fetching.get() {
                                                    view! {
                                                        <p style="color: #94a3b8; font-style: italic; font-size: 0.85rem;">
                                                            "Loading..."
                                                        </p>
                                                    }.into_view()
                                                } else if let Some(ref log) = agent_log.get() {
                                                    if log.available {
                                                        view! {
                                                            <>
                                                                <span style=style_label>"Prompt:"</span>
                                                                <pre style=style_pre>
                                                                    {log.prompt.clone().unwrap_or_else(|| "No prompt available".into())}
                                                                </pre>
                                                                <span style=style_label>"Response:"</span>
                                                                <pre style=style_pre>
                                                                    {log.response.clone().unwrap_or_else(|| "No response available".into())}
                                                                </pre>
                                                                {log.reasoning.as_ref().filter(|r| !r.is_empty()).map(|r| {
                                                                    view! {
                                                                        <>
                                                                            <span style=style_label>"Reasoning:"</span>
                                                                            <pre style=style_pre>{r.clone()}</pre>
                                                                        </>
                                                                    }.into_view()
                                                                })}
                                                            </>
                                                        }.into_view()
                                                    } else {
                                                        view! {
                                                            <p style="color: #64748b; font-style: italic; font-size: 0.85rem;">
                                                                "Agent log data not available."
                                                            </p>
                                                        }.into_view()
                                                    }
                                                } else {
                                                    view! {
                                                        <p style="color: #64748b; font-style: italic; font-size: 0.85rem;">
                                                            "Click to load logs"
                                                        </p>
                                                    }.into_view()
                                                }
                                            }}
                                        </div>
                                    </details>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                    </details>
                }
            }).collect::<Vec<_>>()}
        </div>
    }
}
