# Change: Migration & Validation

## Intent
Port the shell scaffolding scripts to Rust, validate that the new harness produces results consistent with the existing v5.14 subagent-based pipeline, and add operational tooling for daily use.

## Scope
Rust reimplementation of scaffold_pr.sh (git cleanup + checkout + diff extraction), validation harness that compares harness output against known v5.14 baseline, and CI-ready entrypoint.

Out of scope: web dashboard, multi-model judge calibration, statistical significance testing.

## Approach
Rewrite scaffold_pr.sh as git helper functions in `crates/crb-tools/src/lib.rs` using `std::process::Command` for git operations (no git2 crate — reduce deps). Create a validation module in `crates/crb-reporting/src/lib.rs` that computes result deltas between new harness and stored baseline. Add `--validate` and `--ci` flags to `crb-harness/src/main.rs` via clap.

## Why
Shell-based PR scaffolding is fragile, non-portable (Unix-only), and cannot be integrated with Rust's async runtime. A Rust reimplementation provides cross-platform compatibility, better error handling, and zero runtime dependency on Python.

## What Changes
Port scaffold_pr.sh to Rust git helper functions in crb-tools. Create validation module in crb-reporting comparing harness output against v5.14 baseline. Add --validate and --ci flags to crb-harness CLI.
