use leptos::*;
use leptos_meta::*;
use leptos_router::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

// ─── WASM Entry Point ─────────────────────────────────────────────────────────

#[wasm_bindgen(start)]
pub fn main() {
    _ = console_log::init_with_level(log::Level::Debug);
    console_error_panic_hook::set_once();
    leptos::mount_to_body(|| view! { <App/> });
}

pub mod components;
pub mod pages;

// ─── API Types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    pub id: String,
    pub name: String,
    pub pr_count: u32,
    #[serde(default)]
    pub avg_f1: Option<f64>,
    #[serde(default)]
    pub avg_precision: Option<f64>,
    #[serde(default)]
    pub avg_recall: Option<f64>,
    #[serde(default)]
    pub total_cost: Option<f64>,
    #[serde(default)]
    pub duration_secs: Option<f64>,
    #[serde(default)]
    pub model: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunDetail {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub results: Vec<PrResult>,
    #[serde(default)]
    pub aggregate: Option<AggregateMetrics>,
    #[serde(default)]
    pub total_cost: Option<f64>,
    #[serde(default)]
    pub duration_secs: Option<f64>,
    pub model: String,
    pub status: String,
    #[serde(default)]
    pub config: Option<RunConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrResult {
    pub pr_number: u32,
    pub title: String,
    #[serde(default)]
    pub f1: Option<f64>,
    #[serde(default)]
    pub precision: Option<f64>,
    #[serde(default)]
    pub recall: Option<f64>,
    #[serde(default)]
    pub cost: Option<f64>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateMetrics {
    pub avg_f1: f64,
    pub avg_precision: f64,
    pub avg_recall: f64,
    pub total_cost: f64,
    pub total_prs: u32,
    pub duration_secs: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunConfig {
    pub model: String,
    pub dataset: String,
    pub roles: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewRunRequest {
    pub model: String,
    pub dataset: String,
    pub roles: Vec<String>,
    #[serde(default)]
    pub pr_filter: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewRunResponse {
    pub run_id: String,
    pub status: String,
    pub total_prs: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub datasets: Vec<String>,
    #[serde(default)]
    pub roles: Vec<String>,
    /// Whether reduce-diff mode is enabled (compile-time feature flag).
    #[serde(default)]
    pub reduce_diff_enabled: bool,
}

/// Per-dataset config loaded from dataset.toml
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DatasetConfig {
    #[serde(default)]
    pub defaults: DatasetDefaults,
}

/// Default values that auto-fill the New Run form when a dataset is selected.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DatasetDefaults {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub pr_filter: Option<String>,
    #[serde(default)]
    pub concurrency: Option<usize>,
    #[serde(default)]
    pub max_findings: Option<usize>,
    #[serde(default)]
    pub roles: Option<String>,
}

/// Dataset info from GET /api/config/datasets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetInfo {
    pub id: String,
    pub path: String,
    pub pr_count: usize,
    #[serde(default)]
    pub config: Option<DatasetConfig>,
}

/// A single PR entry returned by GET /api/datasets/:id/prs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrEntry {
    pub key: String,
    pub url: String,
    pub title: String,
    pub repo: String,
    pub pr_number: u32,
}

// ─── Role display utilities ──────────────────────────────────────────

/// Map a role abbreviation (SA/CL/AR/SEC) to a human-readable display name.
pub fn role_display_name(role: &str) -> &'static str {
    match role {
        "SA" => "Security Auditor",
        "CL" => "Code Logician",
        "AR" => "Architecture Reviewer",
        "SEC" => "Security Evaluator",
        _ => "Unknown",
    }
}

/// The canonical agent role identifiers, matching crb-agents::AGENT_ROLES.
pub const AGENT_ROLES: &[&str] = &["SA", "CL", "AR", "SEC"];

/// A DashboardEvent as serialized by the backend SSE endpoint.
/// Uses the same tagged-enum format (`event`/`data`) as the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "data")]
pub enum DashboardEvent {
    /// An agent has started its review for a given PR.
    #[serde(rename = "agent_started")]
    AgentStarted {
        pr_key: String,
        role: String,
    },
    /// A chunk of streaming response text from an agent.
    #[serde(rename = "agent_chunk")]
    AgentChunk {
        role: String,
        chunk: String,
    },
    /// An agent has finished its review.
    #[serde(rename = "agent_finished")]
    AgentFinished {
        role: String,
        findings: usize,
        success: bool,
    },
    /// A single PR has been fully evaluated.
    #[serde(rename = "pr_completed")]
    PrCompleted {
        pr_key: String,
    },
    /// Progress update during a run.
    #[serde(rename = "run_progress")]
    RunProgress {
        completed_prs: usize,
        total_prs: usize,
        current_pr: Option<String>,
    },
    /// The entire run has finished.
    #[serde(rename = "run_finished")]
    RunFinished {
        total_prs: usize,
    },
}

// ─── Log / Replay Types ─────────────────────────────────────────────┐

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogsListResponse {
    pub run_id: String,
    pub cache_available: bool,
    pub prs: Vec<PrLogsEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrLogsEntry {
    pub pr_key: String,
    pub pr_title: String,
    pub agents: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentLogResponse {
    pub run_id: String,
    pub pr_key: String,
    pub role: String,
    pub prompt: Option<String>,
    pub response: Option<String>,
    pub reasoning: Option<String>,
    pub available: bool,
}

/// Frontend type matching backend PrAgentsResponse
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrAgentsResponse {
    pub run_id: String,
    pub pr_key: String,
    pub pr_title: String,
    pub agents: Vec<PrAgentEntry>,
    pub has_output: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrAgentEntry {
    pub role: String,
    pub has_prompt: bool,
    pub has_response: bool,
    pub has_reasoning: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayStatusResponse {
    pub run_id: String,
    pub status: String,
    pub progress_pct: u32,
    pub completed_prs: u32,
    pub total_prs: u32,
    pub message: String,
}

/// Response from POST /api/runs/:id/convert
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvertStats {
    pub run_id: String,
    pub pr_count: usize,
    pub finding_count: usize,
    pub candidates_path: String,
}

/// Result from POST /api/runs/:id/judge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeResult {
    pub success: bool,
    pub message: String,
    pub stdout: String,
    pub stderr: String,
}

/// Per-PR detail response with verdicts and cost breakdown
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrDetailResponse {
    pub run_id: String,
    pub pr_title: String,
    pub url: String,
    pub findings_count: usize,
    pub golden_count: usize,
    /// PR-level metrics (true_positives, false_positives, etc.) matching backend MetricsJson
    pub metrics: PrDetailMetrics,
    pub verdicts: Vec<VerdictDetail>,
    pub cost: Option<PrCostDetail>,
    /// Raw findings JSON from agents
    #[serde(default)]
    pub findings: serde_json::Value,
    /// Raw agent response texts
    #[serde(default)]
    pub agent_responses: Vec<String>,
}

/// PR-level metrics matching backend `MetricsJson` — not the same as AggregateMetrics.
/// The backend returns `metrics` with these exact field names from the per-PR result file.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrDetailMetrics {
    #[serde(default)]
    pub true_positives: usize,
    #[serde(default)]
    pub false_positives: usize,
    #[serde(default)]
    pub false_negatives: usize,
    #[serde(default)]
    pub precision: f64,
    #[serde(default)]
    pub recall: f64,
    #[serde(default)]
    pub f1: f64,
}

/// A single judge verdict
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerdictDetail {
    #[serde(default)]
    pub reasoning: String,
    #[serde(default, rename = "match")]
    pub match_: bool,
    #[serde(default)]
    pub confidence: f64,
}

/// Cost breakdown for a single PR
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrCostDetail {
    #[serde(default)]
    pub total_usd: f64,
    #[serde(default)]
    pub agent_tokens_in: u64,
    #[serde(default)]
    pub agent_tokens_out: u64,
    #[serde(default)]
    pub judge_tokens_in: u64,
    #[serde(default)]
    pub judge_tokens_out: u64,
    #[serde(default)]
    pub agent_call_count: u64,
    #[serde(default)]
    pub judge_call_count: u64,
}

// ─── Helper: Build API URL ───────────────────────────────────────────────────

pub fn api_url(path: &str) -> String {
    // Use a relative URL so it works regardless of port/proxy
    path.to_string()
}

// ─── App Root ────────────────────────────────────────────────────────────────

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Html attr:lang="en" attr:dir="ltr" />
        <Router>
            <div class="app-shell">
                <Sidebar />
                <main class="main-content">
                    <div class="content-container">
                        <Routes>
                            <Route path="/" view=|| view! { <pages::home::HomePage /> } />
                            <Route path="/runs/:id" view=|| view! { <pages::run_detail::RunDetailPage /> } />
                            <Route path="/runs/:id/prs/:pr_key" view=|| view! { <pages::pr_detail::PrDetailPage /> } />
                            <Route path="/runs/:id/live" view=|| view! { <pages::live::LivePage /> } />
                            <Route path="/new" view=|| view! { <pages::new_run::NewRunPage /> } />
                            <Route path="/*" view=|| view! {
                                <div class="state-container">
                                    <h2>"404 — Page Not Found"</h2>
                                    <p>"The page you're looking for doesn't exist."</p>
                                    <div class="error-state__action">
                                        <a href="/" class="btn btn--primary">"Go Home"</a>
                                    </div>
                                </div>
                            } />
                        </Routes>
                    </div>
                </main>
            </div>
        </Router>
    }
}

// ─── Sidebar ──────────────────────────────────────────────────────────────────

#[component]
fn Sidebar() -> impl IntoView {
    let (collapsed, set_collapsed) = create_signal(false);
    let (mobile_open, set_mobile_open) = create_signal(false);

    let active_class = move |path: &str| -> &'static str {
        let loc = use_location();
        if loc.pathname.get().starts_with(path) { "sidebar__item--active" } else { "" }
    };

    let toggle_collapsed = move |_| {
        set_collapsed.update(|v| *v = !*v);
    };

    let toggle_mobile = move |_| {
        set_mobile_open.update(|v| *v = !*v);
    };

    let sidebar_class = move || {
        let mut cls = "sidebar".to_string();
        if collapsed.get() { cls.push_str(" sidebar--collapsed"); }
        if mobile_open.get() { cls.push_str(" sidebar--mobile-open"); }
        cls
    };

    view! {
        // Mobile hamburger button (visible on small screens)
        <button
            class="btn btn--ghost sidebar__toggle"
            style="position: fixed; top: 12px; left: 12px; z-index: 200; display: none;"
            aria-label="Toggle navigation menu"
            on:click=toggle_mobile
        >
            "☰"
        </button>

        // Mobile overlay backdrop
        {move || {
            if mobile_open.get() {
                view! {
                    <div class="sidebar-overlay sidebar-overlay--open" on:click=move |_| set_mobile_open.set(false)></div>
                }.into_view()
            } else {
                view! { <span></span> }.into_view()
            }
        }}

        <nav class=sidebar_class aria-label="Main navigation">
            <div class="sidebar__header">
                <button class="sidebar__toggle" on:click=toggle_collapsed aria-label="Toggle sidebar">
                    "☰"
                </button>
                <span class="sidebar__brand">"Review Harness"</span>
            </div>

            <ul class="sidebar__nav">
                <li>
                    <a href="/" class=move || format!("sidebar__item {}", active_class("/runs/"))>
                        <span class="sidebar__icon">"🏠"</span>
                        <span class="sidebar__label">"Home"</span>
                    </a>
                </li>
                <li>
                    <a href="/new" class=move || format!("sidebar__item {}", active_class("/new"))>
                        <span class="sidebar__icon">"🆕"</span>
                        <span class="sidebar__label">"New Run"</span>
                    </a>
                </li>
            </ul>

            <div class="sidebar__footer">
                <span class="sidebar__version">"v0.1.0"</span>
            </div>
        </nav>
    }
}
