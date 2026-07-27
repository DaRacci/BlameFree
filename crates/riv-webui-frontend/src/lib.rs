use riv_types::capabilities::ReasoningEffort;
pub use riv_webui_shared::config::AppConfig;
// Local response type — JSON-compatible with the old StartRunResponse format.
// The API has not yet migrated to return Review directly.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[deprecated]
pub struct NewRunResponse {
    pub run_id: String,
    pub status: String,
    pub total_prs: u32,
}
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

    /// Reasoning effort: None (disabled), or Some(ReasoningEffort)
    #[serde(default)]
    pub reasoning_effort: Option<ReasoningEffort>,

    /// Judge model for evaluating findings against goldens.
    #[serde(default = "riv_shared::default_model")]
    pub judge_model: String,

    /// Maximum findings per agent per PR.
    #[serde(default = "default_max_findings")]
    pub max_findings: usize,
}

fn default_max_findings() -> usize {
    20
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
