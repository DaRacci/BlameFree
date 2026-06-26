# Change: Linter Integration via Rust + rig Tool Trait

## Intent
Add linter evaluation to the harness via Rust `rig::tool::Tool` trait implementations that wrap linter subprocess calls. Each linter implements `Tool` — giving the LLM both standalone and tool-call access to linter findings. Scope matches the original design but replaces Python subprocess wrappers with native Rust.

## Scope
Build a `linters.rs` module with one `Tool` impl per linter. Each reads a PR checkout directory, runs the linter via `tokio::process::Command`, parses output into a shared `Finding` struct (serde + schemars), and returns results. Findings feed into the same judge pipeline.

Out of scope: porting `scaffold_pr.sh` to Rust (separate change).

## Approach
Rust `tokio::process::Command` for async subprocess execution. Each linter is a struct implementing `rig::tool::Tool` with auto-generated JSON Schema via `schemars`. Linters run in parallel with each other and with LLM agent calls via `tokio::task::JoinSet`. Linter config is external TOML, not hardcoded.
