# Spec: API Endpoint Contracts

## ADDED Requirements

### Requirement: HTTP Review Submission
The system SHALL accept PR review requests via an HTTP POST endpoint.

#### Scenario: Submit a valid review request
- GIVEN a client sends a POST request to `/review` with a valid JSON body
- WHEN the server processes the request
- THEN it returns HTTP 202 Accepted with a JSON body containing a `review_id` (UUID v4) and `status: "pending"`
- AND it spawns an async task to process the review

#### Scenario: Submit with invalid/missing fields
- GIVEN a POST request to `/review` with missing `repo_url` or `pr_number`
- WHEN the server validates the request body
- THEN it returns HTTP 400 Bad Request with an error message indicating which field is missing or invalid

#### Request body schema
```json
{
    "repo_url": "https://github.com/owner/repo",
    "pr_number": 123,
    "diff": "@@ -1,3 +1,4 @@\n-old code\n+new code\n",
    "base_branch": "main"
}
```

#### Successful response schema (HTTP 202)
```json
{
    "review_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
    "status": "pending"
}
```

### Requirement: Review Status Polling
The system SHALL expose an endpoint to poll review status and retrieve findings.

#### Scenario: Check pending status
- GIVEN a `review_id` for a review that is still queued or processing
- WHEN a client sends GET `/review/{id}`
- THEN it returns HTTP 200 with `status: "pending"` or `status: "processing"`
- AND the response includes `findings: []` (empty) and default metrics

#### Scenario: Check completed status
- GIVEN a `review_id` for a review that has finished
- WHEN a client sends GET `/review/{id}`
- THEN it returns HTTP 200 with `status: "complete"`
- AND the response includes the full `findings` array and computed `metrics`

#### Scenario: Check non-existent review
- GIVEN a `review_id` that does not exist
- WHEN a client sends GET `/review/{id}`
- THEN it returns HTTP 404 Not Found with an error message

#### Response schema (HTTP 200)
```json
{
    "review_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
    "status": "complete",
    "findings": [
        {
            "file": "src/main.rs",
            "line": 42,
            "body": "Unsafe usage of `unwrap()` on a network response...",
            "severity": "High",
            "rule_code": "SA-001",
            "suggestion": "```rust\nlet response = client.get(url).await?;\n```",
            "source_role": "SA"
        }
    ],
    "metrics": {
        "total_findings": 5,
        "critical_count": 0,
        "high_count": 2,
        "medium_count": 2,
        "low_count": 1,
        "by_role": {
            "SA": 2,
            "CL": 1,
            "SEC": 2
        }
    },
    "context": {
        "owner": "owner",
        "repo": "repo",
        "pr_number": 123,
        "language": "Rust",
        "tech_stack": ["Tokio", "Axum", "Serde"],
        "modules": ["src/routes", "src/models", "src/db"],
        "changed_files": ["src/main.rs", "src/routes/review.rs"],
        "diff": "@@ -1,3 +1,4 @@\n..."
    }
}
```

### Requirement: GitHub-Compatible Comments
The system SHALL provide a separate endpoint returning findings in GitHub-compatible format.

#### Scenario: Get comments for completed review
- GIVEN a completed review
- WHEN a client sends GET `/review/{id}/comments`
- THEN it returns HTTP 200 with a JSON array of comment objects
- AND each comment has `file`, `line`, `body`, `severity`, `rule_code`, and `suggestion` fields

#### Scenario: Get comments for incomplete review
- GIVEN a review that is still pending or processing
- WHEN a client sends GET `/review/{id}/comments`
- THEN it returns HTTP 409 Conflict with message "review still processing"

#### Response schema (HTTP 200)
```json
[
    {
        "file": "src/main.rs",
        "line": 42,
        "body": "**Unsafe unwrap usage**\n\nUsing `.unwrap()` on a `Result` from a network call will panic if the request fails. Consider using `?` operator or proper error handling with `match`.\n\n**Analysis chain:**\n1. `client.get(url)` returns `Result<Response, Error>`\n2. `.unwrap()` discards the error variant\n3. Network failures cause panics in production\n\n**Severity:** High\n**Rule:** SA-001\n**Found by:** SA agent",
        "severity": "High",
        "rule_code": "SA-001",
        "suggestion": "```rust\nlet response = client.get(url).await?;\n```"
    }
]
```

### Requirement: Health Check
The system SHALL expose a health check endpoint.

#### Scenario: Server is healthy
- GIVEN the server is running
- WHEN a client sends GET `/health`
- THEN it returns HTTP 200 with status "ok", version string, and uptime seconds

#### Response schema (HTTP 200)
```json
{
    "status": "ok",
    "version": "0.1.0",
    "uptime_secs": 3600
}
```

### Requirement: Cancel Review
The system SHALL allow clients to cancel a running review.

#### Scenario: Cancel pending review
- GIVEN a review in `pending` or `processing` status
- WHEN a client sends POST `/review/{id}/cancel`
- THEN it returns HTTP 200 with status "cancelled"

#### Scenario: Cancel completed review
- GIVEN a review in `complete` or `failed` status
- WHEN a client sends POST `/review/{id}/cancel`
- THEN it returns HTTP 409 Conflict with error message "review already completed"

#### Response schema (HTTP 200)
```json
{
    "review_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
    "status": "cancelled"
}
```

### Requirement: List Recent Reviews
The system SHALL provide a listing endpoint for recent review jobs.

#### Scenario: List with default limit
- GIVEN the server has processed some reviews
- WHEN a client sends GET `/reviews`
- THEN it returns HTTP 200 with a JSON array of review summaries, ordered by creation time (most recent first)
- AND the default limit is 10 (configurable via `?limit=` query parameter, max 100)

#### Response schema (HTTP 200)
```json
[
    {
        "review_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
        "repo_url": "https://github.com/owner/repo",
        "pr_number": 123,
        "status": "complete",
        "created_at": "2026-06-26T12:00:00Z",
        "findings_count": 5
    }
]
```

### Requirement: Error Response Format
All error responses SHALL use a consistent JSON structure.

```json
{
    "error": "bad_request",
    "detail": "Field 'repo_url' is required"
}
```

Error codes:
| HTTP Status | `error` value | Typical cause |
|-------------|---------------|---------------|
| 400 | `bad_request` | Missing or invalid fields in request body |
| 404 | `not_found` | Review ID not found |
| 409 | `conflict` | Invalid state transition (e.g., cancel completed review) |
| 500 | `internal_error` | Unexpected server error |

#### Scenario: Bad request
- GIVEN a request with missing required fields
- WHEN the server returns a 400 error
- THEN the response body is {"error": "bad_request", "detail": "Field 'repo_url' is required"}

#### Scenario: Not found
- GIVEN a request for a non-existent review ID
- WHEN the server returns a 404 error
- THEN the response body is {"error": "not_found", "detail": "Review not found"}

#### Scenario: Conflict
- GIVEN a request to cancel a completed review
- WHEN the server returns a 409 error
- THEN the response body is {"error": "conflict", "detail": "review already completed"}

#### Scenario: Internal error
- GIVEN an unexpected server failure
- WHEN the server returns a 500 error
- THEN the response body is {"error": "internal_error", "detail": "..."}
