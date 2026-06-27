# Deterministic Replay Specification

**Type:** Behavioral Spec
**Change:** rig-tokudo
**Status:** Draft

## 1. Purpose

Define the contract for deterministic record/replay of LLM interactions using
`rig-tokudo`'s built-in replay capability. This is the review-harness's #1
planned feature — enabling bit-for-bit identical reruns without API calls.

## 2. Motivation

Code review evaluations must be **reproducible**. Without replay:

1. A PR benchmark run on Monday may produce different results on Tuesday due to
   model changes, API drift, or non-deterministic sampling.
2. Debugging a failing evaluation requires API access and incurs cost.
3. CI/CD comparisons between runs are confounded by LLM randomness.

With deterministic replay:
- A recorded run can be replayed **offline** with identical results
- CI/CD can verify that code changes don't alter evaluation behavior
- Cache-only runs become the default for quick iterations

## 3. Replay Configuration

### 3.1 CLI Flag

```text
--replay-dir <PATH>    Directory for deterministic replay traces.
                       On first run: records all LLM interactions to this dir.
                       On subsequent runs: replays from recorded traces (no API calls).
```

### 3.2 Integration

```rust
let optimized = OptimizedModel::new(model)
    .with_cache(Some(cache_dir))
    .with_pricing(pricing_config)
    .with_replay(replay_dir);  // <-- new
```

- **If `replay_dir` is `None`:** No replay; normal operation with caching
- **If `replay_dir` exists and has traces:** Replay mode — all LLM responses come from the replay file (no API calls)
- **If `replay_dir` does not exist:** Record mode — all LLM calls are made to the API and recorded to `{replay_dir}/traces.jsonl`

## 4. Record Mode

### 4.1 Behavior

On the **first run** with a new `--replay-dir`:

1. The directory is created if it doesn't exist
2. Every LLM call (agent, judge, context gatherer) is recorded to
   `{replay_dir}/traces.jsonl`
3. Each trace entry captures:
   - Prompt (full input text)
   - Response (full output text)
   - Model name
   - Timestamp
   - Token usage (if available)
   - Hash of the complete interaction (for integrity checking)
4. API calls proceed normally; caching is orthogonal to replay

### 4.2 Trace File Format

```jsonl
{"type":"completion","model":"deepseek/deepseek-v4-flash","prompt_hash":"abc123...","prompt":"...","response":"...","input_tokens":450,"output_tokens":120,"timestamp":"2026-06-27T10:00:00Z"}
{"type":"completion","model":"deepseek/deepseek-v4-flash","prompt_hash":"def456...","prompt":"...","response":"...","input_tokens":320,"output_tokens":88,"timestamp":"2026-06-27T10:00:05Z"}
```

Each line is a JSON object representing a single LLM call. The file is
append-only (can be resumed across partial runs).

## 5. Replay Mode

### 5.1 Behavior

On **subsequent runs** with the same `--replay-dir`:

1. Traces are loaded from `{replay_dir}/traces.jsonl`
2. For each LLM call, tokudo computes the prompt hash and looks it up in the
   trace index
3. On match: return the recorded response immediately (no API call)
4. On miss: **fail with an error** (replay miss indicates the evaluation changed)
5. Token usage metadata is restored from the trace record

### 5.2 Replay Matching

| Match Criterion | Description |
|----------------|-------------|
| Primary Key | SHA256 hash of the full prompt text + model name |
| Exact Match | Prompt must be byte-identical to the recorded trace |
| Order Independence | Traces are indexed by hash; order of calls doesn't matter |

### 5.3 Strict Mode (Default)

By default, replay is strict: **every** LLM call must match a recorded trace.
If any call doesn't match (e.g., because the prompts changed), the run fails
immediately with a clear error:

```text
ERROR: Replay miss: no trace found for prompt hash abc123...
Expected traces: 237  |  Current run call count: 242
The evaluation logic appears to have changed since the trace was recorded.
To update the trace, delete the replay directory and re-run.
```

## 6. Orthogonality with Caching

Replay and caching are **independent** features:

| Feature | Scope | Purpose |
|---------|-------|---------|
| Cache (`.with_cache()`) | Keyed by prompt content | Speed up repeated runs of the same benchmark |
| Replay (`.with_replay()`) | Keyed by recorded traces | Deterministic reproduction of a specific run |

Both can be enabled simultaneously:
- Cache provides speed for identical prompts within a run
- Replay provides determinism across runs (even if prompts change slightly)

## 7. Use Cases

### 7.1 CI/CD Verification

```bash
# Record baseline
cargo run --release -- --replay-dir /ci/replay-baseline --prs baseline.json

# Run with code changes — must produce identical output
cargo run --release -- --replay-dir /ci/replay-baseline --prs baseline.json
```

### 7.2 Debugging

```bash
# Record a failing run
cargo run --release -- --replay-dir /tmp/bug-replay --prs failing-pr.json

# Replay offline (no API key needed)
cargo run --release -- --replay-dir /tmp/bug-replay --prs failing-pr.json
```

### 7.3 Cache Warm + Replay Verify

```bash
# Warm cache and record replay simultaneously
cargo run --release -- --cache-dir /tmp/cache --replay-dir /tmp/replay --prs all-prs.json

# Next run: replay from trace (fast, no API)
cargo run --release -- --cache-dir /tmp/cache --replay-dir /tmp/replay --prs all-prs.json
```

## 8. Error Handling

| Scenario | Behavior |
|----------|----------|
| Replay directory doesn't exist | Record mode: create dir, record all calls |
| Replay directory exists but is empty | Record mode: warn "empty replay dir", record all calls |
| Replay directory has traces, run matches | Replay mode: serve all responses from traces |
| Replay directory has traces, run differs (prompt mismatch) | Error: "replay miss" with hash + count details |
| Replay file corrupted (invalid JSON) | Error: "replay file corrupted at line N" |
| Partial replay (some hits, some misses) | Error: fail on first miss; do not mix replay + API |
| Replay + no API key available | OK in replay mode; fail in record mode |
