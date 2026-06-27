# Proposal: Adopt rig-tokudo — Eliminate Custom Caching & Cost Tracking

**Change ID:** rig-tokudo
**Status:** Draft
**Author:** Hermes Agent
**Date:** 2026-06-27

## Summary

Replace ~1,100 lines of custom caching infrastructure (`cache.rs`, `cost.rs`,
`CacheBackend` trait) with the `rig-tokudo` crate, which provides a production-grade
LLM optimization layer: content-addressed caching, real API token counts, prompt
compression, cost-based model routing, deterministic replay, semantic caching,
and structured observability — all with zero rig-compose dependency.

## Motivation

The review-harness maintains three tightly coupled custom modules for LLM
interaction:

| Module | Lines | Function |
|--------|-------|----------|
| `cache.rs` | 603 | SHA256 content-addressed LLM caching with disk persistence |
| `cost.rs` | 299 | Token estimation (char/4 heuristic), USD cost calculation, cache hit rate |
| `CacheBackend` trait (in `crb-consensus`) | 45 | Cross-crate cache interface |
| Wiring in `main.rs` + consensus | ~170 | Passing cache+tracker through 4 function signatures |

Total: ~1,100 lines of custom, hand-rolled infrastructure with known limitations:

1. **Token counts are estimated** as `char_count / 4` — inaccurate for code review
2. **No prompt compression** — full diffs sent to LLM, even for trivial fixes
3. **No cost-based routing** — every PR uses the same expensive model
4. **No deterministic replay** — can't reproduce a run without re-calling the API
5. **Exact SHA256 match only** — semantically identical prompts with different
   formatting miss the cache

`rig-tokudo` solves all of these with a single decorator interface, eliminating
the custom code entirely.

## Scope

- **In scope:**
  - Add `rig-tokudo = "0.2"` dependency to workspace + crb-harness
  - Replace `LlmCache` with `OptimizedModel::with_cache()`
  - Replace `CostTracker` with `OptimizedModel::with_pricing()`
  - Remove `cache.rs`, `cost.rs`, `CacheBackend` trait entirely
  - Enable prompt compression, deterministic replay, structured observability
  - Wire pricing rates from `.env` into tokudo's `PricingConfig`
  - Add `--replay-dir` CLI flag for deterministic replay
  - Update all tests that reference removed types

- **Out of scope:**
  - Changing agent dispatch, role definitions, or system prompts
  - Modifying the consensus pipeline's orchestration logic
  - Changing tool definitions or MCP integration
  - Non-rig model backends

## Key Design Decisions

1. **`OptimizedModel` wraps the existing model** — no change to how agents are
   built or how `rig::Agent::prompt()` is called. The decorator is transparent.
2. **Cache directory format changes** — tokudo uses its own on-disk layout;
   existing cache directories will not be forward-compatible.
3. **Real token counts from API** — tokudo extracts `usage` from provider
   responses, replacing the char/4 heuristic.
4. **Pricing via `PricingConfig`** — model-specific rates read from `.env` vars
   with sensible defaults for deepseek-v4-flash.
5. **Replay is opt-in via `--replay-dir`** — when enabled, all LLM calls are
   recorded to a replay file; a second run with the same dir replays from it.

## Directory Structure

```
review-harness/
├── Cargo.toml                         # [workspace.dependencies] +rig-tokudo = "0.2"
└── crates/
    └── crb-harness/
        ├── Cargo.toml                 # +rig-tokudo dependency
        └── src/
            ├── main.rs                # Replace LlmCache + CostTracker wiring
            ├── cache.rs               # REMOVED (603 lines)
            └── cost.rs                # REMOVED (299 lines)
    └── crb-consensus/
        └── src/
            └── lib.rs                 # Remove CacheBackend trait (45 lines)
                                       # Remove cache_key helper functions
                                       # Remove cache-related parameters
```
