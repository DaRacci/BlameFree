use crb_types::wrappers::{Model, WrappedData};

use crate::cache::{sha256::sha256_hex, traits::CachableData};

/// Compute a content-addressed cache key for an agent LLM call.
///
/// Components (all SHA256 hex digests or plain strings) are concatenated
/// and hashed again to produce a single deterministic key.
pub fn compute_agent_cache_key(
    prompt_hash: &str,
    diff_hash: &str,
    model_name: &str,
    role: &str,
    rules_hash: &str,
) -> String {
    sha256_hex(&format!(
        "{}:{}:{}:{}:{}",
        prompt_hash, diff_hash, model_name, role, rules_hash
    ))
}

/// Compute a content-addressed cache key for a judge LLM call.
#[deprecated]
pub fn compute_judge_cache_key(
    judge_prompt_hash: &str,
    finding_message: &str,
    golden_comment: &str,
    judge_model: &str,
) -> String {
    sha256_hex(&format!(
        "{}:{}:{}:{}",
        judge_prompt_hash, finding_message, golden_comment, judge_model
    ))
}

pub struct JudgeCacheKeyComponents<'a> {
    pub judge_prompt_hash: &'a str,
    pub finding_message: &'a str,
    pub golden_comment: &'a str,
    pub judge_model: &'a Model,
}

impl CachableData for JudgeCacheKeyComponents<'_> {
    fn cache_key(&self) -> String {
        sha256_hex(&format!(
            "{}:{}:{}:{}",
            self.judge_prompt_hash,
            self.finding_message,
            self.golden_comment,
            self.judge_model.get()
        ))
    }

    fn get_savable_data<T>(&self, cache: &dyn super::traits::CacheBackend) -> Option<T>
    where
        T: serde::Serialize + serde::de::DeserializeOwned;
}

/// Compute a content-addressed cache key for a context gatherer LLM call.
pub fn compute_context_cache_key(
    gatherer_prompt_hash: &str,
    diff_hash: &str,
    repo_state_hash: &str,
    model_name: &str,
) -> String {
    sha256_hex(&format!(
        "{}:{}:{}:{}",
        gatherer_prompt_hash, diff_hash, repo_state_hash, model_name
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_agent_cache_key_deterministic() {
        let key1 = compute_agent_cache_key("abc", "def", "gpt-4o", "SA", "rules123");
        let key2 = compute_agent_cache_key("abc", "def", "gpt-4o", "SA", "rules123");
        assert_eq!(key1, key2);
        // Different input should produce different key
        let key3 = compute_agent_cache_key("abc", "xyz", "gpt-4o", "SA", "rules123");
        assert_ne!(key1, key3);
    }

    #[test]
    fn test_compute_judge_cache_key_deterministic() {
        let key1 = compute_judge_cache_key("jph", "finding msg", "golden comment", "gpt-4o-mini");
        let key2 = compute_judge_cache_key("jph", "finding msg", "golden comment", "gpt-4o-mini");
        assert_eq!(key1, key2);
    }
}
