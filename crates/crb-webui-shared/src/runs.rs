use serde::{Deserialize, Serialize};

use crate::config::RoleInfo;

/// Summary of a past benchmark run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    pub id: String,
    pub name: String,
    #[serde(default)]
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
    pub total_tokens: usize,
    #[serde(default)]
    pub duration_secs: Option<f64>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub model: Option<String>,
    pub status: String,
}

/// Detailed run result with per-PR data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunDetail {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub pr_count: usize,
    #[serde(default)]
    pub results: Vec<PrResult>,
    #[serde(default)]
    pub aggregate: Option<AggregateMetrics>,
    #[serde(default)]
    pub total_cost: Option<f64>,
    #[serde(default)]
    pub total_tokens: usize,
    #[serde(default)]
    pub duration_secs: Option<f64>,
    pub model: String,
    pub status: String,
    #[serde(default)]
    pub config: Option<RunConfig>,
}

/// A single PR result in the API response (maps to frontend `PrResult`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrResult {
    pub pr_number: u32,
    pub pr_key: String,
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
    #[serde(default)]
    pub has_agents: bool,
}

/// Aggregate metrics across all PRs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AggregateMetrics {
    pub avg_f1: f64,
    pub avg_precision: f64,
    pub avg_recall: f64,
    #[serde(default)]
    pub total_tp: usize,
    #[serde(default)]
    pub total_fp: usize,
    #[serde(default)]
    pub total_fn: usize,
    #[serde(default)]
    pub total_cost: f64,
    #[serde(default)]
    pub total_prs: u32,
    #[serde(default)]
    pub duration_secs: f64,
}

/// Run config returned in the run detail response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunConfig {
    pub model: String,
    pub dataset: String,
    pub roles: Vec<String>,
}

/// A single JSON result file on disk (for per-PR data).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CostJson {
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

/// Metrics embedded in per-PR result JSON files.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MetricsJson {
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

/// A single judge verdict embedded in per-PR result JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerdictJson {
    #[serde(default)]
    pub reasoning: String,
    #[serde(default, rename = "match")]
    pub match_: bool,
    #[serde(default)]
    pub confidence: f64,
}

/// Response from GET /api/runs/:id/logs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogsListResponse {
    pub run_id: String,
    pub cache_available: bool,
    pub prs: Vec<PrLogsEntry>,
}

/// A single PR's available log entries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrLogsEntry {
    pub pr_key: String,
    pub pr_title: String,
    pub agents: Vec<RoleInfo>,
}

/// Response from GET /api/runs/:id/logs/:pr_key/:role
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

/// Response from GET /api/runs/:id/prs/:pr_key
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrAgentsResponse {
    pub run_id: String,
    pub pr_key: String,
    pub pr_title: String,
    pub agents: Vec<PrAgentEntry>,
    pub has_output: bool,
}

/// Per-agent availability entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrAgentEntry {
    pub role: String,
    pub has_prompt: bool,
    pub has_response: bool,
    pub has_reasoning: bool,
}

/// Detailed per-PR response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrDetailResponse {
    pub run_id: String,
    pub pr_title: String,
    pub url: String,
    pub findings_count: usize,
    pub golden_count: usize,
    pub metrics: MetricsJson,
    pub verdicts: Vec<VerdictJson>,
    pub cost: Option<CostJson>,
    #[serde(default)]
    pub findings: serde_json::Value,
    #[serde(default)]
    pub agent_responses: Vec<String>,
}
