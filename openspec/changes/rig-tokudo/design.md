# Design: rig-tokudo Adoption

## 1. Overview

The review-harness currently has three custom modules for managing LLM
interactions: `cache.rs` (content-addressed caching), `cost.rs` (token
estimation + USD cost), and the `CacheBackend` trait (cross-crate interface
in `crb-consensus`). These are replaced by `rig-tokudo`'s `OptimizedModel`
decorator, which transparently handles caching, pricing, compression, replay,
and observability.

### Before (current architecture)

```text
                         ┌─────────────────────┐
                         │      main.rs         │
                         │  cache_dir -> LlmCache│
                         │  estimate_tokens ->   │
                         │  CostTracker         │
                         └──────┬──────────────┘
                                │ cache: Arc<dyn CacheBackend>
                                │ cost_tracker: Arc<CostTracker>
                                ▼
               ┌──────────────────────────────────────┐
               │          crb-consensus                │
               │  ┌────────────────────────────────┐   │
               │  │ run_reviewers()                 │   │
               │  │  - compute_agent_cache_key()    │   │
               │  │  - cache.lookup_agent_by_key()  │   │
               │  │  - agent.prompt()               │   │
               │  │  - cache.save_agent_with_key()  │   │
               │  └────────────────────────────────┘   │
               │  ┌────────────────────────────────┐   │
               │  │ run_consensus()                 │   │
               │  │  - run_reviewers()              │   │
               │  │  - run_judge()                  │   │
               │  │  - cache.lookup_judge_by_key()  │   │
               │  └────────────────────────────────┘   │
               │                                        │
               │  [CacheBackend trait (45 lines)]       │
               └────────────────────────────────────────┘
                              │
               ┌──────────────┴──────────────┐
               ▼                              ▼
       crb-harness/src/               crb-harness/src/
       cache.rs (603 lines)           cost.rs (299 lines)
       LlmCache struct                CostTracker struct
       SHA256 keying                  char/4 estimation
       index.json + disk files        env-var pricing
```

### After (with rig-tokudo)

```text
                         ┌──────────────────────────────────────┐
                         │            main.rs                    │
                         │  let model = client.completion_model()│
                         │  let optimized = OptimizedModel::new( │
                         │      model)                           │
                         │      .with_cache(cache_dir)           │
                         │      .with_pricing(pricing_config)    │
                         │      .with_compression(true)          │
                         │      .with_replay(replay_dir)         │
                         └────────────────┬─────────────────────┘
                                          │
                                          │ optimized model
                                          ▼
                         ┌──────────────────────────────────────┐
                         │         crb-consensus (simplified)    │
                         │  run_reviewers()                      │
                         │    - agent.prompt()  ← transparently │
                         │       cached + priced by OptimizedModel
                         │  run_consensus()                      │
                         │    - same flow, no cache params needed│
                         │                                      │
                         │  [CacheBackend REMOVED]              │
                         └──────────────────────────────────────┘
                                          │
                              ┌───────────┴───────────┐
                              │                       │
                              ▼                       ▼
                    rig-tokudo::            rig-tokudo::
                    CacheProvider           PriceTracker
                    (auto-managed)          (auto-managed)
```

## 2. OptimizedModel Integration

### 2.1 The Decorator Pattern

`OptimizedModel` wraps a `rig::completion::CompletionModel` and delegates all
calls through a stack of decorators:

```text
Agent.prompt(prompt)
  │
  ▼
OptimizedModel.prompt(prompt)
  ├──► ReplayLayer (if replay-dir set)
  │     ├── replay file exists? -> return cached response
  │     └── no replay -> pass through
  │
  ├──► ObservabilityLayer (tracing spans per call)
  │
  ├──► CompressionLayer
  │     └── compress prompt -> pass compressed prompt
  │
  ├──► CacheLayer
  │     ├── semantic cache hit? -> return cached, record 0 tokens
  │     └── no hit -> pass through
  │
  ├──► PricingLayer
  │     └── compute cost from real token usage
  │
  └──► inner model.prompt()
```

The agent builder in `crb-consensus` changes from:

```rust
// BEFORE: agent.prompt(&diff) -> Manual caching, cost tracking
let agent = build_agent(&client, &model, &role, preamble, prompt_lib, tool_preamble);
// ... caller checks cache, makes API call, saves cache, records cost ...

// AFTER: agent.prompt(&diff) -> Everything handled by tokudo
let agent = build_agent(&optimized_client, &model, &role, preamble, prompt_lib, tool_preamble);
// agent.prompt(&diff) transparently caches, tracks cost, etc.
```

### 2.2 Integration Point in the Pipeline

The key change is in `crates/crb-harness/src/main.rs` around line 650-660 and
the agent dispatch code around line 845-915. Instead of:

```rust
// BEFORE (main.rs)
let cache = LlmCache::new(cache_dir, &pr_key).ok();
let cost_tracker = Arc::new(CostTracker::new());
// ... pass both through 4 function signatures ...

// Inside run_reviewers() in crb-consensus:
let cache_key = compute_agent_cache_key(prompt_hash, diff_hash, ...);
if let Some(cached) = cache.lookup_agent_by_key(&cache_key) { ... }
ct.record_agent(tokens_in, tokens_out, cache_hit);
```

We instead:

```rust
// AFTER (main.rs)
use rig_tokudo::{OptimizedModel, PricingConfig};

let model = client.completion_model(&model_name);
let optimized = OptimizedModel::new(model)
    .with_cache(cache_dir)           // replaces LlmCache
    .with_pricing(pricing_config)    // replaces CostTracker
    .with_compression(true)
    .with_observability(true)
    .with_replay(replay_dir);

// Build agent with the optimized model (same API)
let agent = build_agent_with_model(&optimized, &role, ...);
let response = agent.prompt(&diff).await;
// Caching, pricing, compression — all transparent
```

The `build_agent` function in `crb-harness` or `crb-consensus` needs to accept
the optimized model instead of the raw client+model string.

## 3. Configuration

### 3.1 CLI Flags (main.rs)

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--cache-dir` | `Option<PathBuf>` | `None` | Directory for LLM response cache (unchanged) |
| `--replay-dir` | `Option<PathBuf>` | `None` | **NEW:** Directory for deterministic replay traces |
| `--compress` | `bool` | `false` | **NEW:** Enable prompt compression |

### 3.2 Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `COST_AGENT_INPUT_PER_1M` | `0.14` | Input token price per 1M tokens (deepseek-v4-flash) |
| `COST_AGENT_OUTPUT_PER_1M` | `0.28` | Output token price per 1M tokens |
| `COST_JUDGE_INPUT_PER_1M` | `0.14` | Judge input token price per 1M tokens |
| `COST_JUDGE_OUTPUT_PER_1M` | `0.28` | Judge output token price per 1M tokens |

These map to `rig_tokudo::PricingConfig`:

```rust
let pricing_config = PricingConfig::new()
    .with_model_pricing("deepseek/deepseek-v4-flash", 0.14, 0.28)
    .with_model_pricing("judge-model", 0.14, 0.28);
```

## 4. Data Flow

### 4.1 Agent Prompt -> Tokudo Decorator -> API/Cache -> Response

```text
┌──────────┐     ┌──────────────────┐     ┌──────────────┐
│  Agent   │────▶│ OptimizedModel   │────▶│  ReplayLayer │
│ prompt() │     │  (decorator)     │     │ (optional)   │
└──────────┘     └──────────────────┘     └──────┬───────┘
                                                 │
                                                 ▼
                                        ┌────────────────┐
                                        │Observability-   │
                                        │Layer (tracing)  │
                                        └──────┬─────────┘
                                               │
                                               ▼
                                        ┌────────────────┐
                                        │ Compression-    │
                                        │ Layer           │
                                        └──────┬─────────┘
                                               │
                                               ▼
                                        ┌────────────────┐
                                        │  Cache Layer   │
                                        │ semantic +     │
                                        │ content-addr-  │
                                        │ essed          │
                                        └──┬──────┬──────┘
                                           │      │
                                     cache hit  cache miss
                                           │      │
                                           │      ▼
                                           │  ┌──────────────┐
                                           │  │Pricing Layer │
                                           │  │(real tokens) │
                                           │  └──────┬───────┘
                                           │         │
                                           │         ▼
                                           │  ┌──────────────┐
                                           │  │ inner model  │
                                           │  │ .prompt()    │
                                           │  └──────┬───────┘
                                           │         │
                                           └────┬────┘
                                                │
                                                ▼
                                         ┌──────────────┐
                                         │  Response     │
                                         │ (+ token      │
                                         │  metadata)    │
                                         └──────────────┘
```

### 4.2 Cache Key Migration

Current custom cache key:
```rust
sha256("prompt_hash:diff_hash:model:role:rules_hash")
-> "/agents/{sha256}.agent_SA_response.txt"
```

Tokudo handles its own keying internally — we don't need to compute or pass
cache keys. The cache is transparently managed by the `CacheLayer` decorator.

**Migration concern:** Existing cache directories will not be compatible with
tokudo's on-disk format. The first run with tokudo will be a cold cache.
This is acceptable because:
- The cache is ephemeral — it stores LLM responses across runs of the same
  benchmark, not long-term artifacts.
- Tokudo's cache is more capable (semantic matching), justifying a format
  change.

## 5. Module Changes

### 5.1 Files to Remove

| File | Lines | Notes |
|------|-------|-------|
| `crates/crb-harness/src/cache.rs` | 603 | Entire file — `LlmCache` struct + `CacheBackend` impl + tests |
| `crates/crb-harness/src/cost.rs` | 299 | Entire file — `CostTracker` + `estimate_tokens` + tests |

### 5.2 Files to Modify

| File | Changes |
|------|---------|
| `Cargo.toml` (workspace) | Add `rig-tokudo = "0.2"` to `[workspace.dependencies]` |
| `crates/crb-harness/Cargo.toml` | Add `rig-tokudo = { workspace = true }`, remove `sha2` if unused elsewhere |
| `crates/crb-harness/src/main.rs` | Replace `LlmCache::new()` with `OptimizedModel::with_cache()`; remove `CostTracker` wiring; add `--replay-dir` flag; wire pricing from env |
| `crates/crb-consensus/src/lib.rs` | Remove `CacheBackend` trait; remove `compute_agent_cache_key`, `compute_judge_cache_key`, `compute_context_cache_key`; remove `cache` parameter from `run_reviewers` and `run_consensus`; simplify signatures |
| `crates/crb-harness/src/consensus.rs` (or wherever `evaluate_pr_with_consensus` lives) | Remove cache/cost_tracker params; pass optimized model instead |

### 5.3 Dependencies to Remove

| Dependency | Removed From | Why |
|------------|-------------|-----|
| `sha2` | crb-harness | Only used by `LlmCache::sha256()` — replaced by tokudo internals |

## 6. Migration Path

### 6.1 Component Mapping

| Custom Component | Lines | Tokudo Replacement | Notes |
|------------------|-------|-------------------|-------|
| `LlmCache` (full struct) | 603 | `.with_cache(cache_dir)` | One-liner in the builder |
| `CacheBackend` trait | 45 | *Implicit* — tokudo handles keying internally | No trait needed |
| `CostTracker` | 299 | `.with_pricing(PricingConfig)` | One-liner + env vars |
| `compute_agent_cache_key` | ~15 | *Implicit* — tokudo manages keys | Remove function |
| `compute_judge_cache_key` | ~15 | *Implicit* | Remove function |
| `compute_context_cache_key` | ~10 | *Implicit* | Remove function |
| `estimate_tokens()` | ~10 | Real token counts from API | Much more accurate |
| Cache wiring (4 functions) | ~170 | Single `optimized` variable | Drop 3-4 params per function |
| Manual cache HIT/MISS logging | ~20 | `.with_observability(true)` | Structured tracing |

### 6.2 Compatibility

- `rig-tokudo` works with `rig-core >= 0.39` (our version). No upgrade needed.
- No `rig-compose` dependency — pure extension traits on `rig-core` types.
- All existing `build_agent()` and `agent.prompt()` calls remain unchanged —
  only the model object passed in changes.

## 7. Error Handling

| Error Scenario | Behavior |
|----------------|----------|
| Cache directory unwritable | Cache disabled with warning; API calls proceed normally |
| Pricing config missing model rate | Falls back to default $0.14/$0.28 per 1M tokens |
| Replay file corrupted | Replay disabled with warning; falls through to API call |
| Compression fails (malformed prompt) | Compression skipped for that call; original prompt used |
| Observability layer failure (tracing) | Traces dropped silently; core functionality unaffected |
