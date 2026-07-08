pub mod golden;

pub use golden::{load_golden_datasets, GoldenCommentEntry};

use std::path::Path;

use anyhow::Result;
use crb_shared::sanitize_filename;
use serde::{Deserialize, Serialize};
use tracing::info;

use crb_judge::{JudgeVerdict, Metrics};

/// Summary of cost data for a single PR, suitable for JSON serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostSummary {
    pub agent_tokens_in: usize,
    pub agent_tokens_out: usize,
    pub judge_tokens_in: usize,
    pub judge_tokens_out: usize,
    pub total_usd: f64,
    pub agent_cache_hit_rate: f64,
    pub judge_cache_hit_rate: f64,

    #[serde(default)]
    pub agent_cached_input_tokens: usize,

    #[serde(default)]
    pub agent_cache_creation_input_tokens: usize,

    #[serde(default)]
    pub agent_reasoning_tokens: usize,

    #[serde(default)]
    pub agent_tool_use_prompt_tokens: usize,

    #[serde(default)]
    pub judge_cached_input_tokens: usize,

    #[serde(default)]
    pub judge_cache_creation_input_tokens: usize,

    #[serde(default)]
    pub judge_reasoning_tokens: usize,

    #[serde(default)]
    pub judge_tool_use_prompt_tokens: usize,

    #[serde(default)]
    pub agent_call_count: usize,

    #[serde(default)]
    pub judge_call_count: usize,
}

/// Result of evaluating a single PR.
#[derive(Debug, Clone, Serialize)]
pub struct PrResult {
    pub pr_title: String,
    pub url: String,
    pub findings_count: usize,
    pub golden_count: usize,
    pub metrics: Metrics,
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
