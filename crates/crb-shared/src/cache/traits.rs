use crb_judge::JudgeVerdict;
use rig_core::completion::Usage;

/// Interface for caching LLM interactions (prompts, responses, judge calls).
///
/// This is a trait so that different cache implementations can be injected
/// without creating circular dependencies between crates.
pub trait CacheBackend: Send + Sync {
    /// Save an agent prompt+response pair for the given role.
    fn save_agent(&self, role: &str, prompt: &str, response: &str);

    /// Append a judge call entry (golden comment, finding message, verdict JSON).
    fn save_judge(&self, golden: &str, finding: &str, verdict_json: &str);

    // ── Content-addressed caching methods ─────────────────────────────

    /// Look up a cached agent response by its content-addressed key.
    /// Returns `Some(response_text)` on cache hit, `None` on miss.
    fn lookup_agent_by_key(&self, _cache_key: &str) -> Option<String> {
        None
    }

    /// Look up a cached agent response by its content-addressed key,
    /// also returning the saved API usage data if available.
    /// Returns `Some((response_text, Option<usage>))` on cache hit.
    fn lookup_agent_by_key_with_usage(
        &self,
        _cache_key: &str,
    ) -> Option<(String, Option<Usage>)> {
        // Default: just return response with no usage
        self.lookup_agent_by_key(_cache_key)
            .map(|resp| (resp, None))
    }

    /// Look up a cached judge verdict by its content-addressed key.
    /// Returns `Some(JudgeVerdict)` on cache hit, `None` on miss.
    fn lookup_judge_by_key(&self, _cache_key: &str) -> Option<JudgeVerdict> {
        None
    }

    /// Save an agent prompt+response pair with a content-addressed cache key.
    fn save_agent_with_key(
        &self,
        _cache_key: &str,
        _role: &str,
        _prompt: &str,
        _response: &str,
    ) {
    }

    /// Save an agent prompt+response pair with a content-addressed cache key,
    /// including the API usage data.
    fn save_agent_with_key_and_usage(
        &self,
        _cache_key: &str,
        _role: &str,
        _prompt: &str,
        _response: &str,
        _usage: &Usage,
    ) {
    }

    /// Save agent reasoning/thinking text with a content-addressed cache key.
    fn save_agent_reasoning_with_key(
        &self,
        _cache_key: &str,
        _role: &str,
        _reasoning: &str,
    ) {
    }

    /// Save a judge verdict with a content-addressed cache key.
    fn save_judge_with_key(
        &self,
        _cache_key: &str,
        _golden: &str,
        _finding: &str,
        _verdict_json: &str,
    ) {
    }

    /// Look up a cached context gatherer response by its content-addressed key.
    fn lookup_context_by_key(&self, _cache_key: &str) -> Option<String> {
        None
    }

    /// Save a context gatherer prompt+response pair with a content-addressed cache key.
    fn save_context_with_key(&self, _cache_key: &str, _prompt: &str, _response: &str) {}
}
