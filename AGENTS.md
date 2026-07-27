# AGENTS.md

## What is this?

**Blame Free** is an AI Code review platform, similar to CodeRabbit, but fully open-source, written in Rust, and completely self-hostable.

## Project Layout

```
BlameFree/
├── crates/
│   ├── riv-agents/        # Agent prompt templates
│   ├── riv-auditor/       # Severity auditor
│   ├── riv-benchmark/     # dataset loading, PR iteration, result aggregation
│   ├── riv-cache/         # Content-addressed cache
│   ├── riv-consensus/     # LLM-as-judge, match scoring, and consensus orchestration
│   ├── riv-harness/       # Core evaluation engine: pipeline, runner, config, review orchestration
│   ├── riv-macros/        # Proc-macros for generating boilerplate
│   ├── riv-reporting/     # analytics, cost tracking, and history persistence
│   ├── riv-rules/         # Dynamic rules loaded based on the touched files in a review
│   ├── riv-shared/        # Shared utilities across crates
│   ├── riv-tools/         # LLM Tools, linters, and MCP
│   ├── riv-types/         # Domain types: Generic structs, and enums that are used across multiple crates
│   ├── riv-webui-backend/ # Dashboard backend
│   ├── riv-webui-frontend/# Leptos WASM frontend
│   ├── riv-webui-shared/  # Shared types between frontend and backend
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

Content-addressed caching using `riv-cache`. `CacheBackend` trait with `FilesystemBackend` default.
Cache keys are SHA-256 hashes of prompt + model + input. The `get_or_compute` method encapsulates the load->miss->compute->store pattern.

### PromptLibrary

Singleton at `riv_agents::prompts::PromptLibrary`. Uses `include_dir!` for embedded prompt templates.
Agents are addressed by abbreviation (e.g. `"SA"` for Security Analyst).
Declare agents in `PromptLibrary::config(abbrev)`, never use raw role strings.
