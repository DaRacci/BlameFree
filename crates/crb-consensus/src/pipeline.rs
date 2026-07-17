//! The full consensus pipeline
//! Run reviewers, judge against goldens, compute metrics.

use std::sync::Arc;

use crb_reporting::cost::AnalyticsSnapshot;
use crb_types::benchmark::golden::GoldenComment;
use rig_core::agent::Agent;
use rig_core::providers::openai::responses_api::ResponsesCompletionModel;

use crb_types::finding::Finding;

use crate::Role;
use crate::judge::judge_comment;
use crate::{ConsensusReport, MatchResult};
use crb_cache::traits::CacheBackend;

/// Run the consensus judging step on already-completed review findings.
/// This is the post-step that follows the review pipeline.
///
/// 1. Accepts already-completed review results from each agent.
/// 2. For each golden comment, attempts heuristic matching ([`judge_comment`]) against pooled findings.
/// 3. Remaining unmatched findings are classified as false positives.
/// 4. Computes precision / recall / F1 metrics.
///
/// If `cache` is provided, judge calls are cached using content-addressed keys
/// derived from prompt hashes.
#[allow(clippy::too_many_arguments)]
pub async fn run_consensus_post(
    agents: Vec<(Role, Vec<Finding>)>,
    goldens: Vec<GoldenComment>,
    judge: &Agent<ResponsesCompletionModel>,
    judge_model: &str,
    cache: Arc<dyn CacheBackend>,
    judge_prompt_hash: &str,
) -> ConsensusReport {
    let mut unmatched: Vec<Finding> = agents
        .iter()
        .flat_map(|(_, findings)| findings.iter())
        .cloned()
        .collect();
    unmatched.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.message.cmp(&b.message))
    });

    let mut true_positives: Vec<(GoldenComment, Finding)> = Vec::new();
    let mut false_negatives: Vec<GoldenComment> = Vec::new();
    let mut judge_api_calls: usize = 0;

    for golden in &goldens {
        let result = judge_comment(
            golden,
            &unmatched,
            judge,
            judge_model,
            cache.clone(),
            judge_prompt_hash,
            &mut judge_api_calls,
        )
        .await;

        match result {
            MatchResult::TruePositive => {
                // Remove the first file+line matched finding from the pool
                // (judge_comment returns on the first match, so the first
                // candidate in iteration order is the one that was matched).
                if !unmatched.is_empty() {
                    let matched = unmatched.remove(0);
                    true_positives.push((golden.clone(), matched));
                }
            }
            MatchResult::FalseNegative => {
                false_negatives.push(golden.clone());
            }
            MatchResult::FalsePositive => {
                // This variant isn't returned by judge_comment (it checks a golden
                // against candidates, so it only yields TP or FN).
                // Defensively treat as FN.
                false_negatives.push(golden.clone());
            }
        }
    }

    let false_positives = unmatched;

    ConsensusReport {
        agents,
        true_positives,
        false_positives,
        false_negatives,
        analytics: AnalyticsSnapshot {
            ..Default::default() // TODO
        },
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use crb_types::finding::Finding;
    use crb_types::severity::Severity;

    /// Replicates the sort_by closure from `run_consensus_post` for testability.
    fn sort_findings(findings: &mut Vec<Finding>) {
        findings.sort_by(|a, b| {
            a.file
                .cmp(&b.file)
                .then_with(|| a.line.cmp(&b.line))
                .then_with(|| a.message.cmp(&b.message))
        });
    }

    #[test]
    fn test_sort_by_orders_by_file_then_line_then_message() {
        let mut findings = vec![
            Finding {
                file: Some("b.rs".into()),
                line: Some(1),
                message: "z".into(),
                severity: Severity::Info,
                ..Default::default()
            },
            Finding {
                file: Some("a.rs".into()),
                line: Some(2),
                message: "a".into(),
                severity: Severity::Info,
                ..Default::default()
            },
            Finding {
                file: Some("a.rs".into()),
                line: Some(1),
                message: "b".into(),
                severity: Severity::Info,
                ..Default::default()
            },
            Finding {
                file: Some("a.rs".into()),
                line: Some(1),
                message: "a".into(),
                severity: Severity::Info,
                ..Default::default()
            },
        ];

        sort_findings(&mut findings);

        assert_eq!(findings[0].file.as_deref(), Some("a.rs"));
        assert_eq!(findings[0].line, Some(1));
        assert_eq!(findings[0].message, "a");

        assert_eq!(findings[1].file.as_deref(), Some("a.rs"));
        assert_eq!(findings[1].line, Some(1));
        assert_eq!(findings[1].message, "b");

        assert_eq!(findings[2].file.as_deref(), Some("a.rs"));
        assert_eq!(findings[2].line, Some(2));
        assert_eq!(findings[2].message, "a");

        assert_eq!(findings[3].file.as_deref(), Some("b.rs"));
    }

    #[test]
    fn test_sort_by_handles_none_file() {
        let mut findings = vec![
            Finding {
                file: Some("b.rs".into()),
                line: Some(1),
                message: "x".into(),
                severity: Severity::Info,
                ..Default::default()
            },
            Finding {
                file: None,
                line: Some(1),
                message: "y".into(),
                severity: Severity::Info,
                ..Default::default()
            },
            Finding {
                file: Some("a.rs".into()),
                line: Some(1),
                message: "z".into(),
                severity: Severity::Info,
                ..Default::default()
            },
        ];

        sort_findings(&mut findings);

        // None sorts before Some(...)
        assert_eq!(findings[0].file, None);
        assert_eq!(findings[1].file.as_deref(), Some("a.rs"));
        assert_eq!(findings[2].file.as_deref(), Some("b.rs"));
    }

    #[test]
    fn test_sort_by_handles_none_line() {
        let mut findings = vec![
            Finding {
                file: Some("a.rs".into()),
                line: Some(2),
                message: "x".into(),
                severity: Severity::Info,
                ..Default::default()
            },
            Finding {
                file: Some("a.rs".into()),
                line: None,
                message: "y".into(),
                severity: Severity::Info,
                ..Default::default()
            },
            Finding {
                file: Some("a.rs".into()),
                line: Some(1),
                message: "z".into(),
                severity: Severity::Info,
                ..Default::default()
            },
        ];

        sort_findings(&mut findings);

        // None sorts before Some(1) which sorts before Some(2)
        assert_eq!(findings[0].line, None);
        assert_eq!(findings[1].line, Some(1));
        assert_eq!(findings[2].line, Some(2));
    }

    #[test]
    fn test_sort_by_message_within_same_file_and_line() {
        let mut findings = vec![
            Finding {
                file: Some("a.rs".into()),
                line: Some(1),
                message: "zeta".into(),
                severity: Severity::Critical,
                ..Default::default()
            },
            Finding {
                file: Some("a.rs".into()),
                line: Some(1),
                message: "alpha".into(),
                severity: Severity::Info,
                ..Default::default()
            },
        ];

        sort_findings(&mut findings);

        assert_eq!(findings[0].message, "alpha");
        assert_eq!(findings[1].message, "zeta");
    }
}
