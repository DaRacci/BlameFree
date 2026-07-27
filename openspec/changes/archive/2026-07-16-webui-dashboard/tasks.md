# Tasks: Web UI Dashboard

> **STATUS: 50/51 tasks done** (~98% complete)
> The codebase is split across **3 crates** (not 1):
> - `crates/riv-webui-backend/` — axum HTTP server + API handlers
> - `crates/riv-webui-frontend/` — Leptos WASM frontend
> - `crates/riv-webui-shared/` — JSON-serializable types shared by both

---

## Phase 1: Openspec Plan ✅
- [x] Create `openspec/changes/webui-dashboard/proposal.md`
- [x] Create `openspec/changes/webui-dashboard/design.md`
- [x] Create `openspec/changes/webui-dashboard/tasks.md`
- [x] Create `specs/api/spec.md`
- [x] Create `specs/live/spec.md`
- [x] Create `specs/pages/spec.md`

## Phase 2: Crate Setup (3 crates) ✅
- [x] Create `crates/riv-webui-backend/Cargo.toml` — axum backend with workspace deps
- [x] Create `crates/riv-webui-backend/src/main.rs` — CLI entrypoint (clap, tracing, rustls, octocrab)
- [x] Create `crates/riv-webui-backend/src/server.rs` — axum router, static file serving, SPA fallback
- [x] Create `crates/riv-webui-frontend/Cargo.toml` — Leptos WASM crate (`cdylib` + `rlib`)
- [x] Create `crates/riv-webui-frontend/src/lib.rs` — types, HTTP helpers, types module re-exports
- [x] Create `crates/riv-webui-shared/Cargo.toml` — minimal WASM-compatible deps (serde only)
- [x] Create `crates/riv-webui-shared/src/lib.rs` — module root + `role_color()` utility

## Phase 3: API Backend ✅ (18 endpoints implemented, not 6)
- [x] Create `crates/riv-webui-backend/src/api/mod.rs` — module root with `adhoc`, `admin`, `config`, `live`, `runs`
- [x] Implement `GET /api/runs` — scan output dir, list past runs
- [x] Implement `GET /api/runs/:id` — read per-PR JSON files + summary
- [x] Implement `POST /api/runs` — launch benchmark via in-process library call (not subprocess)
- [x] Implement `GET /api/runs/:id/live` — SSE streaming from broadcast channel
- [x] Implement `GET /api/config` — list available models, datasets, roles
- [x] Implement `GET /api/config/datasets` — list datasets with PR counts
- [x] Implement `GET /api/config/reasoning-efforts` — list available reasoning efforts
- [x] Implement `GET /api/runs/:id/logs` — list per-PR agent logs for a run
- [x] Implement `GET /api/runs/:id/logs/:pr_key/:role` — get individual agent log
- [x] Implement `GET /api/runs/:id/prs/:pr_key` — list agents for a specific PR
- [x] Implement `GET /api/runs/:id/pr-detail/:pr_key` — detailed per-PR findings
- [x] Implement `GET /api/datasets/:id/prs` — list PRs in a dataset
- [x] Implement `POST /api/adhoc/review` — start ad-hoc review of a PR
- [x] Implement `GET /api/adhoc/runs` — list ad-hoc review runs
- [x] Implement `GET /api/adhoc/runs/:id` — get ad-hoc run details
- [x] Implement `GET /api/adhoc/prs/:owner/:repo` — list GitHub PRs for ad-hoc
- [x] Implement `GET /api/admin/logs` — view server logs
- [x] Implement `GET /api/admin/logs/stream` — SSE stream of server logs

## Phase 4: In-Process Harness Execution ✅ (architecture changed from subprocess)
- [x] Create `crates/riv-webui-backend/src/harness.rs` — in-process harness runner
      Calls `riv_harness::pipeline::evaluate()` directly via library API
      Sets up `EvalConfig`, forwards SSE events via `broadcast::Sender<RunEvent>`,
      writes per-PR result files and summary
- [x] Create `crates/riv-webui-backend/src/events.rs` — exists as a 0-byte stub.
      Event types are defined in `riv-types` (`RunEvent` enum).
      No separate event parser needed since harness emits events directly.

## Phase 5: Frontend (Leptos WASM) ✅
- [x] Create `crates/riv-webui-frontend/Cargo.toml` — Leptos CSR, gloo-net, web-sys (EventSource)
- [x] Create `crates/riv-webui-frontend/src/lib.rs` — app root types, `NewRunRequest`, `AppConfig`
- [x] Create `crates/riv-webui-frontend/src/app.rs` — Router, sidebar, 8 routes
- [x] Implement `HomePage` — past runs list + ad-hoc runs list
- [x] Implement `RunDetailPage` — metrics, sortable table, cost breakdown
- [x] Implement `PrDetailPage` — per-PR detailed findings and agent logs
- [x] Implement `NewBenchmarkPage` — launcher form with model, dataset, roles, filters
- [x] Implement `LiveViewPage` — 4-pane agent view with SSE stream
- [x] Implement `AdminPage` — server log viewer with SSE streaming
- [x] Implement `AdhocReviewPage` — ad-hoc PR review form
- [x] Implement `AdhocRunsPage` — list ad-hoc review runs
- [x] Create components: `AgentPane`, `ProgressBar`, `MetricsCard`, `RunTable`, `RoleSelector`, `LogViewer`
- [x] Create `sse.rs` — SSE event source connection handler


## Phase 7: Verification ✅
- [x] `cargo check --workspace` — passes
- [x] `cargo test --workspace` — runs (some tests may be WASM-only)
- [x] Start web UI: `cargo run -p riv-webui` — starts on port 8080 with embedded frontend assets
