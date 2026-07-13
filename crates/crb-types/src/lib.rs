//! Shared event types for code review benchmark runs.

pub mod benchmark;
pub mod wrappers;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::benchmark::{Metrics, MetricsData};

/// Events for the entire lifecycle of a review.
///
/// Serialized with a JSON tag/envelope format suitable for SSE streaming:
/// `{"event":"pr_completed","data":{...}}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "event", content = "data")]
pub enum RunEvent {
    /// An agent has started its review for a given PR.
    AgentStarted { identifier: String, agent: String },

    /// A chunk of streaming response text from an agent.
    AgentChunk { identifier: String, chunk: String },

    /// An agent has finished its review.
    AgentFinished {
        identifier: String,
        findings: usize,
        success: bool,
    },

    ReviewStarted {
        identifier: String,
        total_agents: usize,
    },

    /// A single has been fully evaluated.
    ReviewCompleted {
        identifier: String,
        metrics: MetricsData,
        cost: f64,
        total_tokens: usize,
        agent_calls: usize,
        findings_count: usize,
    },

    /// Progress update during a run.
    RunProgress {
        completed_prs: usize,
        total_prs: usize,
        elapsed_secs: f64,
        total_cost: f64,
        current_pr: Option<String>,
    },

    /// The entire run has finished.
    RunFinished {
        total_prs: usize,
        aggregated: Metrics,
        total_cost: f64,
        total_tokens: usize,
        total_agent_calls: usize,
    },
}

/// The structured verdict returned by the judge LLM.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JudgeVerdict {
    /// Brief explanation of why the judge determined a match or no match.
    pub reasoning: String,

    /// Whether the candidate finding matches the golden comment.
    #[serde(rename = "match")]
    pub match_: bool,

    /// Confidence level for this judgment (0.0–1.0).
    pub confidence: f64,
}

/// Parse a single JSON line into a [`RunEvent`].
///
/// Returns `None` if the line is empty or not valid JSON.
#[deprecated = "Use `serde_json::from_str` directly instead of this helper function."]
pub fn parse_event_line(line: &str) -> Option<RunEvent> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    serde_json::from_str(line).ok()
}
