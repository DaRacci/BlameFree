//! Shared event types for code review benchmark runs.

pub mod benchmark;
pub mod wrappers;

use serde::{Deserialize, Serialize};

use crate::benchmark::Metrics;

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
        metrics: Metrics,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_event_adjacent_tagging() {
        let started = RunEvent::AgentStarted {
            identifier: "test".into(),
            agent: "bot".into(),
        };
        let v = serde_json::to_value(&started).unwrap();
        insta::assert_json_snapshot!(&v);

        let progress = RunEvent::RunProgress {
            completed_prs: 0,
            total_prs: 5,
            elapsed_secs: 0.0,
            total_cost: 0.0,
            current_pr: None,
        };
        let v = serde_json::to_value(&progress).unwrap();
        insta::assert_json_snapshot!(&v);

        let finished = RunEvent::RunFinished {
            total_prs: 1,
            aggregated: Metrics::default(),
            total_cost: 0.0,
            total_tokens: 0,
            total_agent_calls: 0,
        };
        let v = serde_json::to_value(&finished).unwrap();
        insta::assert_json_snapshot!(&v);
    }
}
