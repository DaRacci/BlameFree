# crb-aggregator

Port of the original Python `aggregate_findings.py` — report parsing, semantic deduplication, and candidate formatting for code review findings.

- Parses Phase 4 report markdown with multi-format support (table rows, bullet lists, JSON blocks)
- [`semantic_dedup()`] deduplicates findings by (file, function) grouping with Jaccard similarity fallback, cross-validation tracking, and merged output
- Formats candidates with severity labels, cross-validation badges, and source tracking

## Key types

- [`Candidate`](src/lib.rs) — Formatted output: `text`, `path`, `line`, `source`
- [`Stats`](src/lib.rs) — Aggregate batch statistics with per-report breakdown
- [`Severity`](src/lib.rs) — Enum: `Critical`, `High`, `Medium`, `Low`
- [`aggregate_batch`](src/lib.rs) — Main entry point for processing multiple reports

## CLI usage

```bash
cargo run -p crb-aggregator -- \
  --reports-dir /path/to/reports \
  --output candidates.json

# With PR filter and archive mode
cargo run -p crb-aggregator -- \
  --reports-dir /path/to/reports \
  --output candidates.json \
  --pr-filter "36880,11059" \
  --replace \
  --archive
```
