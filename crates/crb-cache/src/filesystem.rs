use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::paths;
use crate::traits::CacheBackend;
use crate::types::{CacheEntry, CacheIndex};

/// A [`CacheBackend`] that stores cache entries as individual JSON files on
/// the filesystem, together with an in-memory index that is persisted to `index.json`.
pub struct FilesystemBackend {
    base_dir: PathBuf,
    index: Mutex<CacheIndex>,
}

impl FilesystemBackend {
    /// Create a new [`FilesystemBackend`] rooted at `base_dir`.
    ///
    /// The directory is created if it does not exist.
    /// Any existing index stored under [`paths::INDEX_FILE`] is loaded on construction.
    pub fn new(base_dir: &Path) -> Self {
        let dir = base_dir.to_path_buf();
        fs::create_dir_all(&dir).ok();
        let index_path = dir.join(paths::INDEX_FILE);
        let index = CacheIndex::load(&index_path);
        Self {
            base_dir: dir,
            index: Mutex::new(index),
        }
    }
}

impl CacheBackend for FilesystemBackend {
    fn store_raw(&self, key: &str, value: &str) {
        let path = self.base_dir.join(format!("{key}.json"));
        fs::write(&path, value).ok();

        if let Ok(mut idx) = self.index.lock() {
            idx.entries.insert(
                key.to_string(),
                CacheEntry {
                    file_path: format!("{key}.json"),
                    timestamp: crate::types::now(),
                    model: String::new(),
                    tokens_used: None,
                },
            );
            idx.save(&self.base_dir.join(paths::INDEX_FILE));
        }
    }

    fn load_raw(&self, key: &str) -> String {
        let path = self.base_dir.join(format!("{key}.json"));
        fs::read_to_string(&path).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_directory() {
        if let Ok(dir) = tempfile::TempDir::new() {
            let cache_dir = dir.path().join(paths::CACHE_DIR_NAME);
            let _backend = FilesystemBackend::new(&cache_dir);
            assert!(cache_dir.exists(), "cache directory should be created");
            assert!(cache_dir.is_dir(), "cache directory should be a directory");
        }
    }

    #[test]
    fn test_store_creates_file() {
        if let Ok(dir) = tempfile::TempDir::new() {
            let backend = FilesystemBackend::new(dir.path());
            backend.store_raw("my-key", "some data");
            let file_path = dir.path().join("my-key.json");
            assert!(file_path.exists(), "cache file should exist on disk");
            if let Ok(content) = fs::read_to_string(&file_path) {
                assert_eq!(content, "some data");
            }
        }
    }

    #[test]
    fn test_load_missing_key_returns_empty() {
        if let Ok(dir) = tempfile::TempDir::new() {
            let backend = FilesystemBackend::new(dir.path());
            let loaded = backend.load_raw("nonexistent");
            assert!(loaded.is_empty(), "missing key should return empty string");
        }
    }

    #[test]
    fn test_index_updated_on_store() {
        if let Ok(dir) = tempfile::TempDir::new() {
            let backend = FilesystemBackend::new(dir.path());
            backend.store_raw("idx-key", "value");

            let index_path = dir.path().join(paths::INDEX_FILE);
            assert!(index_path.exists(), "index file should exist after store");

            if let Ok(content) = fs::read_to_string(&index_path) {
                let index: CacheIndex = serde_json::from_str(&content).expect("valid JSON");
                assert!(
                    index.entries.contains_key("idx-key"),
                    "index should contain the stored key"
                );
                if let Some(entry) = index.entries.get("idx-key") {
                    assert_eq!(entry.file_path, "idx-key.json");
                }
            }
        }
    }

    #[test]
    fn test_multiple_stores_preserve_index() {
        if let Ok(dir) = tempfile::TempDir::new() {
            let backend = FilesystemBackend::new(dir.path());
            backend.store_raw("key-a", "value-a");
            backend.store_raw("key-b", "value-b");

            let index_path = dir.path().join(paths::INDEX_FILE);
            if let Ok(content) = fs::read_to_string(&index_path) {
                let index: CacheIndex = serde_json::from_str(&content).expect("valid JSON");
                assert_eq!(index.entries.len(), 2);
                assert!(index.entries.contains_key("key-a"));
                assert!(index.entries.contains_key("key-b"));
            }

            assert!(dir.path().join("key-a.json").exists());
            assert!(dir.path().join("key-b.json").exists());
        }
    }

    #[test]
    fn test_load_from_existing_index() {
        if let Ok(dir) = tempfile::TempDir::new() {
            {
                let backend = FilesystemBackend::new(dir.path());
                backend.store_raw("pre-existing", "hello");
            }

            let backend = FilesystemBackend::new(dir.path());
            let loaded = backend.load_raw("pre-existing");
            assert_eq!(loaded, "hello");
        }
    }

    #[test]
    fn test_overwrite_existing_key() {
        if let Ok(dir) = tempfile::TempDir::new() {
            let backend = FilesystemBackend::new(dir.path());
            backend.store_raw("overwrite", "first");
            backend.store_raw("overwrite", "second");

            let loaded = backend.load_raw("overwrite");
            assert_eq!(
                loaded, "second",
                "should return the latest value after overwrite"
            );

            // Verify file content is updated
            let file_path = dir.path().join("overwrite.json");
            if let Ok(content) = fs::read_to_string(&file_path) {
                assert_eq!(content, "second");
            }
        }
    }
}
