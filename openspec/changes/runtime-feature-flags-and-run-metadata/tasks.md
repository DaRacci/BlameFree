# Tasks: Runtime Feature Flags and Run Metadata

## Phase 1: RunMetadata Struct

- [ ] **1.1 Define `RunMetadata` struct** — In `crates/crb-harness/src/metadata.rs`
  - All fields from design.md with `#[serde(default)]`
  - `impl Default` with defaults matching current CLI defaults
- [ ] **1.2 Add `build.rs` for harness commit** — `crates/crb-harness/build.rs`
  - Run `git rev-parse HEAD` → inject as `HARNES_BUILD_COMMIT` env var
  - Graceful fallback when outside git repo
- [ ] **1.3 Export `RunMetadata` from `crb-harness` lib** — `pub use metadata::RunMetadata;`
- [ ] **1.4 Unit tests for `RunMetadata` serialization**
  - Test default serialization
  - Test round-trip with all fields populated
  - Test deserialization from JSON without metadata key (backward compat)

## Phase 2: Wire Metadata into Benchmark Run Flow

- [ ] **2.1 Build `RunMetadata` in benchmark `run_benchmark()`** — Before iterating PRs
  - Populate from CLI args: model, judge_model, reasoning_effort, roles, max_findings, dataset
  - Populate from RuntimeConfig: enabled_features
  - Populate from build.rs env: harness_commit
  - Populate timing: started_at, duration_secs
- [ ] **2.2 Embed metadata in per-PR result JSON** — When writing `output/.../pr_result.json`
  - Add `metadata` key at the top level
- [ ] **2.3 Embed metadata in summary JSON** — When writing `_summary.json`
  - Add `metadata` key at the top level
- [ ] **2.4 Thread `RunMetadata` through `evaluate_pr_with_postprocessing()` signature**
  - Accept `&RunMetadata` and attach to result if needed

## Phase 3: Wire Metadata into Ad-hoc Run Flow

- [ ] **3.1 Build `RunMetadata` in `run_adhoc_review_inner()`**
  - Populate from request: model, roles
  - Populate default for other fields
- [ ] **3.2 Embed metadata in ad-hoc run summary JSON**
  - Add `metadata` key to the output summary file
- [ ] **3.3 Extend `AdhocRunSummary` to carry metadata**
  - Add `metadata: Option<RunMetadata>` field

## Phase 4: Convert Experimental Feature Gates to Runtime

- [ ] **4.1 Add `RuntimeConfig` struct** — In `crates/crb-harness/src/config.rs`
  - Three bool fields: `exp14_template_vars`, `exp14_submit_finding`, `exp16_adaptive_agents`
  - `impl Default` matching current Cargo.toml defaults (all `false`)
  - Global `Lazy<Mutex<RuntimeConfig>>` accessor
- [ ] **4.2 Add CLI args for runtime flags** — `ReviewArgs` struct
  - `--exp14-template-vars`, `--exp14-submit-finding`, `--exp16-adaptive-agents`
  - Wire into `RuntimeConfig::init()` at startup
- [ ] **4.3 Convert `exp14_template_vars` to runtime** — `crb-harness` / `crb-tools`
  - Replace `cfg!(feature = "exp14_template_vars")` with runtime check
  - Wire the flag through to template variable injection
- [ ] **4.4 Convert `exp14_submit_finding` to runtime** — `crb-agents/src/templates.rs`
  - Replace `cfg!(feature = "exp14_submit_finding")` with runtime check
  - This is used inside template rendering context
- [ ] **4.5 Convert `exp16_adaptive_agents` to runtime** — `crb-consensus/src/adaptive.rs`
  - Replace `#[allow(unused_variables)]` pattern with explicit runtime check
  - Conditionally apply adaptive agent dispatch logic
- [ ] **4.6 Remove unused feature flags from Cargo.toml** — After all `cfg!()` references are removed
  - `crates/crb-harness/Cargo.toml`: remove `exp14_template_vars`, `exp16_adaptive_agents` features
  - `crates/crb-tools/Cargo.toml`: remove `exp14_template_vars`, `exp14_submit_finding` features
  - `crates/crb-consensus/Cargo.toml`: remove `exp14_submit_finding`, `exp16_adaptive_agents` features
  - `crates/crb-agents/Cargo.toml`: remove `exp14_submit_finding` feature
  - `crates/crb-webui-backend/Cargo.toml`: remove `exp14_template_vars`, `exp16_adaptive_agents` feature references to crb-harness

## Phase 5: Update Dashboard Events

- [ ] **5.1 Add `RunStarted` variant** — `crates/crb-dashboard/src/lib.rs`
  - `RunStarted { metadata: RunMetadata }`
- [ ] **5.2 Extend `RunFinished` variant** — `crates/crb-dashboard/src/lib.rs`
  - Add `metadata: RunMetadata` field
- [ ] **5.3 Update webui `DashboardEvent`** — `crates/crb-webui/src/events.rs`
  - Add `RunStarted` variant
  - Extend `RunFinished` with `metadata`
- [ ] **5.4 Benchmark main sends `RunStarted`** — At start of `run_benchmark()`
- [ ] **5.5 Benchmark main extends `RunFinished`** — Populate metadata before sending
- [ ] **5.6 Add metadata to `ActiveRun`** — `crates/crb-webui/src/server.rs`
  - `metadata: RunMetadata` field on `ActiveRun`

## Phase 6: WebUI Display

- [ ] **6.1 Add metadata to run detail API response** — `crates/crb-webui/src/api/runs.rs`
  - `RunDetail` gains `metadata: Option<RunMetadata>`
- [ ] **6.2 Add metadata component to frontend** — `crates/crb-webui/frontend/src/`
  - New `RunMetadataPanel` component or section in run detail page
- [ ] **6.3 Show metadata fields in run detail page**
  - Display: model, judge_model, reasoning_effort, roles, dataset, enabled_features, harness_commit, timing

## Phase 7: JSON Output Consistency

- [ ] **7.1 Verify benchmark per-PR JSON includes metadata**
- [ ] **7.2 Verify benchmark summary JSON includes metadata**
- [ ] **7.3 Verify ad-hoc summary JSON includes metadata**
- [ ] **7.4 Verify dashboard event JSON (stdout mode) includes metadata**
- [ ] **7.5 Verify backward compatibility: read old JSON files without metadata**

## Phase 8: Tests

- [ ] **8.1 Unit tests: `RuntimeConfig` default values and CLI parsing**
- [ ] **8.2 Unit tests: feature gate runtime checks produce correct branches**
- [ ] **8.3 Unit tests: `RunMetadata` serialization round-trip**
- [ ] **8.4 Unit tests: backward-compatible deserialization of old files**
- [ ] **8.5 Integration test: benchmark run outputs metadata in JSON**
- [ ] **8.6 Integration test: ad-hoc run outputs metadata in JSON**
- [ ] **8.7 Integration test: dashboard events include metadata**
