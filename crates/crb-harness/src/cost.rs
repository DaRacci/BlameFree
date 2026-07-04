//! Cost tracking for LLM calls during PR evaluation.
//!
//! Tracks token counts, cache hit rates, and computes USD cost
//! from pricing rates configured via environment variables.
//!
//! # Real API usage
//! Token counts now come from real `Usage` data reported by the API
//! through rig-core's `PromptResponse.usage` field (obtained via
//! `.extended_details()` on agent/judge prompt calls).
//!
//! # Pricing
//! Pricing rates are read from environment variables:
//! - `COST_AGENT_INPUT_PER_1M`          (default: 0.14, deepseek-v4-flash)
//! - `COST_AGENT_OUTPUT_PER_1M`         (default: 0.28)
//! - `COST_AGENT_CACHE_READ_PER_1M`     (default: 0.0028)
//! - `COST_AGENT_REASONING_PER_1M`      (default: 0.28, same as output rate)
//! - `COST_JUDGE_INPUT_PER_1M`          (default: 0.14)
//! - `COST_JUDGE_OUTPUT_PER_1M`         (default: 0.28)
//! - `COST_JUDGE_CACHE_READ_PER_1M`     (default: 0.0028)
//! - `COST_JUDGE_REASONING_PER_1M`      (default: 0.28, same as output rate)

use std::sync::Mutex;

use crb_reporting::CostSummary;
use rig_core::completion::Usage;

/// Thread-safe cost tracker for a single PR evaluation.
///
/// Wraps all counters in a `Mutex` so it can be shared across concurrent
/// agent calls via `Arc<CostTracker>`.
pub struct CostTracker {
    inner: Mutex<CostTrackerInner>,
}

#[derive(Debug, Clone, Default)]
struct CostTrackerInner {
    agent_tokens_in: usize,
    agent_tokens_out: usize,
    judge_tokens_in: usize,
    judge_tokens_out: usize,

    agent_cached_input_tokens: usize,
    agent_cache_creation_input_tokens: usize,
    agent_reasoning_tokens: usize,
    agent_tool_use_prompt_tokens: usize,
    judge_cached_input_tokens: usize,
    judge_cache_creation_input_tokens: usize,
    judge_reasoning_tokens: usize,
    judge_tool_use_prompt_tokens: usize,

    agent_call_count: usize,
    judge_call_count: usize,

    agent_cache_hits: usize,
    agent_cache_misses: usize,
    judge_cache_hits: usize,
    judge_cache_misses: usize,
}

impl CostTracker {
    /// Create a new empty cost tracker.
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(CostTrackerInner::default()),
        }
    }

    /// Record an agent LLM call with real API usage data.
    ///
    /// `usage` contains the token counts reported by the API.
    /// Pass `cache_hit = true` if the call was served from cache,
    /// `false` if it was a fresh API call.  On cache hits, usage is still
    /// recorded so analytics show token counts (cost is computed as 0).
    pub fn record_agent(&self, usage: &Usage, cache_hit: bool) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.agent_tokens_in += usage.input_tokens as usize;
            inner.agent_tokens_out += usage.output_tokens as usize;
            inner.agent_cached_input_tokens += usage.cached_input_tokens as usize;
            inner.agent_cache_creation_input_tokens += usage.cache_creation_input_tokens as usize;
            inner.agent_reasoning_tokens += usage.reasoning_tokens as usize;
            inner.agent_tool_use_prompt_tokens += usage.tool_use_prompt_tokens as usize;
            inner.agent_call_count += 1;
            if cache_hit {
                inner.agent_cache_hits += 1;
            } else {
                inner.agent_cache_misses += 1;
            }
        }
    }

    /// Record a judge LLM call with real API usage data.
    ///
    /// `usage` contains the token counts reported by the API.
    /// Pass `cache_hit = true` if the call was served from cache,
    /// `false` if it was a fresh API call.  On cache hits, usage is still
    /// recorded so analytics show token counts (cost is computed as 0).
    pub fn record_judge(&self, usage: &Usage, cache_hit: bool) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.judge_tokens_in += usage.input_tokens as usize;
            inner.judge_tokens_out += usage.output_tokens as usize;
            inner.judge_cached_input_tokens += usage.cached_input_tokens as usize;
            inner.judge_cache_creation_input_tokens += usage.cache_creation_input_tokens as usize;
            inner.judge_reasoning_tokens += usage.reasoning_tokens as usize;
            inner.judge_tool_use_prompt_tokens += usage.tool_use_prompt_tokens as usize;
            inner.judge_call_count += 1;
            if cache_hit {
                inner.judge_cache_hits += 1;
            } else {
                inner.judge_cache_misses += 1;
            }
        }
    }

    /// Record an agent call with a default (zero) Usage.
    /// Used when usage data isn't available (e.g., legacy cache hits
    /// that have no stored usage).
    pub fn record_agent_empty(&self, cache_hit: bool) {
        self.record_agent(&Usage::new(), cache_hit);
    }

    /// Record a judge call with a default (zero) Usage.
    pub fn record_judge_empty(&self, cache_hit: bool) {
        self.record_judge(&Usage::new(), cache_hit);
    }

    /// Total estimated cost in USD, computed from env-configured pricing rates.
    ///
    /// Pricing rates are read from environment variables (see module docs for
    /// defaults). The formula is:
    /// ```text
    /// cost = (tokens_in * input_price_per_token) + (tokens_out * output_price_per_token)
    /// ```
    /// where prices are per-token (derived from per-1M-token rates).
    pub fn total_cost_usd(&self) -> f64 {
        if let Ok(inner) = self.inner.lock() {
            let agent_input_rate = read_price_env("COST_AGENT_INPUT_PER_1M", 0.14);
            let agent_output_rate = read_price_env("COST_AGENT_OUTPUT_PER_1M", 0.28);
            let agent_cache_read_rate = read_price_env("COST_AGENT_CACHE_READ_PER_1M", 0.0028);
            let agent_reasoning_rate = read_price_env("COST_AGENT_REASONING_PER_1M", 0.28);
            let judge_input_rate = read_price_env("COST_JUDGE_INPUT_PER_1M", 0.14);
            let judge_output_rate = read_price_env("COST_JUDGE_OUTPUT_PER_1M", 0.28);
            let judge_cache_read_rate = read_price_env("COST_JUDGE_CACHE_READ_PER_1M", 0.0028);
            let judge_reasoning_rate = read_price_env("COST_JUDGE_REASONING_PER_1M", 0.28);

            // Cached tokens charged at discounted cache read rate
            let agent_uncached_input = inner
                .agent_tokens_in
                .saturating_sub(inner.agent_cached_input_tokens);
            let judge_uncached_input = inner
                .judge_tokens_in
                .saturating_sub(inner.judge_cached_input_tokens);

            let agent_cost = (agent_uncached_input as f64 * agent_input_rate / 1_000_000.0)
                + (inner.agent_tokens_out as f64 * agent_output_rate / 1_000_000.0)
                + (inner.agent_cached_input_tokens as f64 * agent_cache_read_rate / 1_000_000.0)
                + (inner.agent_reasoning_tokens as f64 * agent_reasoning_rate / 1_000_000.0);
            let judge_cost = (judge_uncached_input as f64 * judge_input_rate / 1_000_000.0)
                + (inner.judge_tokens_out as f64 * judge_output_rate / 1_000_000.0)
                + (inner.judge_cached_input_tokens as f64 * judge_cache_read_rate / 1_000_000.0)
                + (inner.judge_reasoning_tokens as f64 * judge_reasoning_rate / 1_000_000.0);

            agent_cost + judge_cost
        } else {
            0.0
        }
    }

    /// Cache hit rate for agent calls (0.0 to 1.0).
    /// Returns 0.0 if no calls were made.
    pub fn agent_cache_hit_rate(&self) -> f64 {
        if let Ok(inner) = self.inner.lock() {
            let total = inner.agent_cache_hits + inner.agent_cache_misses;
            if total == 0 {
                0.0
            } else {
                inner.agent_cache_hits as f64 / total as f64
            }
        } else {
            0.0
        }
    }

    /// Cache hit rate for judge calls (0.0 to 1.0).
    /// Returns 0.0 if no calls were made.
    pub fn judge_cache_hit_rate(&self) -> f64 {
        if let Ok(inner) = self.inner.lock() {
            let total = inner.judge_cache_hits + inner.judge_cache_misses;
            if total == 0 {
                0.0
            } else {
                inner.judge_cache_hits as f64 / total as f64
            }
        } else {
            0.0
        }
    }

    /// Total token counts across both agent and judge calls.
    /// Returns `(total_tokens_in, total_tokens_out)`.
    pub fn total_tokens(&self) -> (usize, usize) {
        if let Ok(inner) = self.inner.lock() {
            let total_in = inner.agent_tokens_in + inner.judge_tokens_in;
            let total_out = inner.agent_tokens_out + inner.judge_tokens_out;
            (total_in, total_out)
        } else {
            (0, 0)
        }
    }

    /// Build a serializable [`CostSummary`] snapshot of the current state.
    pub fn to_summary(&self) -> CostSummary {
        if let Ok(inner) = self.inner.lock() {
            let agent_total = inner.agent_cache_hits + inner.agent_cache_misses;
            let judge_total = inner.judge_cache_hits + inner.judge_cache_misses;

            // Compute cost inline (avoid calling total_cost_usd which tries to re-lock the Mutex)
            let agent_input_rate = read_price_env("COST_AGENT_INPUT_PER_1M", 0.14);
            let agent_output_rate = read_price_env("COST_AGENT_OUTPUT_PER_1M", 0.28);
            let agent_cache_read_rate = read_price_env("COST_AGENT_CACHE_READ_PER_1M", 0.0028);
            let agent_reasoning_rate = read_price_env("COST_AGENT_REASONING_PER_1M", 0.28);
            let judge_input_rate = read_price_env("COST_JUDGE_INPUT_PER_1M", 0.14);
            let judge_output_rate = read_price_env("COST_JUDGE_OUTPUT_PER_1M", 0.28);
            let judge_cache_read_rate = read_price_env("COST_JUDGE_CACHE_READ_PER_1M", 0.0028);
            let judge_reasoning_rate = read_price_env("COST_JUDGE_REASONING_PER_1M", 0.28);
            // Cached tokens charged at discounted cache read rate
            let agent_uncached_input = inner
                .agent_tokens_in
                .saturating_sub(inner.agent_cached_input_tokens);
            let judge_uncached_input = inner
                .judge_tokens_in
                .saturating_sub(inner.judge_cached_input_tokens);

            let agent_cost = (agent_uncached_input as f64 * agent_input_rate / 1_000_000.0)
                + (inner.agent_tokens_out as f64 * agent_output_rate / 1_000_000.0)
                + (inner.agent_cached_input_tokens as f64 * agent_cache_read_rate / 1_000_000.0)
                + (inner.agent_reasoning_tokens as f64 * agent_reasoning_rate / 1_000_000.0);
            let judge_cost = (judge_uncached_input as f64 * judge_input_rate / 1_000_000.0)
                + (inner.judge_tokens_out as f64 * judge_output_rate / 1_000_000.0)
                + (inner.judge_cached_input_tokens as f64 * judge_cache_read_rate / 1_000_000.0)
                + (inner.judge_reasoning_tokens as f64 * judge_reasoning_rate / 1_000_000.0);

            CostSummary {
                agent_tokens_in: inner.agent_tokens_in,
                agent_tokens_out: inner.agent_tokens_out,
                judge_tokens_in: inner.judge_tokens_in,
                judge_tokens_out: inner.judge_tokens_out,
                total_usd: agent_cost + judge_cost,
                agent_cache_hit_rate: if agent_total == 0 {
                    0.0
                } else {
                    inner.agent_cache_hits as f64 / agent_total as f64
                },
                judge_cache_hit_rate: if judge_total == 0 {
                    0.0
                } else {
                    inner.judge_cache_hits as f64 / judge_total as f64
                },
                agent_cached_input_tokens: inner.agent_cached_input_tokens,
                agent_cache_creation_input_tokens: inner.agent_cache_creation_input_tokens,
                agent_reasoning_tokens: inner.agent_reasoning_tokens,
                agent_tool_use_prompt_tokens: inner.agent_tool_use_prompt_tokens,
                judge_cached_input_tokens: inner.judge_cached_input_tokens,
                judge_cache_creation_input_tokens: inner.judge_cache_creation_input_tokens,
                judge_reasoning_tokens: inner.judge_reasoning_tokens,
                judge_tool_use_prompt_tokens: inner.judge_tool_use_prompt_tokens,
                agent_call_count: inner.agent_call_count,
                judge_call_count: inner.judge_call_count,
            }
        } else {
            CostSummary {
                agent_tokens_in: 0,
                agent_tokens_out: 0,
                judge_tokens_in: 0,
                judge_tokens_out: 0,
                total_usd: 0.0,
                agent_cache_hit_rate: 0.0,
                judge_cache_hit_rate: 0.0,
                agent_cached_input_tokens: 0,
                agent_cache_creation_input_tokens: 0,
                agent_reasoning_tokens: 0,
                agent_tool_use_prompt_tokens: 0,
                judge_cached_input_tokens: 0,
                judge_cache_creation_input_tokens: 0,
                judge_reasoning_tokens: 0,
                judge_tool_use_prompt_tokens: 0,
                agent_call_count: 0,
                judge_call_count: 0,
            }
        }
    }
}

impl Default for CostTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Read a `f64` environment variable, returning `default` if unset or invalid.
fn read_price_env(key: &str, default: f64) -> f64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(default)
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
    fn test_tracker_new_is_empty() {
        let tracker = CostTracker::new();
        assert_eq!(tracker.total_tokens(), (0, 0));
        assert_eq!(tracker.total_cost_usd(), 0.0);
        assert_eq!(tracker.agent_cache_hit_rate(), 0.0);
        assert_eq!(tracker.judge_cache_hit_rate(), 0.0);
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

        // Check extended analytics
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
    fn test_cost_summary_serialization() {
        let tracker = CostTracker::new();
        let usage1 = make_usage(4000, 1000, 500, 200, 50, 30);
        let usage2 = make_usage(500, 200, 100, 50, 20, 10);

        tracker.record_agent(&usage1, true);
        tracker.record_judge(&usage2, false);

        let summary = tracker.to_summary();
        assert_eq!(summary.agent_tokens_in, 4000);
        assert_eq!(summary.agent_tokens_out, 1000);
        assert_eq!(summary.judge_tokens_in, 500);
        assert_eq!(summary.judge_tokens_out, 200);
        assert!((summary.agent_cache_hit_rate - 1.0).abs() < 1e-6);
        assert!((summary.judge_cache_hit_rate - 0.0).abs() < 1e-6);

        // Verify it serializes to JSON
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("\"agent_tokens_in\":4000"));
        assert!(json.contains("\"total_usd\""));
        assert!(json.contains("\"agent_cached_input_tokens\":500"));
        assert!(json.contains("\"agent_call_count\":1"));
        assert!(json.contains("\"judge_call_count\":1"));
    }

    #[test]
    fn test_usd_cost_with_default_rates() {
        let tracker = CostTracker::new();
        // 1M tokens in @ $0.14/1M = $0.14; 500K out @ $0.28/1M = $0.14; total = $0.28
        let usage = make_usage(1_000_000, 500_000, 0, 0, 0, 0);
        tracker.record_agent(&usage, false);
        let cost = tracker.total_cost_usd();
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
