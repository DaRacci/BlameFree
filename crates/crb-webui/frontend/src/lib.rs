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
}

/// Dataset info from GET /api/config/datasets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetInfo {
    pub id: String,
    pub path: String,
    pub pr_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEvent {
    pub agent: String,
    pub status: String,
    #[serde(default)]
    pub response: Option<String>,
    #[serde(default)]
    pub pr_number: Option<u32>,
    #[serde(default)]
    pub progress: Option<u32>,
    #[serde(default)]
    pub total: Option<u32>,
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
    pub available: bool,
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
            <NavBar />
            <main>
                <Routes>
                    <Route path="/" view=|| view! { <pages::home::HomePage /> } />
                    <Route path="/runs/:id" view=|| view! { <pages::run_detail::RunDetailPage /> } />
                    <Route path="/runs/:id/live" view=|| view! { <pages::live::LivePage /> } />
                    <Route path="/new" view=|| view! { <pages::new_run::NewRunPage /> } />
                    <Route path="/*" view=|| view! {
                        <div class="container">
                            <h2>"404 — Page Not Found"</h2>
                            <p>"The page you're looking for doesn't exist."</p>
                            <a href="/" class="btn btn-primary">"Go Home"</a>
                        </div>
                    } />
                </Routes>
            </main>
        </Router>
    }
}

// ─── NavBar ──────────────────────────────────────────────────────────────────

#[component]
fn NavBar() -> impl IntoView {
    let active_class = move |path: &str| -> &'static str {
        let loc = use_location();
        if loc.pathname.get().starts_with(path) { "active" } else { "" }
    };

    view! {
        <nav class="navbar">
            <a href="/" class="brand">"Review Harness"</a>
            <a href="/" class=active_class("/")>"Dashboard"</a>
            <a href="/new" class=active_class("/new")>"New Run"</a>
        </nav>
    }
}
