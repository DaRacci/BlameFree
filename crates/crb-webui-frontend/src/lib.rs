use crb_webui_shared::config::RoleInfo;
use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

mod app;
pub mod components;
pub mod pages;
pub mod sse;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewRunRequest {
    pub model: String,
    pub dataset: String,
    pub roles: Vec<String>,

    #[serde(default)]
    pub pr_filter: Option<String>,

    #[serde(default = "default_true")]
    pub use_cache: bool,

    /// Reasoning effort: None (disabled) or Some("low"/"medium"/"high").
    #[serde(default)]
    pub reasoning_effort: Option<String>,
}

fn default_true() -> bool {
    true
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
    pub roles: Vec<RoleInfo>,

    /// Whether reduce-diff mode is enabled (compile-time feature flag).
    #[serde(default)]
    pub reduce_diff_enabled: bool,

    /// Whether OAuth authentication is configured server-side.
    #[serde(default)]
    pub auth_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "event", content = "data")]
pub enum DashboardEvent {
    /// An agent has started its review for a given PR.
    AgentStarted { pr_key: String, role: String },

    /// A chunk of streaming response text from an agent.
    AgentChunk { role: String, chunk: String },

    /// An agent has finished its review.
    AgentFinished {
        role: String,
        findings: usize,
        success: bool,
    },

    /// A single PR has been fully evaluated.
    PrCompleted { pr_key: String },

    /// Progress update during a run.
    RunProgress {
        completed_prs: usize,
        total_prs: usize,
        current_pr: Option<String>,
    },

    /// The entire run has finished.
    RunFinished { total_prs: usize },
}

/// Map a role abbreviation to a human-readable display name.
#[deprecated(note = "Use RoleInfo::display_name() instead, which is more robust and configurable.")]
fn role_display_name(role: &str) -> String {
    match role {
        "SA" => "Security Auditor (SA)".to_string(),
        "CL" => "Code Logician (CL)".to_string(),
        "AR" | "ARCH" => "Architecture Reviewer (ARCH)".to_string(),
        "SEC" => "Security Evaluator (SEC)".to_string(),
        _ => role.to_string(),
    }
}

async fn fetch_json<T: serde::de::DeserializeOwned>(url: &str) -> Result<T, String> {
    let response = Request::get(url)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !response.ok() {
        return Err(format!("Server returned {}", response.status()));
    }

    let data: T = response
        .json()
        .await
        .map_err(|e| format!("Parse error: {}", e))?;

    Ok(data)
}
