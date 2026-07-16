# Design: Web UI Dashboard

## Architecture (3-Crate Split)

The codebase is split into **3 crates** under `crates/`:

| Crate | Path | Purpose |
|-------|------|---------|
| **crb-webui-backend** | `crates/crb-webui-backend/` | axum HTTP server, API handlers, in-process harness execution, auth |
| **crb-webui-frontend** | `crates/crb-webui-frontend/` | Leptos CSR WASM frontend, pages, components, SSE client |
| **crb-webui-shared** | `crates/crb-webui-shared/` | JSON-serializable types (`Serialize`+`Deserialize`) shared by backend and frontend |

```
┌─────────────────────────────────────────────────────────────┐
│                    Web Browser                               │
│  ┌─────────────────────────────────────────────────────────┐│
│  │           crb-webui-frontend (Leptos WASM)               ││
│  │  /         -> HomePage (past runs + ad-hoc runs lists)   ││
│  │  /runs/:id -> RunDetailPage (metrics, table, cost)       ││
│  │  /runs/:id/live -> LivePage (4-pane agent SSE monitor)   ││
│  │  /runs/:id/prs/:pr_key -> PrDetailPage (per-PR findings) ││
│  │  /new      -> NewRunPage (benchmark launcher form)       ││
│  │  /adhoc    -> AdhocRunsPage (ad-hoc review runs)         ││
│  │  /adhoc/new -> AdhocReviewPage (ad-hoc PR form)          ││
│  │  /admin    -> AdminPage (log viewer with SSE)           ││
│  └───────────────────────┬─────────────────────────────────┘│
└──────────────────────────┼──────────────────────────────────┘
                           │ HTTP / SSE / WASM
┌──────────────────────────┼──────────────────────────────────┐
│               crb-webui-backend (axum, port 8080)            │
│  ┌─────────────────────────────────────────────────────────┐│
│  │  API Endpoints (18 total)                               ││
│  │  GET    /api/runs                       -> list runs    ││
│  │  POST   /api/runs                       -> start run    ││
│  │  GET    /api/runs/:id                   -> run detail   ││
│  │  GET    /api/runs/:id/live              -> SSE stream   ││
│  │  GET    /api/runs/:id/logs              -> list logs    ││
│  │  GET    /api/runs/:id/logs/:pr/:role    -> agent log    ││
│  │  GET    /api/runs/:id/prs/:pr_key       -> PR agents    ││
│  │  GET    /api/runs/:id/pr-detail/:pr_key -> PR detail   ││
│  │  GET    /api/config                     -> config       ││
│  │  GET    /api/config/datasets            -> datasets     ││
│  │  GET    /api/config/reasoning-efforts   -> reasoning    ││
│  │  GET    /api/datasets/:id/prs           -> dataset PRs  ││
│  │  POST   /api/adhoc/review               -> ad-hoc rev   ││
│  │  GET    /api/adhoc/runs                 -> ad-hoc runs  ││
│  │  GET    /api/adhoc/runs/:id             -> ad-hoc run   ││
│  │  GET    /api/adhoc/prs/:owner/:repo     -> GitHub PRs   ││
│  │  GET    /api/admin/logs                 -> server logs  ││
│  │  GET    /api/admin/logs/stream          -> logs SSE     ││
│  └─────────────────────────────────────────────────────────┘│
│                                                              │
│  ┌─────────────────────────────────────────────────────────┐│
│  │  In-Process Harness Runner (crates/crb-webui-backend/   ││
│  │  src/harness.rs)                                        ││
│  │                                                         ││
│  │  Calls crb_harness::pipeline::evaluate() directly       ││
│  │  as a library function (NOT a subprocess).              ││
│  │  Sets up EvalConfig with dashboard_tx broadcast sender, ││
│  │  runs each PR through the pipeline, writes result files,││
│  │  and sends RunEvents via tokio broadcast channel.       ││
│  └─────────────────────────────────────────────────────────┘│
│                                                              │
│  ┌─────────────────────────────────────────────────────────┐│
│  │  Shared Types (crb-webui-shared/)                       ││
│  │  - runs.rs: RunSummary, RunDetail, PrResult, VerdictJson││
│  │  - config.rs: RoleInfo, DatasetInfo, ReasoningEfforts   ││
│  │  - adhoc.rs: AdhocRunSummary, GithubPrListItem          ││
│  │  - admin.rs: LogsResponse                               ││
│  │  - auth.rs: AuthUser                                    ││
│  └─────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

## Backend Design

### State

```rust
struct AppState {
    output_dir: PathBuf,             // Where per-PR JSON result files live
    dataset_dir: PathBuf,            // Where datasets are stored
    static_dir: Option<PathBuf>,     // Optional disk-based static frontend dir
    models: String,                  // Comma-separated available models
    benchmark_dir: Option<PathBuf>,  // Path to benchmark (offline/ diffs)
    active_runs: Arc<RwLock<HashMap<String, ActiveRun>>>,  // Running benchmarks
    config: WebUiConfig,             // Web UI config (includes optional OAuth)
    session_store: SessionStore,     // OAuth session store
    octocrab: octocrab::Octocrab,    // GitHub API client
    log_file: PathBuf,               // Server log file path
}

struct ActiveRun {
    created_at: UnixTime,
    config: BenchmarkConfig,
    tx: broadcast::Sender<RunEvent>,  // SSE event broadcast
    completed_prs: usize,
    total_prs: usize,
    finished: bool,
}
```

### API Endpoints (18 total)

| Method | Path | Description |
|--------|------|-------------|
| GET | /api/runs | List past benchmark runs |
| POST | /api/runs | Start new benchmark (in-process) |
| GET | /api/runs/:id | Run detail with per-PR results |
| GET | /api/runs/:id/live | SSE stream of live events |
| GET | /api/runs/:id/logs | List per-PR agent logs |
| GET | /api/runs/:id/logs/:pr_key/:role | Get individual agent log |
| GET | /api/runs/:id/prs/:pr_key | List agents for a specific PR |
| GET | /api/runs/:id/pr-detail/:pr_key | Detailed per-PR findings |
| GET | /api/config | Available models, datasets, roles |
| GET | /api/config/datasets | List available datasets |
| GET | /api/config/reasoning-efforts | List reasoning effort options |
| GET | /api/datasets/:id/prs | List PRs within a dataset |
| POST | /api/adhoc/review | Start ad-hoc PR review |
| GET | /api/adhoc/runs | List ad-hoc review runs |
| GET | /api/adhoc/runs/:id | Get ad-hoc run details |
| GET | /api/adhoc/prs/:owner/:repo | List GitHub PRs for ad-hoc |
| GET | /api/admin/logs | View server logs |
| GET | /api/admin/logs/stream | SSE stream of server logs |

### SSE Streaming

- Uses `axum::response::Sse` with `tokio_stream::wrappers::BroadcastStream`
- Event payloads are `data: <json>\n\n`
- Multiple clients can watch the same run simultaneously
- Events use `crb_types::RunEvent` enum (serialized as JSON):
  - `AgentStarted` — `{ pr_key, role }`
  - `AgentChunk` — `{ role, chunk }`
  - `AgentFinished` — `{ role, findings, success }`
  - `PrCompleted` — `{ pr_key, metrics, cost, ... }`
  - `RunProgress` — `{ completed_prs, total_prs, elapsed_secs, running_cost }`
  - `RunFinished` — `{ total_prs, aggregated, total_cost }`

### In-Process Harness Execution

- Backend calls `crb_harness::pipeline::evaluate()` directly as a Rust library call
- NOT a subprocess — no `--dashboard-events` flag needed
- `EvalConfig.dashboard_tx: Option<broadcast::Sender<RunEvent>>` routes events directly
- On `POST /api/runs`, returns the run ID immediately via `201 Created`
- A background `tokio::spawn` task runs the harness
- Client opens SSE connection to `/api/runs/:id/live`
- If client disconnects, the run continues to completion
- Per-PR result files and a summary JSON are written to `output/<run_id>/`

## Frontend Design

### Pages

1. **Home (`/`)** — List of past benchmark runs + ad-hoc review runs with mini metrics summary. "New Benchmark" and "Ad-hoc Review" buttons. Auto-refresh for active runs.
2. **Run Detail (`/runs/:id`)** — Aggregate metrics (F1, precision, recall), per-PR sortable table, cost breakdown. Also handles ad-hoc run detail at `/adhoc/runs/:id`.
3. **PR Detail (`/runs/:id/prs/:pr_key`)** — Per-PR findings, verdicts, and agent log tabs.
4. **New Benchmark (`/new`)** — Form with model selector, dataset selector, concurrency slider, pr filter, role selector with incompatibility hints, reasoning effort.
5. **Live View (`/runs/:id/live`)** — 4-column agent pane layout showing streaming LLM responses, progress bar, elapsed time, running cost. SSE-driven.
6. **Ad-hoc Review (`/adhoc/new`)** — Form to enter owner/repo/PR number, select model and roles.
7. **Ad-hoc Runs (`/adhoc`)** — List of ad-hoc review runs.
8. **Admin (`/admin`)** — Server log viewer with SSE streaming (live tail).

### Component Tree

```
App
├── Sidebar (collapsible nav, auth-aware)
└── Router
    ├── HomePage
    │   └── MetricsCard (×4 — runs, PRs, badge trends)
    ├── RunDetailPage
    │   ├── MetricsCard (×4 — F1, precision, recall, cost)
    │   └── RunTable (sortable PR results)
    ├── PrDetailPage
    │   └── RunTable / findings list
    ├── NewBenchmarkPage
    │   ├── RoleSelector (checkbox grid with incompatibility)
    │   └── form controls
    ├── LivePage
    │   ├── AgentPane (×N — one per role)
    │   ├── ProgressBar
    │   └── MetricsCard / elapsed timer / cost display
    ├── AdhocReviewPage
    │   └── form controls (owner, repo, pr, model, roles)
    ├── AdhocRunsPage
    │   └── RunTable
    └── AdminPage
        └── LogViewer (SSE-powered live log tail)
```

### Components

- **AgentPane** — `role: String, state: AgentPaneState` — Title bar with status, response lines, border color
- **ProgressBar** — `completed: usize, total: usize` — CSS progress bar with label "X/Y PRs"
- **MetricsCard** — `title: String, value: f64, subtitle: Option<String>` — Card with big number, label
- **RunTable** — `results: Vec<PrResult>` — Sortable table: title, F1, precision, recall, findings, cost
- **RoleSelector** — Checkbox grid with incompatibility warnings
- **LogViewer** — `lines: Vec<String>` — Scrollable log with auto-scroll and SSE live updates

### State Management

```rust
// Frontend signals (Leptos reactive)
struct AppConfig {
    models: Vec<String>,
    datasets: Vec<String>,
    roles: Vec<RoleInfo>,
    auth_enabled: bool,
}

struct NewRunRequest {
    model: String,
    dataset: String,
    roles: Vec<String>,
    pr_filter: Option<String>,
    use_cache: bool,
    reasoning_effort: Option<String>,
}

// Live page signals
struct LivePageState {
    run_id: String,
    event_source: Option<EventSource>,
    agent_panes: RwSignal<Vec<AgentPaneState>>,
    progress: RwSignal<(usize, usize)>,
    elapsed: RwSignal<f64>,
    cost: RwSignal<f64>,
    current_pr: RwSignal<Option<String>>,
}
```

### Communication Flow (Live View)

```
crb_harness::pipeline::evaluate (in-process library call)
  └── Calls dashboard_tx.send(RunEvent::AgentChunk { role: "SA", chunk: "..." })
      ───────────────────────────────────────────┐
                                                 ▼
In-process broadcast::Sender<RunEvent> in server::ActiveRun
  └── forwards to all subscribers
      ───────────────────────────────────────────┐
                                                 ▼
SSE handler (api/live.rs) reads from BroadcastStream
  └── Writes SSE `data: <json>\n\n` to HTTP response
      ───────────────────────────────────────────┐
                                                 ▼
Leptos WASM frontend receives EventSource (sse.rs)
  └── Parses JSON events, updates reactive signals
  └── UI re-renders automatically via Leptos reactivity
```

### OAuth Authentication

- Optional GitHub/GitLab/Google OAuth 2.0 login
- Routes: `/auth/login`, `/auth/callback`, `/auth/logout`, `/auth/me`
- Session cookies for persistent auth state
- Graceful fallback: if no OAuth configured, API works unauthenticated
