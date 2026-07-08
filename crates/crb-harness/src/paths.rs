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

/// Cache storage directory (flat, shared across runs).
pub const CACHE_DIR_NAME: &str = "_cache";

/// Agents sub-directory inside a PR's cache folder.
pub const AGENTS_DIR: &str = "agents";

/// Judge sub-directory inside a PR's cache folder.
pub const JUDGE_DIR: &str = "judge";

/// Context sub-directory inside a PR's cache folder.
pub const CONTEXT_DIR: &str = "context";

/// Aggregate summary for a run.
pub const SUMMARY_FILE: &str = "_summary.json";

/// Append-only run-history log.
pub const RUNS_FILE: &str = "_runs.json";

/// Per-PR cache index.
pub const INDEX_FILE: &str = "index.json";

/// Default output directory name.
pub const OUTPUT_DIR_DEFAULT: &str = "output";

/// Default cache directory name (legacy, used only by CLI defaults).
pub const CACHE_DIR_DEFAULT: &str = "cache";
