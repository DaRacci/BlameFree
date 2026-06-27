# Tasks: Web UI Dashboard

## Phase 1: Openspec Plan ✅
- [x] Create `openspec/changes/webui-dashboard/proposal.md`
- [x] Create `openspec/changes/webui-dashboard/design.md`
- [x] Create `openspec/changes/webui-dashboard/tasks.md`
- [ ] Create `specs/api/spec.md`
- [ ] Create `specs/live/spec.md`
- [ ] Create `specs/pages/spec.md`

## Phase 2: Crate Setup
- [ ] Create `crates/crb-webui/Cargo.toml` with workspace dependencies
- [ ] Create `crates/crb-webui/src/main.rs` CLI entrypoint
- [ ] Create `crates/crb-webui/src/server.rs` axum setup

## Phase 3: API Backend
- [ ] Create `crates/crb-webui/src/api/mod.rs`
- [ ] Implement `GET /api/runs` — scan output dir, list runs
- [ ] Implement `GET /api/runs/:id` — read per-PR JSON files
- [ ] Implement `POST /api/runs` — spawn harness subprocess
- [ ] Implement `GET /api/runs/:id/live` — SSE streaming
- [ ] Implement `GET /api/config` — list available configs
- [ ] Implement `GET /api/config/datasets` — list datasets

## Phase 4: Subprocess & Events
- [ ] Create `crates/crb-webui/src/harness.rs` — subprocess manager
- [ ] Create `crates/crb-webui/src/events.rs` — parse JSON events

## Phase 5: Frontend (Leptos WASM)
- [ ] Create `crates/crb-webui/frontend/Cargo.toml`
- [ ] Create `crates/crb-webui/frontend/src/lib.rs` — app root
- [ ] Create `crates/crb-webui/frontend/src/pages/mod.rs`
- [ ] Implement `HomePage` (past runs list)
- [ ] Implement `RunDetailPage` (metrics, table, cost)
- [ ] Implement `NewBenchmarkPage` (launcher form)
- [ ] Implement `LiveViewPage` (4-pane agent view + SSE)
- [ ] Create `crates/crb-webui/frontend/src/components/`
- [ ] AgentPane component
- [ ] ProgressBar component
- [ ] MetricsCard component
- [ ] RunTable component

## Phase 6: Harness Integration
- [ ] Add `--dashboard-events` flag to crb-harness CLI
- [ ] When `--dashboard-events`, emit JSON events to stdout

## Phase 7: Verification
- [ ] `cargo check --workspace`
- [ ] `cargo test --workspace`
- [ ] Start web UI: `cargo run -p crb-webui -- --port 8080`
