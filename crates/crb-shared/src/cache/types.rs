use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Type alias for cache results.
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// A single entry in the run history log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunHistoryEntry {
    /// Unique run identifier.
    pub run_id: String,
    /// ISO-8601 timestamp of the run.
    pub timestamp: String,
    /// Model used for review agents.
    pub model: String,
    /// Model used for the judge.
    pub judge_model: String,
    /// Total PRs in this run.
    pub total_prs: usize,
    /// Run duration in seconds.
    pub duration_secs: f64,
    /// Total cost in USD.
    pub total_cost_usd: f64,
    /// Total tokens consumed.
    pub total_tokens: usize,
    /// Agent cache hit rate (0.0–1.0).
    pub agent_cache_hit_rate: f64,
    /// Judge cache hit rate (0.0–1.0).
    pub judge_cache_hit_rate: f64,
}

/// A single entry in the cache index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CacheEntry {
    /// Relative path from the PR cache directory to the cached file.
    pub(crate) file_path: String,

    /// Unix epoch timestamp with nanosecond precision (seconds.nanoseconds).
    pub(crate) timestamp: String,

    /// Model name used for this interaction.
    pub(crate) model: String,

    /// Optional token count (set if the provider reports it).
    pub(crate) tokens_used: Option<u32>,
}

/// In-memory index of all cached entries for a single PR.
/// Persisted to `index.json` after each write.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CacheIndex {
    /// Maps cache_key -> entry metadata.
    pub(crate) entries: HashMap<String, CacheEntry>,
}

impl CacheIndex {
    pub(crate) fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Load the index from a JSON file.
    pub(crate) fn load(path: &std::path::Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_else(|e| {
                tracing::warn!("Failed to parse cache index at {}: {e}", path.display());
                Self::new()
            }),
            Err(_) => Self::new(),
        }
    }

    /// Save the index to a JSON file.
    pub(crate) fn save(&self, path: &std::path::Path) {
        if let Err(e) =
            std::fs::write(path, serde_json::to_string_pretty(self).unwrap_or_default())
        {
            tracing::warn!("Failed to write cache index: {e}");
        }
    }
}

/// Statistics for a single PR's cache usage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrCacheStats {
    /// The PR key identifying this cache subdirectory.
    pub pr_key: String,
    /// Number of entries in the cache index.
    pub entry_count: usize,
    /// Total byte size of the PR cache directory on disk.
    pub total_size_bytes: u64,
    /// Timestamp of the oldest cached entry, if any.
    pub oldest_entry: Option<String>,
    /// Timestamp of the newest cached entry, if any.
    pub newest_entry: Option<String>,
}

/// Aggregate statistics across all PRs in a cache directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalCacheStats {
    /// Number of PR directories found.
    pub pr_count: usize,
    /// Total entries across all PR indices.
    pub total_entries: usize,
    /// Total byte size across all PR cache directories.
    pub total_size_bytes: u64,
    /// Per-PR breakdown of cache stats.
    pub per_pr: Vec<PrCacheStats>,
}

/// Result of a cache prune operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PruneResult {
    /// Number of PR directories completely removed.
    pub prs_removed: usize,
    /// Total entries removed across all PRs.
    pub entries_removed: usize,
    /// Total bytes freed by removing entries/files.
    pub bytes_freed: u64,
    /// Number of PR directories kept after pruning.
    pub prs_kept: usize,
}

/// Result of a cache scrub operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrubResult {
    /// Number of PR directories scanned.
    pub pr_dirs_scanned: usize,
    /// Stale entries found (files missing from disk).
    pub stale_entries_found: usize,
    /// Orphan files found (on disk but not in index).
    pub orphan_files_found: usize,
    /// Corrupted index files found.
    pub corrupted_indices_found: usize,
    /// Indices rebuilt from filesystem scan.
    pub indices_rebuilt: usize,
    /// Stale entries that were removed (if repair mode).
    pub stale_entries_removed: usize,
    /// Orphan files that were removed (if repair mode).
    pub orphan_files_removed: usize,
}
