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

    #[deprecated = "This is handled by [`crb_types::benchmark::Metrics`]"]
    pub duration_secs: f64,

    #[deprecated = "This is handled by [`crb_reporting::cost::CostSnapshot`]"]
    pub total_cost_usd: f64,

    #[deprecated = "This is handled by [`crb_types::benchmark::Metrics`]"]
    pub total_tokens: usize,

    #[deprecated = "This is handled by [`crb_types::benchmark::Metrics`]"]
    pub agent_cache_hit_rate: f64,

    #[deprecated = "This is handled by [`crb_types::benchmark::Metrics`]"]
    pub judge_cache_hit_rate: f64,
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
