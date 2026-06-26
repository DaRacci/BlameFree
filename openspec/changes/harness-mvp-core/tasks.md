# Tasks: MVP Core Harness

## Phase 1 — Scaffold & Data
- [ ] cargo init, create Cargo.toml with rig-core, tokio, clap, serde, serde_json, schemars, anyhow, tracing, csv
- [ ] Create src/ directory structure: main.rs, agents.rs, judge.rs, reporting.rs, config.rs
- [ ] Copy Martian golden_comments/ into datasets/golden_comments/
- [ ] Create manifest.json referencing all 5 language datasets
- [ ] Set up pre-committed diff files for the 50 benchmark PRs

## Phase 2 — Core Loop
- [ ] Implement config.rs: CliArgs struct with clap derive, provider/env config
- [ ] Implement agents.rs: build_agent() with per-role prompt templates via rig AgentBuilder, provider routing for OpenRouter/OpenAI/Anthropic
- [ ] Implement judge.rs: Martian JUDGE_PROMPT integration via rig Extractor, JudgeVerdict struct with schemars
- [ ] Implement main.rs: clap CLI, tokio::task::JoinSet concurrency loop, evaluate_pr() orchestration
- [ ] Implement reporting.rs: per-PR JSON via serde, summary CSV via csv crate, tracing spans for latency

## Phase 3 — Validation
- [ ] Run against regression set (3 PRs) and compare results against known v5.14 baseline
- [ ] Run against validation set (9 PRs) and validate precision/recall within noise margin
- [ ] Test with concurrency=1 (serial mode) for debugging
- [ ] Test with concurrency=24 (full batch) for performance

## Phase 4 — Polish
- [ ] Add tracing spans per PR, per agent, per judge call with structured fields
- [ ] Add retry logic with exponential backoff for transient LLM failures (reqwest retry middleware or manual)
- [ ] Add --dry-run flag that shows config and PR count without making API calls
- [ ] Add --resume flag to skip already-processed PRs (check output dir for existing results)
- [ ] Write README.md with usage examples, provider setup, and benchmark workflow
