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
