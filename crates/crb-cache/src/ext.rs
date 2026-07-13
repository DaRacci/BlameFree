use crb_types::wrappers::{Model, Prompt};
use sha2::{Digest, Sha256};

use crate::traits::CacheKey;

impl CacheKey for Prompt {
    fn cache_key(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.0.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

impl CacheKey for Model {
    fn cache_key(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.0.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}
