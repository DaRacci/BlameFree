# Proposal: Multi-Agent Consensus (Council/Gate Pattern)

**Change ID:** multi-agent-consensus
**Status:** Draft
**Author:** Hermes Agent
**Date:** 2026-06-26

## Summary

Introduce a multi-agent council/consensus pattern that orchestrates N independent
LLM reviewer agents (SA, CL, AR, SEC) to review the same PR diff concurrently,
then aggregates their structured findings through a judge agent for
precision/recall/F1 evaluation against golden comments.

## Motivation

Code review quality varies dramatically by reviewer perspective. A single LLM
agent may miss security vulnerabilities, logic errors, architectural concerns, or
code style issues depending on prompting. By running **four role-specialized
agents in parallel** and aggregating their outputs, we achieve:

1. **Broader coverage** — each agent's system prompt focuses on a distinct
   review dimension (static analysis, correctness, architecture, security).
2. **Reliable metrics** — the judge compares all candidate findings against
   golden comments to compute TP/FP/FN and precision/recall/F1.
3. **Deterministic, reproducible runs** — agents are independent (no
   inter-agent debate), so results are stable across runs.
4. **Minimal orchestration complexity** — `futures::join!` four agent calls;
   no need for a custom "council" library.

Without this pattern, the harness relies on a single review pass, leaving gaps
in review coverage and offering no objective quality measurement against ground
truth.

## Scope

- **In scope:**
  - Four reviewer agent roles: Static Analysis (SA), Code Logic (CL),
    Architecture (AR), Security (SEC).
  - Concurrent agent invocation via `tokio::JoinSet`.
  - Shared `Finding` struct (file, line, message, severity) across agents.
  - Judge agent for TP/FP/FN evaluation against golden comments.
  - Precision, recall, and F1 aggregation.
  - Optional consensus pass for disagreement resolution.

- **Out of scope:**
  - Inter-agent debate rounds (agents respond to each other).
  - Role-specific diff slicing (all agents get the full diff).
  - CI/CD integration.
  - Non-rig model orchestration.

## Key Design Decisions

1. **Independent agents** — No inter-agent communication. Simpler, faster,
   deterministic, and reproducible.
2. **Shared Finding struct** — Unified schema across all agents, linters,
   and the judge.
3. **Full diff for all agents** — No role-specific diff slicing (complexity
   with no proven benefit).
4. **Judge is a separate agent** — Different system prompt, potentially
   different model, distinct from reviewer agents.
5. **`rig::extractor::Extractor` for structured output** — Auto-deserialized
   into `Vec<Finding>` via schemars. No manual JSON parsing.
6. **`tokio::JoinSet` for concurrency** — Bounded parallelism, easy error
   propagation, fair scheduling.

## Directory Structure

```
review-harness/
├── Cargo.toml                     # [workspace] members = ["crates/*"]
└── crates/
    └── crb-consensus/             # Multi-agent orchestration crate
        ├── Cargo.toml             # deps: rig-core, tokio, crb-agents, crb-judge
        └── src/
            └── lib.rs             # Module exports, consensus flow entry point
                                   # Agent builder, role definitions, system prompts
                                   # Judge agent, golden comment matching, types
```
