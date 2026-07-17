//! Cost tracking for LLM calls during PR evaluation.
//!
//! Tracks token counts, cache hit rates, and computes USD cost estimates.
//! The data types (`AnalyticsSnapshot`, `SessionUsage`, `CacheUsage`) live in
//! `crb_types::cost` so they can be shared with the webui crate.

use std::collections::HashMap;

use mti::prelude::MagicTypeId;
use rig_core::completion::Usage;
use tokio::sync::Mutex;

pub use crb_types::cost::{AnalyticsSnapshot, CacheUsage, SessionUsage};

/// Trait for providing session usage data from different source types.
pub trait SessionUsageProvider
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

/// Thread-safe cost tracker for a single PR evaluation.
///
/// Wraps all counters in a `Mutex` so it can be shared across concurrent agent calls via `Arc<CostTracker>`.
#[derive(Debug, Default)]
pub struct AnalyticsTracker {
    inner: Mutex<AnalyticsTrackerInner>,
}

#[derive(Debug, Clone, Default)]
struct AnalyticsTrackerInner {
    sessions: HashMap<MagicTypeId, SessionUsage>,

    cache_usage: HashMap<MagicTypeId, CacheUsage>,
}

impl AnalyticsTracker {
    /// Create a new empty cost tracker.
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(AnalyticsTrackerInner::default()),
        }
    }

    /// Record a call
    pub async fn record<T>(&self, key: MagicTypeId, provider: T, cache_hit: bool)
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

    /// Build a [`crb_types::cost::AnalyticsSnapshot`] of the current state.
    pub async fn to_snapshot(&self) -> AnalyticsSnapshot {
        let inner = self.inner.lock().await;
        AnalyticsSnapshot {
            sessions: inner.sessions.clone(),
            cache_usage: inner.cache_usage.clone(),
        }
    }
}
