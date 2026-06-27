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
    let (connected, set_connected) = create_signal(false);

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
        <div class="live-nav">
            <span class="dot"></span>
            <span style="font-weight: 600;">
                {move || {
                    let p = progress.get();
                    match p.status.as_str() {
                        "connecting" => "Connecting...",
                        "running" => "Live",
                        "complete" => "Complete",
                        s => s,
                    }
                    .to_string()
                }}
            </span>
            <span style="color: #64748b; font-size: 0.85rem;">
                {move || {
                    if total() > 0 {
                        format!("{} / {} PRs", done(), total())
                    } else {
                        "Waiting for data...".into()
                    }
                }}
            </span>
            <a href={format!("/runs/{}", run_id())} style="margin-left: auto; color: #94a3b8; text-decoration: none;">
                "Detail View →"
            </a>
        </div>

        <div class="container">
            // ─── Progress bar ────────────────────────────────────────
            {move || {
                if total() > 0 {
                    view! {
                        <div class="card">
                            <h3>"Overall Progress"</h3>
                            <ProgressBar value=done() max=total() label=format!("{} / {} ({})", done(), total(), pct()) />
                        </div>
                    }.into_view()
                } else if progress.get().status == "running" {
                    view! {
                        <div class="card">
                            <h3>"Overall Progress"</h3>
                            <ProgressBar value=0 max=1 label="Waiting...".to_string() />
                        </div>
                    }.into_view()
                } else {
                    view! { <span></span> }.into_view()
                }
            }}

            // ─── Agent grid ───────────────────────────────────────────
            <div class="agent-grid">
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
