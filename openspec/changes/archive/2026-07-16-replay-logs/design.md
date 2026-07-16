# Design: Replay and Log Viewing

## Log Viewing (PrDetailPage)

### UI
- Log viewing is a **separate page** at `/runs/:id/prs/:pr_key` (not a tab on the run detail page)
- Each PR row in `RunDetailPage` has a "Logs" link when cache is available (navigates to PrDetailPage)
- PrDetailPage shows:
  - Breadcrumb: Home / run-id / pr-key
  - PR title heading
  - Agent cards in a responsive grid (auto-fit, min 450px)
  - Each card has:
    - Role-colored left border and dot
    - Agent display name and availability badges (✓ Prompt, ✓ Response, ✓ Reasoning)
    - Collapsible `<details>` sections for Prompt, Response, and Reasoning in `<pre>` blocks
- If no cache data exists for a PR, show "No cached agent logs available" empty state
- If cache is unavailable, disabled "Logs" button with tooltip

### Data Flow
1. Frontend navigates to `/runs/:id/prs/:pr_key`
2. On mount, calls `GET /api/runs/:id/prs/:pr_key` to get PR title + agent list with availability flags
3. For each agent, concurrently calls `GET /api/runs/:id/logs/:pr_key/:role` to fetch prompt+response+reasoning
4. Results rendered in a `HashMap<String, AgentLogResponse>` per role
5. Individual agent cards show empty state if log fetch fails or data is unavailable

### Backend Cache Lookup (resolve_cache_dir)
1. `output/<run_id>/_cache/` (new flat layout, shared across runs)
2. `output/<run_id>/<run_id>/cache/` (legacy nested by run_id)
3. `output/<run_id>/.cache/<run_id>/` (legacy parallel cache dir)
4. `output/<run_id>/../cache/` (flat no-run-id layout)

### Cache File Patterns

Content-addressed cache (ca-test):
```
cache/{run_id}/{pr_key}/agents/{hash}.agent_{role}_{prompt|response|reasoning}.txt
cache/{run_id}/{pr_key}/index.json
cache/{run_id}/{pr_key}/metadata.json
```

Simple cache (smoke-test):
```
cache/{run_id}/{pr_key}/agent_{role}_{prompt|response|reasoning}.txt
cache/{run_id}/{pr_key}/metadata.json
cache/{run_id}/_summary.json
```



## API Design

### Endpoints (Implemented)

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/api/runs/:id/logs` | List available log files for a run |
| GET | `/api/runs/:id/logs/:pr_key/:role` | Get specific agent log (prompt + response + reasoning) |
| GET | `/api/runs/:id/prs/:pr_key` | Get PR title + per-agent availability info |
| GET | `/api/runs/:id/pr-detail/:pr_key` | Get full PR detail (metrics, verdicts, findings) |



### Frontend Components (Implemented)

| Component | Props | Description |
|-----------|-------|-------------|
| PrDetailPage | (route params: id, pr_key) | Dedicated page showing per-agent log cards |
| LogViewer | logs: LogsListResponse, run_id: String | Alternative collapsible PR-section view (unused, superseded by PrDetailPage) |



### Types (in crb-webui-shared/src/runs.rs)

All log and replay types are defined in the shared crate, not in the backend.
