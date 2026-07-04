# Proposal: Web UI Dashboard

**Change ID:** webui-dashboard
**Status:** Draft
**Author:** Hermes Agent
**Date:** 2026-06-27

## Summary

Add a web-based UI dashboard (`crb-webui`) as a new crate that provides a browser GUI for the review harness. This replaces the need to parse terminal output and JSON files by offering a visual interface for benchmark results, live monitoring, and benchmark launching.

## Motivation

The current user experience for the review harness is entirely terminal-based:
- Running benchmarks: CLI flags and terminal output
- Viewing results: manually reading JSON files in the `output/` directory
- Live monitoring: Ratatui TUI (or nothing if `--dashboard` not passed)

This works for developers immersed in the project but creates friction for:
1. **Quick result browsing** — opening JSON files to compare metrics across PRs
2. **Live monitoring at a distance** — the TUI requires SSH + terminal attachment
3. **Benchmark launching** — remembering all CLI flags and datasets
4. **Historical comparison** — scanning the filesystem to find past runs

A web dashboard solves all of these with a single browser interface.

## Scope

- **In scope:**
  - New `crb-webui` crate with axum HTTP backend
  - Leptos WASM frontend with isomorphic routing
  - REST API for listing past runs, viewing results, and launching benchmarks
  - SSE streaming for live agent monitoring
  - Static file serving of the Leptos WASM bundle
  - CLI entrypoint: `cargo run -p crb-webui`

- **Out of scope:**
  - Authentication / multi-user support
  - Persistence beyond filesystem reads of `output/` directory
  - Mobile responsive design
  - Real-time result persistence (no database)
  - Editing/deleting past runs

## Key Design Decisions

1. **Leptos + axum** — Leptos provides isomorphic Rust WASM with SSR-like hydration. Axum is chosen for its async SSE support and ecosystem.
2. **SSE over WebSocket** — SSE is simpler to implement, works with standard HTTP, and fits the server->client streaming pattern (no bidirectional communication needed).
3. **Subprocess management** — The backend spawns `crb-harness --dashboard-events` as a subprocess and reads JSON events from its stdout. This decouples the harness from the web UI.
4. **Filesystem-based persistence** — Past runs are discovered by scanning the `output/` directory for per-PR JSON files and aggregate summaries.
5. **Optional `--dashboard-events` flag on crb-harness** — When set, the harness outputs structured JSON events (one per line) to stdout instead of (or in addition to) tracing output.
