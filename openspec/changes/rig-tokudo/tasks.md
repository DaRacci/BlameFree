# Tasks: rig-tokudo Adoption

## Phase 1: Add Dependency + Replace Caching Layer

- [ ] **1.1 Add `rig-tokudo` to workspace dependencies**
  - **File:** `Cargo.toml` (workspace root)
  - **Change:** Add `rig-tokudo = "0.2"` to `[workspace.dependencies]`
  - **Verify:** `cargo check` succeeds

- [ ] **1.2 Add `rig-tokudo` to crb-harness dependencies**
  - **File:** `crates/crb-harness/Cargo.toml`
  - **Change:** Add `rig-tokudo = { workspace = true }`
  - **Verify:** `cargo check -p crb-harness` succeeds

- [ ] **1.3 Replace `LlmCache` with `OptimizedModel::with_cache()` in main.rs**
  - **File:** `crates/crb-harness/src/main.rs`
  - **Change:**
    - Remove `use crb_harness::cache::LlmCache;` and import `rig_tokudo::OptimizedModel`
    - Replace `let cache = LlmCache::new(&cache_dir, &pr_key).ok();` with
      `let optimized = OptimizedModel::new(client.completion_model(&model)).with_cache(&cache_dir);`
    - Remove `cache` variable from all downstream function calls
    - Wire the optimized model through to agent construction
  - **Verify:** Code compiles; existing 50-PR benchmark produces identical results

- [ ] **1.4 Remove `CacheBackend` trait from `crb-consensus`**
  - **File:** `crates/crb-consensus/src/lib.rs`
  - **Change:**
    - Remove the `CacheBackend` trait definition (lines ~83-122)
    - Remove `cache: Option<Arc<dyn CacheBackend>>` parameter from `run_reviewers()` and `run_consensus()`
    - Remove all `cache_key` helper functions (`compute_agent_cache_key`, `compute_judge_cache_key`, `compute_context_cache_key`)
    - Remove `prompt_hash`, `diff_hash`, `rules_hash`, `judge_prompt_hash`, `judge_model` parameters from these functions
    - Remove all cache lookup/save calls inside these functions
  - **Verify:** `cargo check -p crb-consensus` succeeds; all callers in crb-harness updated

- [ ] **1.5 Remove `cache.rs` file**
  - **File:** `crates/crb-harness/src/cache.rs`
  - **Change:** Delete the file (603 lines)
  - **Verify:** No unresolved imports reference types from this module

## Phase 2: Replace Cost Tracking

- [ ] **2.1 Replace `CostTracker` with `OptimizedModel::with_pricing()`**
  - **File:** `crates/crb-harness/src/main.rs`
  - **Change:**
    - Remove `use cost::CostTracker;` and `let cost_tracker = Arc::new(CostTracker::new());`
    - Add `OptimizedModel::with_pricing(pricing_config)` to the optimized model builder chain
    - Create `PricingConfig` from env vars (same `COST_*_PER_1M` variables)
    - Remove `cost_tracker` parameter from all function signatures
    - Replace `ct.record_agent(...)` / `ct.record_judge(...)` calls with tokudo's automatic tracking
  - **Verify:** Cost summary still produced correctly in output reports

- [ ] **2.2 Remove `cost.rs` file**
  - **File:** `crates/crb-harness/src/cost.rs`
  - **Change:** Delete the file (299 lines)
  - **Verify:** No unresolved imports reference types from this module

- [ ] **2.3 Wire pricing config from environment**
  - **File:** `crates/crb-harness/src/main.rs`
  - **Change:** Create `PricingConfig` helper that reads env vars:
    ```rust
    let pricing_config = PricingConfig::new()
        .with_model_pricing("deepseek/deepseek-v4-flash",
            read_env_f64("COST_AGENT_INPUT_PER_1M", 0.14),
            read_env_f64("COST_AGENT_OUTPUT_PER_1M", 0.28))
        .with_model_pricing("judge-model",
            read_env_f64("COST_JUDGE_INPUT_PER_1M", 0.14),
            read_env_f64("COST_JUDGE_OUTPUT_PER_1M", 0.28));
    ```
  - **Verify:** Environment variables are read correctly; defaults apply when unset

## Phase 3: Enable New Features

- [ ] **3.1 Enable prompt compression**
  - **File:** `crates/crb-harness/src/main.rs`
  - **Change:** Add `.with_compression(true)` to the optimized model builder chain
  - **Verify:** Prompts are compressed before being sent to the LLM API

- [ ] **3.2 Add `--replay-dir` CLI flag**
  - **File:** `crates/crb-harness/src/main.rs`
  - **Change:**
    - Add `replay_dir: Option<PathBuf>` to the Clap CLI args struct
    - Pass as `.with_replay(replay_dir)` to the optimized model builder
  - **Verify:** With `--replay-dir /tmp/replay`, a run records traces to disk

- [ ] **3.3 Enable structured observability**
  - **File:** `crates/crb-harness/src/main.rs`
  - **Change:** Add `.with_observability(true)` to the optimized model builder chain
  - **Verify:** Structured tracing spans appear in logs for each LLM call

- [ ] **3.4 Wire optimized model through agent construction**
  - **File:** `crates/crb-harness/src/main.rs` (or `crb-agents/src/lib.rs`)
  - **Change:** The `build_agent()` function must accept the optimized model
    instead of `(client, model_name)`. Either:
    - Option A: Pass `&OptimizedModel` directly to `build_agent()`
    - Option B: Build inside `main.rs` and use closures to capture the optimized model
  - **Verify:** All agents use the optimized model; caching/cost tracking is transparent

## Phase 4: Cleanup

- [ ] **4.1 Remove `sha2` dependency if unused**
  - **File:** `crates/crb-harness/Cargo.toml`
  - **Change:** Remove `sha2 = { workspace = true }` if no other code uses it
  - **Verify:** `cargo check -p crb-harness` succeeds

- [ ] **4.2 Remove `mod cache;` and `mod cost;` declarations**
  - **File:** `crates/crb-harness/src/main.rs` (or `lib.rs`)
  - **Change:** Remove `mod cache;` and `mod cost;` module declarations
  - **Verify:** No "unused module" warnings

- [ ] **4.3 Remove dead imports and references**
  - **File:** All crates that referenced `cache::*`, `cost::*`, `CacheBackend`
  - **Change:** Run `cargo fix --edition-idioms --allow-dirty` or manually remove unused imports
  - **Verify:** `cargo check` with no warnings

- [ ] **4.4 Update tests**
  - **File:** `crates/crb-harness/tests/` and any test files referencing `LlmCache`, `CostTracker`
  - **Change:** Remove or rewrite tests that tested custom caching/cost implementation details
  - **Verify:** `cargo test` passes

- [ ] **4.5 Run full test suite**
  - **Command:** `cargo test --workspace`
  - **Verify:** All tests pass; no regressions

## Phase 5: Verification

- [ ] **5.1 Run existing 50-PR benchmark**
  - **Command:** `cargo run --release -- --cache-dir /tmp/bench-cache --prs data/bench/50-prs.json`
  - **Expected:** Identical review results to pre-tokudo run (same findings, same metrics)
  - **Verify:** Output matches golden reference

- [ ] **5.2 Second run with cache**
  - **Command:** Same as 5.1 (reuse cache dir)
  - **Expected:** Instant completion — all calls served from cache
  - **Verify:** Reports show 100% cache hit rate; zero API calls made

- [ ] **5.3 Verify cost tracking matches previous estimates**
  - **Expected:** Cost summary output matches previous manual estimates within normal variance
  - **Verify:** Tokudo's real token counts are within 10% of char/4 estimates

- [ ] **5.4 Enable replay — record and replay**
  - **Command (record):** `cargo run --release -- --replay-dir /tmp/replay-1 --prs data/bench/50-prs.json`
  - **Command (replay):** `cargo run --release -- --replay-dir /tmp/replay-1 --prs data/bench/50-prs.json`
  - **Expected:** Both runs produce identical output
  - **Verify:** Diff of output JSON files is empty
