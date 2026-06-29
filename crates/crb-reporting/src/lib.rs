use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::info;

use crb_judge::{JudgeVerdict, Metrics};

// ── Data structures (shared across modules) ─────────────────────────────────

/// A single entry from a golden-comments dataset, representing one PR's
/// expected review findings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenCommentEntry {
    pub pr_title: String,
    pub url: String,
    #[serde(default)]
    pub original_url: Option<String>,
    #[serde(default)]
    pub az_comment: Option<String>,
    pub comments: Vec<GoldenComment>,
}

/// A single golden (expected) comment for a PR.
///
/// The `file` and `line` fields are populated from the dataset JSON when available.
/// Not all datasets include this metadata — both fields are `Option` to handle
/// datasets that only have `comment` and `severity`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenComment {
    pub comment: String,
    pub severity: String,
    /// Source file path, if the dataset includes it (e.g. from `path` field
    /// in benchmark_data.json or `golden_comments` entries).
    #[serde(default)]
    pub file: Option<String>,
    /// Line number in the source file, if the dataset includes it.
    #[serde(default)]
    pub line: Option<u32>,
}

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

// ── Output ──────────────────────────────────────────────────────────────────

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

// ── Input ───────────────────────────────────────────────────────────────────

/// Load all golden-comment entries from every `.json` file under `dataset_dir`.
///
/// Each JSON file is expected to deserialize as a `DatasetFile` containing
/// a top-level `entries` array.  Malformed files are logged and skipped.
pub fn load_golden_datasets(dataset_dir: &Path) -> Result<Vec<GoldenCommentEntry>> {
    let mut entries = Vec::new();

    if !dataset_dir.exists() {
        info!("Dataset directory does not exist, skipping: {}", dataset_dir.display());
        return Ok(entries);
    }

    let mut dir = std::fs::read_dir(dataset_dir)?;
    while let Some(entry) = dir.next() {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "json") {
            let content = std::fs::read_to_string(&path)?;
            match serde_json::from_str::<DatasetFile>(&content) {
                Ok(dataset) => {
                    info!(
                        "Loaded {} entries from {}",
                        dataset.entries.len(),
                        path.display()
                    );
                    entries.extend(dataset.entries);
                }
                Err(_) => {
                    // Try parsing as a raw array (backward compat with Martian format)
                    match serde_json::from_str::<Vec<GoldenCommentEntry>>(&content) {
                        Ok(raw_entries) => {
                            info!(
                                "Loaded {} entries from {} (raw array format)",
                                raw_entries.len(),
                                path.display()
                            );
                            entries.extend(raw_entries);
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse {}: {}", path.display(), e);
                        }
                    }
                }
            }
        }
    }

    Ok(entries)
}

/// Top-level structure of a golden-comments JSON file.
#[derive(Debug, Clone, Deserialize)]
struct DatasetFile {
    entries: Vec<GoldenCommentEntry>,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

pub fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}
