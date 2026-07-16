# AGENTS.md

## What is this?

**Blame Free** is an AI Code review platform, similar to CodeRabbit, but fully open-source, written in Rust, and completely self-hostable.

## Project Layout

```
review-harness/
├── crates/
│   ├── crb-agents/        # Agent prompt templates
│   ├── crb-auditor/       # Severity auditor
│   ├── crb-benchmark/     # dataset loading, PR iteration, result aggregation
│   ├── crb-cache/         # Content-addressed cache
│   ├── crb-consensus/     # LLM-as-judge, match scoring, and consensus orchestration
│   ├── crb-harness/       # Core evaluation engine: pipeline, runner, config, review orchestration
│   ├── crb-macros/        # Proc-macros for generating boilerplate
│   ├── crb-reporting/     # analytics, cost tracking, and history persistence
│   ├── crb-rules/         # Dynamic rules loaded based on the touched files in a review
│   ├── crb-shared/        # Shared utilities across crates
│   ├── crb-tools/         # LLM Tools, linters, and MCP
│   ├── crb-types/         # Domain types: Generic structs, and enums that are used across multiple crates
│   ├── crb-webui-backend/ # Dashboard backend
│   ├── crb-webui-frontend/# Leptos WASM frontend
│   ├── crb-webui-shared/  # Shared types between frontend and backend
└── .forgejo/workflows/    # CI (Forgejo Actions)
```

## Build & Test

```bash
# Check compilation (fast)
cargo check --workspace

# Run all tests
cargo nextest

# Update insta snapshots after intentional changes
cargo test --workspace     # shows diff
cargo insta review         # interactively accept/reject changes
```

## Testing Conventions

- **insta** for value snapshots (`.snap` files, `cargo insta review`). NO raw `assert_eq!` for comparing structured output.
- **trybuild** for proc-macro compile-pass/fail tests (`.stderr` files, `TRYBUILD=overwrite`).
- **No useless serde round-trip tests.** Testing `#[derive(Serialize, Deserialize)]` round-trips is testing serde itself — only test custom serde logic (`rename`, `with`, `tag`, custom impls).
- Unit tests go in `#[cfg(test)] mod tests` blocks inline. Integration tests go in `tests/` directory.

## Key Systems

### Cache System

Content-addressed caching using `crb-cache`. `CacheBackend` trait with `FilesystemBackend` default.
Cache keys are SHA-256 hashes of prompt + model + input. The `get_or_compute` method encapsulates the load->miss->compute->store pattern.

### PromptLibrary

Singleton at `crb_agents::prompts::PromptLibrary`. Uses `include_dir!` for embedded prompt templates.
Agents are addressed by abbreviation (e.g. `"SA"` for Security Analyst).
Declare agents in `PromptLibrary::config(abbrev)`, never use raw role strings.
