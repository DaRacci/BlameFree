# Tasks: MVP Core Harness

## Phase 1 — Workspace scaffold:
- [x] Create workspace Cargo.toml with [workspace] members = ["crates/*"]
- [x] Create all 8 crate directories with stub Cargo.toml files
- [x] Wire inter-crate dependencies
- [x] cargo check (if rustc available)

## Phase 2 — Core Loop
- [x] Implement `crates/crb-harness/src/main.rs`: clap CLI, tokio::task::JoinSet concurrency loop, evaluate_pr() orchestration
- [x] Implement `crates/crb-agents/src/lib.rs`: build_agent() with per-role prompt templates via rig AgentBuilder, provider routing for OpenRouter/OpenAI/Anthropic
- [x] Implement `crates/crb-judge/src/lib.rs`: Martian JUDGE_PROMPT integration via rig Extractor, JudgeVerdict struct with schemars
- [x] Implement `crates/crb-reporting/src/lib.rs`: per-PR JSON via serde, summary CSV via csv crate, tracing spans for latency
- [x] Implement `crates/crb-consensus/src/lib.rs`: run_consensus(), evaluate_pr() orchestration wiring

## Phase 3 — Validation
- [ ] Run against regression set (3 PRs) and compare results against known v5.14 baseline
- [ ] Run against validation set (9 PRs) and validate precision/recall within noise margin
- [ ] Test with concurrency=1 (serial mode) for debugging
- [ ] Test with concurrency=24 (full batch) for performance

## Phase 4 — Polish
- [x] Add tracing spans per PR, per agent, per judge call with structured fields
- [ ] Add retry logic with exponential backoff for transient LLM failures (reqwest retry middleware or manual)
- [x] Add --dry-run flag that shows config and PR count without making API calls
- [x] Add --resume flag to skip already-processed PRs (check output dir for existing results)
- [x] Write README.md with usage examples, provider setup, and benchmark workflow
