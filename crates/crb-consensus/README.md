# crb-consensus

Multi-agent consensus orchestration for code review evaluation — runs multiple LLM reviewers concurrently and matches their findings against golden comments.

- [`run_reviewers()`] spawns SA/CL/AR/SEC agents concurrently with 120s timeouts, content-addressed caching, and finding cap enforcement
- Heuristic matching ([`judge_comment()`]) matches golden comments to findings by file, line, and regex on message — no LLM call needed for obvious matches
- Full pipeline ([`run_consensus()`]) runs reviewers, performs heuristic matching, falls back to the LLM judge, and computes precision/recall/F1

## Key types

- [`CacheBackend`](src/lib.rs) — Trait for caching LLM interactions (agent prompts, judge calls) with content-addressed key methods
- [`ConsensusReport`](src/lib.rs) — Output with agents, true positives, false positives, false negatives, and aggregate metrics
- [`Role`](src/lib.rs) — Enum for SA, CL, AR, SEC with `as_str()` conversion
- [`ReviewerConfig`](src/lib.rs) — Configuration: role, model, max_findings
- [`MatchResult`](src/lib.rs) — `TruePositive`, `FalsePositive`, `FalseNegative`
