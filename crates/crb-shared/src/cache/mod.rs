//! LLM interaction cache with content-addressed indexing.
//!
//! This module provides the unified cache infrastructure moved from
//! `crb-consensus` and `crb-harness`.

pub mod sha256;
pub mod keys;
pub mod traits;
pub mod types;
pub mod paths;
pub mod filesystem;

// ── Re-exports for convenience ────────────────────────────────────────────

pub use sha256::sha256_hex;
pub use keys::{
    compute_agent_cache_key,
    compute_judge_cache_key,
    compute_context_cache_key,
};
pub use traits::CacheBackend;
pub use types::{
    Result,
    RunHistoryEntry,
    PrCacheStats,
    GlobalCacheStats,
    PruneResult,
    ScrubResult,
};
pub use filesystem::LlmCache;
