use std::{fs, path::Path};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::info;

/// Append-only run-history log.
pub const RUNS_FILE: &str = "_runs.json";

/// A single run entry appended to [`RUNS_FILE`] in the cache directory.
///
/// Records metadata about each run for historical tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunHistoryEntry {
    pub run_id: String,
    pub timestamp: String,
    pub model: String,
    pub judge_model: String,
    pub total_prs: usize,
}

// TODO: Is this needed or does the new cache system handle this automatically?
/// Append a run history entry to the runs file in the cache directory.
pub fn append_run_history(cache_dir: &Path, entry: &RunHistoryEntry) -> Result<()> {
    let path = cache_dir.join(RUNS_FILE);
    let mut runs: Vec<RunHistoryEntry> = if path.exists() {
        let content = fs::read_to_string(&path).unwrap_or_else(|_| "[]".to_string());
        serde_json::from_str(&content).unwrap_or_else(|_| Vec::new())
    } else {
        Vec::new()
    };
    runs.push(entry.clone());
    fs::write(&path, serde_json::to_string_pretty(&runs)?)?;
    info!("Appended run history to: {}", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_append_run_history_creates_file() {
        let dir = tempfile::TempDir::new().expect("tempdir creation should succeed");
        let entry = RunHistoryEntry {
            run_id: "run-001".into(),
            timestamp: "2026-07-16T12:00:00Z".into(),
            model: "gpt-4".into(),
            judge_model: "gpt-4".into(),
            total_prs: 5,
        };

        let result = append_run_history(dir.path(), &entry);
        assert!(result.is_ok());

        let runs_path = dir.path().join(RUNS_FILE);
        assert!(runs_path.exists(), "_runs.json should exist");
    }

    #[test]
    fn test_append_run_history_appends_to_existing() {
        let dir = tempfile::TempDir::new().expect("tempdir creation should succeed");
        let entry1 = RunHistoryEntry {
            run_id: "run-001".into(),
            timestamp: "2026-07-16T12:00:00Z".into(),
            model: "gpt-4".into(),
            judge_model: "gpt-4".into(),
            total_prs: 5,
        };
        let entry2 = RunHistoryEntry {
            run_id: "run-002".into(),
            timestamp: "2026-07-16T13:00:00Z".into(),
            model: "claude-3".into(),
            judge_model: "gpt-4".into(),
            total_prs: 3,
        };

        assert!(append_run_history(dir.path(), &entry1).is_ok());
        assert!(append_run_history(dir.path(), &entry2).is_ok());

        let content =
            fs::read_to_string(dir.path().join(RUNS_FILE)).expect("read should succeed");
        let runs: Vec<RunHistoryEntry> =
            serde_json::from_str(&content).expect("deserialization should succeed");
        assert_eq!(runs.len(), 2);
    }

    #[test]
    fn test_append_run_history_content() {
        let dir = tempfile::TempDir::new().expect("tempdir creation should succeed");
        let entry = RunHistoryEntry {
            run_id: "run-001".into(),
            timestamp: "2026-07-16T12:00:00Z".into(),
            model: "gpt-4".into(),
            judge_model: "gpt-4-turbo".into(),
            total_prs: 10,
        };

        assert!(append_run_history(dir.path(), &entry).is_ok());

        let content =
            fs::read_to_string(dir.path().join(RUNS_FILE)).expect("read should succeed");
        let runs: Vec<RunHistoryEntry> =
            serde_json::from_str(&content).expect("deserialization should succeed");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].run_id, "run-001");
        assert_eq!(runs[0].model, "gpt-4");
        assert_eq!(runs[0].total_prs, 10);
    }

    #[test]
    fn test_runs_file_constant() {
        assert_eq!(RUNS_FILE, "_runs.json");
    }
}
