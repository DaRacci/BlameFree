//! Multi-agent consensus orchestration for code review evaluation.
//!
//! Orchestrates multiple LLM reviewer agents concurrently,
//! then aggregates their structured findings via heuristic matching and LLM
//! judge fallback against golden comments.

pub mod adaptive;
pub mod agent;
pub mod execution;
pub mod harness;
pub mod judge;
pub mod pipeline;

use crb_reporting::{cost::AnalyticsSnapshot, golden::GoldenComment};
use crb_shared::finding::Finding;
use crb_types::benchmark::MetricsProvider;
use regex::Regex;
use rig_core::completion::{AssistantContent, Message, Usage};
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use tracing::warn;

/// Regex to extract JSON from markdown code blocks.
#[allow(clippy::unwrap_used)]
static RE_CODEBLOCK_JSON: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)```(?:json)?\s*\n(.*?)\n\s*```").unwrap());

/// Regex to find any JSON array in a response.
#[allow(clippy::unwrap_used)]
static RE_JSON_ARRAY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[[\s\S]*\]").unwrap());

/// The role of a reviewer agent.
///
/// This is a dynamic newtype around a string abbreviation.
/// Valid values are loaded at runtime from the agent manifest (`prompts/agents/*.md`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize, Serialize)]
pub struct Role(pub String);

impl Role {
    /// Convert to the string identifier used by `crb_agents::build_agent`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for Role {
    fn from(s: &str) -> Self {
        Role(s.to_uppercase())
    }
}

impl From<String> for Role {
    fn from(s: String) -> Self {
        Role(s.to_uppercase())
    }
}

/// Configuration for a single reviewer agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[deprecated = "Use EvalConfig instead."]
pub struct ReviewerConfig {
    /// The reviewer role.
    pub role: Role,

    /// The LLM Model identifier for this reviewer.
    pub model: String,

    /// The Maximum number of findings this agent should produce.
    pub max_findings: usize,
}

/// Result of matching a golden comment against candidate findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchResult {
    /// A candidate finding matches the golden comment.
    TruePositive,

    /// A candidate finding has no matching golden comment.
    FalsePositive,

    /// A golden comment has no matching candidate finding.
    FalseNegative,
}

/// Output of a full consensus run.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ConsensusReport {
    /// Findings from each agent, grouped by role.
    pub agents: Vec<(Role, Vec<Finding>)>,

    /// Goldens that were matched by at least one finding.
    pub true_positives: Vec<(GoldenComment, Finding)>,

    /// Findings that matched no golden.
    pub false_positives: Vec<Finding>,

    /// Goldens that matched no finding.
    pub false_negatives: Vec<GoldenComment>,

    /// TP / (TP + FP)
    #[deprecated(note = "Use the `precision()` method instead.")]
    pub precision: f64,

    /// TP / (TP + FN)
    #[deprecated(note = "Use the `recall()` method instead.")]
    pub recall: f64,

    /// F1 = harmonic mean of precision and recall
    #[deprecated(note = "Use the `f1()` method instead.")]
    pub f1: f64,

    /// Analytics usage for the agent LLM calls.
    pub analytics: AnalyticsSnapshot,

    /// Number of agent LLM calls that were cache misses.
    #[deprecated(note = "Use the `analytics` field instead.")]
    pub agent_api_calls: usize,

    /// Number of judge LLM calls that were cache misses.
    #[deprecated(note = "Use the `analytics` field instead.")]
    pub judge_api_calls: usize,

    /// Number of judge LLM calls that were cache hits.
    #[deprecated(note = "Use the `analytics` field instead.")]
    pub judge_cache_hits: usize,

    /// Aggregate token usage from all agent API calls.
    #[deprecated(note = "Use the `analytics` field instead.")]
    pub agent_usage: Usage,

    /// Aggregate token usage from all judge API calls.
    #[deprecated(note = "Use the `analytics` field instead.")]
    pub judge_usage: Usage,
}

impl MetricsProvider for ConsensusReport {
    fn true_positives(&self) -> usize {
        self.true_positives.len()
    }

    fn false_positives(&self) -> usize {
        self.false_positives.len()
    }

    fn false_negatives(&self) -> usize {
        self.false_negatives.len()
    }
}

/// Attempt to parse findings from an agent response using a 3-strategy
/// fallback:
///
/// 1. Direct JSON parse of the full response
/// 2. Extract JSON from markdown code blocks via [`RE_CODEBLOCK_JSON`]
/// 3. Find any JSON array via [`RE_JSON_ARRAY`]
///
/// If `context` is non-empty, a warning is logged with that context on
/// failure (e.g. `"CACHED"`, `""` for silent failure).
#[deprecated = "Use output_schema::<Vec<Finding>>() from rig instead of manual JSON parsing."]
pub fn parse_findings_from_response(response: &str, role: &Role, context: &str) -> Vec<Finding> {
    serde_json::from_str(response).unwrap_or_else(|_| {
        if let Some(caps) = RE_CODEBLOCK_JSON.captures(response) {
            // This is safe, it will always have atleast one group if it matches
            #[allow(clippy::unwrap_used)]
            let inner = caps.get(1).unwrap().as_str().trim();
            if let Ok(f) = serde_json::from_str::<Vec<Finding>>(inner) {
                return f;
            }
        }

        if let Some(m) = RE_JSON_ARRAY.find(response) {
            if let Ok(f) = serde_json::from_str::<Vec<Finding>>(m.as_str()) {
                return f;
            }
        }

        if !context.is_empty() {
            warn!(
                "Failed to parse {} findings for role {:?}. Response (truncated): {}",
                context,
                role,
                &response[..std::cmp::min(200, response.len())],
            );
        }
        Vec::new()
    })
}

/// Try to extract the last assistant text message from a chat history.
///
/// When [`PromptError::MaxTurnsError`] fires, the agent's accumulated conversation is available in `chat_history`.
/// This function walks it in reverse to find the most recent `Message::Assistant` whose content includes an `AssistantContent::Text` variant
/// that text is often a partial or complete JSON findings array the model produced before being cut off.
pub fn extract_last_assistant_text(history: &[Message]) -> Option<String> {
    for msg in history.iter().rev() {
        let Message::Assistant { content, .. } = msg else {
            continue;
        };

        for item in content.iter() {
            let AssistantContent::Text(text) = item else {
                continue;
            };

            let t = text.text.trim().to_string();
            if !t.is_empty() {
                return Some(t);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use crb_shared::severity::Severity;

    use super::*;

    /// Assert that the precision, recall, and F1 metrics of a report
    /// are all equal to the given expected value (within 1e-6).
    fn assert_metrics(report: &ConsensusReport, expected: f64) {
        const EPS: f64 = 1e-6;
        assert!((report.precision() - expected).abs() < EPS);
        assert!((report.recall() - expected).abs() < EPS);
        assert!((report.f1() - expected).abs() < EPS);
    }

    #[test]
    fn test_judge_comment_no_candidates() {
        // Empty candidates -> no file+line match -> FalseNegative
        let golden = GoldenComment {
            comment: r".*".into(),
            severity: Severity::Critical,
        };

        // We can't call judge_comment directly in unit tests because it requires a real LLM agent.
        // Instead we test that empty candidates produce FN.
        // The file+line pre-filter returns empty -> FalseNegative.
        let candidates: Vec<Finding> = vec![];
        let file_matches: Vec<&Finding> = candidates
            .iter()
            .filter(|f| golden.matches_candidate(f))
            .collect();
        assert!(file_matches.is_empty());
    }

    #[test]
    fn test_consensus_report_perfect() {
        // All findings match all goldens
        let report = ConsensusReport {
            agents: vec![],
            true_positives: vec![(
                GoldenComment {
                    comment: "foo".into(),
                    severity: Severity::Critical,
                },
                Finding {
                    file: Some("a.rs".into()),
                    line: Some(1),
                    message: "foo".into(),
                    severity: Severity::Critical,
                    ..Default::default()
                },
            )],
            ..Default::default()
        };

        assert_eq!(report.true_positives.len(), 1);
        assert_eq!(report.false_positives.len(), 0);
        assert_eq!(report.false_negatives.len(), 0);
        assert_metrics(&report, 1.0);
    }

    #[test]
    fn test_consensus_report_no_matches() {
        let report = ConsensusReport {
            false_positives: vec![Finding {
                file: Some("a.rs".into()),
                line: Some(1),
                message: "unexpected".into(),
                severity: Severity::Info,
                severity_audited: false,
                ..Default::default()
            }],
            false_negatives: vec![GoldenComment {
                comment: "expected".into(),
                severity: Severity::Critical,
            }],
            ..Default::default()
        };

        assert_eq!(report.true_positives.len(), 0);
        assert_eq!(report.false_positives.len(), 1);
        assert_eq!(report.false_negatives.len(), 1);
        assert_metrics(&report, 0.0);
    }
}
