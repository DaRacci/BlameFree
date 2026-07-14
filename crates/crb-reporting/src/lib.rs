//! Reporting for metrics, analytics, cost etc.

pub mod cost;
pub mod golden;
pub mod history;

pub use golden::{GoldenCommentEntry, load_golden_datasets};

use std::{fs, path::Path};

use anyhow::Result;
use crb_shared::{sanitize_filename, url::parse_github_url};
use serde::Serialize;
use tracing::info;

use crb_types::benchmark::{JudgeVerdict, Metrics, MetricsProvider};

use crate::cost::AnalyticsSnapshot;

/// Result of evaluating a single PR.
#[derive(Debug, Clone, Serialize)]
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
#[deprecated = "Needs either a rewrite, or to be refacotred to use the new cost tracking system."]
pub async fn print_terminal_summary(results: &[PrResult]) {
    let separator = "═══════════════════════════════════════════════";
    println!("\n{separator}");

    let mut grand_total_tokens = 0usize;
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

            grand_total_tokens += (tokens_in + tokens_out) as usize;
            grand_total_cost += pr_cost;

            println!(
                " {}: F1={:.3}, {} findings, {:.1}K tokens, ${:.4}",
                pr_label,
                f1,
                findings_count,
                tokens_in as f64 / 1000.0,
                pr_cost,
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
