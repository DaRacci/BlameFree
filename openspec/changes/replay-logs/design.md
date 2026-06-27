# Design: Replay and Log Viewing

## Log Viewing (Run Detail Page)

### UI
- New tab bar on run detail page: `[ Results ] [ Logs ]`
- Logs tab lists per-PR agent logs grouped by PR
- Each PR section is collapsible, showing agent role headers (SA, CL, AR, SEC)
- Each agent section has collapsible prompt and response blocks in `<pre>` with dark-theme syntax highlighting
- If no cache data exists for a run, show "No cache available for this run" empty state

### Data Flow
1. Frontend calls `GET /api/runs/:id/logs` on tab switch
2. Backend scans `cache/{run_id}` directory for available PR keys
3. Frontend renders PR list with expandable sections
4. Clicking an agent row calls `GET /api/runs/:id/logs/:pr_key/:role` to fetch specific prompt+response
5. Backend reads `cache/{run_id}/{pr_key}/agents/{hash}.agent_{role}_{prompt|response}.txt`

## Replay Mode

### UI
- "Replay Run" button on run detail page (visible when cache exists)
- Opens a modal overlay with:
  - Progress bar (0-100%)
  - Status text: "Replaying from cache..." / "Comparing results..." / "Complete"
  - On completion: "View Comparison" button showing original vs replay metrics table
- The comparison table shows per-PR: F1, precision, recall (original vs replay) with pass/fail indicators

### Data Flow
1. User clicks "Replay Run"
2. `POST /api/runs/:id/replay` — server spawns `crb-harness` with `--cache-dir` set to original run's cache dir
3. Since cache is content-addressed, the harness reads cached API responses and produces results immediately
4. Frontend polls `GET /api/runs/:id/replay/status` every 500ms
5. On completion, frontend fetches replay results from `output/{run_id}-replay/`
6. Frontend shows original vs replay comparison

## API Design

### New Endpoints

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/api/runs/:id/logs` | List available log files for a run |
| GET | `/api/runs/:id/logs/:pr_key/:role` | Get specific agent log (prompt + response) |
| POST | `/api/runs/:id/replay` | Start replay from cache |
| GET | `/api/runs/:id/replay/status` | Poll replay progress |

### Frontend Components

| Component | Props | Description |
|-----------|-------|-------------|
| TabBar | tabs, active_tab, on_switch | Generic tab switcher |
| LogViewer | logs: Vec<AgentLog> | Collapsible agent log sections |
| AgentLogView | role, prompt, response | Single agent's prompt+response |
| ReplayOverlay | visible, progress, status, on_close | Modal with progress bar |
| ComparisonTable | original, replay | Side-by-side metrics table |

### Cache File Patterns

Content-addressed cache (ca-test):
```
cache/{run_id}/{pr_key}/agents/{hash}.agent_{role}_{prompt|response}.txt
cache/{run_id}/{pr_key}/index.json
cache/{run_id}/{pr_key}/metadata.json
```

Simple cache (smoke-test):
```
cache/{run_id}/{pr_key}/agent_{role}_{prompt|response}.txt
cache/{run_id}/{pr_key}/metadata.json
cache/{run_id}/_summary.json
```
