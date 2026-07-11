use serde::{Deserialize, Serialize};

use crate::config::RoleInfo;

/// Summary of a past benchmark run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    /// Unique run identifier.
    pub id: String,

    /// Human-readable run name.
    pub name: String,

    /// Number of PRs in this run.
    #[serde(default)]
    pub pr_count: u32,

    /// Average F1 score, if computed.
    #[serde(default)]
    pub avg_f1: Option<f64>,

    /// Average precision, if computed.
    #[serde(default)]
    pub avg_precision: Option<f64>,

    /// Average recall, if computed.
    #[serde(default)]
    pub avg_recall: Option<f64>,

    /// Total cost in USD.
    #[serde(default)]
    pub total_cost: Option<f64>,

    /// Total tokens consumed.
    #[serde(default)]
    pub total_tokens: usize,

    /// Duration in seconds.
    #[serde(default)]
    pub duration_secs: Option<f64>,

    // TODO: Convert to time type
    /// ISO-8601 timestamp of creation.
    #[serde(default)]
    pub created_at: String,

    /// Model used for evaluation.
    #[serde(default)]
    pub model: Option<String>,

    // TODO: Convert to enum
    /// Run status (e.g. "running", "completed", "failed").
    pub status: String,
}

/// Detailed run result with per-PR data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunDetail {
    /// Unique run identifier.
    pub id: String,
    /// Human-readable run name.
    pub name: String,
    /// Number of PRs in this run.
    #[serde(default)]
    pub pr_count: usize,
    /// Per-PR results.
    #[serde(default)]
    pub results: Vec<PrResult>,
    /// Aggregate metrics across all PRs.
    #[serde(default)]
    pub aggregate: Option<AggregateMetrics>,
    /// Total cost in USD.
    #[serde(default)]
    pub total_cost: Option<f64>,
    /// Total tokens consumed.
    #[serde(default)]
    pub total_tokens: usize,
    /// Duration in seconds.
    #[serde(default)]
    pub duration_secs: Option<f64>,
    /// Model used for evaluation.
    pub model: String,
    /// Run status.
    pub status: String,
    /// Run configuration.
    #[serde(default)]
    pub config: Option<RunConfig>,
}

/// A single PR result in the API response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrResult {
    /// PR number.
    pub pr_number: u32,
    /// PR key (e.g. "owner/repo/pull/N").
    pub pr_key: String,
    /// PR title.
    pub title: String,
    /// F1 score, if computed.
    #[serde(default)]
    pub f1: Option<f64>,
    /// Precision score, if computed.
    #[serde(default)]
    pub precision: Option<f64>,
    /// Recall score, if computed.
    #[serde(default)]
    pub recall: Option<f64>,
    /// Cost in USD.
    #[serde(default)]
    pub cost: Option<f64>,
    /// Status string.
    #[serde(default)]
    pub status: Option<String>,
    /// Whether this PR has agent data available.
    #[serde(default)]
    pub has_agents: bool,
}

/// Aggregate metrics across all PRs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AggregateMetrics {
    /// Average F1 score across all evaluated PRs.
    pub avg_f1: f64,
    /// Average precision across all evaluated PRs.
    pub avg_precision: f64,
    /// Average recall across all evaluated PRs.
    pub avg_recall: f64,
    /// Total true positives.
    #[serde(default)]
    pub total_tp: usize,
    /// Total false positives.
    #[serde(default)]
    pub total_fp: usize,
    /// Total false negatives.
    #[serde(default)]
    pub total_fn: usize,
    /// Total cost in USD.
    #[serde(default)]
    pub total_cost: f64,
    /// Total number of PRs evaluated.
    #[serde(default)]
    pub total_prs: u32,
    /// Duration of the run in seconds.
    #[serde(default)]
    pub duration_secs: f64,
}

/// Run config returned in the run detail response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunConfig {
    /// Model used for the run.
    pub model: String,
    /// Dataset identifier.
    pub dataset: String,
    /// Reviewer roles.
    pub roles: Vec<String>,
}

/// A single JSON result file on disk (for per-PR data).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CostJson {
    /// Total cost in USD.
    #[serde(default)]
    pub total_usd: f64,
    /// Agent input tokens consumed.
    #[serde(default)]
    pub agent_tokens_in: u64,
    /// Agent output tokens produced.
    #[serde(default)]
    pub agent_tokens_out: u64,
    /// Judge input tokens consumed.
    #[serde(default)]
    pub judge_tokens_in: u64,
    /// Judge output tokens produced.
    #[serde(default)]
    pub judge_tokens_out: u64,
    /// Number of agent API calls.
    #[serde(default)]
    pub agent_call_count: u64,
    /// Number of judge API calls.
    #[serde(default)]
    pub judge_call_count: u64,
}

/// Metrics embedded in per-PR result JSON files.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MetricsJson {
    /// True positives count.
    #[serde(default)]
    pub true_positives: usize,
    /// False positives count.
    #[serde(default)]
    pub false_positives: usize,
    /// False negatives count.
    #[serde(default)]
    pub false_negatives: usize,
    /// Precision score.
    #[serde(default)]
    pub precision: f64,
    /// Recall score.
    #[serde(default)]
    pub recall: f64,
    /// F1 score.
    #[serde(default)]
    pub f1: f64,
}

/// A single judge verdict embedded in per-PR result JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerdictJson {
    /// Reasoning text from the judge.
    #[serde(default)]
    pub reasoning: String,
    /// Whether the finding matched the golden comment.
    #[serde(default, rename = "match")]
    pub match_: bool,
    /// Confidence score for the judgment (0.0–1.0).
    #[serde(default)]
    pub confidence: f64,
}

/// Response from GET /api/runs/:id/logs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogsListResponse {
    /// Run ID for this log response.
    pub run_id: String,
    /// Whether cache data is available for this run.
    pub cache_available: bool,
    /// Per-PR log entries.
    pub prs: Vec<PrLogsEntry>,
}

/// A single PR's available log entries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrLogsEntry {
    /// PR key (e.g. "owner/repo/pull/N").
    pub pr_key: String,
    /// PR title.
    pub pr_title: String,
    /// Agent roles available for this PR.
    pub agents: Vec<RoleInfo>,
}

/// Response from GET /api/runs/:id/logs/:pr_key/:role
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentLogResponse {
    /// Run ID.
    pub run_id: String,
    /// PR key.
    pub pr_key: String,
    /// Agent role abbreviation.
    pub role: String,
    /// The prompt sent to the agent, if available.
    pub prompt: Option<String>,
    /// The agent's response, if available.
    pub response: Option<String>,
    /// Reasoning text, if available.
    pub reasoning: Option<String>,
    /// Whether this log entry is accessible.
    pub available: bool,
}

/// Response from GET /api/runs/:id/prs/:pr_key
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrAgentsResponse {
    /// Run ID.
    pub run_id: String,
    /// PR key.
    pub pr_key: String,
    /// PR title.
    pub pr_title: String,
    /// Per-agent availability list.
    pub agents: Vec<PrAgentEntry>,
    /// Whether any agent output exists.
    pub has_output: bool,
}

/// Per-agent availability entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrAgentEntry {
    /// Role abbreviation.
    pub role: String,
    /// Whether a prompt is available for this agent.
    pub has_prompt: bool,
    /// Whether a response is available for this agent.
    pub has_response: bool,
    /// Whether reasoning text is available for this agent.
    pub has_reasoning: bool,
}

/// Detailed per-PR response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrDetailResponse {
    /// Run ID.
    pub run_id: String,
    /// PR title.
    pub pr_title: String,
    /// PR URL.
    pub url: String,
    /// Number of findings.
    pub findings_count: usize,
    /// Number of golden comments.
    pub golden_count: usize,
    /// Evaluation metrics.
    pub metrics: MetricsJson,
    /// Judge verdicts for each finding-vs-golden comparison.
    pub verdicts: Vec<VerdictJson>,
    /// Cost data for this PR.
    pub cost: Option<CostJson>,
    /// Raw findings data.
    #[serde(default)]
    pub findings: serde_json::Value,
    /// Raw agent response texts.
    #[serde(default)]
    pub agent_responses: Vec<String>,
}
