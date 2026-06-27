# Caching Specification

**Type:** Behavioral Spec
**Change:** rig-tokudo
**Status:** Draft

## 1. Purpose

Define the caching contract for LLM interactions when using `rig-tokudo`'s
`OptimizedModel::with_cache()`. The cache replaces the custom `LlmCache`
structure and is transparently managed by tokudo's `CacheLayer` decorator.

## 2. Cache Semantics

### 2.1 Content-Addressed Caching

Every LLM interaction is cached by a key derived from the full input (prompt,
model, parameters). If the exact same input is presented again, the cached
response is returned without making an API call.

**Contract:**
- Given identical prompt input, model, and parameters → cache hit
- Given different prompt input (even one character difference) → cache miss
- Cache key derivation is internal to tokudo; callers must not compute keys

### 2.2 Semantic Caching

In addition to exact content-addressed matching, tokudo supports **semantic
caching**: prompts that are semantically equivalent (but textually different)
share a cache entry.

**Contract:**
- A prompt that is semantically identical to a previous prompt → **may** be a
  cache hit (depends on tokudo's semantic similarity threshold)
- Semantic caching is transparent — callers do not opt in separately beyond
  enabling `.with_cache()`

### 2.3 Cache Boundaries

| Dimension | Boundary | Behavior |
|-----------|----------|----------|
| Model | Different model → separate cache | Tokudo partitions by model name |
| Cache directory | Different dir → separate cache | Each dir is isolated |
| PR key | Different PR → separate sub-cache | Determined by tokudo's layout |

## 3. Cache Lifecycle

### 3.1 Initialization

```rust
// The cache is initialized when OptimizedModel is built with .with_cache():
let optimized = OptimizedModel::new(model)
    .with_cache(Some(cache_dir));
// When cache_dir is None, caching is disabled; every call hits the API
```

**Preconditions:**
- `cache_dir` is a writable directory path
- If `cache_dir` does not exist, it is created

**Postconditions:**
- Caching is active for all LLM calls made through this `OptimizedModel`
- Previous cache entries (if any) are available for lookup

### 3.2 Cache Read

Triggered automatically before every LLM call:

1. Tokudo computes a cache key from the prompt, model, and parameters
2. If a cached response exists → return it immediately (no API call)
3. If no cached response → proceed to API call

**Observability:**
- Cache hits and misses are reported via structured tracing (when`.with_observability(true)` is enabled)
- Cache hit rate statistics are available through tokudo's observability output

### 3.3 Cache Write

Triggered automatically after every successful LLM call:

1. Tokudo computes a cache key from the prompt, model, and parameters
2. The response (and metadata) is written to the cache directory
3. The write is synchronous and blocking (completed before the caller receives the response)

**Persistence:**
- Cache entries persist across process restarts
- Cache directory can be shared across runs to warm the cache
- No explicit "flush" or "save" operation needed

## 4. Cache Replacement

### 4.1 What We Remove

| Custom Component | Tokudo Equivalent |
|------------------|-------------------|
| `LlmCache::new(base, pr_key)` | `OptimizedModel::with_cache(cache_dir)` |
| `LlmCache::sha256(input)` | *Internal* — key derivation is hidden |
| `LlmCache::compute_agent_key(...)` | *Internal* |
| `LlmCache::compute_judge_key(...)` | *Internal* |
| `LlmCache::compute_context_key(...)` | *Internal* |
| `LlmCache::lookup_agent(key)` | *Automatic via decorator* |
| `LlmCache::save_agent_cached(...)` | *Automatic via decorator* |
| `LlmCache::lookup_judge(key)` | *Automatic via decorator* |
| `LlmCache::save_judge_cached(...)` | *Automatic via decorator* |
| `LlmCache::lookup_context(key)` | *Automatic via decorator* |
| `LlmCache::save_context_cached(...)` | *Automatic via decorator* |
| `CacheBackend` trait | *Not needed* — tokudo handles all keying internally |

### 4.2 Cache Format Incompatibility

**Important:** The on-disk cache format used by tokudo is different from our
custom `index.json` + file-per-entry layout. Existing cache directories will
not be readable by tokudo. On first use, the cache directory is treated as
empty and re-populated.

This is acceptable because:
- Caches are per-benchmark ephemeral artifacts
- Tokudo's format is more capable (semantic keying, metadata)
- Migration represents one cold-cache run

## 5. Error Handling

| Scenario | Behavior |
|----------|----------|
| Cache directory not writable | Warning logged; caching disabled for this run |
| Cache entry corrupted (invalid JSON, truncated) | Entry skipped; falls through to API call |
| Cache directory doesn't exist | Directory created automatically |
| Concurrent writes from multiple processes | Best-effort (tokudo uses atomic file operations where possible) |
| Disk full during cache write | Warning logged; API response still returned to caller even if cache write fails |

## 6. Dependencies

- `rig-tokudo = "0.2"` — provides the `OptimizedModel` with `.with_cache()`
- No `sha2` dependency needed (was used only for manual key derivation)
- No additional cache backends (tokudo uses filesystem by default)
