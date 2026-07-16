# Proposal: Live TUI Dashboard

**Change ID:** live-dashboard
**Status:** Draft
**Author:** Hermes Agent
**Date:** 2026-06-27

## Summary

Add an optional Ratatui-based terminal UI dashboard to `crb-harness` that shows all 4 agent processes (SA, CL, AR, SEC) in real-time — their current thought streams, progress, and running cost — alongside a progress bar for the overall PR evaluation run.

## Why

Current tracing output gives no real-time visibility into agent thought streams, progress, or cost during batch PR evaluation. When running dozens of PRs with 4 concurrent agents each, users cannot see what agents are thinking, gauge progress, track costs, or spot stuck agents.

## What Changes

Add an optional Ratatui-based terminal UI dashboard to crb-harness showing all 4 agent processes (SA, CL, AR, SEC) in real-time — their current thought streams, progress, and running cost — alongside a progress bar for the overall PR evaluation run. Event-driven via mpsc channel, renders at 10fps, gated by --dashboard CLI flag.

## Motivation

The current output is structured tracing: `tracing::info!()` lines logged to stderr showing agent starts, cache hits/misses, and findings. This works well for batch CI but gives no visibility into **what the agents are thinking while they run**. When evaluating dozens of PRs with 4 concurrent agents each, the user sees:

```
[2026-06-27T10:00:01Z INFO  harness] Starting agent role=SA for PR #42
[2026-06-27T10:00:01Z INFO  harness] Starting agent role=CL for PR #42
[2026-06-27T10:00:05Z INFO  agent] CACHE HIT for agent role=SA
[2026-06-27T10:00:12Z INFO  agent] Agent CL returned 3 findings
...
```

There is no way to:

1. See all 4 agent thought processes **simultaneously** in a single view.
2. Gauge how far through a batch of PRs the harness has progressed.
3. Track running cost in real-time without digging into per-agent tracing spans.
4. Spot which agent is stuck or taking unusually long.

A live TUI dashboard solves all four with a single terminal window showing agent panes, a progress bar, and real-time cost.

## Scope

- **In scope:**
  - New `crb-dashboard` module within `crb-harness` (not a separate crate)
  - Ratatui + crossterm dependencies
  - Event-driven architecture: agents emit events over an `mpsc` channel, dashboard task renders at 10fps
  - 4-pane agent view (one per role: SA, CL, AR, SEC), each showing latest thought chunk
  - Bottom progress bar showing completed X of Y PRs
  - Running cost display (total USD and per-agent breakdown)
  - Optional `--dashboard` CLI flag; without it, existing tracing behavior is unchanged
  - Graceful fallback: dashboard skipped when `--dashboard` is absent, no overhead

- **Out of scope:**
  - Web dashboard (this is a pure TUI, no HTTP server)
  - Persistent dashboard state (no file storage of dashboard data)
  - Per-agent interaction (clicking, selecting, expanding panes)
  - Historical scrollback (only live/current state)
  - Windows terminal support (crossterm supports it, but not tested)
  - Color themes or customization

## Key Design Decisions

1. **Event-driven via `mpsc`** — Agents don't know about the dashboard. They send lightweight event messages (agent started, chunk received, finished) through a shared channel. The dashboard task reads from the channel and updates internal state.
2. **Dashboard runs as a `tokio::spawn` task** — Pinned to the same runtime as agent tasks, rendering at ~10fps via `tokio::time::interval`.
3. **No lock contention** — The dashboard owns all UI state internally. Events are received via channel. No shared `Arc<Mutex<>>` state.
4. **Optional opt-in via `--dashboard`** — CLI flag gates the entire dashboard setup. Without it, `tracing_subscriber::fmt()` writes to stderr as today. With it, tracing output is suppressed and the TUI takes over the terminal.
5. **One-shot PR evaluation loop** — The dashboard wraps the existing `for pr in prs { join_set.spawn(...) }` loop. It tracks which PRs are queued, active, and complete.

## Directory Structure

```
review-harness/
└── crates/
    └── crb-harness/
        ├── Cargo.toml             # + ratatui, crossterm dependencies
        └── src/
            ├── main.rs            # Wire --dashboard flag, spawn dashboard task
            ├── dashboard.rs       # Dashboard struct, AgentPane, rendering
            ├── dashboard_event.rs # DashboardEvent enum, channel types
            └── config.rs          # + --dashboard flag to CliArgs
```
