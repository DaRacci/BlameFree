# Proposal: CRG Context Map for Code Review

**Change ID:** crg-context-map
**Status:** Draft
**Author:** Hermes Agent
**Date:** 2026-06-29

## Summary

Replace expensive live tool-calling agents with a pre-computed Code-Reference Graph (CRG) context map injected into the agent prompt. The CRG is built once per worktree via tree-sitter parsing, stores symbols, definitions, dependencies, call graphs, and diff context in a compact queryable structure, and is rendered as ~1K-3K tokens of compact text directly injected into the system prompt. Five lightweight read-only query tools (`context_query`, `read_context_section`, `find_references`, `find_definition`, `show_diff_context`) remain as fallback for deep exploration.

## Problem Statement

Current code review agents rely on live tool calls — `read_file`, `grep`, `terminal`, `list_dir` — to gather repository context. This has three critical failures:

1. **Token cost is ~8x too high** — Each agent makes 3-6 tool turns, each returning file/command output. A single PR review across 4 agents consumes 60K-200K tokens at $0.06-$0.20/PR.
2. **deepseek-v4-flash cannot terminate tool loops** — The model will continue calling tools indefinitely unless a hard cap is enforced, which degrades review quality when the cap is prematurely hit.
3. **Latency suffers** — Live tool calls are sequential (parse -> read -> think -> call again), adding 10-30s of wall-clock time per agent.

## Solution

Pre-compute a Code-Reference Graph (CRG) context map once per worktree checkout, then:

1. **Inject** the compact text rendering (~1K-3K tokens) into the agent's system prompt via `{context_map}` template variable.
2. **Register 5 lightweight query tools** that read from the pre-computed graph — no live filesystem operations, no subprocess spawning.
3. **Eliminate the terminal tool** entirely in context-map mode, which removes the tool-loop termination problem.

**Result:** 2-10x cheaper, 2-5x faster, no tool-loop issues, F1 ≥ 75% (current baseline).

## Scope

### In scope

- **`crb-context` crate** — Rust-based CRG builder with tree-sitter bindings, per-language extractors (Python, JS/TS, Rust, Go, Java, Ruby), dependency graph, call graph, diff-aware context, and compact text renderer.
- **Context map data schema** — File tree, symbol definitions, references, dependency graph, call graph, diff context, test coverage mapping.
- **5 query tools** — `context_query`, `read_context_section`, `find_references`, `find_definition`, `show_diff_context`.
- **Prompt injection strategy** — `{context_map}` template variable, PageRank-based ranking, budget-aware token selection.
- **Cache integration** — Content-addressed caching keyed by `(repo_state_hash, diff_hash, model_name)`.
- **Integration with consensus pipeline** — Wire context map build into `evaluate_pr_with_consensus`.

### Out of scope

- Full IDE-like semantic analysis (no type inference, no cross-language references). CRG is syntax-based, not type-aware.
- Real-time code navigation (context map is a snapshot taken at build time).
- Live tool fallback mode (the terminal/grep/read_file tools from the current agent-tools change remain available as an optional toggle, but are not part of this change).

## Key Design Decisions

1. **Rust over Python** for the builder — Performance (parallel parsing via rayon) + integration with existing Rust harness. Python prototype first for fast iteration (Phase 0).
2. **Compact text over JSON** for prompt injection — ~10x smaller, directly readable by LLMs, preserves line-based structure.
3. **PageRank over naive inclusion** for context selection — Proven in aider, prioritizes files with most dependencies.
4. **Diff-first ranking** — Changed files + their callers/callees get priority. This is the review-specific twist.
5. **Keep 5 query tools** as fallback — Agents get the full compact map in the prompt and can query deeper without live filesystem access.
6. **Pre-cache file content** — `read_context_section` reads from the build-time snapshot, not the live filesystem (prevents TOCTOU issues, works offline).

## Directory Structure

```
review-harness/
├── crates/crb-context/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs              # Public API: build_context_map()
│       ├── parser.rs           # tree-sitter parsing orchestration
│       ├── definitions.rs      # Extract symbol definitions per language
│       ├── references.rs       # Extract symbol references/calls
│       ├── dependencies.rs     # Extract import/dependency graph
│       ├── diff.rs             # Diff-aware context extraction (PR changes)
│       ├── graph.rs            # Build and traverse the CRG
│       ├── ranking.rs          # PageRank-based context ranking
│       ├── serialization.rs    # Output formats (JSON, compact text)
│       ├── tools.rs            # Query tool definitions (context_query, etc.)
│       └── cache.rs            # Content-addressed caching logic
└── scripts/
    └── build_context_map.py    # Python prototype (Phase 0)
```

## Token Cost Comparison

| Approach | Prompt Size | Turns | Total Tokens | Cost Factor |
|---|---|---|---|---|
| **Live tool calls (current)** | ~2K + variable tool output | 3-6 turns | ~15K-50K per agent | 1x (baseline) |
| **Context map injection** | ~2K + ~1K-3K context | 1 turn (no tools) | ~3K-5K | **~3-10x cheaper** |
| **Hybrid (inject + tools)** | ~2K + 1K-3K context | 1-2 turns | ~5K-10K | **~2-5x cheaper** |

**Per PR cost (all 4 agents, deepseek-v4-flash):**

| Approach | Tokens/PR | Est. Cost |
|---|---|---|
| Live tool calls (6 turns each, 4 agents) | 60K-200K | $0.06-$0.20 |
| Context map injection only | 12K-20K | $0.01-$0.02 |
| Hybrid (inject + 2 tools) | 20K-40K | $0.02-$0.04 |
