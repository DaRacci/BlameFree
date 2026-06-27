# Spec: Replay Endpoints

## POST /api/runs/:id/replay

### Purpose
Start a replay of a completed run using only cached API responses. Produces identical results since cache is content-addressed.

### Request
- `id` — Run ID (path parameter)

### Response (202 Accepted)
```json
{
  "run_id": "run-1234567890",
  "replay_id": "replay-0",
  "status": "started",
  "cache_available": true
}
```

### Response when cache unavailable
```json
{
  "error": "No cache available for run run-1234567890"
}
```
Status: 400

### Implementation Details
1. Check `cache/{run_id}` exists
2. Spawn `crb-harness` with `--cache-dir cache/{run_id}` and `--output-dir output/{run_id}-replay`
3. Use same config as original run (read from `output/{run_id}/_summary.json`)
4. Store replay process handle in `AppState::replays: Arc<RwLock<HashMap<String, ReplayState>>>`
5. Since cache is content-addressed, every API call should be a cache hit, making replay virtually instant
6. Run progress is tracked via the same dashboard-events mechanism

---

## GET /api/runs/:id/replay/status

### Purpose
Poll the status and progress of a replay operation.

### Request
- `id` — Run ID

### Response (200 OK)
```json
{
  "run_id": "run-1234567890",
  "status": "running",
  "progress_pct": 45,
  "completed_prs": 3,
  "total_prs": 7,
  "message": "Replaying PR #4/7..."
}
```

### Response when complete
```json
{
  "run_id": "run-1234567890",
  "status": "completed",
  "progress_pct": 100,
  "completed_prs": 7,
  "total_prs": 7,
  "message": "Replay complete",
  "replay_output_dir": "output/run-1234567890-replay"
}
```

### Response when no replay in progress
```json
{
  "run_id": "run-1234567890",
  "status": "idle",
  "progress_pct": 0,
  "completed_prs": 0,
  "total_prs": 0,
  "message": "No replay in progress"
}
```

### Error codes
- 404 — Run not found

### Implementation Details
1. Look up `replay_id` in `AppState::replays`
2. If not found, return status "idle"
3. If process has exited, return status "completed" or "failed"
4. Track progress by reading dashboard events or polling the output directory for new files
