# crb-dashboard

TUI (terminal UI) dashboard library for live monitoring of benchmark runs, driven by [`ratatui`] and [`crossterm`].

- Renders a 4-pane agent view (SA, CL, AR, SEC) showing streaming responses, status, and finding counts
- Displays real-time aggregate metrics (cost, tokens, precision/recall/F1) and per-PR summaries
- Falls back silently to event draining when stdout is not a TTY

## Key types

- [`DashboardEvent`](src/lib.rs) — Event enum: `AgentStarted`, `AgentChunk`, `AgentFinished`, `PrCompleted`, `RunFinished`
- [`Dashboard`](src/lib.rs) — Main state machine with agent panes, progress tracking, and event handling
- [`AggregateMetrics`](src/lib.rs) — Running totals for TP, FP, FN, precision, recall, F1
- [`AgentPane`](src/lib.rs) — Per-role agent state with scrollback buffer

## Usage

The dashboard is activated via `--dashboard` on the `crb-harness` CLI:

```bash
cargo run --bin crb-harness -- --dashboard --concurrency 4
```

Press `q` or `Esc` to exit the TUI at any time.
