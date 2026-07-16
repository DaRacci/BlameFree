# Proposal: Replay and Log Viewing

**Change ID:** replay-logs
**Status:** Draft
**Author:** Hermes Agent
---

## Why

Users reviewing past benchmark runs currently have no way to see the LLM's thought process or raw agent logs. The run detail page only shows aggregate metrics (F1, precision, recall) and per-PR result summaries. When investigating failures or low scores, users must SSH into the server and manually inspect cache files. This is cumbersome and blocks iterative debugging.

Adding a "Logs" tab on the run detail page showing agent prompts/responses, plus a "Replay" mode that replays the run from cache without API calls, gives users full visibility into agent behavior and lets them reproduce results instantly.

## What Changes

- **Log viewing**: A new "Logs" tab on the run detail page showing per-PR agent prompts and responses loaded from the run's cache directory.
- **Replay mode**: A "Replay Run" button that re-executes the run using the content-addressed cache (no API calls), producing identical results verified by hash comparison, then displays a comparison table.

## Non-goals

- Real-time streaming of past runs (SSE for current runs is already implemented).
- Editing or forking runs.
- Modifying the cache format itself.
