//! Multi-agent consensus orchestration for code review evaluation.
//!
//! Orchestrates multiple LLM reviewer agents concurrently,
//! then aggregates their structured findings via heuristic matching and LLM
//! judge fallback against golden comments.

pub mod adaptive;
pub mod judge;
pub mod pipeline;

use crb_reporting::{cost::AnalyticsSnapshot, golden::GoldenComment};
use crb_shared::finding::Finding;
use crb_types::benchmark::MetricsProvider;
use serde::{Deserialize, Serialize};

/// The role of a reviewer agent.
///
/// This is a dynamic newtype around a string abbreviation.
/// Valid values are loaded at runtime from the agent manifest (`prompts/agents/*.md`).
///
/// Prefer [`Role::from_abbreviation`] for construction — it validates against
/// the loaded [`PromptLibrary`](crb_agents::prompts::PromptLibrary).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize, Serialize)]
pub struct Role(pub String);

impl Role {
    /// Convert to the string identifier used by `crb_agents::build_agent`.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Construct a `Role` from an abbreviation, validating that it exists
    /// in the loaded [`PromptLibrary`](crb_agents::prompts::PromptLibrary).
    ///
    /// Returns `None` if the abbreviation is not a known agent role.
    pub fn from_abbreviation(abbreviation: &str) -> Option<Self> {
        let upper = abbreviation.to_uppercase();
        crb_agents::prompts::PromptLibrary::get_instance()
            .config(&upper)
            .map(|_| Role(upper))
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

    /// Analytics usage for the agent LLM calls.
    pub analytics: AnalyticsSnapshot,
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
        let candidates: Vec<Finding> = vec![];
        assert!(candidates.is_empty());
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
    fn test_role_display() {
        let role = Role("SECURITY".to_string());
        assert_eq!(format!("{role}"), "SECURITY");
        assert_eq!(role.as_str(), "SECURITY");
    }

    #[test]
    fn test_role_from_str() {
        let role: Role = "security".into();
        assert_eq!(role.as_str(), "SECURITY");
    }

    #[test]
    fn test_role_from_string() {
        let role: Role = Role::from("rust".to_string());
        assert_eq!(role.as_str(), "RUST");
    }

    #[test]
    fn test_match_result_json_serialization() {
        insta::assert_json_snapshot!(MatchResult::TruePositive);
        insta::assert_json_snapshot!(MatchResult::FalsePositive);
        insta::assert_json_snapshot!(MatchResult::FalseNegative);
    }

    #[test]
    fn test_consensus_report_mixed_metrics() {
        let report = ConsensusReport {
            agents: vec![],
            true_positives: vec![(
                GoldenComment {
                    comment: "bug".into(),
                    severity: Severity::Critical,
                },
                Finding {
                    file: Some("a.rs".into()),
                    line: Some(1),
                    message: "bug".into(),
                    severity: Severity::Critical,
                    ..Default::default()
                },
            )],
            false_positives: vec![Finding {
                file: Some("b.rs".into()),
                line: Some(2),
                message: "not a bug".into(),
                severity: Severity::Info,
                ..Default::default()
            }],
            false_negatives: vec![GoldenComment {
                comment: "missed".into(),
                severity: Severity::Low,
            }],
            ..Default::default()
        };

        assert_eq!(report.true_positives(), 1);
        assert_eq!(report.false_positives(), 1);
        assert_eq!(report.false_negatives(), 1);

        let eps = 1e-6;
        assert!((report.precision() - 0.5).abs() < eps);
        assert!((report.recall() - 0.5).abs() < eps);
        assert!((report.f1() - 0.5).abs() < eps);
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
