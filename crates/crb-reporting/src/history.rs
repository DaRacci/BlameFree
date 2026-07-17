use std::{fs, path::Path};

use anyhow::Result;
use tracing::info;

use crb_webui_shared::runs::RunMeta;

/// Append-only run-history log.
pub const RUNS_FILE: &str = "_runs.json";

// TODO: Is this needed or does the new cache system handle this automatically?
/// Append a run history entry to the runs file in the cache directory.
pub fn append_run_history(cache_dir: &Path, entry: &RunMeta) -> Result<()> {
    let path = cache_dir.join(RUNS_FILE);
    let mut runs: Vec<RunMeta> = if path.exists() {
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
        let entry = RunMeta {
            id: "run-001".into(),
            name: "run-001".into(),
            pr_count: 5,
            total_cost: None,
            total_tokens: 0,
            duration_secs: None,
            model: Some("gpt-4".into()),
            status: crb_webui_shared::runs::RunStatus::Completed,
        };

        let result = append_run_history(dir.path(), &entry);
        assert!(result.is_ok());

        let runs_path = dir.path().join(RUNS_FILE);
        assert!(runs_path.exists(), "_runs.json should exist");
    }

    #[test]
    fn test_append_run_history_appends_to_existing() {
        let dir = tempfile::TempDir::new().expect("tempdir creation should succeed");
        let entry1 = RunMeta {
            id: "run-001".into(),
            name: "run-001".into(),
            pr_count: 5,
            total_cost: None,
            total_tokens: 0,
            duration_secs: None,
            model: Some("gpt-4".into()),
            status: crb_webui_shared::runs::RunStatus::Completed,
        };
        let entry2 = RunMeta {
            id: "run-002".into(),
            name: "run-002".into(),
            pr_count: 3,
            total_cost: None,
            total_tokens: 0,
            duration_secs: None,
            model: Some("claude-3".into()),
            status: crb_webui_shared::runs::RunStatus::Completed,
        };

        assert!(append_run_history(dir.path(), &entry1).is_ok());
        assert!(append_run_history(dir.path(), &entry2).is_ok());

        let content = fs::read_to_string(dir.path().join(RUNS_FILE)).expect("read should succeed");
        let runs: Vec<RunMeta> =
            serde_json::from_str(&content).expect("deserialization should succeed");
        assert_eq!(runs.len(), 2);
    }

    #[test]
    fn test_append_run_history_content() {
        let dir = tempfile::TempDir::new().expect("tempdir creation should succeed");
        let entry = RunMeta {
            id: "run-001".into(),
            name: "run-001".into(),
            pr_count: 10,
            total_cost: None,
            total_tokens: 0,
            duration_secs: None,
            model: Some("gpt-4".into()),
            status: crb_webui_shared::runs::RunStatus::Completed,
        };

        assert!(append_run_history(dir.path(), &entry).is_ok());

        let content = fs::read_to_string(dir.path().join(RUNS_FILE)).expect("read should succeed");
        let runs: Vec<RunMeta> =
            serde_json::from_str(&content).expect("deserialization should succeed");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].id, "run-001");
        assert_eq!(runs[0].model, Some("gpt-4".to_string()));
        assert_eq!(runs[0].pr_count, 10);
    }

    #[test]
    fn test_runs_file_constant() {
        assert_eq!(RUNS_FILE, "_runs.json");
    }
}
