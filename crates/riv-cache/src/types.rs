use std::{collections::HashMap, fs, path::Path, time};

use serde::{Deserialize, Serialize};
use tracing::warn;

/// Current timestamp as a formatted string.
pub fn now() -> String {
    let dur = time::SystemTime::now()
        .duration_since(time::UNIX_EPOCH)
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
    pub fn load(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_else(|e| {
                warn!("Failed to parse cache index at {}: {e}", path.display());
                Self::new()
            }),
            Err(_) => Self::new(),
        }
    }

    /// Persist the index to a JSON file.
    pub fn save(&self, path: &Path) {
        if let Err(e) = fs::write(path, serde_json::to_string_pretty(self).unwrap_or_default()) {
            warn!("Failed to write cache index: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn test_now_format() {
        let ts = now();
        // Format should be: seconds.nanoseconds (9 digits nanos)
        let parts: Vec<&str> = ts.split('.').collect();
        assert_eq!(parts.len(), 2, "timestamp should have exactly one dot separator");
        // Seconds part should be a non-empty number
        assert!(!parts[0].is_empty(), "seconds part should not be empty");
        assert!(parts[0].chars().all(|c| c.is_ascii_digit()), "seconds part should be digits only");
        // Nanos part should be exactly 9 digits
        assert_eq!(parts[1].len(), 9, "nanos part should be 9 digits");
        assert!(parts[1].chars().all(|c| c.is_ascii_digit()), "nanos part should be digits only");
    }

    #[test]
    fn test_cache_entry_deserialization() -> Result<(), Box<dyn std::error::Error>> {
        let json = r#"{
            "file_path": "test.json",
            "timestamp": "1000000000.000000000",
            "model": "claude-3",
            "tokens_used": null
        }"#;
        let entry: CacheEntry = serde_json::from_str(json)?;
        assert_eq!(entry.file_path, "test.json");
        assert_eq!(entry.model, "claude-3");
        assert!(entry.tokens_used.is_none());
        Ok(())
    }

    #[test]
    fn test_cache_index_new_is_empty() {
        let index = CacheIndex::new();
        assert!(index.entries.is_empty());
    }

    #[test]
    fn test_cache_index_load_nonexistent_file() {
        let path = PathBuf::from("/tmp/__nonexistent_cache_index__.json");
        let index = CacheIndex::load(&path);
        assert!(index.entries.is_empty());
    }

    #[test]
    fn test_cache_index_load_invalid_json() {
        if let Ok(dir) = tempfile::TempDir::new() {
            let path = dir.path().join("invalid.json");
            if fs::write(&path, "this is not valid json").is_ok() {
                let index = CacheIndex::load(&path);
                assert!(index.entries.is_empty());
            }
        }
    }

    #[test]
    fn test_cache_index_save_and_load() {
        if let Ok(dir) = tempfile::TempDir::new() {
            let path = dir.path().join("index.json");

            let mut index = CacheIndex::new();
            index.entries.insert(
                "key1".into(),
                CacheEntry {
                    file_path: "key1.json".into(),
                    timestamp: "1000000000.000000000".into(),
                    model: "gpt-4".into(),
                    tokens_used: Some(42),
                },
            );
            index.save(&path);

            assert!(path.exists(), "index file should exist after save");

            let loaded = CacheIndex::load(&path);
            assert_eq!(loaded.entries.len(), 1);
            if let Some(entry) = loaded.entries.get("key1") {
                assert_eq!(entry.file_path, "key1.json");
                assert_eq!(entry.model, "gpt-4");
                assert_eq!(entry.tokens_used, Some(42));
            }
        }
    }

    #[test]
    fn test_cache_index_insert_multiple_entries() {
        if let Ok(dir) = tempfile::TempDir::new() {
            let path = dir.path().join("index.json");

            let mut index = CacheIndex::new();
            index.entries.insert(
                "alpha".into(),
                CacheEntry {
                    file_path: "alpha.json".into(),
                    timestamp: "1000000000.000000000".into(),
                    model: "gpt-4".into(),
                    tokens_used: None,
                },
            );
            index.entries.insert(
                "beta".into(),
                CacheEntry {
                    file_path: "beta.json".into(),
                    timestamp: "1000000001.000000000".into(),
                    model: "claude-3".into(),
                    tokens_used: Some(99),
                },
            );
            index.save(&path);

            let loaded = CacheIndex::load(&path);
            assert_eq!(loaded.entries.len(), 2);
            assert!(loaded.entries.contains_key("alpha"));
            assert!(loaded.entries.contains_key("beta"));
        }
    }
}
