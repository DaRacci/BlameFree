use std::{fs, path::Path};

use anyhow::Result;
use riv_types::review::Review;
use tracing::info;

/// Append-only run-history log.
pub const RUNS_FILE: &str = "_runs.json";

// TODO: Is this needed or does the new cache system handle this automatically?
/// Append a run history entry to the runs file in the cache directory.
pub fn append_run_history(cache_dir: &Path, entry: &Review) -> Result<()> {
    let path = cache_dir.join(RUNS_FILE);
    let mut runs: Vec<Review> = if path.exists() {
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
    use mti::prelude::{MagicTypeIdExt, V7};
    use riv_types::review::{ReviewMetadata, ReviewStatus};
    use std::collections::HashMap;

    use super::*;

    #[test]
    #[ignore = "Needs migration to Review field schema — previously used RunMeta with name/pr_count/total_cost/total_tokens"]
    fn test_append_run_history_creates_file() {
        let dir = tempfile::TempDir::new().expect("tempdir creation should succeed");
        let entry = Review {
            id: "run-001".create_type_id::<V7>(),
            agent_sessions: Vec::new(),
            analytics: None,
            duration: None,
            status: ReviewStatus::Completed,
            metadata: (),
        };

        let result = append_run_history(dir.path(), &entry);
        assert!(result.is_ok());

        let runs_path = dir.path().join(RUNS_FILE);
        assert!(runs_path.exists(), "_runs.json should exist");
    }

    #[test]
    #[ignore = "Needs migration to Review field schema — previously used RunMeta with name/pr_count/total_cost/total_tokens"]
    fn test_append_run_history_appends_to_existing() {
        let dir = tempfile::TempDir::new().expect("tempdir creation should succeed");
        let entry1 = Review {
            id: "run-001".create_type_id::<V7>(),
            agent_sessions: Vec::new(),
            analytics: None,
            duration: None,
            status: ReviewStatus::Completed,
            metadata: ReviewMetadata::Plain,
        };
        let entry2 = Review {
            id: "run-002".create_type_id::<V7>(),
            agent_sessions: Vec::new(),
            analytics: None,
            duration: None,
            status: ReviewStatus::Completed,
            metadata: ReviewMetadata::Plain,
        };

        assert!(append_run_history(dir.path(), &entry1).is_ok());
        assert!(append_run_history(dir.path(), &entry2).is_ok());

        let content = fs::read_to_string(dir.path().join(RUNS_FILE)).expect("read should succeed");
        let runs: Vec<Review> =
            serde_json::from_str(&content).expect("deserialization should succeed");
        assert_eq!(runs.len(), 2);
    }

    #[test]
    #[ignore = "Needs migration to Review field schema — previously used RunMeta with name/pr_count/total_cost/total_tokens"]
    fn test_append_run_history_content() {
        let dir = tempfile::TempDir::new().expect("tempdir creation should succeed");
        let entry = Review {
            id: "run-001".create_type_id::<V7>(),
            agent_sessions: Vec::new(),
            analytics: None,
            duration: None,
            status: ReviewStatus::Completed,
            metadata: ReviewMetadata::Plain,
        };

        assert!(append_run_history(dir.path(), &entry).is_ok());

        let content = fs::read_to_string(dir.path().join(RUNS_FILE)).expect("read should succeed");
        let runs: Vec<Review> =
            serde_json::from_str(&content).expect("deserialization should succeed");
        assert_eq!(runs.len(), 1);
    }
}
