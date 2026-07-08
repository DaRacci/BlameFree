//! Event types for the web UI dashboard.
//!
//! These mirror the DashboardEvent types from `crb-dashboard` but are defined
//! locally with full Serde support for JSON serialization to SSE clients.

use serde::{Deserialize, Serialize};

/// Event sent from the harness subprocess to the web UI over SSE.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "data")]
pub enum DashboardEvent {
    /// An agent has started its review for a given PR.
    #[serde(rename = "agent_started")]
    AgentStarted { pr_key: String, role: String },

    /// A chunk of streaming response text from an agent.
    #[serde(rename = "agent_chunk")]
    AgentChunk { role: String, chunk: String },

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
        metrics: MetricsData,
        cost: f64,
        total_tokens: usize,
        agent_calls: usize,
        findings_count: usize,
    },

    /// Progress update during a run.
    #[serde(rename = "run_progress")]
    RunProgress {
        completed_prs: usize,
        total_prs: usize,
        elapsed_secs: f64,
        total_cost: f64,
        current_pr: Option<String>,
    },

    /// The entire run has finished.
    #[serde(rename = "run_finished")]
    RunFinished {
        total_prs: usize,
        aggregated: AggregateMetrics,
        total_cost: f64,
        total_tokens: usize,
        total_agent_calls: usize,
    },
}

/// Metrics data for a single PR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsData {
    pub true_positives: usize,
    pub false_positives: usize,
    pub false_negatives: usize,
    pub precision: f64,
    pub recall: f64,
    pub f1: f64,
}

/// Aggregate metrics across all PRs.
pub use crb_dashboard::AggregateMetrics;

/// Parse a single line of JSON from the harness subprocess stdout.
///
/// Returns `None` if the line is empty or not valid JSON.
pub fn parse_event_line(line: &str) -> Option<DashboardEvent> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    serde_json::from_str(line).ok()
}
