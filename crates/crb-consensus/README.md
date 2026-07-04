# crb-consensus

Multi-agent consensus orchestration for code review evaluation — runs multiple LLM reviewers concurrently and aggregates their findings via heuristic matching and LLM judge fallback.

- [`run_reviewers()`] spawns all configured reviewer agents concurrently with 900s timeouts, content-addressed caching, and finding cap enforcement
- Heuristic matching ([`judge_comment()`]) matches golden comments to findings by file, line, and Jaccard text similarity — no LLM call needed for obvious matches
- Full pipeline ([`run_consensus()`]) runs reviewers, performs heuristic matching, falls back to the LLM judge, and computes precision/recall/F1
- [`evaluate_pr_with_consensus()`] provides a drop-in convenience function matching the existing `evaluate_pr()` signature

## Key types

- [`CacheBackend`](src/lib.rs) — Trait for caching LLM interactions (agent prompts, judge calls, context gatherer) with content-addressed key methods
- [`ConsensusReport`](src/lib.rs) — Output with agents, true positives, false positives, false negatives, aggregate metrics, API call counts, cache hit rates, and token usage
- [`Role`](src/lib.rs) — Dynamic newtype around a string abbreviation (e.g. "SA", "CL", "AR", "SEC", "GEN"), loaded at runtime from the agent manifest
- [`ReviewerConfig`](src/lib.rs) — Configuration: role, model, max_findings
- [`MatchResult`](src/lib.rs) — `TruePositive`, `FalsePositive`, `FalseNegative`
- [`GoldenComment`](src/lib.rs) — A golden (expected) comment with file, line, message_regex, severity, and source role
