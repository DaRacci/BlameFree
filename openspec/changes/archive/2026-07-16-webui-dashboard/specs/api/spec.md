# Delta for API Endpoints

> **Implementation location:**
> - `crates/crb-webui-backend/src/api/runs.rs` — run endpoints
> - `crates/crb-webui-backend/src/api/config.rs` — config endpoints
> - `crates/crb-webui-backend/src/api/adhoc.rs` — ad-hoc review endpoints
> - `crates/crb-webui-backend/src/api/admin.rs` — admin endpoints
> - `crates/crb-webui-backend/src/api/live.rs` — SSE streaming
> - `crates/crb-webui-shared/src/runs.rs` — request/response types
> - `crates/crb-webui-shared/src/config.rs` — config types
> - `crates/crb-webui-shared/src/adhoc.rs` — ad-hoc types
> - `crates/crb-webui-shared/src/admin.rs` — admin types

## ADDED Requirements

### Requirement: List Runs

**Description:** GET /api/runs SHALL return a list of all benchmark runs, both completed (from disk) and active (in-memory). Active/running runs sort first, then completed by most-recent creation time.

**Request:** `GET /api/runs`

**Response `200 OK`:**
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
- Scans each subdirectory of `output/` for JSON result files
- If `_summary.json` exists in a run dir, use its aggregated data
- Otherwise, compute aggregate metrics from per-PR JSON files
- Running benchmarks appear with `status: "running"`
- Active runs are held in `state.active_runs` and merged with disk results

#### Scenario: Empty output directory returns empty array

Given the server's output directory contains no subdirectories
When a GET /api/runs request is made
Then the response is `200 OK` with an empty JSON array `[]`

#### Scenario: Completed run returns aggregated summary

Given the output directory contains a run directory with `_summary.json`
And `_summary.json` has aggregate metrics (avg_f1, avg_precision, avg_recall, total_cost, total_tokens, duration_secs, model)
When a GET /api/runs request is made
Then the response contains a `RunSummary` entry for that run with `status: "completed"` and the aggregated values

#### Scenario: Active run without disk output appears as running

Given a run is in progress in `state.active_runs`
And its output directory does not yet exist on disk
When a GET /api/runs request is made
Then the response includes a `RunSummary` for that run with `status: "running"` and placeholder metric values (0.0)

#### Scenario: Active runs sort before completed runs

Given there are both active running runs and completed runs
When a GET /api/runs request is made
Then running runs appear first in the array, sorted by creation time descending
And completed runs appear after running runs, sorted by creation time descending

---

### Requirement: Get Run Detail

**Description:** GET /api/runs/:id SHALL return detailed per-PR results and aggregate metrics for a specific benchmark run, including config if available from active state.

**Request:** `GET /api/runs/:id`

**Response `200 OK`:**
```json
{
  "id": "smoke-5",
  "name": "smoke-5",
  "pr_count": 2,
  "results": [
    {
      "pr_number": 7,
      "pr_key": "discourse-graphite/pull/7",
      "title": "Update color function",
      "f1": 0.5,
      "precision": 0.333,
      "recall": 1.0,
      "cost": 0.008,
      "status": "done",
      "has_agents": true
    }
  ],
  "aggregate": {
    "true_positives": 3,
    "false_positives": 6,
    "false_negatives": 0,
    "duration_secs": 120.0
  },
  "total_cost": 0.015,
  "total_tokens": 15000,
  "duration_secs": 120.5,
  "model": "gpt-4o",
  "status": "completed",
  "config": {
    "model": "gpt-4o",
    "dataset": "golden_comments",
    "roles": ["SA", "CL", "AR", "SEC"]
  }
}
```

**Response `404`:**
```json
{ "error": "Run not found: <id>" }
```

**Notes:**
- If the run is still active (in memory), returns the running state with empty results
- Completed runs read per-PR JSON files from the run directory
- `config` is only populated for active runs (not persisted on disk)
- `has_agents` indicates whether cached agent logs exist for that PR

#### Scenario: Active running run returns partial data

Given a run is in progress in `state.active_runs`
And `active_run.finished` is false
When a GET /api/runs/:id request is made
Then the response has `status: "running"`, an empty `results` array, and a populated `config`

#### Scenario: Completed run returns per-PR results

Given the run directory exists on disk with per-PR JSON files and a `_summary.json`
When a GET /api/runs/:id request is made
Then the response has `status: "completed"`, `results` populated from per-PR JSON files, and `aggregate` populated from summary

#### Scenario: Missing run returns 404

Given no directory or active state exists for the given run ID
When a GET /api/runs/:id request is made
Then the response is `404` with `{ "error": "Run not found: <id>" }`

---

### Requirement: Start Benchmark Run

**Description:** POST /api/runs SHALL start a new benchmark run, create an active run entry with a broadcast channel for SSE events, spawn the harness asynchronously, and return immediately.

**Request:** `POST /api/runs`

**Request Body:**
```json
{
  "model": "gpt-4o",
  "judge_model": "gpt-4o-mini",
  "dataset_dir": "golden_comments",
  "concurrency": 4,
  "max_findings": 20,
  "cache_dir": null,
  "roles": ["SA", "CL", "AR", "SEC"],
  "skip_consensus": false,
  "skip_linters": false,
  "pr_filter": null,
  "use_cache": true,
  "reasoning_effort": null
}
```

**Response `201 Created`:**
```json
{
  "run_id": "run-1719480000",
  "status": "started",
  "total_prs": 10
}
```

**Notes:**
- Backend calls `crb_harness::pipeline::evaluate()` directly via in-process library call
- Returns immediately with run_id; client opens SSE stream to see progress
- The `EvalConfig.dashboard_tx` field routes events via broadcast channel
- `use_cache` defaults to `true`
- `reasoning_effort` is an optional string: `null`, `"low"`, `"medium"`, `"high"`

#### Scenario: Valid config starts a run and returns 201

Given a valid POST /api/runs request body with model, dataset_dir, and roles
When the server receives the request
Then the response is `201 Created` with a `run_id`, `status: "started"`, and `total_prs` matching the dataset's PR count
And an ActiveRun is inserted into `state.active_runs`
And a background tokio task is spawned to execute the harness

#### Scenario: PR count is calculated before returning

Given a dataset with 42 PRs
When a POST /api/runs request is made with that dataset
Then the response's `total_prs` field is `42`

---

### Requirement: SSE Live Stream

**Description:** GET /api/runs/:id/live SHALL return a Server-Sent Events stream of live agent events for an active run. Uses `broadcast::Sender<RunEvent>` to forward events from the running harness to connected clients.

**Request:** `GET /api/runs/:id/live`

**Response:** SSE stream with `text/event-stream` content type.

**Event format:** Each event is a JSON object serialized from the `RunEvent` enum with the shape:
```json
{
  "event": "agent_started",
  "data": {
    "identifier": "discourse-7",
    "agent": "SA"
  }
}
```

**Keep-alive:** Every 15 seconds via `axum::response::sse::KeepAlive`.

**Notes:**
- If the run ID is not in `active_runs`, returns 404 with error message
- Uses `BroadcastStream` wrapper — lagged clients silently drop events

#### Scenario: Active run returns SSE stream

Given a run is in progress in `state.active_runs`
When a GET /api/runs/:id/live request is made
Then the response is a `text/event-stream` SSE connection backed by the run's broadcast channel

#### Scenario: Inactive run returns 404

Given no active run exists with the requested ID
When a GET /api/runs/:id/live request is made
Then the response is `404` with `{ "error": "No active run: <id>" }`

---

### Requirement: List Logs

**Description:** GET /api/runs/:id/logs SHALL list available PR keys and their agent roles for a run, merging data from the output directory (canonical PR source) with the cache directory (agent availability).

**Request:** `GET /api/runs/:id/logs`

**Response `200 OK`:**
```json
{
  "run_id": "smoke-5",
  "cache_available": true,
  "prs": [
    {
      "pr_key": "discourse-graphite-pull-7",
      "pr_title": "Update color function",
      "agents": [{ "name": "SA", "abbreviation": "SA", "incompatible_with_roles": [] }]
    }
  ]
}
```

**Notes:**
- PR keys come from JSON files in the run's output directory
- Agent roles are scanned from the cache directory's `agents/` subdirectory
- Supports both content-addressed cache layout (`*.agent_{role}_{type}.txt`) and simple layout (`agent_{role}_{type}.txt`)

#### Scenario: Returns all PRs with agent roles from cache

Given the run output directory contains per-PR JSON files
And the cache directory contains agent log files for some PRs
When a GET /api/runs/:id/logs request is made
Then the response includes all PRs from the output directory
And each PR includes its agent roles from the cache directory

#### Scenario: No cache returns empty agents

Given the run output directory exists but no cache directory is available
When a GET /api/runs/:id/logs request is made
Then `cache_available` is `false` and each PR has an empty `agents` list

---

### Requirement: Get Agent Log

**Description:** GET /api/runs/:id/logs/:pr_key/:role SHALL return the prompt, response, and reasoning text for a specific agent on a specific PR.

**Request:** `GET /api/runs/:id/logs/:pr_key/:role`

**Response `200 OK`:**
```json
{
  "run_id": "smoke-5",
  "pr_key": "discourse-graphite-pull-7",
  "role": "SA",
  "prompt": "Analyze the following PR diff...",
  "response": "Found potential issue...",
  "reasoning": "The color function uses wrong variable...",
  "available": true
}
```

**Notes:**
- Reads from cache directory (content-addressed or simple layout)
- Returns `available: true` if data was found, `false` otherwise

#### Scenario: Cached agent log returns prompt, response, and reasoning

Given the cache directory contains agent log files for the given PR key and role
When a GET /api/runs/:id/logs/:pr_key/:role request is made
Then the response contains `prompt`, `response`, and `reasoning` fields with the file contents
And `available` is `true`

#### Scenario: Missing agent log returns available=false

Given no agent log files exist for the given PR key and role
When a GET /api/runs/:id/logs/:pr_key/:role request is made
Then the response has `available: false` and all text fields are `null`

---

### Requirement: List PR Agents

**Description:** GET /api/runs/:id/prs/:pr_key SHALL return the list of agent roles available for a specific PR, with indicators of whether prompt/response/reasoning data exists for each.

**Request:** `GET /api/runs/:id/prs/:pr_key`

**Response `200 OK`:**
```json
{
  "run_id": "smoke-5",
  "pr_key": "discourse-graphite-pull-7",
  "pr_title": "Update color function",
  "agents": [
    { "role": "SA", "has_prompt": true, "has_response": true, "has_reasoning": false }
  ],
  "has_output": true
}
```

#### Scenario: Returns agent availability for a known PR

Given the run directory and cache directory exist for this PR
When a GET /api/runs/:id/prs/:pr_key request is made
Then the response includes an `agents` array with per-role availability flags
And `has_output` indicates whether any agent output exists

---

### Requirement: PR Detail

**Description:** GET /api/runs/:id/pr-detail/:pr_key SHALL return detailed per-PR findings, verdicts, metrics, and cost data from the PR's JSON result file.

**Request:** `GET /api/runs/:id/pr-detail/:pr_key`

**Response `200 OK`:**
```json
{
  "run_id": "smoke-5",
  "pr_title": "Update color function",
  "url": "https://github.com/discourse/discourse/pull/7",
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
  "verdicts": [{ "reasoning": "...", "match": true, "confidence": 0.95 }],
  "cost": { "total_usd": 0.003, "agent_tokens_in": 1000, "agent_tokens_out": 500, "judge_tokens_in": 0, "judge_tokens_out": 0, "agent_call_count": 1, "judge_call_count": 0 },
  "findings": {},
  "agent_responses": []
}
```

#### Scenario: Reads PR result JSON from disk

Given the run directory contains a JSON result file for the requested PR key
When a GET /api/runs/:id/pr-detail/:pr_key request is made
Then the response returns the parsed content of that JSON file
With all metrics, verdicts, cost data, findings, and agent responses

---

### Requirement: Get Config

**Description:** GET /api/config SHALL return available models (as string list), dataset IDs, reviewer roles with incompatibility info, and whether OAuth is enabled.

**Request:** `GET /api/config`

**Response `200 OK`:**
```json
{
  "models": ["gpt-4o", "claude-sonnet-4-20250514"],
  "datasets": ["golden_comments", "smoke"],
  "roles": [
    { "name": "SA", "abbreviation": "SA", "incompatible_with_roles": [] },
    { "name": "CL", "abbreviation": "CL", "incompatible_with_roles": [] }
  ],
  "auth_enabled": false
}
```

**Notes:**
- Models are sourced from the comma-separated `models` config string
- Datasets are scanned from the configured `dataset_dir`
- Roles are sourced from `PromptLibrary::get_instance()`
- `auth_enabled` is `true` when `state.config.oauth` is `Some`

#### Scenario: Returns parsed config from server state

Given the server has configured models, datasets, and roles
When a GET /api/config request is made
Then models are returned as a flat string array
And datasets are returned as a flat string array of dataset IDs
And roles include abbreviation, name, and incompatibility information

---

### Requirement: List Datasets

**Description:** GET /api/config/datasets SHALL list available datasets with their path, PR count, and optional per-dataset config (defaults for model, concurrency, max_findings, roles).

**Request:** `GET /api/config/datasets`

**Response `200 OK`:**
```json
[
  {
    "id": "golden_comments",
    "path": "datasets/golden_comments",
    "pr_count": 42,
    "config": {
      "defaults": { "model": "gpt-4o", "concurrency": 4, "max_findings": 20, "roles": "SA,CL,AR,SEC" }
    }
  }
]
```

**Notes:**
- Datasets are sorted by PR count descending
- Each dataset directory is scanned for JSON files; PR count is the count of PR entries
- Optional `dataset.toml` files provide per-dataset defaults

#### Scenario: Returns all datasets with PR counts

Given the configured dataset_dir contains subdirectories with JSON PR files
When a GET /api/config/datasets request is made
Then each dataset is returned with its `id`, `path`, `pr_count`, and optional `config`
And datasets are sorted by `pr_count` descending

---

### Requirement: List Reasoning Efforts

**Description:** GET /api/config/reasoning-efforts SHALL return available reasoning effort levels from the `ReasoningEffort` enum.

**Request:** `GET /api/config/reasoning-efforts`

**Response `200 OK`:**
```json
{
  "levels": ["low", "medium", "high"]
}
```

#### Scenario: Returns effort levels from enum variants

When a GET /api/config/reasoning-efforts request is made
Then the response contains a `levels` array with string variants of `ReasoningEffort::variants()`

---

### Requirement: List Dataset PRs

**Description:** GET /api/datasets/:id/prs SHALL list all PRs within a specific dataset, extracted from the JSON files in that dataset directory.

**Request:** `GET /api/datasets/:id/prs`

**Response `200 OK`:**
```json
[
  {
    "key": "discourse-graphite/pull/7",
    "url": "https://github.com/discourse/discourse/pull/7",
    "title": "Update color function",
    "repo": "discourse",
    "pr_number": 7
  }
]
```

**Notes:**
- Supports both JSON array format and object-with-entries format
- PR keys are derived from GitHub URL using the pattern `repo/pull/N`
- Unknown dataset returns an empty array

#### Scenario: Known dataset returns PR entries

Given the dataset directory exists and contains JSON files with PR entries
When a GET /api/datasets/:id/prs request is made
Then the response is an array of `PrEntry` objects with key, url, title, repo, and pr_number

#### Scenario: Unknown dataset returns empty array

Given the dataset directory does not exist
When a GET /api/datasets/:id/prs request is made
Then the response is an empty JSON array `[]`

---

### Requirement: Start Ad-hoc Review

**Description:** POST /api/adhoc/review SHALL start an ad-hoc PR review for a given GitHub URL. Parses the URL, fetches the PR diff via GitHub API, runs the harness agents, and returns immediately.

**Request:** `POST /api/adhoc/review`

**Request Body:**
```json
{
  "url": "https://github.com/discourse/discourse/pull/7",
  "model": "gpt-4o",
  "roles": ["SA", "CL", "AR", "SEC"]
}
```

**Response `200 OK`:**
```json
{
  "run_id": "adhoc-1719480000",
  "pr_title": "Update color function",
  "status": "running"
}
```

**Notes:**
- Accepts `url` (not separate owner/repo/pr_number fields)
- Parses URL via `crb_shared::url::parse_github_url`
- Invalid URL returns 400 with error message
- Runs asynchronously via `tokio::spawn`

#### Scenario: Valid URL starts a review and returns run_id

Given a valid GitHub PR URL and model
When a POST /api/adhoc/review request is made
Then the response is `200 OK` with a `run_id`, the PR's `title`, and `status: "running"`
And a background task is spawned to execute the review

#### Scenario: Invalid URL returns 400

Given an invalid GitHub URL that cannot be parsed
When a POST /api/adhoc/review request is made
Then the response is `400 BAD_REQUEST` with `{ "error": "Invalid GitHub PR URL..." }`

---

### Requirement: List Ad-hoc Runs

**Description:** GET /api/adhoc/runs SHALL list all previous ad-hoc review runs from the `output/adhoc/` directory.

**Request:** `GET /api/adhoc/runs`

**Response `200 OK`:**
```json
[
  {
    "id": "adhoc-1719480000",
    "pr_url": "https://github.com/discourse/discourse/pull/7",
    "pr_title": "Update color function",
    "status": "completed",
    "created_at": "2026-07-16T10:00:00Z",
    "model": "gpt-4o",
    "roles": ["SA", "CL"],
    "findings_count": 5,
    "total_cost": 0.005
  }
]
```

**Notes:**
- Scans the `output/adhoc/` directory for run subdirectories
- Sorted by creation time descending

#### Scenario: Returns all ad-hoc runs from disk

Given the `output/adhoc/` directory contains run subdirectories with `_summary.json`
When a GET /api/adhoc/runs request is made
Then each run is returned with its id, pr_url, pr_title, status, model, roles, findings_count, and total_cost
And results are sorted by `created_at` descending

---

### Requirement: Get Ad-hoc Run Detail

**Description:** GET /api/adhoc/runs/:id SHALL return detailed results for a specific ad-hoc review run, including per-PR results and aggregate metrics.

**Request:** `GET /api/adhoc/runs/:id`

**Response `200 OK`:**
```json
{
  "id": "adhoc-1719480000",
  "name": "adhoc-1719480000",
  "pr_count": 1,
  "results": [{ "pr_number": 0, "pr_key": "...", "title": "...", "f1": 0.5, "precision": 0.333, "recall": 1.0, "cost": 0.005, "status": "done", "has_agents": false }],
  "aggregate": { "true_positives": 3, "false_positives": 6, "false_negatives": 0, "duration_secs": 120.0 },
  "total_cost": 0.005,
  "total_tokens": 0,
  "duration_secs": 120.0,
  "model": "gpt-4o",
  "status": "completed",
  "config": { "model": "gpt-4o", "dataset": "", "roles": ["SA", "CL"] }
}
```

**Response `404`:**
```json
{ "error": "Ad-hoc run not found" }
```

#### Scenario: Existing run returns detail with metrics

Given the ad-hoc run directory exists with `_summary.json` and per-PR JSON files
When a GET /api/adhoc/runs/:id request is made
Then the response includes all PR results, aggregate metrics, total cost, model, and status

#### Scenario: Missing run returns 404

Given the ad-hoc run directory does not exist
When a GET /api/adhoc/runs/:id request is made
Then the response is `404` with `{ "error": "Ad-hoc run not found" }`

---

### Requirement: List Repo PRs

**Description:** GET /api/adhoc/prs/:owner/:repo SHALL proxy to the GitHub API to list open pull requests for a repository (avoids CORS issues from the frontend).

**Request:** `GET /api/adhoc/prs/:owner/:repo`

**Response `200 OK`:**
```json
[
  { "number": 7, "title": "Update color function", "html_url": "https://github.com/discourse/discourse/pull/7" }
]
```

**Notes:**
- Uses `octocrab` to call the GitHub API
- Returns up to 100 open PRs
- GitHub API error returns 502

#### Scenario: Fetches open PRs from GitHub API

Given the backend has a configured GitHub API client
When a GET /api/adhoc/prs/:owner/:repo request is made
Then the response is an array of open PRs with number, title, and html_url

#### Scenario: GitHub API error returns 502

Given the GitHub API request fails
When a GET /api/adhoc/prs/:owner/:repo request is made
Then the response is `502 BAD_GATEWAY` with an error message

---

### Requirement: Get Admin Logs

**Description:** GET /api/admin/logs SHALL return the last 500 lines from the server's log file.

**Request:** `GET /api/admin/logs`

**Response `200 OK`:**
```json
{
  "logs": "2026-07-16 INFO Starting crb-webui on port 8080\n...",
  "available": true,
  "message": null
}
```

**Notes:**
- Reads last 500 lines efficiently using reverse seek
- Returns `available: false` if log file can't be read, with an explanatory message

#### Scenario: Log file exists returns last 500 lines

Given the server's log file is configured and accessible
When a GET /api/admin/logs request is made
Then the response contains the last 500 lines of the log file
And `available` is `true`

#### Scenario: Log file missing returns available=false

Given no log file is configured or it cannot be read
When a GET /api/admin/logs request is made
Then `available` is `false` and `message` explains the error

---

### Requirement: Admin Logs Stream

**Description:** GET /api/admin/logs/stream SHALL return a Server-Sent Events stream of server log entries (live tail). Sends initial batch of last 500 lines, then polls the log file every second for new content.

**Request:** `GET /api/admin/logs/stream`

**Response:** SSE stream with `text/event-stream` content type.

**Notes:**
- First data batch contains last 500 lines as individual events
- Subsequent events append new lines as they appear in the log file
- Keep-alive every 15 seconds

#### Scenario: First connection sends last 500 lines then polls

When a GET /api/admin/logs/stream request is made
Then the client receives the last 500 lines as initial SSE events
And subsequent new log lines are pushed every ~1 second as they appear
