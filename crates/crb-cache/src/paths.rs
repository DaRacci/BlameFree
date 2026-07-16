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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_dir_name_constant() {
        assert_eq!(CACHE_DIR_NAME, "_cache");
    }

    #[test]
    fn test_agents_dir_constant() {
        assert_eq!(AGENTS_DIR, "agents");
    }

    #[test]
    fn test_judge_dir_constant() {
        assert_eq!(JUDGE_DIR, "judge");
    }

    #[test]
    fn test_context_dir_constant() {
        assert_eq!(CONTEXT_DIR, "context");
    }

    #[test]
    fn test_index_file_constant() {
        assert_eq!(INDEX_FILE, "index.json");
    }
}
