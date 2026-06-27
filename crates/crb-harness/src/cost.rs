//! Cost tracking for LLM calls during PR evaluation.
//!
//! Tracks token counts, cache hit rates, and computes USD cost
//! from pricing rates configured via environment variables.
//!
//! # Token estimation
//! We don't get real token counts from rig-core's completion API yet.
//! Token counts are estimated as `char_count / 4`, which is a rough
//! approximation. This should be replaced with real API token counts
//! when the provider reports them.
//!
//! # Pricing
//! Pricing rates are read from environment variables:
//! - `COST_AGENT_INPUT_PER_1M`   (default: 0.14, deepseek-v4-flash)
//! - `COST_AGENT_OUTPUT_PER_1M`  (default: 0.28)
//! - `COST_JUDGE_INPUT_PER_1M`   (default: 0.14)
//! - `COST_JUDGE_OUTPUT_PER_1M`  (default: 0.28)

use std::sync::Mutex;

use crb_reporting::CostSummary;

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

    /// Record an agent LLM call with estimated token counts.
    ///
    /// `tokens_in` and `tokens_out` should be character-count estimates
    /// (see [`estimate_tokens`]). Pass `cache_hit = true` if the call
    /// was served from cache, `false` if it was a fresh API call.
    pub fn record_agent(&self, tokens_in: usize, tokens_out: usize, cache_hit: bool) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.agent_tokens_in += tokens_in;
            inner.agent_tokens_out += tokens_out;
            if cache_hit {
                inner.agent_cache_hits += 1;
            } else {
                inner.agent_cache_misses += 1;
            }
        }
    }

    /// Record a judge LLM call with estimated token counts.
    ///
    /// `tokens_in` and `tokens_out` should be character-count estimates
    /// (see [`estimate_tokens`]). Pass `cache_hit = true` if the call
    /// was served from cache, `false` if it was a fresh API call.
    pub fn record_judge(&self, tokens_in: usize, tokens_out: usize, cache_hit: bool) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.judge_tokens_in += tokens_in;
            inner.judge_tokens_out += tokens_out;
            if cache_hit {
                inner.judge_cache_hits += 1;
            } else {
                inner.judge_cache_misses += 1;
            }
        }
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
            let judge_input_rate = read_price_env("COST_JUDGE_INPUT_PER_1M", 0.14);
            let judge_output_rate = read_price_env("COST_JUDGE_OUTPUT_PER_1M", 0.28);

            let agent_cost = (inner.agent_tokens_in as f64 * agent_input_rate / 1_000_000.0)
                + (inner.agent_tokens_out as f64 * agent_output_rate / 1_000_000.0);
            let judge_cost = (inner.judge_tokens_in as f64 * judge_input_rate / 1_000_000.0)
                + (inner.judge_tokens_out as f64 * judge_output_rate / 1_000_000.0);

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
            let judge_input_rate = read_price_env("COST_JUDGE_INPUT_PER_1M", 0.14);
            let judge_output_rate = read_price_env("COST_JUDGE_OUTPUT_PER_1M", 0.28);
            let agent_cost = (inner.agent_tokens_in as f64 * agent_input_rate / 1_000_000.0)
                + (inner.agent_tokens_out as f64 * agent_output_rate / 1_000_000.0);
            let judge_cost = (inner.judge_tokens_in as f64 * judge_input_rate / 1_000_000.0)
                + (inner.judge_tokens_out as f64 * judge_output_rate / 1_000_000.0);

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
            }
        }
    }
}

impl Default for CostTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Estimate the number of tokens from a character count.
///
/// Uses a rough 4:1 character-to-token ratio, which is a reasonable
/// approximation for English text.
///
/// **Note:** This is an estimate and should be replaced with real API
/// token counts when the provider reports them (e.g., via the OpenAI
/// token usage API response field).
pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count() / 4
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

    #[test]
    fn test_estimate_tokens() {
        // 46 chars → 11 tokens (integer division)
        assert_eq!(estimate_tokens("hello world, this is a test string right here!"), 11);
        // Empty string → 0 tokens
        assert_eq!(estimate_tokens(""), 0);
        // Single char → 0 tokens (integer division)
        assert_eq!(estimate_tokens("a"), 0);
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
        tracker.record_agent(100, 50, true);
        tracker.record_agent(200, 100, false);
        tracker.record_judge(30, 20, true);

        let (total_in, total_out) = tracker.total_tokens();
        assert_eq!(total_in, 100 + 200 + 30);
        assert_eq!(total_out, 50 + 100 + 20);

        assert!((tracker.agent_cache_hit_rate() - 0.5).abs() < 1e-6);
        assert!((tracker.judge_cache_hit_rate() - 1.0).abs() < 1e-6);
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
        tracker.record_agent(4000, 1000, true);
        tracker.record_judge(500, 200, false);

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
    }

    #[test]
    fn test_usd_cost_with_default_rates() {
        let tracker = CostTracker::new();
        // 1M tokens in @ $0.14/1M = $0.14; 500K out @ $0.28/1M = $0.14; total = $0.28
        tracker.record_agent(1_000_000, 500_000, false);
        let cost = tracker.total_cost_usd();
        assert!((cost - 0.28).abs() < 0.001, "Expected ~0.28, got {cost}");
    }
}
