use std::{fs, path::Path};

use anyhow::Result;
use crb_reporting::RunHistoryEntry;
use tracing::info;

use crate::paths;

// TODO: Is this needed or does the new cache system handle this automatically?
/// Append a run history entry to the runs file in the cache directory.
fn append_run_history(cache_dir: &Path, entry: &RunHistoryEntry) -> Result<()> {
    let path = cache_dir.join(paths::RUNS_FILE);
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
