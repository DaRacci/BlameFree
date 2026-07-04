# crb-adapter

Converts per-PR harness findings to `candidates.json` for the Python `step3_judge_comments.py` pipeline, with an optional judge invocation step.

## Purpose

The adapter bridges the Rust harness output format and the Python offline benchmark pipeline (`withmartian/code-review-benchmark`). It:

- Scans `output/{run_id}/` for per-PR JSON files produced by `crb-harness`
- Reads each file's `findings` array (supporting both `pr_url` and `url` field names for backward compatibility)
- Writes a unified `candidates.json` in the format expected by `step3_judge_comments.py`
- Optionally runs the Python judge directly after conversion (`--judge`)

## Place in the workspace

The adapter sits between `crb-harness` (which produces per-PR result JSON) and the offline Python `step3_judge_comments.py` script. It converts findings into the `{ PR_URL → { tool_name → [candidate, ...] } }` structure required by the judge pipeline.

## Key types

- [`Args`](src/main.rs) — CLI arguments: `--run-id`, `--output-dir`, `--judge`
- [`Candidates`](src/main.rs) — `BTreeMap<String, ToolCandidates>` mapping PR URLs to per-tool candidate lists
- [`CandidateFinding`](src/main.rs) — A single candidate with `text`, optional `path`/`line`, and `source`
- [`HarnessFinding`](src/main.rs) — Flexible deserialization of harness findings (accepts extra fields like `rule_code`, `severity`, `source`)
- [`PerPrFile`](src/main.rs) — Per-PR input JSON supporting both `pr_url` and `url` field names

## CLI usage

```bash
# Convert findings from run "ca-test-1"
cargo run -p crb-adapter -- --run-id ca-test-1

# Custom output directory and run judge
cargo run -p crb-adapter -- \
  --run-id my-run \
  --output-dir /data/output \
  --judge
```

## Feature flags

No feature flags — this crate uses only workspace dependencies (`clap`, `serde`, `serde_json`).
