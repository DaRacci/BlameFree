use leptos::*;
use leptos_router::*;
use crate::{AgentEvent, api_url};
use crate::components::agent_pane::AgentPane;
use crate::components::progress_bar::ProgressBar;

#[component]
pub fn LivePage() -> impl IntoView {
    let params = use_params_map();
    let run_id = move || params.get().get("id").cloned().unwrap_or_default();

    // ─── Agent state ─────────────────────────────────────────────────
    let (agent_reviewer, set_agent_reviewer) = create_signal::<AgentState>(AgentState::default("reviewer"));
    let (agent_summarizer, set_agent_summarizer) = create_signal::<AgentState>(AgentState::default("summarizer"));
    let (agent_tester, set_agent_tester) = create_signal::<AgentState>(AgentState::default("tester"));
    let (agent_analyst, set_agent_analyst) = create_signal::<AgentState>(AgentState::default("analyst"));

    // ─── Overall progress ─────────────────────────────────────────────
    let (progress, set_progress) = create_signal::<ProgressInfo>(ProgressInfo {
        done: 0,
        total: 0,
        status: "connecting".into(),
    });
    let (_connected, set_connected) = create_signal(false);

    // ─── SSE connection ───────────────────────────────────────────────
    let _connect = {
        let id = run_id();
        let set_rev = set_agent_reviewer.clone();
        let set_sum = set_agent_summarizer.clone();
        let set_test = set_agent_tester.clone();
        let set_anal = set_agent_analyst.clone();
        let set_prog = set_progress.clone();
        let set_conn = set_connected.clone();

        spawn_local(async move {
            if id.is_empty() {
                set_prog.update(|p| p.status = "no_run_id".into());
                return;
            }

            let url = api_url(&format!("/api/runs/{}/live", id));

            // EventSource via wasm_bindgen
            match connect_sse(&url).await {
                Ok(mut rx) => {
                    set_conn.set(true);
                    set_prog.update(|p| p.status = "running".into());
                    while let Ok(event) = rx.recv().await {
                        match serde_json::from_str::<AgentEvent>(&event) {
                            Ok(ev) => {
                                // Update the correct agent
                                match ev.agent.as_str() {
                                    "reviewer" => set_rev.update(|s| s.update_from_event(&ev)),
                                    "summarizer" => set_sum.update(|s| s.update_from_event(&ev)),
                                    "tester" => set_test.update(|s| s.update_from_event(&ev)),
                                    "analyst" => set_anal.update(|s| s.update_from_event(&ev)),
                                    _ => {}
                                }
                                // Update progress
                                if let (Some(p), Some(t)) = (ev.progress, ev.total) {
                                    set_prog.update(|pr| {
                                        pr.done = p;
                                        pr.total = t;
                                    });
                                }
                            }
                            Err(e) => {
                                // Keep going — skip malformed events
                                log::warn!("Failed to parse SSE event: {}", e);
                            }
                        }
                    }
                    set_prog.update(|p| p.status = "complete".into());
                }
                Err(e) => {
                    set_prog.update(|p| p.status = format!("error: {}", e));
                }
            }
        });
    };

    let total = move || progress.get().total;
    let done = move || progress.get().done;
    let pct = move || {
        let t = total();
        if t > 0 { (done() as f64 / t as f64 * 100.0) as u32 } else { 0 }
    };

    view! {
        <div class="live-view-page">
            // ─── Page Header ──────────────────────────────────────────
            <div class="page-header">
                <div class="page-header__title">
                    <span class="live-header__dot" style="width: 10px; height: 10px; border-radius: 50%; background: var(--accent-red, #f85149); display: inline-block;"></span>
                    <span>
                        {move || {
                            let p = progress.get();
                            match p.status.as_str() {
                                "connecting" => format!("Live: {}", run_id()),
                                "running" => format!("🔴 Live: {}", run_id()),
                                "complete" => format!("✅ {} (completed)", run_id()),
                                s => format!("{}: {}", s, run_id()),
                            }
                        }}
                    </span>
                </div>
                <div class="page-header__actions">
                    <a href={format!("/runs/{}", run_id())} class="btn btn--ghost">"⬅ Back"</a>
                </div>
            </div>

            // ─── Status ───────────────────────────────────────────────
            {move || {
                let p = progress.get();
                if p.status == "connecting" {
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
                } else if p.status.starts_with("error") || p.status == "no_run_id" {
                    view! {
                        <div class="error-state" role="alert">
                            <div class="error-state__icon">"⚠️"</div>
                            <h3 class="error-state__heading">"Connection lost"</h3>
                            <p class="error-state__message">{format!("Status: {}", p.status)}</p>
                            <div class="error-state__action">
                                <button class="btn btn--primary">"🔄 Reconnect"</button>
                            </div>
                        </div>
                    }.into_view()
                } else {
                    view! {
                        // ─── Live Metrics ─────────────────────────────
                        <div class="content-grid content-grid--metrics">
                            <div class="metric-card">
                                <p class="metric-card__label">"Progress"</p>
                                <p class="metric-card__value">{format!("{}/{}", done(), total())}</p>
                            </div>
                            <div class="metric-card">
                                <p class="metric-card__label">"Status"</p>
                                <p class="metric-card__value">{p.status.clone()}</p>
                            </div>
                            <div class="metric-card">
                                <p class="metric-card__label">"Completed"</p>
                                <p class="metric-card__value">{format!("{}%", pct())}</p>
                            </div>
                            {move || {
                                let total_prs = total();
                                if total_prs > 0 {
                                    view! {
                                        <div class="metric-card">
                                            <p class="metric-card__label">"Current PR"</p>
                                            <p class="metric-card__value">{format!("#{}", done() + 1)}</p>
                                        </div>
                                    }.into_view()
                                } else {
                                    view! { <span></span> }.into_view()
                                }
                            }}
                        </div>

                        // ─── Agent Panes Grid ─────────────────────────
                        <div class="content-grid content-grid--agent-panes" style="margin-top: var(--spacing-lg, 16px);">
                            <AgentPane
                                name="Reviewer"
                                status=move || agent_reviewer.get().status.clone()
                                response=move || agent_reviewer.get().response.clone()
                                current_pr=move || agent_reviewer.get().current_pr
                            />
                            <AgentPane
                                name="Summarizer"
                                status=move || agent_summarizer.get().status.clone()
                                response=move || agent_summarizer.get().response.clone()
                                current_pr=move || agent_summarizer.get().current_pr
                            />
                            <AgentPane
                                name="Tester"
                                status=move || agent_tester.get().status.clone()
                                response=move || agent_tester.get().response.clone()
                                current_pr=move || agent_tester.get().current_pr
                            />
                            <AgentPane
                                name="Analyst"
                                status=move || agent_analyst.get().status.clone()
                                response=move || agent_analyst.get().response.clone()
                                current_pr=move || agent_analyst.get().current_pr
                            />
                        </div>

                        // ─── Bottom Progress Bar ──────────────────────
                        <div class="bottom-bar" style="margin-top: var(--spacing-xl, 24px); padding: var(--spacing-md, 12px); background: var(--bg-surface, #161b22); border: 1px solid var(--border-default, #30363d); border-radius: var(--radius-lg, 8px);">
                            {move || {
                                if total() > 0 {
                                    view! {
                                        <ProgressBar value=done() max=total() label=format!("{} / {} PRs ({}%)", done(), total(), pct()) />
                                        <div class="bottom-bar__info" style="display: flex; justify-content: space-between; align-items: center; margin-top: var(--spacing-sm, 8px); font-size: var(--text-sm, 14px); color: var(--text-secondary, #8b949e);">
                                            <span>{format!("Current PR: #{}", done() + 1)}</span>
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

// ─── Data structures ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct AgentState {
    name: String,
    status: String,
    response: Option<String>,
    current_pr: Option<u32>,
}

impl AgentState {
    fn default(name: &str) -> Self {
        Self {
            name: name.to_string(),
            status: "pending".into(),
            response: None,
            current_pr: None,
        }
    }

    fn update_from_event(&mut self, ev: &AgentEvent) {
        self.status = ev.status.clone();
        self.response = ev.response.clone();
        self.current_pr = ev.pr_number;
    }
}

#[derive(Debug, Clone)]
struct ProgressInfo {
    done: u32,
    total: u32,
    status: String,
}

// ─── SSE connection via web_sys::EventSource ──────────────────────────────────

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use futures::channel::mpsc;
use web_sys::MessageEvent;

async fn connect_sse(url: &str) -> Result<mpsc::UnboundedReceiver<String>, String> {
    let (tx, rx) = mpsc::unbounded();

    let es = web_sys::EventSource::new(url)
        .map_err(|e| format!("Failed to construct EventSource: {:?}", e))?;

    let tx_clone = tx.clone();
    let closure = Closure::wrap(Box::new(move |event: MessageEvent| {
        if let Some(text) = event.data().as_string() {
            let _ = tx_clone.unbounded_send(text);
        } else {
            log::warn!("SSE message with non-string data");
        }
    }) as Box<dyn FnMut(MessageEvent)>);

    es.set_onmessage(Some(closure.as_ref().unchecked_ref()));
    closure.forget();

    Ok(rx)
}
