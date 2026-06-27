# Dashboard Events Specification

**Type:** Contract Spec
**Change:** live-dashboard
**Status:** Draft

## 1. Purpose

Define the exact types, contracts, and constraints for the event system that drives the live TUI dashboard. This is the contract between agent code (producers) and the dashboard task (consumer).

## 2. Event Types

All events are sent over a single `mpsc::channel`. The sender type is `mpsc::Sender<DashboardEvent>` and is passed as `Option<mpsc::Sender<DashboardEvent>>` throughout the codebase. When `None`, no events are sent and no overhead is incurred.

### 2.1 DashboardEvent Enum

```rust
/// Events emitted by agent code and consumed by the dashboard task.
#[derive(Debug, Clone)]
pub enum DashboardEvent {
    /// A single agent role has started processing a PR.
    AgentStarted {
        /// Which agent role (SA, CL, AR, SEC).
        role: AgentRole,
        /// Title of the PR being evaluated.
        pr_title: String,
    },

    /// A chunk of streaming text from an agent's thought process.
    AgentChunk {
        /// Which agent role generated this chunk.
        role: AgentRole,
        /// Text chunk from the agent's response stream.
        text: String,
    },

    /// A single agent has finished its review for a PR.
    AgentFinished {
        /// Which agent role completed.
        role: AgentRole,
        /// Title of the PR that was being evaluated.
        pr_title: String,
        /// Number of findings produced (0 if none or parse failure).
        findings_count: usize,
        /// Wall-clock duration of this agent's run in milliseconds.
        duration_ms: u64,
    },

    /// All agents for a single PR have completed (plus judge, if applicable).
    PrCompleted {
        /// Title of the completed PR.
        pr_title: String,
        /// Wall-clock duration of the full PR evaluation in milliseconds.
        total_duration_ms: u64,
    },

    /// Overall run progress update (sent as results are collected in main loop).
    RunProgress {
        /// Number of PRs completed so far.
        completed: usize,
        /// Total number of PRs in the batch.
        total: usize,
        /// Current cost snapshot.
        cost_summary: CostSummary,
    },
}
```

### 2.2 Supporting Types

```rust
/// Agent role identifier — maps 1:1 to the four reviewer agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentRole {
    SA,  // Static Analysis
    CL,  // Code Logic
    AR,  // Architecture
    SEC, // Security
}

impl AgentRole {
    /// Human-readable name.
    pub fn name(&self) -> &'static str {
        match self {
            AgentRole::SA => "SA",
            AgentRole::CL => "CL",
            AgentRole::AR => "AR",
            AgentRole::SEC => "SEC",
        }
    }

    /// Full role description.
    pub fn description(&self) -> &'static str {
        match self {
            AgentRole::SA => "Static Analysis",
            AgentRole::CL => "Code Logic",
            AgentRole::AR => "Architecture",
            AgentRole::SEC => "Security",
        }
    }
}

impl From<&str> for AgentRole {
    fn from(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "SA" | "STATIC_ANALYSIS" => AgentRole::SA,
            "CL" | "CODE_LOGIC" => AgentRole::CL,
            "AR" | "ARCHITECTURE" => AgentRole::AR,
            "SEC" | "SECURITY" => AgentRole::SEC,
            _ => panic!("Unknown agent role: {s}"),
        }
    }
}
```

```rust
/// Snapshot of current cost and API call metrics.
#[derive(Debug, Clone, Default)]
pub struct CostSummary {
    /// Total cost in USD across all agents.
    pub total_usd: f64,
    /// Per-role cost in USD.
    pub per_role: HashMap<AgentRole, f64>,
    /// Total API calls made (including cache misses).
    pub api_calls: usize,
    /// Cache hits (responses served from cache without API call).
    pub cache_hits: usize,
}
```

## 3. Channel Contract

### 3.1 Channel Capacity

```rust
/// Bounded channel capacity. 1024 events is sufficient for:
/// - 4 agents × ~100 chunks each (~400 chunks)
/// - ~4 PRs in flight × 4 agents each
/// - Progress updates every few seconds
const DASHBOARD_CHANNEL_CAPACITY: usize = 1024;
```

### 3.2 Sending Contract

```rust
/// Send an event to the dashboard. Non-blocking — uses try_send.
/// If the channel is full, the event is silently dropped (best-effort).
///
/// This must NEVER block the calling agent task.
fn send_dashboard_event(
    tx: &Option<mpsc::Sender<DashboardEvent>>,
    event: DashboardEvent,
) {
    if let Some(ref tx) = tx {
        let _ = tx.try_send(event); // silently drop on full channel
    }
}
```

### 3.3 Receiving Contract

```rust
/// Receive all pending events from the channel (non-blocking).
/// Returns all events received since the last call.
///
/// The dashboard task calls this once per render frame (every 100ms).
fn drain_events(rx: &mut mpsc::Receiver<DashboardEvent>) -> Vec<DashboardEvent> {
    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    events
}
```

## 4. Wire Points

### 4.1 Producer Sides (where events are sent)

| Location | Function/Scope | Event(s) Emitted | When |
|----------|---------------|------------------|------|
| `main.rs` agent spawn (single-agent path, ~line 852) | Inside `agent_set.spawn(async move { ... })` | `AgentStarted` | Before cache check / API call |
| `main.rs` agent response parsing (~line 886) | After `agent.stream_prompt()` chunk | `AgentChunk` | Per-streaming chunk |
| `main.rs` after agent finishes (~line 899) | After `parse_agent_findings()` | `AgentFinished` | After findings parsed, before returning |
| `main.rs` main loop (~line 290-305) | After `set.join_next()` result collected | `PrCompleted` | Per completed PR |
| `main.rs` main loop (~line 290-305) | After each result pushed to `results` | `RunProgress` | Per completed PR (with cost snapshot) |
| `crb-consensus` agent builder | Inside `run_reviewers()` agent spawns | Same as above | Same triggers |

### 4.2 Consumer Side (where events are consumed)

| Location | Function | Events Processed | Side Effect |
|----------|----------|-----------------|-------------|
| `dashboard.rs` | `DashboardState::apply(event)` | All variants | Updates in-memory state |
| `dashboard.rs` | `run()` event loop | Drained every 100ms | Triggers `terminal.draw()` |

## 5. Wire Pattern (Code Template)

### 5.1 Sending AgentStarted + AgentChunk + AgentFinished

```rust
// Inside evaluate_pr_single_agent(), inside the agent spawn:
use dashboard_event::{send_dashboard_event, DashboardEvent, AgentRole};

let d_tx: Option<mpsc::Sender<DashboardEvent>> = /* from function parameter */;

// Emit: agent started
send_dashboard_event(&d_tx, DashboardEvent::AgentStarted {
    role: AgentRole::from(role.as_str()),
    pr_title: pr.pr_title.clone(),
});

// Make streaming API call
let mut full_response = String::new();
let mut stream = agent.stream_prompt(&diff).await.map_err(|e| e.to_string())?;

while let Some(chunk) = stream.next().await {
    if let StreamingChunk::Text(text) = chunk {
        full_response.push_str(&text);

        // Emit: streaming chunk
        send_dashboard_event(&d_tx, DashboardEvent::AgentChunk {
            role: AgentRole::from(role.as_str()),
            text,
        });
    }
}

// Parse findings
let findings = parse_agent_findings(&full_response);
let duration = start.elapsed().as_millis() as u64;

// Emit: agent finished
send_dashboard_event(&d_tx, DashboardEvent::AgentFinished {
    role: AgentRole::from(role.as_str()),
    pr_title: pr.pr_title.clone(),
    findings_count: findings.len(),
    duration_ms: duration,
});
```

### 5.2 Sending PrCompleted + RunProgress

```rust
// Inside main(), after while let Some(res) = set.join_next().await:
if let Ok(Ok(result)) = res {
    // Emit: PR completed
    send_dashboard_event(&d_tx, DashboardEvent::PrCompleted {
        pr_title: result.pr_title.clone(),
        total_duration_ms: /* computed from result */ 0, // TODO
    });

    results.push(result);

    // Emit: run progress with cost snapshot
    let cost_summary = CostSummary {
        total_usd: cost_tracker.total_usd(),
        per_role: cost_tracker.per_role_breakdown(), // TODO: add this method
        api_calls: cost_tracker.api_calls(),
        cache_hits: cost_tracker.cache_hits(),
    };
    send_dashboard_event(&d_tx, DashboardEvent::RunProgress {
        completed: results.len(),
        total: prs_to_evaluate.len(),
        cost_summary,
    });
}
```

## 6. Error Handling

| Scenario | Producer Behavior | Consumer Behavior |
|----------|------------------|-------------------|
| Channel full | `try_send` silently drops event | Missed events; next frame may show stale state |
| Channel closed (dashboard crashed) | `try_send` returns `Err` silently | N/A — no consumer |
| `d_tx` is `None` | `send_dashboard_event` is no-op | N/A |
| Duplicate `AgentStarted` (same role, new PR) | Agent emits new start | Dashboard resets that pane's buffer |
| `AgentChunk` after `AgentFinished` | Agent shouldn't emit this; ignored by consumer | Ignored by state machine |
| Missing `AgentStarted` before `AgentChunk` | Bug in producer code | Chunk silently ignored; agent stays idle |
