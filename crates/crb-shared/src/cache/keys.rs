use crate::cache::sha256::sha256_hex;

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

    #[test]
    fn test_sha256_hex() {
        let h = sha256_hex("hello");
        assert_eq!(h.len(), 64); // SHA256 hex is 64 chars
    }
}
