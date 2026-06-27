# Tasks: Live TUI Dashboard

## Phase 1: Module Creation

- [ ] **1.1 Create `dashboard_event.rs`** — `crates/crb-harness/src/dashboard_event.rs`
  - Define `DashboardEvent` enum with variants: `AgentStarted`, `AgentChunk`, `AgentFinished`, `PrCompleted`, `RunProgress`
  - Define `DashboardChannel = mpsc::Sender<DashboardEvent>`
  - Define `AgentRole` enum for SA/CL/AR/SEC
  - Define `AgentStatus` enum, `AgentPaneState`, `CostSummary`, `DashboardState` structs
  - **Verify:** Module compiles with `cargo check -p crb-harness`

- [ ] **1.2 Create `dashboard.rs`** — `crates/crb-harness/src/dashboard.rs`
  - Implement `DashboardState::new()`, `DashboardState::apply(event)`, `DashboardState::render(frame)`
  - Implement per-agent pane rendering with thought buffer, status, cost
  - Implement progress bar (Ratatui `Gauge`)
  - Implement cost footer
  - Implement `pub async fn run(rx, total_prs)` — terminal setup, 10fps event loop, input handling
  - **Verify:** Module compiles; can run in isolation with a mock event source

## Phase 2: Dependencies

- [ ] **2.1 Add Ratatui + Crossterm to crb-harness**
  - **File:** `crates/crb-harness/Cargo.toml`
  - **Change:** Add `ratatui = { version = "0.28", optional = true }` and `crossterm = { version = "0.28", features = ["event-stream"], optional = true }`
  - **Alternative (if not using features):** Add as unconditional dependencies
  - **Verify:** `cargo check -p crb-harness` succeeds

- [ ] **2.2 Add dashboard feature flag (optional)**
  - **File:** `crates/crb-harness/Cargo.toml`
  - **Change:** Add `[features]` section with `dashboard = ["ratatui", "crossterm"]`
  - **Verify:** `cargo check -p crb-harness --no-default-features` succeeds; `cargo check -p crb-harness --features dashboard` succeeds

## Phase 3: Wire Into CLI

- [ ] **3.1 Add `--dashboard` CLI flag**
  - **File:** `crates/crb-harness/src/config.rs` (or whichever file defines `CliArgs`)
  - **Change:** Add `#[arg(long, default_value_t = false)] pub dashboard: bool;`
  - **Verify:** `cargo run -- --help` shows `--dashboard` flag

## Phase 4: Wire Into Main Loop

- [ ] **4.1 Create dashboard channel and spawn dashboard task in main()**
  - **File:** `crates/crb-harness/src/main.rs`
  - **Change:** In `main()`, after CLI arg parsing and before the PR evaluation loop:
    - If `--dashboard`, create `mpsc::channel(1024)`, spawn dashboard task via `tokio::spawn(dashboard::run(rx, total_prs))`
    - Pass `tx` (sender) cloned to each `evaluate_pr_with_postprocessing` call
    - After results loop, send a final `RunProgress` (completed=total) and wait for dashboard task
  - **Verify:** Dashboard appears when `--dashboard` is passed, with all UI chrome but no agent events yet

## Phase 5: Wire Into Single-Agent Path

- [ ] **5.1 Add `DashboardChannel` parameter to `evaluate_pr_single_agent`**
  - **File:** `crates/crb-harness/src/main.rs`
  - **Change:** Add `d_tx: Option<mpsc::Sender<DashboardEvent>>` parameter to `evaluate_pr_single_agent()`
  - **Verify:** Compiles with both `Some(tx)` and `None` callers

- [ ] **5.2 Add `DashboardChannel` parameter to `evaluate_pr_with_postprocessing`**
  - **File:** `crates/crb-harness/src/main.rs`
  - **Change:** Add `d_tx: Option<mpsc::Sender<DashboardEvent>>` parameter, pass through to `evaluate_pr_single_agent` and `evaluate_pr_consensus`
  - **Verify:** Compiles

- [ ] **5.3 Emit `AgentStarted` before each agent API call**
  - **File:** `crates/crb-harness/src/main.rs` (inside the agent spawn in `evaluate_pr_single_agent`)
  - **Change:** Before cache check / API call, send `DashboardEvent::AgentStarted { role, pr_title }`
  - **Verify:** Dashboard shows agent panes transitioning from "Idle" to "Running"

- [ ] **5.4 Emit `AgentChunk` during agent API call**
  - **File:** `crates/crb-harness/src/main.rs`
  - **Change:** Use `agent.stream_prompt()` instead of `agent.prompt()` when dashboard is active. For each streaming chunk, send `DashboardEvent::AgentChunk { role, text }`. If streaming is unsupported, send full response as a single chunk after completion.
  - **Verify:** Dashboard agent panes show scrolling text as agents think

- [ ] **5.5 Emit `AgentFinished` after agent completes**
  - **File:** `crates/crb-harness/src/main.rs`
  - **Change:** After parsing findings, send `DashboardEvent::AgentFinished { role, pr_title, findings_count, duration_ms }`
  - **Verify:** Dashboard agents show "Finished" status with findings count

## Phase 6: Wire Into Consensus Path

- [ ] **6.1 Thread `DashboardChannel` through consensus pipeline**
  - **File:** `crates/crb-consensus/src/lib.rs`
  - **Change:** Add `d_tx: Option<mpsc::Sender<DashboardEvent>>` parameter to `evaluate_pr_with_consensus()`, `run_consensus()`, `run_reviewers()`, `build_reviewer_agent()`, and all internal spawns
  - **Verify:** Compiles; dashboard events flow from the consensus path

## Phase 7: Emit Progress and Cost Events

- [ ] **7.1 Emit `PrCompleted` after each PR finishes**
  - **File:** `crates/crb-harness/src/main.rs`
  - **Change:** When a joined result is collected in the main loop, send `DashboardEvent::PrCompleted { pr_title, total_duration_ms }`
  - **Verify:** Dashboard progress bar advances after each completed PR

- [ ] **7.2 Emit `RunProgress` with cost snapshot**
  - **File:** `crates/crb-harness/src/main.rs`
  - **Change:** After collecting each result, build a `CostSummary` from the `CostTracker` and send `DashboardEvent::RunProgress { completed, total, cost_summary }`
  - **Verify:** Dashboard footer shows real-time cost updates

## Phase 8: Verify Fallback

- [ ] **8.1 Verify tracing behavior without `--dashboard`**
  - **Command:** `cargo run -- --pr-filter "rust-lang/rust" --skip-consensus --cache-dir /tmp/test-cache`
  - **Expected:** Exact same tracing output as before — no dashboard channel created, no events emitted, no Ratatui dependency loaded
  - **Verify:** Output matches pre-dashboard trace format

- [ ] **8.2 Verify `--dashboard` on non-TTY**
  - **Command:** `cargo run -- --dashboard --pr-filter "rust-lang/rust" --skip-consensus --cache-dir /tmp/test-cache 2>&1 | cat`
  - **Expected:** Warning about non-TTY; falls back to tracing mode (or exits gracefully)
  - **Verify:** No terminal corruption; output is readable text

## Phase 9: Smoke Tests

- [ ] **9.1 Dashboard with single PR, single-agent mode**
  - **Command:** `cargo run -- --dashboard --pr-filter "rust-lang/rust" --skip-consensus --cache-dir /tmp/test-cache`
  - **Verify:** Dashboard shows 4 agent panes (SA, CL, AR, SEC) updating in real-time for a single PR. Progress bar shows 1/1. Cost footer updates. Press `q` exits cleanly.

- [ ] **9.2 Dashboard with multiple PRs, single-agent mode**
  - **Command:** `cargo run -- --dashboard --pr-filter "serde" --skip-consensus --cache-dir /tmp/test-cache`
  - **Verify:** Progress bar advances through multiple PRs. Each PR shows agent activity. Total cost accumulates.

- [ ] **9.3 Dashboard with consensus mode**
  - **Command:** `cargo run -- --dashboard --pr-filter "rust-lang/rust" --cache-dir /tmp/test-cache`
  - **Verify:** Dashboard works with consensus pipeline; events flow through `crb-consensus`.

- [ ] **9.4 Dashboard with cached PRs (fast completion)**
  - **Command:** Re-run 9.1 (cache now warm)
  - **Verify:** Dashboard shows instant agent completion (cache hits); progress bar moves quickly.

## Phase 10: Polish

- [ ] **10.1 Handle terminal resize events**
  - **File:** `crates/crb-harness/src/dashboard.rs`
  - **Change:** Subscribe to `Event::Resize` from crossterm; re-compute layout on resize
  - **Verify:** Dashboard reflows when terminal is resized

- [ ] **10.2 Add pause/resume (`p` key)**
  - **File:** `crates/crb-harness/src/dashboard.rs`
  - **Change:** Toggle `paused` flag on `p` key press; stop rendering updates while paused (events still accumulate)
  - **Verify:** `p` toggles pause; thought buffer continues to accumulate during pause

- [ ] **10.3 Add elapsed time cost projection**
  - **File:** `crates/crb-harness/src/dashboard.rs`
  - **Change:** Show projected total cost based on current rate: `Projected: $0.21 (at current rate)`
  - **Verify:** Projection appears in cost footer after 2+ PRs complete

- [ ] **10.4 Clean terminal state on panic**
  - **File:** `crates/crb-harness/src/dashboard.rs`
  - **Change:** Use a `Drop` guard or `set_hook` to restore terminal state if the dashboard task panics
  - **Verify:** `panic!()` test restores terminal correctly
