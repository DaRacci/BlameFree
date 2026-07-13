use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Current timestamp as a formatted string.
pub fn now() -> String {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}.{:09}", dur.as_secs(), dur.subsec_nanos())
}

/// A single cache entry tracked in the index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub file_path: String,
    pub timestamp: String,
    pub model: String,
    pub tokens_used: Option<u32>,
}

/// On-disk index mapping cache keys to their entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheIndex {
    pub entries: HashMap<String, CacheEntry>,
}

impl CacheIndex {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Load the index from a JSON file, returning an empty index on failure.
    pub fn load(path: &std::path::Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_else(|e| {
                tracing::warn!("Failed to parse cache index at {}: {e}", path.display());
                Self::new()
            }),
            Err(_) => Self::new(),
        }
    }

    /// Persist the index to a JSON file.
    pub fn save(&self, path: &std::path::Path) {
        if let Err(e) = std::fs::write(
            path,
            serde_json::to_string_pretty(self).unwrap_or_default(),
        ) {
            tracing::warn!("Failed to write cache index: {e}");
        }
    }
}
