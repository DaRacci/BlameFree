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
- [x] Add `LogsListResponse`/`PrLogsEntry`/`AgentLogResponse`/`PrAgentsResponse`/`PrAgentEntry` types to `crb-webui-shared/src/runs.rs` (extra `reasoning` field)
- [x] Implement `GET /api/runs/:id/logs` — scan cache dir, return list of available PR+agent log entries
- [x] Implement `GET /api/runs/:id/logs/:pr_key/:role` — read prompt+response+reasoning from cache files
- [x] Implement `GET /api/runs/:id/prs/:pr_key` — PR title + per-agent availability (extra endpoint, not in original spec)
- [x] Support both content-addressed (ca-test) and simple (smoke-test) cache layouts
- [x] Graceful `resolve_cache_dir` — tries 4 fallback paths

### 2.3 Router Updates
- [x] Add log and PR-detail routes to `src/server.rs`

## Phase 3: Frontend Implementation

### 3.1 Shared Types (crb-webui-shared)
- [x] `LogsListResponse` + `PrLogsEntry` types
- [x] `AgentLogResponse` type with `reasoning` field
- [x] `PrAgentsResponse` + `PrAgentEntry` types
- [x] `PrDetailResponse` type

### 3.2 Log Viewing — PrDetailPage (route: `/runs/:id/prs/:pr_key`)
- [x] Create `PrDetailPage` component at `/runs/:id/prs/:pr_key` route
- [x] Fetch PR agent list, then lazy-fetch individual agent logs per role
- [x] Display agent cards in responsive grid with role-colored headers
- [x] Collapsible `<details>` sections for Prompt/Response/Reasoning in `<pre>` blocks
- [x] Agent availability indicators per card header (✓ Prompt, ✓ Response, ✓ Reasoning)
- [x] Empty/loading/error states
- [x] Route registered in `app.rs`

### 3.3 LogViewer Component (Legacy — exists but unused)
- [x] Alternative collapsible PR-section view in `components/log_viewer.rs`
- [x] Lazy-load agent logs on expand with prompt/response/reasoning sections
- [x] Empty states for no cache / no PRs

### 3.4 Run Detail — Logs Link
- [x] Each PR row in `RunDetailPage` has "Logs" link → `/runs/:id/prs/:pr_key` (when `has_agents`)
- [x] Disabled "Logs" button with tooltip when no cache

## Phase 4: Build & Verify
- [x] Backend compiles
- [x] Frontend compiles
- [x] Routes registered correctly
- [x] Run server and test with sample cache data

