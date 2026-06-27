# Tasks: Replay and Log Viewing

## Phase 1: Openspec Plan [DONE]
- [x] Create proposal.md
- [x] Create design.md
- [x] Create tasks.md
- [x] Create specs/logs/spec.md
- [x] Create specs/replay/spec.md
- [x] Create specs/frontend/spec.md

## Phase 2: Backend Implementation

### 2.1 Log Endpoints
- [ ] Add `LogEntry`/`AgentLog` response types to `src/api/runs.rs`
- [ ] Implement `GET /api/runs/:id/logs` — scan cache dir, return list of available PR+agent log entries
- [ ] Implement `GET /api/runs/:id/logs/:pr_key/:role` — read prompt+response from cache files
- [ ] Support both content-addressed (ca-test) and simple (smoke-test) cache layouts

### 2.2 Replay Endpoint
- [ ] Add `ReplayStatus` response type
- [ ] Add replay state tracking in AppState (ConcurrentHashMap for replay statuses)
- [ ] Implement `POST /api/runs/:id/replay` — spawn harness with `--cache-dir` pointing to original run's cache
- [ ] Implement `GET /api/runs/:id/replay/status` — return progress percentage
- [ ] Write replay output to `output/{run_id}-replay/`

### 2.3 Router Updates
- [ ] Add new routes to `src/server.rs`
- [ ] Update `src/api/mod.rs` with new handler exports

## Phase 3: Frontend Implementation

### 3.1 API Types
- [ ] Add `LogListResponse`, `AgentLogResponse`, `ReplayStatusResponse` types to `lib.rs`
- [ ] Add `LogEntry`, `AgentLog`, `ComparisonResult` types

### 3.2 Logs Tab
- [ ] Add tab bar component (Results | Logs)
- [ ] Create LogViewer component with collapsible PR sections
- [ ] Create AgentLogView component with collapsible prompt/response
- [ ] Wire up fetch calls to new log endpoints
- [ ] Handle empty state (no cache)

### 3.3 Replay Mode
- [ ] Add "Replay Run" button to run detail page
- [ ] Create ReplayOverlay component with progress bar
- [ ] Implement polling of `/api/runs/:id/replay/status`
- [ ] Create ComparisonTable component
- [ ] Handle completion and error states

## Phase 4: Build & Verify
- [ ] `cargo build -p crb-webui` — backend compiles
- [ ] `trunk build --release` — frontend compiles
- [ ] Verify routes registered correctly
- [ ] Run server and test with sample cache data

## Phase 5: Commit
- [ ] `jj new -m "feat: run logs viewing and replay mode"`
- [ ] `jj describe -m "..."` with full commit message
