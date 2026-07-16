# Delta for SSE Live Events

> **Implementation:**
> - SSE handler: `crates/crb-webui-backend/src/api/live.rs`
> - Event types: `crb_types::RunEvent` enum (serialized to JSON with tag/content format)
> - SSE client: `crates/crb-webui-frontend/src/sse.rs`
> - In-process events sent via `broadcast::Sender<RunEvent>` from `crates/crb-webui-backend/src/harness.rs`

**Important architecture note:** Events are not emitted by a subprocess. Instead, `crb-webui-backend/src/harness.rs` calls `crb_harness::pipeline::evaluate()` directly as a library call. Events are sent through a `broadcast::Sender<RunEvent>` stored in `ActiveRun.tx`.

## Transport

- Standard SSE with `text/event-stream` content type
- Each event is a JSON object prefixed with `data: ` and suffixed with `\n\n`
- No `event:` field — client uses the `event` JSON tag field to discriminate
- Keep-alive pings every 15 seconds via `axum::response::sse::KeepAlive`
- Events use `#[serde(tag = "event", content = "data")]` — each SSE data line contains `{"event": "...", "data": {...}}`

## ADDED Requirements

### Requirement: Agent Started Event

**Description:** An agent_started event SHALL be emitted when an agent begins its review for a specific PR. The frontend uses this to create a new agent pane in the `PrState` map and stamp the role's status to "reviewing".

**Event:**
```json
{
  "event": "agent_started",
  "data": {
    "identifier": "discourse-graphite/pull/7",
    "agent": "SA"
  }
}
```

#### Scenario: Frontend creates agent pane on agent_started

Given the SSE connection is established for an active run
When an `agent_started` event is received with identifier "discourse-graphite/pull/7" and agent "SA"
Then the frontend ensures a `PrState` exists for that identifier
And the `PerAgentState` for role "SA" has its status set to "reviewing"
And if this is the first PR, it is auto-selected in the PR tab bar

#### Scenario: Missing PR state is created on agent_started

Given no `PrState` exists yet for the event's identifier
When an `agent_started` event is received
Then a new `PrState` is created with `PerAgentState` entries for all known roles
And the specific agent's status is set to "reviewing"

---

### Requirement: Agent Chunk Event

**Description:** An agent_chunk event SHALL be emitted to stream incremental response text from an agent as it reviews. The frontend appends the chunk to the agent's accumulated response display.

**Event:**
```json
{
  "event": "agent_chunk",
  "data": {
    "identifier": "SA",
    "chunk": "Analyzing the PR diff... Found potential issue with color function..."
  }
}
```

**Notes:**
- The `identifier` field contains the role abbreviation (e.g., "SA"), not the PR key
- The frontend tracks which PR each role is working on via `role_current_pr` map

#### Scenario: Frontend appends chunk to agent's response

Given a `PerAgentState` for role "SA" exists and is mapped to a PR via `role_current_pr`
When an `agent_chunk` event is received with identifier "SA" and a chunk of text
Then the chunk is appended to the agent's `response` string
And the live view displays the updated response text

---

### Requirement: Agent Finished Event

**Description:** An agent_finished event SHALL be emitted when an agent completes its review, indicating whether it succeeded and how many findings were produced.

**Event:**
```json
{
  "event": "agent_finished",
  "data": {
    "identifier": "SA",
    "findings": 3,
    "success": true
  }
}
```

**Notes:**
- The `identifier` field contains the role abbreviation
- On the frontend, triggers a check if all agents for the current PR are done

#### Scenario: Frontend marks agent as done

Given a `PerAgentState` for role "SA" is active
When an `agent_finished` event is received with `success: true` and `findings: 3`
Then the agent's status is set to "done"
And the findings count is stored

#### Scenario: Frontend marks agent as failed

When an `agent_finished` event is received with `success: false`
Then the agent's status is set to "failed"

#### Scenario: All agents done marks PR completed

Given all agents for a PR have received `agent_finished` events
When the last agent finishes
Then the PR's `completed` flag is set to `true`

---

### Requirement: Review Started Event

**Description:** A review_started event SHALL be emitted when a review begins for a PR with a known number of participating agents. The frontend does not currently use this for dashboard display but logs it as a progress signal.

**Event:**
```json
{
  "event": "review_started",
  "data": {
    "identifier": "discourse-graphite/pull/7",
    "total_agents": 4
  }
}
```

#### Scenario: Frontend receives review_started without UI change

Given an active SSE connection
When a `review_started` event is received
Then the frontend does not modify any agent pane states
And the event is acknowledged without errors

---

### Requirement: Review Completed Event

**Description:** A review_completed event SHALL be emitted when a single PR has been fully evaluated. The frontend marks the PR as completed, regardless of individual agent states.

**Event:**
```json
{
  "event": "review_completed",
  "data": {
    "identifier": "discourse-graphite/pull/7",
    "metrics": { "true_positives": 3, "false_positives": 6, "false_negatives": 0, "duration_secs": 120.0 },
    "cost": 0.0032,
    "total_tokens": 3500,
    "agent_calls": 4,
    "findings_count": 0
  }
}
```

#### Scenario: Frontend marks PR as completed

Given a PR is being tracked in the `PrState` map
When a `review_completed` event is received for that PR's identifier
Then the PR's `completed` flag is set to `true`

---

### Requirement: Run Progress Event

**Description:** A run_progress event SHALL be emitted periodically during a run to update the client on overall progress, elapsed time, total cost, and the current PR being processed.

**Event:**
```json
{
  "event": "run_progress",
  "data": {
    "completed_prs": 5,
    "total_prs": 10,
    "elapsed_secs": 185.3,
    "total_cost": 0.047,
    "current_pr": "discourse-graphite/pull/7"
  }
}
```

#### Scenario: Frontend updates progress bar and metrics

Given an active run with SSE connection
When a `run_progress` event is received with `completed_prs: 5` and `total_prs: 10`
Then the frontend updates the `ProgressBar` to show 5/10 PRs (50%)
And the status and active PRs metrics are updated

#### Scenario: New PR from run_progress creates PrState

Given a `run_progress` event contains a `current_pr` not yet tracked
When the event is received
Then a new `PrState` is created for that PR key
And it is added to the PR order list
And auto-selected if no PR is currently selected

---

### Requirement: Run Finished Event

**Description:** A run_finished event SHALL be emitted when the entire benchmark run completes. Contains aggregated metrics across all PRs.

**Event:**
```json
{
  "event": "run_finished",
  "data": {
    "total_prs": 10,
    "aggregated": {
      "true_positives": 30,
      "false_positives": 60,
      "false_negatives": 5,
      "duration_secs": 1200.0
    },
    "total_cost": 0.12,
    "total_tokens": 52000,
    "total_agent_calls": 40
  }
}
```

#### Scenario: Frontend marks run as complete

When a `run_finished` event is received
Then the frontend sets the status to "complete"
And the connection is no longer expected to produce further events

#### Scenario: Broadcast channel overflow drops lagged events

Given an SSE client that lags behind the producer
When the broadcast channel overflows
Then lagged events are silently dropped by the `BroadcastStream` wrapper
And the client continues receiving subsequent events
