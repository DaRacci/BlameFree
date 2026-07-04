use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::info;

/// Top-level structure of a golden-comments JSON file.
#[derive(Debug, Clone, Deserialize)]
struct DatasetFile {
    entries: Vec<GoldenCommentEntry>,
}

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

/// A single golden comment for a PR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenComment {
    /// The expected comment text
    pub comment: String,

    /// The expected severity of the comment
    pub severity: String,

    /// Source file path, if the dataset includes it (e.g. from `path` field
    /// in benchmark_data.json or `golden_comments` entries).
    #[serde(default)]
    pub file: Option<String>,

    /// Line number in the source file, if the dataset includes it.
    #[serde(default)]
    pub line: Option<u32>,
}

/// Load all golden-comment entries from every `.json` file under `dataset_dir`.
///
/// Each JSON file is expected to deserialize as a `DatasetFile` containing
/// a top-level `entries` array.  Malformed files are logged and skipped.
#[allow(clippy::cognitive_complexity)]
pub fn load_golden_datasets(dataset_dir: &Path) -> Result<Vec<GoldenCommentEntry>> {
    let mut entries = Vec::new();

    if !dataset_dir.exists() {
        info!(
            "Dataset directory does not exist, skipping: {}",
            dataset_dir.display()
        );
        return Ok(entries);
    }

    for entry in std::fs::read_dir(dataset_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "json") {
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
