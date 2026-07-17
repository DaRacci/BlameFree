//! Reporting for metrics, analytics, cost etc.

pub mod cost;
pub mod golden;
pub mod history;

use std::{fs, path::Path};

use anyhow::Result;
use crb_shared::{sanitize_filename, url::parse_github_url};
use serde::{Deserialize, Serialize};
use tracing::info;

use crb_types::benchmark::{JudgeVerdict, Metrics, MetricsProvider};

use crate::cost::AnalyticsSnapshot;

/// Result of evaluating a single PR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrResult {
    /// PR title.
    pub pr_title: String,

    /// URL to the PR.
    pub url: String,

    /// Number of findings produced by the agents.
    pub findings_count: usize,

    /// Number of golden comments for this PR.
    pub golden_count: usize,

    /// Evaluation metrics.
    pub metrics: Metrics,

    /// Judge verdicts for each finding-vs-golden comparison.
    pub verdicts: Vec<JudgeVerdict>,

    /// Cost tracking data for this PR evaluation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<AnalyticsSnapshot>,

    /// Raw findings data.
    #[serde(default)]
    pub findings: serde_json::Value,

    /// Raw agent response texts.
    #[serde(default)]
    pub agent_responses: Vec<String>,
}

/// Write per-PR JSON result files to `output_dir`.
///
/// Each PR gets `<sanitized-title>.json` with its full result.
pub fn write_report(results: &[PrResult], output_dir: &Path) -> Result<()> {
    fs::create_dir_all(output_dir)?;

    // Per-PR JSON
    for result in results {
        let filename = sanitize_filename(&result.pr_title);
        let path = output_dir.join(format!("{filename}.json"));
        let json = serde_json::to_string_pretty(result)?;
        fs::write(&path, json)?;
        info!("Wrote per-PR result: {}", path.display());
    }

    Ok(())
}

/// Print a terminal summary of cost and cache hit rates for all PRs.
pub async fn print_terminal_summary(results: &[PrResult]) {
    let separator = "═══════════════════════════════════════════════";
    println!("\n{separator}");

    let mut grand_total_tokens_in = 0u64;
    let mut grand_total_tokens_out = 0u64;
    let mut grand_total_cost = 0.0f64;

    for result in results {
        let pr_label = parse_github_url(&result.url)
            .map(|(owner, repo, num)| format!("{owner}/{repo}/{num}"))
            .unwrap_or_else(|_| result.pr_title.clone());

        let f1 = result.metrics.f1();
        let findings_count = result.findings_count;

        if let Some(ref cost) = result.cost {
            let (tokens_in, tokens_out) = cost.total_tokens().await;
            let pr_cost = cost.total_cost();

            grand_total_tokens_in += tokens_in;
            grand_total_tokens_out += tokens_out;
            grand_total_cost += pr_cost;

            let total_tokens_k = (tokens_in + tokens_out) as f64 / 1000.0;
            println!(
                " {}: F1={:.3}, {} findings, {:.1}K tokens, ${:.4}",
                pr_label, f1, findings_count, total_tokens_k, pr_cost,
            );
        } else {
            println!(
                " {}: F1={:.3}, {} findings, -- tokens, $--",
                pr_label, f1, findings_count,
            );
        }
    }

    let total_agent_rate: f64 = results
        .iter()
        .filter_map(|r| r.cost.as_ref())
        .map(|c| c.hit_rate())
        .sum();
    let pr_count_with_cost = results.iter().filter(|r| r.cost.is_some()).count();

    let avg_agent_rate = if pr_count_with_cost > 0 {
        total_agent_rate / pr_count_with_cost as f64
    } else {
        0.0
    };

    let grand_total_tokens = grand_total_tokens_in + grand_total_tokens_out;
    println!("{separator}");
    println!(
        " TOTAL: {} PR(s), {:.1}K tokens, ${:.4}",
        results.len(),
        grand_total_tokens as f64 / 1000.0,
        grand_total_cost,
    );
    println!(" Agent cache hit rate: {:.1}%", avg_agent_rate * 100.0);
    println!("{separator}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cost::{CacheUsage, SessionUsage};
    use std::collections::HashMap;

    fn make_cost_snapshot() -> AnalyticsSnapshot {
        let mut sessions = HashMap::new();
        sessions.insert(
            "agent:test".to_string(),
            SessionUsage {
                input_tokens: 100,
                output_tokens: 50,
                cached_input_tokens: 20,
                cache_creation_input_tokens: 10,
                reasoning_tokens: 5,
                tool_use_prompt_tokens: 3,
                call_count: 1,
                tool_use_count: 0,
            },
        );
        let mut cache_usage = HashMap::new();
        cache_usage.insert(
            "agent:test".to_string(),
            CacheUsage {
                cache_hits: 1,
                cache_misses: 0,
            },
        );
        AnalyticsSnapshot {
            sessions,
            cache_usage,
        }
    }

    fn make_pr_result(pr_title: &str, url: &str, cost: Option<AnalyticsSnapshot>) -> PrResult {
        PrResult {
            pr_title: pr_title.to_string(),
            url: url.to_string(),
            findings_count: 3,
            golden_count: 2,
            metrics: Metrics {
                true_positives: 2,
                false_positives: 1,
                false_negatives: 0,
                duration_secs: 12.5,
            },
            verdicts: vec![
                JudgeVerdict {
                    reasoning: "Correct match".into(),
                    match_: true,
                    confidence: 0.95,
                },
                JudgeVerdict {
                    reasoning: "False positive".into(),
                    match_: false,
                    confidence: 0.3,
                },
            ],
            findings: serde_json::Value::Null,
            agent_responses: vec![],
            cost,
        }
    }

    #[test]
    fn test_pr_result_serialization_roundtrip() {
        let cost = make_cost_snapshot();
        let result = make_pr_result(
            "Fix security vulnerability",
            "https://github.com/owner/repo/pull/42",
            Some(cost),
        );
        insta::assert_json_snapshot!(&result);

        // Serialize to JSON string and parse as generic value to verify fields
        let json_str = serde_json::to_string(&result).expect("serialization should succeed");
        let parsed: serde_json::Value =
            serde_json::from_str(&json_str).expect("deserialization should succeed");

        assert_eq!(parsed["pr_title"], "Fix security vulnerability");
        assert_eq!(
            parsed["url"],
            "https://github.com/owner/repo/pull/42"
        );
        assert_eq!(parsed["findings_count"], 3);
        assert_eq!(parsed["golden_count"], 2);
        assert!(parsed["cost"].is_object());
        assert!(parsed["cost"]["sessions"].is_object());
        assert!(parsed["cost"]["cache_usage"].is_object());
    }

    #[test]
    fn test_pr_result_cost_skipped_when_none() {
        let result = make_pr_result(
            "Minor refactor",
            "https://github.com/owner/repo/pull/7",
            None,
        );
        let json_str = serde_json::to_string(&result).expect("serialization should succeed");

        #[allow(clippy::print_stdout)]
        let contains_cost = json_str.contains("\"cost\"");
        assert!(!contains_cost, "JSON should not contain cost field when None");
    }

    #[test]
    fn test_pr_result_empty_verdicts() {
        let result = PrResult {
            pr_title: "Empty verdicts".into(),
            url: "https://github.com/owner/repo/pull/0".into(),
            findings_count: 0,
            golden_count: 0,
            metrics: Metrics::default(),
            verdicts: Vec::new(),
            findings: serde_json::Value::Null,
            agent_responses: vec![],
            cost: None,
        };
        insta::assert_json_snapshot!(&result);
    }
}
