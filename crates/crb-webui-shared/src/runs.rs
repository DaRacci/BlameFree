use crb_types::benchmark::Metrics;
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
    pub aggregate: Option<Metrics>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RoleInfo;
    use crb_types::benchmark::Metrics;

    // ── RunSummary ────────────────────────────────────────────────────────

    #[test]
    fn test_run_summary_serde_roundtrip() {
        let orig = RunSummary {
            id: "run-001".into(),
            name: "Benchmark v2 run".into(),
            pr_count: 42,
            avg_f1: Some(0.85),
            avg_precision: Some(0.90),
            avg_recall: Some(0.80),
            total_cost: Some(12.50),
            total_tokens: 150000,
            duration_secs: Some(3600.0),
            created_at: "2024-03-01T12:00:00Z".into(),
            model: Some("gpt-4o".into()),
            status: "completed".into(),
        };
        let json = serde_json::to_string(&orig).unwrap();
        let deserialized: RunSummary = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&orig);
        let _ = deserialized;
    }

    #[test]
    fn test_run_summary_default_fields() {
        let json = r#"{"id":"r1","name":"test","status":"running"}"#;
        let summary: RunSummary = serde_json::from_str(json).unwrap();
        insta::assert_debug_snapshot!(summary);
    }

    // ── RunDetail ─────────────────────────────────────────────────────────

    #[test]
    fn test_run_detail_serde_roundtrip() {
        let orig = RunDetail {
            id: "run-002".into(),
            name: "Evaluation run".into(),
            pr_count: 10,
            results: vec![PrResult {
                pr_number: 1,
                pr_key: "owner/repo/pull/1".into(),
                title: "Fix bug".into(),
                f1: Some(0.75),
                precision: Some(0.80),
                recall: Some(0.70),
                cost: Some(0.05),
                status: Some("completed".into()),
                has_agents: true,
            }],
            aggregate: Some(Metrics {
                true_positives: 10,
                false_positives: 2,
                false_negatives: 3,
                duration_secs: 120.0,
            }),
            total_cost: Some(0.50),
            total_tokens: 5000,
            duration_secs: Some(120.0),
            model: "gpt-4o-mini".into(),
            status: "completed".into(),
            config: None,
        };
        let json = serde_json::to_string(&orig).unwrap();
        let deserialized: RunDetail = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&orig);
        let _ = deserialized;
    }

    #[test]
    fn test_run_detail_with_config() {
        let config = RunConfig {
            model: "gpt-4o".into(),
            dataset: "benchmark-v2".into(),
            roles: vec!["FE".into(), "BE".into()],
        };
        let orig = RunDetail {
            id: "run-003".into(),
            name: "Config test".into(),
            pr_count: 5,
            results: vec![],
            aggregate: None,
            total_cost: None,
            total_tokens: 0,
            duration_secs: None,
            model: "gpt-4o".into(),
            status: "pending".into(),
            config: Some(config),
        };
        let json = serde_json::to_string(&orig).unwrap();
        let deserialized: RunDetail = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&orig);
        let _ = deserialized;
    }

    // ── PrResult ──────────────────────────────────────────────────────────

    #[test]
    fn test_pr_result_serde_roundtrip() {
        let orig = PrResult {
            pr_number: 42,
            pr_key: "owner/repo/pull/42".into(),
            title: "Add feature".into(),
            f1: Some(0.92),
            precision: Some(0.95),
            recall: Some(0.89),
            cost: Some(0.12),
            status: Some("completed".into()),
            has_agents: true,
        };
        let json = serde_json::to_string(&orig).unwrap();
        let deserialized: PrResult = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&orig);
        let _ = deserialized;
    }

    #[test]
    fn test_pr_result_default_fields() {
        let json = r#"{"pr_number":1,"pr_key":"a/b/pull/1","title":"T"}"#;
        let result: PrResult = serde_json::from_str(json).unwrap();
        insta::assert_debug_snapshot!(result);
    }

    // ── RunConfig ─────────────────────────────────────────────────────────

    #[test]
    fn test_run_config_serde_roundtrip() {
        let orig = RunConfig {
            model: "claude-3.5-sonnet".into(),
            dataset: "security-bench".into(),
            roles: vec!["SEC".into(), "INFRA".into()],
        };
        let json = serde_json::to_string(&orig).unwrap();
        let deserialized: RunConfig = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&orig);
        let _ = deserialized;
    }

    // ── CostJson ──────────────────────────────────────────────────────────

    #[test]
    fn test_cost_json_serde_roundtrip() {
        let orig = CostJson {
            total_usd: 1.23,
            agent_tokens_in: 1000,
            agent_tokens_out: 500,
            judge_tokens_in: 2000,
            judge_tokens_out: 300,
            agent_call_count: 5,
            judge_call_count: 10,
        };
        let json = serde_json::to_string(&orig).unwrap();
        let deserialized: CostJson = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&orig);
        let _ = deserialized;
    }

    #[test]
    fn test_cost_json_default() {
        let cost = CostJson::default();
        insta::assert_debug_snapshot!(cost);
    }

    #[test]
    fn test_cost_json_empty_json() {
        let json = "{}";
        let cost: CostJson = serde_json::from_str(json).unwrap();
        insta::assert_debug_snapshot!(cost);
    }

    // ── MetricsJson ───────────────────────────────────────────────────────

    #[test]
    fn test_metrics_json_serde_roundtrip() {
        let orig = MetricsJson {
            true_positives: 15,
            false_positives: 3,
            false_negatives: 2,
            precision: 0.8333,
            recall: 0.8824,
            f1: 0.8571,
        };
        let json = serde_json::to_string(&orig).unwrap();
        let deserialized: MetricsJson = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&orig);
        let _ = deserialized;
    }

    #[test]
    fn test_metrics_json_default() {
        let metrics = MetricsJson::default();
        insta::assert_debug_snapshot!(metrics);
    }

    #[test]
    fn test_metrics_json_empty_json() {
        let json = "{}";
        let metrics: MetricsJson = serde_json::from_str(json).unwrap();
        insta::assert_debug_snapshot!(metrics);
    }

    // ── VerdictJson ───────────────────────────────────────────────────────

    #[test]
    fn test_verdict_json_serde_roundtrip() {
        let orig = VerdictJson {
            reasoning: "The finding matches the golden comment.".into(),
            match_: true,
            confidence: 0.95,
        };
        let json = serde_json::to_string(&orig).unwrap();
        let deserialized: VerdictJson = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&orig);
        let _ = deserialized;
    }

    #[test]
    fn test_verdict_json_rename_match() {
        let json = r#"{"reasoning":"ok","match":true,"confidence":0.8}"#;
        let verdict: VerdictJson = serde_json::from_str(json).unwrap();
        insta::assert_debug_snapshot!(verdict);
    }

    #[test]
    fn test_verdict_json_default_fields() {
        let json = r#"{}"#;
        let verdict: VerdictJson = serde_json::from_str(json).unwrap();
        insta::assert_debug_snapshot!(verdict);
    }

    // ── LogsListResponse ──────────────────────────────────────────────────

    #[test]
    fn test_logs_list_response_serde_roundtrip() {
        let role_info = RoleInfo {
            name: "Frontend Engineer".into(),
            abbreviation: "FE".into(),
            incompatible_with_roles: vec![],
        };
        let orig = LogsListResponse {
            run_id: "run-001".into(),
            cache_available: true,
            prs: vec![PrLogsEntry {
                pr_key: "owner/repo/pull/1".into(),
                pr_title: "Fix UI".into(),
                agents: vec![role_info],
            }],
        };
        let json = serde_json::to_string(&orig).unwrap();
        let deserialized: LogsListResponse = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&orig);
        let _ = deserialized;
    }

    // ── PrLogsEntry ───────────────────────────────────────────────────────

    #[test]
    fn test_pr_logs_entry_serde_roundtrip() {
        let orig = PrLogsEntry {
            pr_key: "owner/repo/pull/42".into(),
            pr_title: "Bug fix".into(),
            agents: vec![],
        };
        let json = serde_json::to_string(&orig).unwrap();
        let deserialized: PrLogsEntry = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&orig);
        let _ = deserialized;
    }

    // ── AgentLogResponse ──────────────────────────────────────────────────

    #[test]
    fn test_agent_log_response_serde_roundtrip() {
        let orig = AgentLogResponse {
            run_id: "run-001".into(),
            pr_key: "owner/repo/pull/1".into(),
            role: "FE".into(),
            prompt: Some("Review this PR".into()),
            response: Some("Found 3 issues".into()),
            reasoning: Some("Checked all files".into()),
            available: true,
        };
        let json = serde_json::to_string(&orig).unwrap();
        let deserialized: AgentLogResponse = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&orig);
        let _ = deserialized;
    }

    #[test]
    fn test_agent_log_response_unavailable() {
        let orig = AgentLogResponse {
            run_id: "run-001".into(),
            pr_key: "owner/repo/pull/2".into(),
            role: "BE".into(),
            prompt: None,
            response: None,
            reasoning: None,
            available: false,
        };
        let json = serde_json::to_string(&orig).unwrap();
        let deserialized: AgentLogResponse = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&orig);
        let _ = deserialized;
    }

    // ── PrAgentsResponse ──────────────────────────────────────────────────

    #[test]
    fn test_pr_agents_response_serde_roundtrip() {
        let orig = PrAgentsResponse {
            run_id: "run-001".into(),
            pr_key: "owner/repo/pull/1".into(),
            pr_title: "Fix the bug".into(),
            agents: vec![PrAgentEntry {
                role: "FE".into(),
                has_prompt: true,
                has_response: true,
                has_reasoning: false,
            }],
            has_output: true,
        };
        let json = serde_json::to_string(&orig).unwrap();
        let deserialized: PrAgentsResponse = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&orig);
        let _ = deserialized;
    }

    // ── PrAgentEntry ──────────────────────────────────────────────────────

    #[test]
    fn test_pr_agent_entry_serde_roundtrip() {
        let orig = PrAgentEntry {
            role: "SEC".into(),
            has_prompt: false,
            has_response: false,
            has_reasoning: false,
        };
        let json = serde_json::to_string(&orig).unwrap();
        let deserialized: PrAgentEntry = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&orig);
        let _ = deserialized;
    }

    // ── PrDetailResponse ──────────────────────────────────────────────────

    #[test]
    fn test_pr_detail_response_serde_roundtrip() {
        let orig = PrDetailResponse {
            run_id: "run-001".into(),
            pr_title: "Add new endpoint".into(),
            url: "https://github.com/owner/repo/pull/42".into(),
            findings_count: 5,
            golden_count: 3,
            metrics: MetricsJson {
                true_positives: 2,
                false_positives: 1,
                false_negatives: 1,
                precision: 0.667,
                recall: 0.667,
                f1: 0.667,
            },
            verdicts: vec![VerdictJson {
                reasoning: "Matches".into(),
                match_: true,
                confidence: 0.9,
            }],
            cost: Some(CostJson {
                total_usd: 0.05,
                ..Default::default()
            }),
            findings: serde_json::json!({"type": "bug", "severity": "high"}),
            agent_responses: vec!["Response text".into()],
        };
        let json = serde_json::to_string(&orig).unwrap();
        let deserialized: PrDetailResponse = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&orig);
        let _ = deserialized;
    }

    #[test]
    fn test_pr_detail_response_default_findings() {
        let json = r#"{"run_id":"r1","pr_title":"Test","url":"https://example.com","findings_count":0,"golden_count":0,"metrics":{},"verdicts":[],"cost":null}"#;
        let detail: PrDetailResponse = serde_json::from_str(json).unwrap();
        insta::assert_debug_snapshot!(detail);
    }
}
