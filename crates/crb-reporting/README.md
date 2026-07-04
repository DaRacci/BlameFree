# crb-reporting

Dataset loading, result types, and output writing for the code review benchmark harness.

- [`load_golden_datasets()`] loads all golden-comment entries from every `.json` file under a dataset directory (supports both `DatasetFile { entries: [...] }` and raw array formats)
- [`write_report()`] writes per-PR JSON result files to an output directory with sanitized filenames
- Defines shared data structures used across the harness: [`PrResult`], [`GoldenCommentEntry`], [`CostSummary`]

## Key types

- [`GoldenCommentEntry`](src/lib.rs) — A single PR entry: `pr_title`, `url`, `comments`
- [`PrResult`](src/lib.rs) — Evaluation result for a PR: `pr_title`, `url`, `findings_count`, `golden_count`, `metrics`, `verdicts`, `cost`
- [`CostSummary`](src/lib.rs) — Cost data: `agent_tokens_in/out`, `judge_tokens_in/out`, `total_usd`, `agent_cache_hit_rate`, `judge_cache_hit_rate`
