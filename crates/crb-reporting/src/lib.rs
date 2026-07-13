//! Reporting for benchmark evaluation results.
//!
//! Provides [`PrResult`] — the output type for a single PR evaluation — and
//! the [`CostSummary`] struct for tracking token/dollar usage.  Also loads
//! golden-comment datasets via [`golden::load_golden_datasets`] for evaluation.

pub mod golden;

pub use golden::{GoldenCommentEntry, load_golden_datasets};

use std::path::Path;

use anyhow::Result;
use crb_shared::sanitize_filename;
use serde::{Deserialize, Serialize};
use tracing::info;

use crb_types::JudgeVerdict;

/// Summary of cost data for a single PR, suitable for JSON serialization.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CostSummary {
    /// Total input tokens consumed by all agent calls.
    pub agent_tokens_in: usize,
    /// Total output tokens produced by all agent calls.
    pub agent_tokens_out: usize,
    /// Total input tokens consumed by all judge calls.
    pub judge_tokens_in: usize,
    /// Total output tokens produced by all judge calls.
    pub judge_tokens_out: usize,
    /// Total cost in USD across all API calls.
    pub total_usd: f64,
    /// Fraction of agent cache lookups that were hits (0.0 – 1.0).
    pub agent_cache_hit_rate: f64,
    /// Fraction of judge cache lookups that were hits (0.0 – 1.0).
    pub judge_cache_hit_rate: f64,

    #[serde(default)]
    /// Cached input tokens from agent cache hits.
    pub agent_cached_input_tokens: usize,

    #[serde(default)]
    /// Input tokens used for agent cache creation (misses that populate cache).
    pub agent_cache_creation_input_tokens: usize,

    #[serde(default)]
    /// Reasoning tokens used by the agent (for reasoning-capable models).
    pub agent_reasoning_tokens: usize,

    #[serde(default)]
    /// Tool-use prompt tokens consumed by the agent.
    pub agent_tool_use_prompt_tokens: usize,

    #[serde(default)]
    /// Cached input tokens from judge cache hits.
    pub judge_cached_input_tokens: usize,

    #[serde(default)]
    /// Input tokens used for judge cache creation.
    pub judge_cache_creation_input_tokens: usize,

    #[serde(default)]
    /// Reasoning tokens used by the judge.
    pub judge_reasoning_tokens: usize,

    #[serde(default)]
    /// Tool-use prompt tokens consumed by the judge.
    pub judge_tool_use_prompt_tokens: usize,

    #[serde(default)]
    /// Number of agent API calls made (excluding cache hits).
    pub agent_call_count: usize,

    #[serde(default)]
    /// Number of judge API calls made (excluding cache hits).
    pub judge_call_count: usize,
}

/// Aggregated evaluation metrics computed from judge verdicts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Metrics {
    /// Number of true positives (agent findings that matched a golden comment).
    pub true_positives: usize,

    /// Number of false positives (agent findings that matched no golden comment).
    pub false_positives: usize,

    /// Number of false negatives (golden comments that matched no finding).
    pub false_negatives: usize,

    /// Precision = tp / (tp + fp).
    pub precision: f64,

    /// Recall = tp / (tp + fn).
    pub recall: f64,

    /// F1 score (harmonic mean of precision and recall).
    pub f1: f64,
}

/// Result of evaluating a single PR.
#[derive(Debug, Clone, Serialize)]
pub struct PrResult {
    /// PR title.
    pub pr_title: String,
    /// URL to the PR.
    pub url: String,
    /// Number of findings produced by the agents.
    pub findings_count: usize,
    /// Number of golden (expected) comments for this PR.
    pub golden_count: usize,
    /// Evaluation metrics (precision, recall, F1).
    pub metrics: Metrics,
    /// Judge verdicts for each finding-vs-golden comparison.
    pub verdicts: Vec<JudgeVerdict>,
    /// Cost tracking data for this PR evaluation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<CostSummary>,
}

/// Write per-PR JSON result files to `output_dir`.
///
/// Each PR gets `<sanitized-title>.json` with its full result.
pub fn write_report(results: &[PrResult], output_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(output_dir)?;

    // Per-PR JSON
    for result in results {
        let filename = sanitize_filename(&result.pr_title);
        let path = output_dir.join(format!("{filename}.json"));
        let json = serde_json::to_string_pretty(result)?;
        std::fs::write(&path, json)?;
        info!("Wrote per-PR result: {}", path.display());
    }

    Ok(())
}

// ── Trait impls for shared benchmark pipeline ────────────────────────────────

impl crb_shared::benchmark_pipeline::HasEvalMetrics for PrResult {
    fn true_positives(&self) -> usize {
        self.metrics.true_positives
    }
    fn false_positives(&self) -> usize {
        self.metrics.false_positives
    }
    fn false_negatives(&self) -> usize {
        self.metrics.false_negatives
    }
    fn cost_usd(&self) -> f64 {
        self.cost.as_ref().map_or(0.0, |c| c.total_usd)
    }
    fn total_tokens(&self) -> usize {
        self.cost.as_ref().map_or(0, |c| {
            c.agent_tokens_in + c.agent_tokens_out + c.judge_tokens_in + c.judge_tokens_out
        })
    }
    fn agent_calls(&self) -> usize {
        4
    }
}
