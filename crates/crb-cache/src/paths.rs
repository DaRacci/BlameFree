//! Shared path constants used across crates for cache storage.

/// Cache storage directory.
pub const CACHE_DIR_NAME: &str = "_cache";

/// Agents sub-directory inside a PR's cache folder.
pub const AGENTS_DIR: &str = "agents";

/// Judge sub-directory inside a PR's cache folder.
pub const JUDGE_DIR: &str = "judge";

/// Context sub-directory inside a PR's cache folder.
pub const CONTEXT_DIR: &str = "context";

/// Per-PR cache index.
pub const INDEX_FILE: &str = "index.json";
