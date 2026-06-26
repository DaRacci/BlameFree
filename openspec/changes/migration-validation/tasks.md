# Tasks: Migration & Validation

- [x] Implement git helper functions in `crates/crb-tools/src/lib.rs`: clean_repo (git clean -fdx), checkout_pr (git fetch + checkout), extract_diff (git diff)
- [x] Implement validation in `crates/crb-harness/src/validation.rs`: Baseline struct (serde Deserialize), compute_delta(), threshold checks
- [x] Add `--validate` flag to `crates/crb-harness/src/main.rs` (clap derive)
- [x] Add `--ci` flag that runs scaffold → evaluate → validate → report in one command
- [x] Add `--cached-diffs` flag to skip scaffolding and use pre-extracted diff files
- [x] Store v5.14 baseline JSON from most recent known-good run
- [ ] Validate against regression set (3 PRs) — should match within noise
- [ ] Validate against full 50 PR set — run twice, check stability
- [ ] Write Rust integration tests (scaffolding round-trip, validation delta computation)
- [x] Create run_ci.sh entrypoint script (cargo build --release -p crb-harness + ./target/release/crb-harness --ci ...)
- [x] Remove dependency on scaffold_pr.sh from the new workflow
- [ ] Document new workflow in harness README.md
