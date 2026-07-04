# Design: Web UI Dashboard

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                     Web Browser                          │
│  ┌─────────────────────────────────────────────────────┐│
│  │          Leptos WASM Frontend                        ││
│  │  / -> Home (past runs list + sparklines)             ││
│  │  /runs/:id -> Run detail (metrics, table, cost)      ││
│  │  /new -> Benchmark launcher form                     ││
│  │  /live/:id -> Live agent monitoring (SSE)            ││
│  └───────────────┬─────────────────────────────────────┘│
└──────────────────┼──────────────────────────────────────┘
                   │ HTTP / SSE
┌──────────────────┼──────────────────────────────────────┐
│                  ▼                                      │
│  axum HTTP Server (port 8080)                           │
│  ┌────────────────────────────────────────────────────┐ │
│  │  /api/runs          -> list past runs               │ │
│  │  /api/runs/:id      -> run detail + per-PR results  │ │
│  │  POST /api/runs     -> start new benchmark          │ │
│  │  /api/runs/:id/live -> SSE stream                   │ │
│  │  /api/config        -> models, prompts, datasets    │ │
│  │  /api/config/datasets -> list datasets              │ │
│  │  /                  -> serve static WASM bundle     │ │
│  └────────────────────────┬───────────────────────────┘ │
│                           │                              │
│  ┌────────────────────────▼───────────────────────────┐ │
│  │  Subprocess Manager                                │ │
│  │  spawns crb-harness --dashboard-events             │ │
│  │  reads stdout -> JSON events -> broadcast to SSE     │ │
│  └────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

## Backend Design

### State

```rust
struct AppState {
    // Shared state
    output_dir: PathBuf,        // Where per-PR JSON results live
    harness_path: PathBuf,      // Path to crb-harness binary
    active_runs: RwLock<HashMap<String, RunState>>,
}

struct RunState {
    id: String,
    created_at: Instant,
    config: BenchmarkConfig,
    process: Option<Child>,
    tx: broadcast::Sender<DashboardEvent>,
    completed_prs: usize,
    total_prs: usize,
    finished: bool,
}
```

### API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | /api/runs | List past benchmark runs |
| GET | /api/runs/:id | Run detail with per-PR results |
| POST | /api/runs | Start new benchmark |
| GET | /api/runs/:id/live | SSE stream of live events |
| GET | /api/config | Available models, prompts, datasets |
| GET | /api/config/datasets | List available datasets |

### SSE Streaming

- Uses `axum::response::Sse` with `tokio_stream::wrappers::BroadcastStream`
- Event payloads are `data: <json>\n\n`
- Multiple clients can watch the same run simultaneously
- Events:
  - `agent_started` — `{ pr_key, role }`
  - `agent_chunk` — `{ role, chunk }`
  - `agent_finished` — `{ role, findings, success }`
  - `pr_completed` — `{ pr_key, metrics, cost, ... }`
  - `run_progress` — `{ completed_prs, total_prs, elapsed_secs, running_cost }`
  - `run_finished` — `{ total_prs, aggregated, total_cost }`

### Subprocess Management

- Backend spawns `crb-harness --dashboard-events [other-args]`
- Reads stdout line-by-line, each line is a JSON event
- On `POST /api/runs`, returns the run ID immediately
- Client opens SSE connection to `/api/runs/:id/live`
- If client disconnects, subprocess continues (runs to completion)
- Graceful shutdown: send SIGTERM to running processes on server exit

## Frontend Design

### Pages

1. **Home (`/`)** — List of past runs with mini metrics summary. "New Benchmark" button.
2. **Run Detail (`/runs/:id`)** — Aggregate metrics (F1, precision, recall), per-PR sortable table, cost breakdown.
3. **New Benchmark (`/new`)** — Form with model selector, dataset selector, concurrency slider, prompts dir, max findings, optional cache dir.
4. **Live View (`/live/:id`)** — 4-column agent pane layout showing streaming LLM responses, progress bar, elapsed time, running cost.

### Layout Wireframe

```
┌─────────────────────────────────────────────────────────┐
│  🏠 Home │ 📊 Runs │ 🆕 New Benchmark                  │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  ┌────SA────┐  ┌────CL────┐                             │
│  │ status   │  │ status   │                             │
│  │ response │  │ response │                             │
│  │ text     │  │ text     │                             │
│  └──────────┘  └──────────┘                             │
│  ┌────AR────┐  ┌───SEC────┐                             │
│  │ status   │  │ status   │                             │
│  │ response │  │ response │                             │
│  │ text     │  │ text     │                             │
│  └──────────┘  └──────────┘                             │
│                                                         │
│  [████████████░░░░░░░░] 12/50 PRs | 3m42s | $0.12      │
│  PR: discourse-graphite/pull/7 -> F1=0.33               │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

### Component Tree

```
App
├── Navbar
├── Router
│   ├── HomePage
│   │   ├── RunList
│   │   │   └── RunCard (×N) — mini metrics, sparkline
│   │   └── NewBenchmarkButton
│   ├── RunDetailPage
│   │   ├── AggregateMetricsCard (F1, precision, recall)
│   │   ├── CostBreakdown
│   │   └── PrResultsTable (sortable)
│   ├── NewBenchmarkPage
│   │   ├── ModelSelector
│   │   ├── DatasetSelector
│   │   ├── ConcurrencySlider
│   │   └── BenchmarkConfigFields
│   └── LiveViewPage
│       ├── AgentPane (×4 — SA, CL, AR, SEC)
│       │   ├── StatusIndicator
│       │   └── ResponseStream
│       ├── ProgressBar
│       ├── ElapsedTimer
│       └── RunningCostDisplay
```

### State Management

```rust
// Frontend signals (Leptos reactive)
#[derive(Clone)]
struct RunSummary {
    id: String,
    name: String,
    pr_count: usize,
    avg_f1: f64,
    total_cost: f64,
    created_at: String,
}

#[derive(Clone)]
struct LiveState {
    agent_panes: Vec<AgentPaneState>,
    completed_prs: usize,
    total_prs: usize,
    elapsed_secs: f64,
    total_cost: f64,
    current_pr: Option<String>,
}

#[derive(Clone)]
struct AgentPaneState {
    role: String,
    status: String,
    response_buffer: Vec<String>,
    findings: usize,
}
```

### Communication Flow (Live View)

```
crb-harness (subprocess)
  └── stdout: {"event":"agent_chunk","role":"SA","chunk":"..."}
      ───────────────────────────────────────────┐
                                                 ▼
axum backend reads line-by-line
  └── Parses JSON -> dashboard event
  └── Broadcasts to tokio::sync::broadcast channel
      ───────────────────────────────────────────┐
                                                 ▼
SSE handler reads from broadcast channel
  └── Writes SSE `data: <json>\n\n` to response
      ───────────────────────────────────────────┐
                                                 ▼
Leptos WASM frontend receives EventSource
  └── Updates reactive signals (agent_panes, progress, etc.)
  └── UI re-renders automatically
```
