# Change: Linter Integration via Rust + rig Tool Trait

## Intent
Add linter evaluation to the harness via Rust `rig::tool::Tool` trait implementations that wrap linter subprocess calls. Each linter implements `Tool` — giving the LLM both standalone and tool-call access to linter findings. Scope matches the original design but replaces Python subprocess wrappers with native Rust.

## Why
The harness needs static analysis capabilities to supplement LLM agent findings with concrete linter results. Without linter integration, the harness misses real code issues that static analysis would catch, reducing the comprehensiveness of the review.

## What Changes
Add crb-tools crate with Rust Tool trait implementations wrapping linter subprocess calls. Each linter reads a PR checkout directory, runs via tokio::process::Command, parses output into a shared Finding struct, and returns results. Linters run concurrently with LLM agent calls.

## Scope
Build a `crb-tools` crate with one `Tool` impl per linter. Each reads a PR checkout directory, runs the linter via `tokio::process::Command`, parses output into a shared `Finding` struct (serde + schemars), and returns results. Findings feed into the same judge pipeline.

Out of scope: porting `scaffold_pr.sh` to Rust (separate change).

## Approach
Rust `tokio::process::Command` for async subprocess execution. Each linter is a struct implementing `rig::tool::Tool` with auto-generated JSON Schema via `schemars`. Linters run in parallel with each other and with LLM agent calls via `tokio::task::JoinSet`. Linter config is external TOML, not hardcoded.
