use crb_judge::JudgeVerdict;
use crb_types::wrappers::{Diff, WrappedData};
use rig_core::completion::Usage;
use serde::{Serialize, de::DeserializeOwned};

use crate::cache::sha256::sha256_hex;

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
    fn lookup_agent_by_key_with_usage(&self, _cache_key: &str) -> Option<(String, Option<Usage>)> {
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
    fn save_agent_with_key(&self, _cache_key: &str, _role: &str, _prompt: &str, _response: &str) {}

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
    fn save_agent_reasoning_with_key(&self, _cache_key: &str, _role: &str, _reasoning: &str) {}

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

pub trait CachableData: Send + Sync {
    /// The content type of the data.
    const CONTENT_TYPE: &'static str = "plain/text";

    fn cache_key(&self) -> String;

    fn get_savable_data<T>(&self, cache: &dyn CacheBackend) -> Option<T>
    where
        T: Serialize + DeserializeOwned;
}

impl CachableData for String {
    const CONTENT_TYPE: &'static str = "text/plain";

    fn cache_key(&self) -> String {
        format!("{}", sha256_hex(self))
    }

    fn get_savable_data<T>(&self, _cache: &dyn CacheBackend) -> Option<T>
    where
        T: Serialize + DeserializeOwned,
    {
        if let Ok(data) = serde_json::from_str::<T>(self) {
            Some(data)
        } else {
            None
        }
    }
}

impl CachableData for Diff {
    const CONTENT_TYPE: &'static str = "plain/text";

    fn cache_key(&self) -> String {
        format!("{}", sha256_hex(self.get()))
    }

    fn get_savable_data<T>(&self, _cache: &dyn CacheBackend) -> Option<T>
    where
        T: Serialize + DeserializeOwned,
    {
        if let Ok(data) = serde_json::from_str::<T>(self) {
            Some(data)
        } else {
            None
        }
    }
}
