//! Shared path constants used across crates.
//!
//! Centralising these strings prevents drift between the write side
//! (harness, cache) and the read side (webui API).
//!
//! # Conventions
//!
//! * `*_DIR` — names of directories (e.g. `"_cache"`)
//! * `*_FILE` — names of well-known files (e.g. `"_summary.json"`)
//! * `*_DEFAULT` — default values for CLI args or config fallbacks

// NOTE: Cache-specific path constants (CACHE_DIR_NAME, AGENTS_DIR, JUDGE_DIR,
// CONTEXT_DIR, INDEX_FILE) have been moved to crb-shared::cache::paths.

/// Aggregate summary for a run.
pub const SUMMARY_FILE: &str = "_summary.json";

/// Append-only run-history log.
pub const RUNS_FILE: &str = "_runs.json";

/// Default output directory name.
pub const OUTPUT_DIR_DEFAULT: &str = "output";

/// Default cache directory name (legacy, used only by CLI defaults).
pub const CACHE_DIR_DEFAULT: &str = "cache";
