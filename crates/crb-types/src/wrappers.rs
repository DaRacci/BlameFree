use crb_cache::traits::CacheKey;
use sha2::{Digest, Sha256};

pub trait WrappedData {
    fn get(&self) -> &str;
}

pub struct Diff(pub String);

impl WrappedData for Diff {
    fn get(&self) -> &str {
        &self.0
    }
}

pub struct Prompt(pub String);

impl WrappedData for Prompt {
    fn get(&self) -> &str {
        &self.0
    }
}

pub struct Model(pub String);

impl WrappedData for Model {
    fn get(&self) -> &str {
        &self.0
    }
}

impl CacheKey for Diff {
    fn cache_key(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.0.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

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
