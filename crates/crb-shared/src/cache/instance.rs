use std::collections::HashMap;

use crate::cache::traits::{CachableData, CacheBackend};

/// A cache instance for use during a review session.
pub struct CacheInstance<'b, 'k> {
    backend: Box<&'b dyn CacheBackend>,

    input_identifiers: HashMap<&'k str, String>,
}

impl CacheInstance<'_, '_> {
    /// Creates a new cache instance with the given backend.
    pub fn new(backend: Box<&dyn CacheBackend>) -> CacheInstance {
        CacheInstance {
            backend,
            input_identifiers: HashMap::new(),
        }
    }

    pub fn save_data<T>(&mut self, key: &str, value: &T)
    where
        T: CachableData,
    {
        let data = value.get_savable_data(&self.backend);
    }
}
