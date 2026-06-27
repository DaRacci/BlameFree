# Spec: Log Endpoints

## GET /api/runs/:id/logs

### Purpose
List all available log files (agent prompts + responses) for a completed run.

### Request
- `id` — Run ID (path parameter). Matches a subdirectory under `output/` and `cache/`.

### Response (200 OK)
```json
{
  "run_id": "run-1234567890",
  "cache_available": true,
  "prs": [
    {
      "pr_key": "scale-color__lightness_must_use__secondary_for_dark_themes",
      "pr_title": "Scale color lightness must use secondary for dark themes",
      "agents": ["SA", "CL", "AR", "SEC"]
    }
  ]
}
```

### Response (404)
```json
{ "error": "Run not found: run-1234567890" }
```

### Error codes
- 404 — Run ID not found in either `output/` or `cache/`
- 500 — Filesystem error reading cache directory

### Implementation Details
1. Check `cache/{run_id}` exists
2. If not, return `{ "cache_available": false, "prs": [] }`
3. Read each subdirectory under `cache/{run_id}/`
4. For each PR directory, list `agent_{role}_{prompt|response}.txt` files (simple) or `agents/{hash}.agent_{role}_{prompt|response}.txt` (content-addressed)
5. Deduplicate agent roles found

---

## GET /api/runs/:id/logs/:pr_key/:role

### Purpose
Get a specific agent's prompt and response from cache.

### Request
- `id` — Run ID
- `pr_key` — PR directory key (URL-encoded slug)
- `role` — Agent role (SA, CL, AR, SEC)

### Response (200 OK)
```json
{
  "run_id": "run-1234567890",
  "pr_key": "scale-color__lightness_must_use__secondary_for_dark_themes",
  "role": "SA",
  "prompt": "You are a code reviewer...",
  "response": "## Summary\nThe PR modifies...",
  "available": true
}
```

### Response when not available
```json
{
  "run_id": "run-1234567890",
  "pr_key": "scale-color__lightness_must_use__secondary_for_dark_themes",
  "role": "SA",
  "prompt": null,
  "response": null,
  "available": false
}
```

### Error codes
- 404 — Run not found or PR key not found

### Implementation Details
1. For content-addressed layout: search `cache/{id}/{pr_key}/agents/*.agent_{role}_prompt.txt` and `*_response.txt`
2. For simple layout: read `cache/{id}/{pr_key}/agent_{role}_{prompt|response}.txt`
3. Return raw file contents. Handle UTF-8 decoding gracefully with lossy conversion.
