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
    fn test_run_event_agent_started_roundtrip() {
        let original = RunEvent::AgentStarted {
            identifier: "pr-1".into(),
            agent: "claude".into(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: RunEvent = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&original);
        let _ = deserialized;
    }

    #[test]
    fn test_run_event_agent_chunk_roundtrip() {
        let original = RunEvent::AgentChunk {
            identifier: "pr-1".into(),
            chunk: "some streaming text".into(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: RunEvent = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&original);
        let _ = deserialized;
    }

    #[test]
    fn test_run_event_agent_finished_roundtrip() {
        let original = RunEvent::AgentFinished {
            identifier: "pr-1".into(),
            findings: 3,
            success: true,
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: RunEvent = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&original);
        let _ = deserialized;
    }

    #[test]
    fn test_run_event_review_started_roundtrip() {
        let original = RunEvent::ReviewStarted {
            identifier: "batch-1".into(),
            total_agents: 2,
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: RunEvent = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&original);
        let _ = deserialized;
    }

    #[test]
    fn test_run_event_review_completed_roundtrip() {
        let original = RunEvent::ReviewCompleted {
            identifier: "batch-1".into(),
            metrics: Metrics {
                true_positives: 5,
                false_positives: 2,
                false_negatives: 1,
                duration_secs: 10.5,
            },
            cost: 0.5,
            total_tokens: 1000,
            agent_calls: 3,
            findings_count: 8,
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: RunEvent = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&original);
        let _ = deserialized;
    }

    #[test]
    fn test_run_event_run_progress_roundtrip() {
        let original = RunEvent::RunProgress {
            completed_prs: 3,
            total_prs: 10,
            elapsed_secs: 25.5,
            total_cost: 1.2,
            current_pr: Some("pr-42".into()),
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: RunEvent = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&original);
        let _ = deserialized;
    }

    #[test]
    fn test_run_event_run_finished_roundtrip() {
        let original = RunEvent::RunFinished {
            total_prs: 10,
            aggregated: Metrics {
                true_positives: 10,
                false_positives: 3,
                false_negatives: 2,
                duration_secs: 120.3,
            },
            total_cost: 5.5,
            total_tokens: 5000,
            total_agent_calls: 20,
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: RunEvent = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&original);
        let _ = deserialized;
    }

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
