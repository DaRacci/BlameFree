# Tasks: Migration & Validation

- [ ] Implement scaffolding.rs: clean_repo (git clean -fdx), checkout_pr (git fetch + checkout), extract_diff (git diff)
- [ ] Implement validation.rs: Baseline struct (serde Deserialize), compute_delta(), threshold checks
- [ ] Add `--validate` flag to main.rs (clap derive)
- [ ] Add `--ci` flag that runs scaffold → evaluate → validate → report in one command
- [ ] Add `--cached-diffs` flag to skip scaffolding and use pre-extracted diff files
- [ ] Store v5.14 baseline JSON from most recent known-good run
- [ ] Validate against regression set (3 PRs) — should match within noise
- [ ] Validate against full 50 PR set — run twice, check stability
- [ ] Write Rust integration tests (scaffolding round-trip, validation delta computation)
- [ ] Create run_ci.sh entrypoint script (cargo build --release + ./target/release/crb-harness --ci ...)
- [ ] Remove dependency on scaffold_pr.sh from the new workflow
- [ ] Document new workflow in harness README.md
