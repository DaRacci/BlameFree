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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_cache_key_deterministic() {
        let prompt = Prompt("test prompt".into());
        let key1 = prompt.cache_key();
        let key2 = prompt.cache_key();
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_prompt_cache_key_length() {
        let prompt = Prompt("test prompt".into());
        let key = prompt.cache_key();
        assert_eq!(key.len(), 64);
    }

    #[test]
    fn test_prompt_cache_key_different_inputs() {
        let prompt_a = Prompt("hello".into());
        let prompt_b = Prompt("world".into());
        assert_ne!(prompt_a.cache_key(), prompt_b.cache_key());
    }

    #[test]
    fn test_prompt_cache_key_empty() {
        let prompt = Prompt(String::new());
        let key = prompt.cache_key();
        assert_eq!(key.len(), 64);
    }

    #[test]
    fn test_model_cache_key_deterministic() {
        let model = Model("gpt-4".into());
        let key1 = model.cache_key();
        let key2 = model.cache_key();
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_model_cache_key_length() {
        let model = Model("gpt-4".into());
        let key = model.cache_key();
        assert_eq!(key.len(), 64);
    }

    #[test]
    fn test_model_cache_key_different_inputs() {
        let model_a = Model("gpt-4".into());
        let model_b = Model("claude-3".into());
        assert_ne!(model_a.cache_key(), model_b.cache_key());
    }
}
