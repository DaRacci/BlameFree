# Live TUI Dashboard Specification

**Type:** Behavioural Spec
**Change:** live-dashboard
**Status:** Draft

## 1. Purpose

Define the contract for the Ratatui-based live TUI dashboard that renders real-time agent thought streams, progress, and running cost during PR evaluation.

## 2. Rendering Contract

### 2.1 Layout

| Region | Height | Content |
|--------|--------|---------|
| Title bar | 1 line | `crb-harness Live Dashboard  HH:MM:SS` |
| Agent panes | remaining (min 10) | 4 equal-width columns: SA, CL, AR, SEC |
| Progress bar | 3 lines | `PRs: [████░░░░░░░] 3/15` |
| Footer | 1 line | Cost summary + keybindings hint |

Minimum terminal size: **80 columns × 24 rows**. Below this, the dashboard renders with a warning banner but does not crash.

### 2.2 Agent Pane Rendering

Each agent pane (SA, CL, AR, SEC) displays:

```
┌─ SA ─────────────────────────────────┐
│ PR: #42 — rust-lang/rust             │  ← bold, white
│                                      │
│ Analyzing the diff for safety        │  ← scrolling text buffer
│ vulnerabilities. The `unsafe` block  │
│ in line 42 uses a raw pointer...     │
│                                      │
│ Running (00:01:23)                   │  ← dim, green
│ Cost: $0.014                         │  ← dim, white
└──────────────────────────────────────┘
```

#### 2.2.1 Border Styling

| Agent Status | Border Color |
|-------------|-------------|
| `Idle` | Dim (gray) |
| `Running` | Green |
| `Finished` | Cyan |
| `Failed` | Red |

#### 2.2.2 Thought Buffer

- **Capacity:** 2000 characters per pane (older text discarded first).
- **Scroll:** No scrollback — only latest text shown. Future: scrollable via mouse wheel.
- **Streaming indicator:** When agent is actively streaming, a `▍` block character is appended to the last line.
- **Empty state:** Shows `"Waiting for agent to start..."` in dim text.
- **Overflow indicator:** If truncated, `...(truncated)` is appended to the buffer.

### 2.3 Progress Bar

```
┌─ PRs ───────────────────────────────────┐
│  ████████████████░░░░░░░░░░  3/15       │
└──────────────────────────────────────────┘
```

- `Gauge` widget with `Cyan` fill on `DarkGray` background.
- Label shows `{completed}/{total}`.
- Ratio = `completed / total`.
- When completed == total, gauge turns full `Green`.

### 2.4 Footer

```
 Total cost: $0.052  |  API calls: 12  |  Cache: 8 hits / 4 misses  |  [q] quit  [p] pause
```

- All values right-aligned to available width.
- Cost is formatted to 4 decimal places in USD.

## 3. Event Processing Contract

### 3.1 Event Ordering

Events from the same agent are emitted in order:

```
AgentStarted(role=SA, pr="PR #42")
  → AgentChunk(role=SA, text="Analyzing...")
  → AgentChunk(role=SA, text="Found unsafe...")
  → AgentFinished(role=SA, pr="PR #42", findings=3, duration=12345)
```

Events from different agents may interleave arbitrarily. The dashboard applies events to the correct agent pane based on the `role` field.

### 3.2 Event Processing Guarantees

| Requirement | Detail |
|-------------|--------|
| Non-blocking | Events are sent via `try_send` — agents never block on a full channel |
| Best-effort | If the channel is full, oldest events are dropped; dashboard may lag but never crash |
| At-most-once | Events may be dropped but are never duplicated |
| Race-safe | `AgentChunk` after `AgentFinished` is silently ignored (agent state transitions are final) |
| No ordering between agents | SA and CL events may arrive in any order; each agent's own events are in-order |

### 3.3 State Machine Per Agent Pane

```
Idle ──[AgentStarted]──▶ Running ──[AgentFinished]──▶ Finished
                           │
                           └──[AgentChunk]──▶ Running (text appended)
```

- `AgentChunk` received while in `Idle` → ignored.
- `AgentFinished` while in `Idle` → ignored.
- `AgentStarted` while in `Running` → resets buffer, stays `Running` (new PR).
- Any event received in `Finished` → ignored until next `AgentStarted`.

## 4. Input Handling

| Key | Action |
|-----|--------|
| `q` | Exit dashboard immediately. Main loop continues; agents finish in background. Terminal is restored. |
| `p` | Pause/resume rendering. Events continue to accumulate in state but frame is not redrawn. Useful for reading a paused agent's output. |
| `Ctrl+C` | Interrupt entire process (same as without dashboard). Terminal is restored via Drop. |

## 5. Terminal Lifecycle

### 5.1 Startup

```
1. Check `--dashboard` flag → if not set, return (no-op).
2. Check `std::io::IsTerminal::is_terminal()` on stdout.
   - If false: warn "Dashboard requires a TTY; falling back to tracing mode."
   - Return early; no terminal setup.
3. Enable raw mode via `crossterm::terminal::enable_raw_mode()`.
4. Enter alternate screen via `execute!(LeaveAlternateScreen)`.
5. Create `mpsc::channel(1024)`.
6. Spawn dashboard task.
```

### 5.2 Shutdown

```
1. User presses `q` → set `should_exit = true` → break render loop.
2. Main loop sends final `RunProgress(completed=total)` → stops after processing.
3. Dashboard task processes remaining events, renders one final frame.
4. Execute `LeaveAlternateScreen`.
5. Disable raw mode.
6. Print completion summary to stdout (same as current `print_terminal_summary()`).
```

### 5.3 Panic Safety

A `Drop` guard is installed before terminal setup:

```rust
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}
```

If the dashboard task panics, the guard ensures the terminal is restored to a usable state.

## 6. Performance Budget

| Metric | Budget |
|--------|--------|
| Render frame rate | 10 fps (100ms interval) |
| Max memory (dashboard) | 64 KB (4 panes × 2000 chars + state overhead) |
| Max channel size | 1024 events |
| Max CPU (dashboard) | < 5% of one core on average hardware |
| Max latency added to agent path | 0 (try_send never blocks) |
