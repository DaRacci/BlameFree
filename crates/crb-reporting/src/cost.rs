//! Cost tracking for LLM calls during PR evaluation.
//!
//! Tracks token counts, cache hit rates, and computes USD cost estimates.

use std::collections::HashMap;

use rig_core::completion::Usage;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

/// Thread-safe cost tracker for a single PR evaluation.
///
/// Wraps all counters in a `Mutex` so it can be shared across concurrent agent calls via `Arc<CostTracker>`.
#[derive(Debug, Default)]
pub struct CostTracker {
    inner: Mutex<CostTrackerInner>,
}

/// Snapshot of cost and usage statistics.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CostSnapshot {
    pub sessions: HashMap<String, SessionUsage>,
    pub cache_usage: HashMap<String, CacheUsage>,
}

/// Token usage and call counts for a single agent session
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct SessionUsage {
    /// The total number of tokens sent.
    pub input_tokens: u64,

    /// The total number of output tokens received.
    pub output_tokens: u64,

    /// The total number of tokens that were served from cache.
    pub cached_input_tokens: u64,

    /// The total number of tokens that were used to create a cache entry.
    pub cache_creation_input_tokens: u64,

    /// The total number of tokens used for reasoning.
    pub reasoning_tokens: u64,

    /// The total number of tokens used for tool use prompts.
    pub tool_use_prompt_tokens: u64,

    /// The total number of calls made by the agent.
    pub call_count: u64,

    /// The total number of tool calls made by the agent.
    pub tool_use_count: u64,
}

/// Cache usage statistics for a single agent session
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct CacheUsage {
    /// The total number of cache hits for the agent.
    pub cache_hits: u64,

    /// The total number of cache misses by the agent.
    pub cache_misses: u64,
}

impl CacheUsage {
    pub fn hit_rate(hits: usize, misses: usize) -> f64 {
        let total = hits + misses;
        if total != 0 {
            return hits as f64 / total as f64;
        }

        0.0
    }
}

trait SessionUsageProvider
where
    Self: Sized + Copy,
{
    fn get_usage(&self) -> SessionUsage;
}

impl SessionUsageProvider for Usage {
    fn get_usage(&self) -> SessionUsage {
        SessionUsage {
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            cached_input_tokens: self.cached_input_tokens,
            cache_creation_input_tokens: self.cache_creation_input_tokens,
            reasoning_tokens: self.reasoning_tokens,
            tool_use_prompt_tokens: self.tool_use_prompt_tokens,
            call_count: 1,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Default)]
struct CostTrackerInner {
    sessions: HashMap<String, SessionUsage>,

    cache_usage: HashMap<String, CacheUsage>,

    #[deprecated]
    agent_tokens_in: usize,
    #[deprecated]
    agent_tokens_out: usize,
    #[deprecated]
    judge_tokens_in: usize,
    #[deprecated]
    judge_tokens_out: usize,

    #[deprecated]
    agent_cached_input_tokens: usize,
    #[deprecated]
    agent_cache_creation_input_tokens: usize,
    #[deprecated]
    agent_reasoning_tokens: usize,
    #[deprecated]
    agent_tool_use_prompt_tokens: usize,
    #[deprecated]
    judge_cached_input_tokens: usize,
    #[deprecated]
    judge_cache_creation_input_tokens: usize,
    #[deprecated]
    judge_reasoning_tokens: usize,
    #[deprecated]
    judge_tool_use_prompt_tokens: usize,

    #[deprecated]
    agent_call_count: usize,
    #[deprecated]
    judge_call_count: usize,

    #[deprecated]
    agent_cache_hits: usize,
    #[deprecated]
    agent_cache_misses: usize,
    #[deprecated]
    judge_cache_hits: usize,
    #[deprecated]
    judge_cache_misses: usize,
}

impl CostTracker {
    /// Create a new empty cost tracker.
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(CostTrackerInner::default()),
        }
    }

    /// Record a call with API usage data.
    pub async fn record<T>(&self, key: String, provider: T, cache_hit: bool)
    where
        T: SessionUsageProvider,
    {
        let mut inner = self.inner.lock().await;

        if !inner.sessions.contains_key(&key) {
            inner.sessions.insert(key.clone(), Default::default());
        }

        if !inner.cache_usage.contains_key(&key) {
            inner.cache_usage.insert(key.clone(), Default::default());
        }

        {
            let usage = provider.get_usage();
            let _ = inner.sessions.entry(key.clone()).and_modify(|s| {
                s.input_tokens += usage.input_tokens;
                s.output_tokens += usage.output_tokens;
                s.cached_input_tokens += usage.cached_input_tokens;
                s.cache_creation_input_tokens += usage.cache_creation_input_tokens;
                s.reasoning_tokens += usage.reasoning_tokens;
                s.tool_use_prompt_tokens += usage.tool_use_prompt_tokens;
                s.call_count += usage.call_count;
            });
        };

        {
            let _ = inner.cache_usage.entry(key).and_modify(|c| {
                if cache_hit {
                    c.cache_hits += 1;
                } else {
                    c.cache_misses += 1;
                }
            });
        }
    }

    /// Record an agent LLM call with real API usage data.
    ///
    /// `usage` contains the token counts reported by the API.
    /// Pass `cache_hit = true` if the call was served from cache,
    /// `false` if it was a fresh API call.  On cache hits, usage is still
    /// recorded so analytics show token counts (cost is computed as 0).
    #[deprecated]
    pub fn record_agent(&self, usage: &Usage, cache_hit: bool) {
        unimplemented!()
    }

    /// Record a judge LLM call with real API usage data.
    ///
    /// `usage` contains the token counts reported by the API.
    /// Pass `cache_hit = true` if the call was served from cache,
    /// `false` if it was a fresh API call.  On cache hits, usage is still
    /// recorded so analytics show token counts (cost is computed as 0).
    #[deprecated]
    pub fn record_judge(&self, usage: &Usage, cache_hit: bool) {
        unimplemented!()
    }

    /// Record an agent call with a default (zero) Usage.
    /// Used when usage data isn't available (e.g., legacy cache hits that have no stored usage).
    #[deprecated]
    pub fn record_agent_empty(&self, cache_hit: bool) {
        unimplemented!()
    }

    /// Record a judge call with a default (zero) Usage.
    #[deprecated]
    pub fn record_judge_empty(&self, cache_hit: bool) {
        unimplemented!()
    }

    /// Total estimated cost in USD, computed from env-configured pricing rates.
    ///
    /// Pricing rates are read from environment variables (see module docs for defaults).
    /// The formula is:
    /// ```text
    /// cost = (tokens_in * input_price_per_token) + (tokens_out * output_price_per_token)
    /// ```
    /// where prices are per-token (derived from per-1M-token rates).
    #[deprecated]
    pub fn total_cost(&self) -> f64 {
        unimplemented!()
    }

    /// Cache hit rate for agent calls (0.0 to 1.0).
    /// Returns 0.0 if no calls were made.
    #[deprecated]
    pub async fn agent_cache_hit_rate(&self) -> f64 {
        unimplemented!()
    }

    /// Cache hit rate for judge calls (0.0 to 1.0).
    /// Returns 0.0 if no calls were made.
    #[deprecated]
    pub fn judge_cache_hit_rate(&self) -> f64 {
        unimplemented!()
    }

    /// Total token counts across both agent and judge calls.
    /// Returns `(total_tokens_in, total_tokens_out)`.
    #[deprecated]
    pub async fn total_tokens(&self) -> (u64, u64) {
        let inner = self.inner.lock().await;
        inner
            .sessions
            .iter()
            .fold((0, 0), |(acc_in, acc_out), (_, usage)| {
                (acc_in + usage.input_tokens, acc_out + usage.output_tokens)
            })
    }

    /// Build a [`CostSnapshot`] of the current state.
    pub async fn to_snapshot(&self) -> CostSnapshot {
        let inner = self.inner.lock().await;
        CostSnapshot {
            sessions: inner.sessions.clone(),
            cache_usage: inner.cache_usage.clone(),
        }
    }
}

impl CostSnapshot {
    /// Total estimated cost in USD, computed from env-configured pricing rates.
    ///
    /// Pricing rates are read from environment variables (see module docs for defaults).
    /// The formula is:
    /// ```text
    /// cost = (tokens_in * input_price_per_token) + (tokens_out * output_price_per_token)
    /// ```
    /// where prices are per-token (derived from per-1M-token rates).
    pub fn total_cost(&self) -> f64 {
        todo!()
    }

    // Total token counts across both agent and judge calls.
    /// Returns `(total_tokens_in, total_tokens_out)`.
    pub async fn total_tokens(&self) -> (u64, u64) {
        self.sessions
            .iter()
            .fold((0, 0), |(acc_in, acc_out), (_, usage)| {
                (acc_in + usage.input_tokens, acc_out + usage.output_tokens)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_usage(
        input: u64,
        output: u64,
        cached: u64,
        cache_create: u64,
        reasoning: u64,
        tool: u64,
    ) -> Usage {
        let mut u = Usage::new();
        u.input_tokens = input;
        u.output_tokens = output;
        u.total_tokens = input + output;
        u.cached_input_tokens = cached;
        u.cache_creation_input_tokens = cache_create;
        u.reasoning_tokens = reasoning;
        u.tool_use_prompt_tokens = tool;
        u
    }

    #[test]
    fn test_record_agent_and_judge() {
        let tracker = CostTracker::new();
        let usage1 = make_usage(100, 50, 10, 5, 3, 2);
        let usage2 = make_usage(200, 100, 20, 10, 6, 4);
        let usage3 = make_usage(30, 20, 5, 0, 1, 0);

        tracker.record_agent(&usage1, true); // cache hit
        tracker.record_agent(&usage2, false); // cache miss
        tracker.record_judge(&usage3, true); // cache hit

        let (total_in, total_out) = tracker.total_tokens();
        assert_eq!(total_in, 100 + 200 + 30);
        assert_eq!(total_out, 50 + 100 + 20);

        assert!((tracker.agent_cache_hit_rate() - 0.5).abs() < 1e-6);
        assert!((tracker.judge_cache_hit_rate() - 1.0).abs() < 1e-6);

        let summary = tracker.to_summary();
        assert_eq!(summary.agent_cached_input_tokens, 10 + 20);
        assert_eq!(summary.agent_reasoning_tokens, 3 + 6);
        assert_eq!(summary.agent_tool_use_prompt_tokens, 2 + 4);
        assert_eq!(summary.judge_cached_input_tokens, 5);
        assert_eq!(summary.judge_reasoning_tokens, 1);
        assert_eq!(summary.agent_call_count, 2);
        assert_eq!(summary.judge_call_count, 1);
    }

    #[test]
    fn test_cache_hit_rate_no_calls() {
        let tracker = CostTracker::new();
        assert_eq!(tracker.agent_cache_hit_rate(), 0.0);
        assert_eq!(tracker.judge_cache_hit_rate(), 0.0);
    }

    #[test]
    fn test_usd_cost_with_default_rates() {
        let tracker = CostTracker::new();
        // 1M tokens in @ $0.14/1M = $0.14; 500K out @ $0.28/1M = $0.14; total = $0.28
        let usage = make_usage(1_000_000, 500_000, 0, 0, 0, 0);
        tracker.record_agent(&usage, false);
        let cost = tracker.total_cost();
        assert!((cost - 0.28).abs() < 0.001, "Expected ~0.28, got {cost}");
    }

    #[test]
    fn test_record_empty_usage() {
        let tracker = CostTracker::new();
        tracker.record_agent_empty(false);
        tracker.record_judge_empty(true);

        let summary = tracker.to_summary();
        assert_eq!(summary.agent_tokens_in, 0);
        assert_eq!(summary.agent_call_count, 1);
        assert_eq!(summary.judge_call_count, 1);
        assert!(
            (summary.judge_cache_hit_rate - 1.0).abs() < 0.001,
            "Expected judge_cache_hit_rate ~1.0"
        );
        assert!(
            (summary.agent_cache_hit_rate - 0.0).abs() < 0.001,
            "Expected agent_cache_hit_rate ~0.0"
        );
    }
}
