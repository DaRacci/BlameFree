# Design: Live TUI Dashboard

## 1. Overview

The live TUI dashboard renders a real-time view of the multi-agent PR evaluation process in the terminal. It is an **optional** feature gated behind `--dashboard`. When enabled, tracing output is suppressed (or routed to a file), and a Ratatui layout takes over the terminal showing:

- 4 agent panes (SA, CL, AR, SEC) with latest thought chunk
- A progress bar for the overall PR batch
- A running cost footer with total USD and per-agent breakdown

The dashboard is **event-driven**: agents emit lightweight `DashboardEvent` messages over an `mpsc` channel. A dedicated dashboard task receives these events, updates internal state, and triggers a re-render at ~10fps.

### Before (current flow)

```
main()
  └─ for pr in prs {
       └─ spawn(evaluate_pr_with_postprocessing())
            ├─ evaluate_pr_single_agent()  [tracing::info! everywhere]
            │    ├─ agent SA  [tracing spans]
            │    ├─ agent CL  [tracing spans]
            │    ├─ agent AR  [tracing spans]
            │    └─ agent SEC [tracing spans]
            └─ collect findings, write report
     }
  └─ print_terminal_summary()
```

### After (with --dashboard)

```
main()
  ├─ spawn(dashboard_task(channel_rx))     ← NEW: runs at 10fps
  └─ for pr in prs {
       └─ spawn(evaluate_pr_with_postprocessing())
            ├─ evaluate_pr_single_agent()
            │    ├─ agent SA  -> emit AgentStarted, AgentChunk, AgentFinished
            │    ├─ agent CL  -> emit AgentStarted, AgentChunk, AgentFinished
            │    ├─ agent AR  -> emit AgentStarted, AgentChunk, AgentFinished
            │    └─ agent SEC -> emit AgentStarted, AgentChunk, AgentFinished
            │    └─ on complete -> emit PrCompleted
            └─ collect findings, write report
     }
  └─ join dashboard_task, print_terminal_summary()
```

## 2. Architecture

```
┌──────────────────────────────────────────────────────────┐
│                     main loop                             │
│                                                           │
│  for pr in prs {                                         │
│    spawn(evaluate_pr)                                     │
│      ┌─────────────────────────────────────────┐          │
│      │ evaluate_pr_single_agent()              │          │
│      │  spawn agent SA  ───┐                   │          │
│      │  spawn agent CL  ───┤  emit events on   │          │
│      │  spawn agent AR  ───┤  channel_tx        │          │
│      │  spawn agent SEC ───┘                   │          │
│      │  on join: emit PrCompleted              │          │
│      └─────────────────────────────────────────┘          │
│  }                                                        │
│                                                           │
│  while let Some(res) = set.join_next() {                  │
│    results.push(res);                                     │
│    emit RunProgress { completed, total, cost }            │
│  }                                                        │
└────────────────────┬─────────────────────────────────────┘
                     │ channel_tx (mpsc::Sender<DashboardEvent>)
                     ▼
┌──────────────────────────────────────────────────────────┐
│              dashboard task (tokio::spawn)                │
│                                                           │
│  DashboardState {                                         │
│    agents: HashMap<Role, AgentPaneState>,                 │
│    progress: (completed, total),                          │
│    cost: CostTracker snapshot,                            │
│  }                                                        │
│                                                           │
│  loop {                                                   │
│    try_recv_all_events() -> update state                   │
│    render() via Ratatui                                   │
│    tokio::time::sleep(100ms)                              │
│  }                                                        │
└──────────────────────────────────────────────────────────┘
```

### 2.1 Event Channel

```rust
/// Bounded channel for dashboard events.
/// Capacity: 1024 — plenty for streaming agent chunks.
type DashboardChannel = mpsc::Sender<DashboardEvent>;
```

```rust
/// Events emitted by agents and consumed by the dashboard.
enum DashboardEvent {
    /// An agent has started processing a PR.
    AgentStarted {
        role: AgentRole,
        pr_title: String,
    },
    /// A streaming chunk of agent thought/output.
    AgentChunk {
        role: AgentRole,
        text: String,
    },
    /// An agent has finished its review for a PR.
    AgentFinished {
        role: AgentRole,
        pr_title: String,
        findings_count: usize,
        duration_ms: u64,
    },
    /// A single PR evaluation is fully complete (all agents + judge).
    PrCompleted {
        pr_title: String,
        total_duration_ms: u64,
    },
    /// The overall run progress has advanced.
    RunProgress {
        completed: usize,
        total: usize,
        cost_summary: CostSummary,
    },
}
```

### 2.2 Agent Pane State

```rust
/// Per-agent dashboard state.
struct AgentPaneState {
    role: AgentRole,
    pr_title: String,
    /// Last N characters of agent thought stream (ring buffer).
    thought_buffer: String,
    /// Whether the agent is currently running.
    status: AgentStatus,
    /// Duration of current/recent run.
    duration_ms: u64,
    /// Findings returned (after finish).
    findings_count: Option<usize>,
}

enum AgentStatus {
    Idle,
    Running { started_at: Instant },
    Finished { duration_ms: u64 },
    Failed { error: String },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AgentRole {
    SA,
    CL,
    AR,
    SEC,
}
```

### 2.3 Dashboard State

```rust
struct DashboardState {
    agents: EnumMap<AgentRole, AgentPaneState>,
    completed: usize,
    total: usize,
    cost_summary: CostSummary,
    start_time: Instant,
}

struct CostSummary {
    total_usd: f64,
    per_role: HashMap<AgentRole, f64>,
    api_calls: usize,
    cache_hits: usize,
}
```

## 3. Layout

```
┌──────────────────────────────────────────────────────────┐
│  crb-harness Live Dashboard                 00:01:23     │
├──────────┬──────────┬──────────┬──────────┤              │
│  SA      │  CL      │  AR      │  SEC     │              │
│  PR: #42 │  PR: #42 │  PR: #42 │  PR: #42 │              │
│  ─────── │  ─────── │  ─────── │  ─────── │              │
│  Analyzi │  The cod │  The arc │  [WARN]  │              │
│  ng the  │  e logic │  hitectur │  Unsafe  │              │
│  diff fo │  in `pro │  e of thi │  `raw    │              │
│  r safet │  cess_or │  s module │  _ptr`   │              │
│  y vulne │  der` us │  follows  │  derefer │              │
│  rabilit │  es an u │  a clean  │  ence in │              │
│  ies...  │  nsafe c │  layered  │  `src/   │              │
│          │  ast.    │  pattern. │  unsafe. │              │
│          │          │           │  rs:120  │              │
│          │          │           │          │              │
│  Status: │  Status: │  Status: │  Status: │              │
│  Running │  Running │  Running │  Running │              │
│  00:00:… │  00:00:… │  00:00:… │  00:00:… │              │
│          │          │           │          │              │
│  Cost:   │  Cost:   │  Cost:   │  Cost:   │              │
│  $0.014  │  $0.012  │  $0.015  │  $0.011  │              │
├──────────┴──────────┴──────────┴──────────┤              │
│  PRs: [████████░░░░░░░░░░░░░░] 3/15       │              │
│                                           │              │
│  Total cost: $0.052  |  API: 12  |       │              │
│  Cache: 8 hits / 4 misses                │              │
└──────────────────────────────────────────────────────────┘
```

### 3.1 Layout Components

| Component | Region | Description |
|-----------|--------|-------------|
| Title bar | Row 0 | App name + elapsed wall time |
| Agent panes | Rows 1-3 | 4 equal columns, each with role header, PR title, thought stream, status, cost |
| Progress bar | Row 4 | `PRs: [████░░░] 3/15` |
| Cost footer | Row 5 | Total cost, API calls, cache hit/miss |

### 3.2 Thought Buffer

Each agent pane shows a scrolling text area of the agent's latest output. The buffer is capped at **2000 characters** (oldest text is dropped when exceeded). This prevents unbounded memory growth for long-running agents.

New chunks are appended with a `▍` (partial block) character to indicate streaming is in progress. When the agent finishes, the final text is displayed in its entirety with a checkmark indicator.

## 4. Wire-in Points

### 4.1 Main Loop (main.rs, lines ~253-305)

The current main loop spawns one task per PR and collects results:

```rust
let mut set = tokio::task::JoinSet::new();
for pr in prs_to_evaluate {
    // ... clone args ...
    set.spawn(async move {
        let _permit = sem.acquire().await.expect("semaphore closed");
        evaluate_pr_with_postprocessing(...).await
    });
}

let mut results = Vec::new();
while let Some(res) = set.join_next().await {
    match res {
        Ok(Ok(result)) => {
            info!("Completed: {}", result.pr_title);
            results.push(result);
        }
        // ...
    }
}
```

**With dashboard:** The dashboard task is spawned before the loop. The loop emits `RunProgress` events as results are collected. The `evaluate_pr_with_postprocessing` function receives a `DashboardChannel` and forwards it to all agent spawns inside.

```rust
if args.dashboard {
    let (tx, rx) = mpsc::channel(1024);
    let d_tx = tx.clone();

    // Spawn dashboard task
    let dashboard_handle = tokio::spawn(async move {
        dashboard::run(rx, num_prs).await;
    });

    for pr in prs_to_evaluate {
        let d_tx = d_tx.clone();
        set.spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore closed");
            evaluate_pr_with_postprocessing(..., d_tx).await
        });
    }

    // Collect results, emit RunProgress
    while let Some(res) = set.join_next().await {
        // ... update dashboard with progress ...
        let _ = d_tx.send(DashboardEvent::RunProgress { ... }).await;
    }

    // Signal dashboard to stop
    dashboard_handle.await?;
}
```

### 4.2 evaluate_pr_single_agent (main.rs, lines ~818-906)

This function spawns 4 agent tasks in a `JoinSet`, one per role. Each agent currently:

1. Computes cache key
2. Checks cache -> hit: return cached, miss: call API
3. Parses findings
4. Records cost

**With dashboard:** Each agent task receives a `DashboardChannel` and emits:

- `AgentStarted` before the API call
- `AgentChunk` periodically during the API call (if we can get streaming chunks)
- `AgentFinished` after findings are parsed

```rust
// Before API call
let _ = d_tx.send(DashboardEvent::AgentStarted {
    role: role.into(),
    pr_title: pr.pr_title.clone(),
}).await;

// During API call (if using streaming)
while let Some(chunk) = stream.next().await {
    let _ = d_tx.send(DashboardEvent::AgentChunk {
        role: role.into(),
        text: chunk,
    }).await;
}

// After completion
let _ = d_tx.send(DashboardEvent::AgentFinished {
    role: role.into(),
    pr_title: pr.pr_title.clone(),
    findings_count: findings.len(),
    duration_ms: start.elapsed().as_millis() as u64,
}).await;
```

### 4.3 evaluate_pr_consensus (consensus path)

The same `DashboardChannel` is threaded through `evaluate_pr_consensus` -> `run_consensus` -> `run_reviewers` -> `build_reviewer_agent` and into each agent's prompt call. Each role emits the same events as in the single-agent path.

### 4.4 Agent Streaming Support

The dashboard's most valuable feature is showing **what the agent is thinking** in real-time. Currently `agent.prompt(&diff).await` returns a complete response. To get streaming chunks, we need to use `agent.stream_prompt(&diff)` instead, which yields `StreamingResponse` chunks.

```rust
// Dashboard-aware agent call (streaming variant)
let mut stream = agent.stream_prompt(&diff).await.map_err(|e| e.to_string())?;
let mut full_response = String::new();

while let Some(chunk) = stream.next().await {
    match chunk {
        StreamingChunk::Text(text) => {
            full_response.push_str(&text);
            let _ = d_tx.send(DashboardEvent::AgentChunk {
                role: role.into(),
                text,
            }).await;
        }
        StreamingChunk::ToolCall(tool) => {
            // Tool calls shown differently
        }
        _ => {}
    }
}
```

If streaming is not available (e.g., the model/provider doesn't support it), the agent sends the full response as a single `AgentChunk` after completion.

## 5. Rendering

### 5.1 Ratatui Setup

```rust
use ratatui::{prelude::*, widgets::*};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

pub async fn run(mut rx: mpsc::Receiver<DashboardEvent>, total_prs: usize) -> Result<()> {
    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let mut state = DashboardState::new(total_prs);

    // Event loop: render at 10fps
    let mut interval = tokio::time::interval(Duration::from_millis(100));
    loop {
        // Drain all pending events (non-blocking)
        while let Ok(event) = rx.try_recv() {
            state.apply(event)?;
        }

        // Render
        terminal.draw(|f| state.render(f))?;

        // Handle input (q to quit gracefully)
        if event::poll(Duration::from_millis(10))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') {
                    break;
                }
            }
        }

        interval.tick().await;
    }

    // Teardown
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}
```

### 5.2 DashboardState::render()

```rust
impl DashboardState {
    fn render(&self, frame: &mut Frame) {
        let area = frame.size();
        let chunks = Layout::vertical([
            Constraint::Length(1),     // Title bar
            Constraint::Min(10),       // Agent panes (4 columns)
            Constraint::Length(3),     // Progress bar
            Constraint::Length(1),     // Cost footer
        ]).split(area);

        let elapsed = format_elapsed(self.start_time.elapsed());
        frame.render_widget(
            Paragraph::new(format!(" crb-harness Live Dashboard  {}", elapsed))
                .style(Style::new().bold().white().on_blue()),
            chunks[0],
        );

        // Agent panes (4 columns)
        let pane_chunks = Layout::horizontal([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ]).split(chunks[1]);

        for (i, role) in [AgentRole::SA, AgentRole::CL, AgentRole::AR, AgentRole::SEC].iter().enumerate() {
            self.render_agent_pane(frame, pane_chunks[i], role);
        }

        // Progress bar
        self.render_progress_bar(frame, chunks[2]);

        // Cost footer
        self.render_cost_footer(frame, chunks[3]);

        // Quit hint
        frame.render_widget(
            Paragraph::new(" [q] quit  |  [p] pause/resume")
                .style(Style::new().dim()),
            chunks[3],
        );
    }

    fn render_agent_pane(&self, frame: &mut Frame, area: Rect, role: &AgentRole) {
        let state = &self.agents[role];
        let border_style = match state.status {
            AgentStatus::Running { .. } => Style::new().fg(Color::Green),
            AgentStatus::Finished { .. } => Style::new().fg(Color::Cyan),
            AgentStatus::Failed { .. } => Style::new().fg(Color::Red),
            AgentStatus::Idle => Style::new().dim(),
        };

        let pane = Block::default()
            .title(format!(" {} ", role.name()))
            .borders(Borders::ALL)
            .border_style(border_style);

        // Inner content: thought buffer + status + cost
        let inner = pane.inner(area);
        let content_chunks = Layout::vertical([
            Constraint::Length(1),   // PR title
            Constraint::Min(1),      // Thought text
            Constraint::Length(2),   // Status row
            Constraint::Length(1),   // Cost row
        ]).split(inner);

        // PR title
        frame.render_widget(
            Paragraph::new(format!(" PR: {}", state.pr_title)).style(Style::new().bold()),
            content_chunks[0],
        );

        // Thought buffer (scrollable text)
        let thought_text = if state.thought_buffer.is_empty() {
            " Waiting for agent to start...".into()
        } else {
            state.thought_buffer.clone()
        };
        frame.render_widget(
            Paragraph::new(thought_text)
                .style(Style::new().fg(Color::White))
                .block(Block::default()),
            content_chunks[1],
        );

        // Status
        let status_text = match state.status {
            AgentStatus::Running { .. } => format!(" Running ({})", format_duration(state.duration_ms)),
            AgentStatus::Finished { d } => format!(" Finished in {}", format_duration(d)),
            AgentStatus::Failed { ref e } => format!(" Failed: {}", e),
            AgentStatus::Idle => " Idle".into(),
        };
        frame.render_widget(
            Paragraph::new(status_text).style(Style::new().dim()),
            content_chunks[2],
        );

        // Cost
        if let Some(cost) = self.cost_summary.per_role.get(role) {
            frame.render_widget(
                Paragraph::new(format!(" Cost: ${:.4}", cost)).style(Style::new().dim()),
                content_chunks[3],
            );
        }
    }

    fn render_progress_bar(&self, frame: &mut Frame, area: Rect) {
        let ratio = if self.total > 0 {
            self.completed as f64 / self.total as f64
        } else {
            0.0
        };

        let bar = Gauge::default()
            .block(Block::default().title(" PRs ").borders(Borders::ALL))
            .gauge_style(Style::new().fg(Color::Cyan).bg(Color::DarkGray))
            .ratio(ratio)
            .label(format!("{}/{}", self.completed, self.total));

        frame.render_widget(bar, area);
    }

    fn render_cost_footer(&self, frame: &mut Frame, area: Rect) {
        frame.render_widget(
            Paragraph::new(format!(
                " Total cost: ${:.4}  |  API calls: {}  |  Cache: {} hits / {} misses",
                self.cost_summary.total_usd,
                self.cost_summary.api_calls,
                self.cost_summary.cache_hits,
                self.cost_summary.api_calls - self.cost_summary.cache_hits,
            )),
            area,
        );
    }
}
```

## 6. CLI Integration

A new `--dashboard` flag is added to `CliArgs` in `config.rs`:

```rust
/// Enable live TUI dashboard showing agent progress and cost.
#[arg(long, default_value_t = false)]
pub dashboard: bool,
```

When `--dashboard` is set:

1. `tracing_subscriber` is configured to output to a file instead of stderr (or suppressed entirely, with a `--dashboard-log` flag for file output).
2. The dashboard channel is created and the dashboard task is spawned.
3. All agent spawns receive the dashboard channel and emit events.
4. On completion, the dashboard task stops and the terminal is restored.

When `--dashboard` is **not** set, behavior is identical to current: tracing to stderr, no dashboard channel, no overhead.

## 7. Cost Tracking Integration

Currently cost tracking is done via `CostTracker` in `main.rs`, which is passed to `evaluate_pr_single_agent` and updated per agent call. With the dashboard:

- The `CostTracker` continues to track costs as before.
- After each `PrCompleted` event, a `CostSummary` snapshot is sent as part of `RunProgress`.
- The dashboard displays the latest cost snapshot.

This avoids duplicating cost-tracking logic. The `CostSummary` is sent to the dashboard channel rather than shared via `Arc<Mutex<>>`.

## 8. Error Handling

| Scenario | Behavior |
|----------|----------|
| Terminal too small (< 80x24) | Dashboard renders with truncation warning; user can resize |
| Dashboard channel full (1024) | `try_send` drops oldest event; rendering falls behind but recovers |
| Agent fails mid-stream | `AgentStatus::Failed` shown in red; remaining agents continue |
| User presses `q` | Dashboard exits early; main loop continues; agents finish in background |
| Dashboard task panics | Error logged; terminal is restored via `Drop` impl |
| `--dashboard` on non-TTY terminal | Falls back gracefully: warn user, run without dashboard |
| Streaming not supported by model | Full response sent as single `AgentChunk` after completion |

## 9. Dependencies

### Added to crb-harness/Cargo.toml

```toml
# TUI dashboard (optional)
ratatui = { version = "0.28", optional = true }
crossterm = { version = "0.28", features = ["event-stream"], optional = true }

[features]
default = []
dashboard = ["ratatui", "crossterm"]
```

Using Cargo features ensures zero overhead when `--dashboard` is not compiled in. The binary size only grows when the `dashboard` feature is enabled.

Alternatively, if compile-time feature gating is too invasive at the wire-in points, dependencies can be unconditional (they are small crates). The `--dashboard` flag at runtime gates the execution path.
