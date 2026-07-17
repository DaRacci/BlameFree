use crb_types::benchmark::metrics::Metrics;
use crb_types::benchmark::{judge::JudgeVerdict, result::PrResult};
use crb_types::cost::AnalyticsSnapshot;
use crb_types::vcs::pr::PrMeta;
use crb_types::wrappers::Model;
use serde::{Deserialize, Serialize};
use strum::{Display, IntoStaticStr};

use crate::config::RoleInfo;

/// Shared metadata fields used by both [`RunSummary`] and [`RunDetail`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMeta {
    /// Unique run identifier.
    pub id: String,

    /// Human-readable run name.
    pub name: String,

    /// Number of PRs in this run.
    #[serde(default)]
    pub pr_count: usize,

    /// Total cost in USD.
    #[serde(default)]
    pub total_cost: Option<f64>,

    /// Total tokens consumed.
    #[serde(default)]
    #[deprecated = "Use [`crb_types::cost::AnalyticsSnapshot`]"]
    pub total_tokens: usize,

    /// Duration in seconds.
    #[serde(default)]
    pub duration_secs: Option<f64>,

    /// Model used for evaluation.
    #[serde(default)]
    pub model: Option<Model>,

    /// The state of the run.
    pub status: RunStatus,
}

/// Summary of a past benchmark run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    /// Shared run metadata
    pub meta: RunMeta,

    /// Aggregate metrics across all PRs.
    pub metrics: Metrics,

    /// Average F1 score, if computed.
    #[serde(default)]
    #[deprecated = "Use [`crb_types::benchmark::metrics::Metrics`]"]
    pub avg_f1: Option<f64>,

    /// Average precision, if computed.
    #[serde(default)]
    #[deprecated = "Use [`crb_types::benchmark::metrics::Metrics`]"]
    pub avg_precision: Option<f64>,

    /// Average recall, if computed.
    #[serde(default)]
    #[deprecated = "Use [`crb_types::benchmark::metrics::Metrics`]"]
    pub avg_recall: Option<f64>,

    // TODO: Convert to time type
    /// ISO-8601 timestamp of creation.
    #[serde(default)]
    pub created_at: String,
}

#[derive(
    Display, IntoStaticStr, Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord,
)]
pub enum RunStatus {
    Pending,
    Running,
    Failed,
    Completed,
    Cancelled,
}

/// Detailed run result with per-PR data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunDetail {
    /// Shared run metadata
    pub meta: RunMeta,

    /// Per-PR results.
    #[serde(default)]
    pub results: Vec<PrResultRow>,

    /// Aggregate metrics across all PRs.
    #[serde(default)]
    pub aggregate: Metrics,

    /// Run configuration.
    #[serde(default)]
    pub config: Option<RunConfig>,
}

/// A single PR result in the API response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrResultRow {
    pub meta: PrMeta,

    /// PR number.
    #[deprecated = "Use [`crb_types::vcs::pr::PrMeta`]"]
    pub pr_number: u32,

    /// PR key (e.g. "owner/repo/pull/N").
    #[deprecated]
    pub pr_key: String,

    /// PR title.
    #[deprecated = "Use [`crb_types::vcs::pr::PrMeta`]"]
    pub title: String,

    /// F1 score, if computed.
    #[serde(default)]
    #[deprecated = "Use [`crb_types::benchmark::metrics::Metrics`]"]
    pub f1: Option<f64>,

    /// Precision score, if computed.
    #[serde(default)]
    #[deprecated = "Use [`crb_types::benchmark::metrics::Metrics`]"]
    pub precision: Option<f64>,

    /// Recall score, if computed.
    #[serde(default)]
    #[deprecated = "Use [`crb_types::benchmark::metrics::Metrics`]"]
    pub recall: Option<f64>,

    pub metrics: Metrics,

    pub analytics: AnalyticsSnapshot,

    /// Cost in USD.
    #[serde(default)]
    #[deprecated = "Use [`crb_types::cost::AnalyticsSnapshot`]"]
    pub cost: Option<f64>,

    /// Status.
    #[serde(default)]
    pub status: Option<RunStatus>,

    /// Whether this PR has agent data available.
    #[serde(default)]
    pub has_agents: bool,
}

/// Response returned when a benchmark run is started.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartRunResponse {
    pub run_id: String,
    pub status: String,
    pub total_prs: u32,
}

/// Run config returned in the run detail response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunConfig {
    /// Model used for the run.
    pub model: String,

    /// Dataset identifier.
    pub dataset: String,

    /// Reviewer roles.
    pub roles: Vec<RoleInfo>,
}

/// Response from GET /api/runs/:id/logs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogsListResponse {
    /// Run ID for this log response.
    pub run_id: String,

    /// Per-PR log entries.
    pub prs: Vec<PrLogsEntry>,
}

/// A single PR's available log entries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrLogsEntry {
    /// PR Details.
    pub meta: PrMeta,

    /// Agent roles available for this PR.
    pub agents: Vec<RoleInfo>,
}

/// Response from GET /api/runs/:id/logs/:pr_key/:role
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentLogResponse {
    /// Run ID.
    pub run_id: String,

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

/// Common payload shared between [`PrDetailResponse`] and the on-disk result format ([`crb_reporting::PrResult`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrResultPayload {
    /// PR title.
    pub pr_title: String,

    /// PR URL.
    pub url: String,

    /// Number of findings.
    #[serde(default)]
    pub findings_count: usize,

    /// Number of golden comments.
    #[serde(default)]
    pub golden_count: usize,

    /// Evaluation metrics.
    #[serde(default)]
    pub metrics: Metrics,

    /// Judge verdicts for each finding-vs-golden comparison.
    #[serde(default)]
    pub verdicts: Vec<JudgeVerdict>,

    /// Cost data for this PR.
    #[serde(default)]
    pub cost: Option<AnalyticsSnapshot>,
}

/// Detailed per-PR response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrDetailResponse {
    /// Shared payload fields.
    pub payload: PrResult,

    /// Raw agent response texts.
    #[serde(default)]
    pub agent_responses: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_summary_default_fields() {
        let json = r#"{"id":"r1","name":"test","status":"Running"}"#;
        let summary: RunSummary = serde_json::from_str(json).unwrap();
        insta::assert_debug_snapshot!(summary);
    }

    #[test]
    fn test_pr_result_default_fields() {
        let json = r#"{"pr_number":1,"pr_key":"a/b/pull/1","title":"T"}"#;
        let result: PrResultRow = serde_json::from_str(json).unwrap();
        insta::assert_debug_snapshot!(result);
    }

    #[test]
    fn test_pr_detail_response_default_findings() {
        let json = r#"{"run_id":"r1","pr_title":"Test","url":"https://example.com","findings_count":0,"golden_count":0,"metrics":{"true_positives":0,"false_positives":0,"false_negatives":0,"duration_secs":0.0},"verdicts":[],"cost":null}"#;
        let detail: PrDetailResponse = serde_json::from_str(json).unwrap();
        insta::assert_debug_snapshot!(detail);
    }
}
