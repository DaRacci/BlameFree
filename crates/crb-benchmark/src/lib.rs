//! Benchmark execution and diff processing for code review evaluation.

pub mod diff_cache;
pub mod diffs;
pub mod judge;
pub mod pr;
pub mod scaffold;

pub const BENCHMARK_DIR: &str = "benchmark";
pub const BENCHMARK_DIFFS_SUBDIR: &str = "diffs";
pub const BENCHMARK_WORKTREE_SUBDIR: &str = "worktree";
pub const BENCHMARK_BASE_REPOS_SUBDIR: &str = "base_repos";

pub const DATASETS_DIR: &str = "datasets/golden_comments";
