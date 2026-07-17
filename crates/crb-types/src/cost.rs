//! Cost and analytics types for PR evaluation.
//!
//! Tracks token counts, cache hit rates, and computes USD cost estimates.
//! These types are shared between crb-reporting (where they're populated)
//! and crb-webui (where they're displayed).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Snapshot of cost and usage statistics.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct AnalyticsSnapshot {
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

/// Default pricing: $0.14 per 1M input tokens, $0.28 per 1M output tokens.
/// Override via `CRB_INPUT_PRICE_PER_M` and `CRB_OUTPUT_PRICE_PER_M` env vars.
fn default_input_price_per_token() -> f64 {
    let price_per_m = std::env::var("CRB_INPUT_PRICE_PER_M")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.14);
    price_per_m / 1_000_000.0
}

fn default_output_price_per_token() -> f64 {
    let price_per_m = std::env::var("CRB_OUTPUT_PRICE_PER_M")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.28);
    price_per_m / 1_000_000.0
}

impl AnalyticsSnapshot {
    /// Compute the cache hit rate for all sessions combined.
    pub fn hit_rate(&self) -> f64 {
        let total_hits: usize = self
            .cache_usage
            .values()
            .map(|c| c.cache_hits as usize)
            .sum();
        let total_misses: usize = self
            .cache_usage
            .values()
            .map(|c| c.cache_misses as usize)
            .sum();
        CacheUsage::hit_rate(total_hits, total_misses)
    }

    /// Total estimated cost in USD, computed from env-configured pricing rates.
    ///
    /// Pricing rates are read from environment variables (see module docs for defaults).
    /// The formula is:
    /// ```text
    /// cost = (tokens_in * input_price_per_token) + (tokens_out * output_price_per_token)
    /// ```
    /// where prices are per-token (derived from per-1M-token rates).
    pub fn total_cost(&self) -> f64 {
        let input_price = default_input_price_per_token();
        let output_price = default_output_price_per_token();

        self.sessions
            .values()
            .fold(0.0, |acc, usage| {
                acc + usage.input_tokens as f64 * input_price
                    + usage.output_tokens as f64 * output_price
            })
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
