# SSE Event Format Specification

## Transport

- Standard SSE with `text/event-stream` content type
- Each event is a JSON object prefixed with `data: ` and suffixed with `\n\n`
- No `event:` field — client uses JSON `event` field to discriminate

## Event Types

### agent_started

```json
{
  "event": "agent_started",
  "pr_key": "discourse-7",
  "role": "SA"
}
```

### agent_chunk

```json
{
  "event": "agent_chunk",
  "role": "SA",
  "chunk": "Analyzing the PR diff... Found potential issue with color function..."
}
```

### agent_finished

```json
{
  "event": "agent_finished",
  "role": "SA",
  "findings": 3,
  "success": true
}
```

### pr_completed

```json
{
  "event": "pr_completed",
  "pr_key": "discourse-7",
  "metrics": {
    "true_positives": 3,
    "false_positives": 6,
    "false_negatives": 0,
    "precision": 0.333,
    "recall": 1.0,
    "f1": 0.5
  },
  "cost": 0.0032,
  "total_tokens": 3500,
  "findings_count": 0,
  "agent_calls": 4
}
```

### run_progress

```json
{
  "event": "run_progress",
  "completed_prs": 5,
  "total_prs": 10,
  "elapsed_secs": 185.3,
  "total_cost": 0.047,
  "current_pr": "discourse-12"
}
```

### run_finished

```json
{
  "event": "run_finished",
  "total_prs": 10,
  "aggregated": {
    "total_tp": 30,
    "total_fp": 60,
    "total_fn": 5,
    "precision": 0.333,
    "recall": 0.857,
    "f1": 0.48
  },
  "total_cost": 0.12,
  "total_tokens": 52000,
  "total_agent_calls": 40
}
```

## Parsing Contract

Each line from `crb-harness --dashboard-events` stdout is a complete JSON event.
Backend forwards these to SSE clients after minimal parsing (validate JSON, add `data: ` prefix).
If a line is not valid JSON, it is silently dropped.
