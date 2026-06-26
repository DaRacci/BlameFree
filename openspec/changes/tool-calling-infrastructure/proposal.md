# Proposal: Tool Calling Infrastructure

**Change ID:** tool-calling-infrastructure
**Status:** Draft
**Author:** Hermes Agent
**Date:** 2026-06-26

## Summary

Define a typed, concurrent, and safe infrastructure for wrapping external tools
(linters, git, diff) as Rig `Tool` trait implementations, enabling the review
harness to invoke them both directly and as LLM-provided tools.

## Motivation

The review harness needs to run several external operations — linters across
multiple languages (Ruff for Python, ESLint for JavaScript/TypeScript,
`go vet` for Go), git operations (checkout, diff, clean), and diff generation —
in a way that is:

1. **Typed** — each tool has well-defined input args and output types.
2. **Concurrent** — multiple linters can run simultaneously via `tokio`.
3. **Safe** — subprocesses are bounded by timeouts, errors are typed, no shell
   injection via argument arrays.
4. **LLM-friendly** — the Rig `Tool` trait auto-generates JSON Schema for args,
   so an LLM can decide when to invoke a tool.

Without this infrastructure, each tool would be ad-hoc, error handling would be
inconsistent, and concurrent execution would require duplicated boilerplate.

## Scope

- **In scope:** LinterTool pattern (Ruff, ESLint, GoVet), GitTool pattern
  (checkout, diff, clean), typed error types (LinterError, GitError),
  TOML-based linter configuration, shared timeout handling, concurrent
  execution via tokio.
- **Out of scope:** Specific linter rule configuration, CI/CD integration,
  non-rig orchestration, git2 crate bindings.

## Key Design Decisions

1. **Rig Tool trait** — All external operations implement `rig::tool::Tool`.
2. **TOML config** — Linter definitions live in TOML, not hardcoded.
3. **Pure parser functions** — Linter output parsers are `fn(&str) -> Result<Vec<Finding>>`,
   testable in isolation.
4. **`std::process::Command` over git2** — Simpler, no C library dependency.
5. **`tokio::time::timeout`** — 60s default per subprocess.

## Directory Structure

```
crb-harness/src/
└── tools/
    ├── mod.rs       # Module exports
    ├── linter.rs    # LinterTool, LinterArgs, LinterError, parsers
    └── git.rs       # GitCleanTool, GitDiffTool, GitArgs, GitError
```
