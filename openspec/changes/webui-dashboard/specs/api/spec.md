# API Specification

## GET /api/runs

List all past benchmark runs by scanning the output directory.

**Response `200 OK`**
```json
[
  {
    "id": "smoke-5",
    "name": "smoke-5",
    "pr_count": 2,
    "avg_f1": 0.5,
    "avg_precision": 0.333,
    "avg_recall": 1.0,
    "total_cost": 0.015,
    "total_tokens": 15000,
    "duration_secs": 120.5,
    "created_at": "2026-06-27T10:00:00Z",
    "model": "gpt-4o",
    "status": "completed"
  }
]
```

**Notes:**
- Scans each subdirectory of `output/` for `<sanitized-title>.json` files
- If `_summary.json` exists in a run dir, use its aggregated data
- Otherwise, compute aggregate metrics from per-PR JSON files

---

## GET /api/runs/:id

Get detailed results for a specific benchmark run.

**Response `200 OK`**
```json
{
  "id": "smoke-5",
  "name": "smoke-5",
  "pr_count": 2,
  "results": [
    {
      "pr_title": "...",
      "url": "https://github.com/...",
      "findings_count": 0,
      "golden_count": 3,
      "metrics": {
        "true_positives": 3,
        "false_positives": 6,
        "false_negatives": 0,
        "precision": 0.333,
        "recall": 1.0,
        "f1": 0.5
      },
      "verdicts": [...]
    }
  ],
  "aggregate": {
    "avg_f1": 0.5,
    "avg_precision": 0.333,
    "avg_recall": 1.0,
    "total_tp": 3,
    "total_fp": 6,
    "total_fn": 0
  },
  "total_cost": 0.015,
  "total_tokens": 15000,
  "duration_secs": 120.5,
  "model": "gpt-4o"
}
```

**Response `404`**
```json
{ "error": "Run not found: <id>" }
```

---

## POST /api/runs

Start a new benchmark run.

**Request Body**
```json
{
  "model": "gpt-4o",
  "judge_model": "gpt-4o-mini",
  "dataset_dir": "datasets/golden_comments",
  "concurrency": 4,
  "max_findings": 20,
  "prompts_dir": "prompts/builtin",
  "cache_dir": null,
  "roles": "SA,CL,AR,SEC",
  "skip_consensus": false,
  "skip_linters": false
}
```

**Response `201 Created`**
```json
{
  "run_id": "run-1719480000",
  "status": "started",
  "total_prs": 10
}
```

**Notes:**
- Backend spawns `crb-harness --dashboard-events [args]` as subprocess
- Returns immediately with run_id
- Client opens SSE stream to see progress

---

## GET /api/runs/:id/live

SSE stream of live agent outputs for an active run.

**Response** — Server-Sent Events stream.

---

## GET /api/config

List available configuration options.

**Response `200 OK`**
```json
{
  "models": [
    { "id": "gpt-4o", "name": "GPT-4o" },
    { "id": "claude-sonnet-4-20250514", "name": "Claude Sonnet 4" }
  ],
  "datasets": [
    { "id": "golden_comments", "path": "datasets/golden_comments", "pr_count": 42 }
  ],
  "roles": ["SA", "CL", "AR", "SEC"]
}
```

---

## GET /api/config/datasets

List available datasets with PR counts.

**Response `200 OK`**
```json
[
  { "id": "golden_comments", "path": "datasets/golden_comments", "pr_count": 42 },
  { "id": "smoke", "path": "datasets/smoke", "pr_count": 5 }
]
```
