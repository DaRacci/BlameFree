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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenComment {
    pub comment: String,
    pub severity: String,
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
}

// ── Output ──────────────────────────────────────────────────────────────────

/// Write per-PR JSON result files and a summary CSV to `output_dir`.
///
/// Each PR gets `<sanitized-title>.json` with its full result.  A `summary.csv`
/// contains aggregated metrics across all evaluated PRs.
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

    // Summary CSV
    let csv_path = output_dir.join("summary.csv");
    let mut wtr = csv::Writer::from_path(&csv_path)?;
    wtr.write_record(&[
        "pr_title",
        "url",
        "findings_count",
        "golden_count",
        "true_positives",
        "false_positives",
        "false_negatives",
        "precision",
        "recall",
        "f1",
    ])?;

    for result in results {
        wtr.write_record(&[
            &result.pr_title,
            &result.url,
            &result.findings_count.to_string(),
            &result.golden_count.to_string(),
            &result.metrics.true_positives.to_string(),
            &result.metrics.false_positives.to_string(),
            &result.metrics.false_negatives.to_string(),
            &format!("{:.4}", result.metrics.precision),
            &format!("{:.4}", result.metrics.recall),
            &format!("{:.4}", result.metrics.f1),
        ])?;
    }
    wtr.flush()?;
    info!("Wrote summary CSV: {}", csv_path.display());

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
