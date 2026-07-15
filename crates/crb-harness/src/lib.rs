use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use crb_reporting::golden::GoldenCommentEntry;
use crb_reporting::PrResult;
use crb_shared::{diff::Diff, sanitize_filename};
use crb_types::RunEvent;
use crb_types::benchmark::{Metrics, MetricsProvider};
use crb_types::wrappers::WrappedData;
use tracing::{info, warn};

use crate::eval::EvalConfig;

#[cfg(feature = "binary")]
pub mod config;
pub mod eval;
pub mod finding;
pub mod model_capabilities;
pub mod paths;
pub mod pipeline;
pub mod review;
pub mod test_utils;

/// Describes which kind of diff to review.
pub enum ReviewMode {
    /// Review a commit range `base..head`.
    Commits { base: String, head: String },

    /// Review the current working tree (unstaged + staged).
    Working,
}

/// Call an async function with exponential backoff retry.
#[doc(hidden)]
pub async fn with_retry<F, Fut, T, E>(f: F, max_retries: usize, base_delay_ms: u64) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut attempt = 0usize;
    loop {
        match f().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                attempt += 1;
                if attempt >= max_retries {
                    return Err(e);
                }
                let delay = Duration::from_millis(base_delay_ms * 2u64.pow(attempt as u32));
                warn!(
                    "Attempt {}/{} failed: {}. Retrying in {}ms",
                    attempt,
                    max_retries,
                    e,
                    delay.as_millis()
                );
                tokio::time::sleep(delay).await;
            }
        }
    }
}

/// Unified evaluation of a single PR.
///
/// This function runs the steps:
/// - diff preprocessing
/// - linter collection
/// - agent review
/// - post-processing (dedup / severity auditor / capping)
/// - metrics computation
/// - dashboard events
/// - metadata
/// - caching
pub async fn evaluate_pr(
    pr: &GoldenCommentEntry,
    diff: &Diff,
    config: &EvalConfig,
) -> Result<PrResult> {
    // ── Phase 1: Send AgentStarted events ──
    let pr_key = sanitize_filename(&pr.pr_title);
    if let Some(ref tx) = config.dashboard_tx {
        for entry in config.agents {
            let _ = tx.send(RunEvent::AgentStarted {
                identifier: pr_key.clone(),
                agent: entry.role_abbreviation.to_string(),
            });
        }
    }

    // ── Phase 2: Run pipeline (delegate to evaluate) ──
    // pipeline::evaluate() handles diff preprocessing, linters, reviewers,
    // post-processing, metrics logging, and sends the ReviewCompleted event.
    let owned_diff = Diff::new(diff.raw.clone());
    let all_findings = crate::pipeline::evaluate(owned_diff, config).await?;

    // ── Phase 3: Metrics ──
    // Metrics require golden data (true/false positives) which pipeline
    // doesn't have access to. Use defaults for now.
    let metrics = Metrics::default();

    // ── Phase 4: Cache metadata ──
    let metadata = serde_json::json!({
        "pr_title": pr.pr_title,
        "url": pr.url,
        "model": config.model.get(),
        "strategy": format!("{:?}", config.strategy),
        "timestamp": format!("{:?}", std::time::SystemTime::now()),
        "findings_count": all_findings.len(),
        "golden_count": pr.comments.len(),
        "metrics": {
            "true_positives": metrics.true_positives,
            "false_positives": metrics.false_positives,
            "false_negatives": metrics.false_negatives,
            "precision": metrics.precision(),
            "recall": metrics.recall(),
            "f1": metrics.f1(),
        },
    });
    if let Some(ref cache) = config.cache {
        match serde_json::to_string(&metadata) {
            Ok(json_str) => cache.store_raw("run_metadata", &json_str),
            Err(e) => warn!("Failed to serialize cache metadata: {e}"),
        }
    }

    // ── Phase 5: Return result ──
    Ok(PrResult {
        pr_title: pr.pr_title.clone(),
        url: pr.url.clone(),
        findings_count: all_findings.len(),
        golden_count: pr.comments.len(),
        metrics,
        verdicts: Vec::new(),
        cost: Some(config.cost_tracker.to_snapshot().await),
    })
}
