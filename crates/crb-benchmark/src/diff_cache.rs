//! Functions for loading cached diffs from disk.
//!
//! Cached diffs live at `{benchmark_dir}/diffs/{owner}_{repo}_{pr_num}.diff`.

use std::fs;
use std::path::Path;
use tracing::{info, warn};

/// Load the diff for a PR from pre-extracted cached diff files.
///
/// Cached diffs live at `{benchmark_dir}/diffs/{owner}_{repo}_{pr_num}.diff`.
pub fn load_cached_diff(
    benchmark_dir: &Path,
    owner: &str,
    repo: &str,
    pr_num: u32,
) -> Option<String> {
    let diffs_dir = benchmark_dir.join("diffs");
    let diff_path = diffs_dir.join(format!("{}_{}_{}.diff", owner, repo, pr_num));
    match fs::read_to_string(&diff_path) {
        Ok(content) => {
            info!(
                "Loaded cached diff ({} bytes) from {}",
                content.len(),
                diff_path.display()
            );
            Some(content)
        }
        Err(e) => {
            warn!(
                "Cached diff not found at {}: {}. Using empty diff.",
                diff_path.display(),
                e
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_cached_diff_nonexistent() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let result = load_cached_diff(dir.path(), "owner", "repo", 42);
        assert!(result.is_none());
    }

    #[test]
    fn load_cached_diff_exists() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let diffs_dir = dir.path().join("diffs");
        std::fs::create_dir_all(&diffs_dir).expect("create diffs dir");
        let diff_path = diffs_dir.join("owner_repo_42.diff");
        std::fs::write(
            &diff_path,
            "--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new",
        )
        .expect("write diff");
        let result = load_cached_diff(dir.path(), "owner", "repo", 42);
        assert!(result.is_some());
        let content = result.unwrap();
        assert!(content.contains("old"));
        assert!(content.contains("new"));
    }
}
