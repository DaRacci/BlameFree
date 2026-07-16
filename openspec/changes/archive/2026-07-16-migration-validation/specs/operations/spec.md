# Delta for Operations

## ADDED Requirements

### Requirement: Rust PR Scaffolding
The system SHALL prepare PR data for evaluation using Rust (no shell scripts).

#### Scenario: Clean checkout
- GIVEN a PR URL mapped to `repos/{owner}/{repo}/{pr_num}/`
- WHEN the scaffolding runs
- THEN it performs `git clean -fdx` on the repo via `std::process::Command`
- AND it checks out the PR's base branch
- AND it verifies the working tree is clean

#### Scenario: Diff extraction
- GIVEN a clean PR checkout on the base branch
- WHEN the scaffolding runs
- THEN it fetches the PR branch via `std::process::Command`
- AND it runs `git diff {base}...{pr-branch}` to extract the diff
- AND it saves the diff to `{pr_dir}/diff.diff`

### Requirement: Baseline Validation
The system SHALL validate harness output against a stored v5.14 baseline.

#### Scenario: Precision/recall delta check
- GIVEN a completed harness run
- WHEN `--validate` flag is passed
- THEN it loads the stored baseline results (v5.14)
- AND it computes per-metric delta: `harness_precision - baseline_precision`
- AND it reports which metrics exceed the noise threshold (±2pp for F1)
- AND it exits with code 0 if all metrics within threshold, 1 otherwise

#### Scenario: Per-PR comparison
- GIVEN a completed harness run with --validate
- WHEN a PR's metrics differ significantly from baseline
- THEN it logs the PR URL, expected vs actual metrics
- AND it recommends manual review

### Requirement: CI Entrypoint
The system SHALL provide a single command that runs the full evaluation pipeline end-to-end.

#### Scenario: Full CI run
- GIVEN a clean workspace
- WHEN `cargo run -- --ci` is invoked
- THEN it scaffolds all 50 PRs (or loads cached diffs)
- AND it evaluates all PRs
- AND it runs validation against baseline
- AND it writes a CI-friendly report to stdout (JSON summary)
- AND it exits with appropriate code

### Requirement: Error Recovery
Structured error objects SHALL be produced via `anyhow` (not shell stderr) for machine-parsable logging.

#### Scenario: Scaffolding error produces structured error
- GIVEN a scaffolding operation fails (e.g., git checkout fails)
- WHEN the error occurs
- THEN the system SHALL produce a structured `anyhow::Error` with a message, source chain, and context
- AND the error SHALL be logged to structured output (JSON) rather than raw stderr
