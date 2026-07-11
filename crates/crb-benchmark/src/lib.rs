//! Benchmark execution and diff processing for code review evaluation.
//!
//! Provides the entry points for running full PR benchmarks (`review_pr`,
//! `review_diff`) as well as diff preprocessing utilities for filtering
//! noise files and reducing context.

pub mod diffs;
pub mod scaffold;
