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
        std::fs::create_dir_all(&dir).ok();
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
        std::fs::write(&path, value).ok();

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
        std::fs::read_to_string(&path).unwrap_or_default()
    }
}
