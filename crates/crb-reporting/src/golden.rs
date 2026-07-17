use std::{fs, path::Path};

use anyhow::Result;
use crb_types::benchmark::golden::GoldenCommentEntry;
use serde::Deserialize;
use tracing::{info, warn};

/// Top-level structure of a golden-comments JSON file.
#[derive(Debug, Clone, Deserialize)]
struct DatasetFile {
    entries: Vec<GoldenCommentEntry>,
}

/// Load all golden-comment entries from every `.json` file under `dataset_dir`.
///
/// Each JSON file is expected to deserialize as a `DatasetFile` containing a top-level `entries` array.
/// Malformed files are logged and skipped.
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

    for entry in fs::read_dir(dataset_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "json") {
            let content = fs::read_to_string(&path)?;
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
                            warn!("Failed to parse {}: {}", path.display(), e);
                        }
                    }
                }
            }
        }
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_load_golden_datasets_empty_dir() {
        let dir = tempfile::TempDir::new().expect("tempdir creation should succeed");
        let entries = load_golden_datasets(dir.path()).expect("load should succeed");
        assert!(entries.is_empty());
    }

    #[test]
    fn test_load_golden_datasets_skips_non_json() {
        let dir = tempfile::TempDir::new().expect("tempdir creation should succeed");
        // Create .txt, .md, and .json files
        fs::write(dir.path().join("readme.txt"), "not json").expect("write should succeed");
        fs::write(dir.path().join("notes.md"), "# Notes").expect("write should succeed");
        fs::write(
            dir.path().join("data.json"),
            r#"{"entries": [{"pr_title":"Only","url":"https://github.com/a/b/pull/1","comments":[]}]}"#,
        )
        .expect("write should succeed");

        let entries = load_golden_datasets(dir.path()).expect("load should succeed");
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn test_load_golden_datasets_multiple_files() {
        let dir = tempfile::TempDir::new().expect("tempdir creation should succeed");
        fs::write(
            dir.path().join("a.json"),
            r#"{"entries": [{"pr_title":"A","url":"https://github.com/a/b/pull/1","comments":[]}]}"#,
        )
        .expect("write should succeed");
        fs::write(
            dir.path().join("b.json"),
            r#"{"entries": [{"pr_title":"B","url":"https://github.com/a/b/pull/2","comments":[]}]}"#,
        )
        .expect("write should succeed");

        let entries = load_golden_datasets(dir.path()).expect("load should succeed");
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_load_golden_datasets_malformed_json() {
        let dir = tempfile::TempDir::new().expect("tempdir creation should succeed");
        fs::write(dir.path().join("bad.json"), "this is not json").expect("write should succeed");

        let entries = load_golden_datasets(dir.path()).expect("load should succeed");
        assert!(entries.is_empty());
    }

    #[test]
    fn test_load_golden_datasets_nonexistent_dir() {
        let non_existent = Path::new("/tmp/this_path_does_not_exist_42xyz");
        let entries = load_golden_datasets(non_existent).expect("load should succeed");
        assert!(entries.is_empty());
    }
}
