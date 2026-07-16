# Proposal: HTTP Review Server (crb-server)

**Change ID:** review-server
**Status:** Draft
**Author:** Hermes Agent
**Date:** 2026-06-26

## Summary

Introduce `crb-server`, a new member crate of the review-harness workspace that exposes the existing multi-agent review pipeline as an HTTP API. This enables remote code review requests via a RESTful interface, with automatic repository context gathering (clone, branch checkout, tech-stack detection, module analysis) injected into agent prompts via the `PromptLibrary` template system.

## Why

The existing crb-harness CLI operates in batch mode on pre-loaded datasets. It has no facility for on-demand per-PR review requests from external tools (GitHub webhooks, CI pipelines, IDE plugins, or UI dashboards).

## What Changes

Introduce crb-server, a new member crate of the review-harness workspace that exposes the existing multi-agent review pipeline as an HTTP API (axum-based). Includes POST /review, GET /review/{id}, GET /review/{id}/comments, GET /health, POST /review/{id}/cancel, GET /reviews endpoints, repo context gathering (clone, checkout, tech-stack detection, module analysis), and job-based async processing.

## Motivation

The existing `crb-harness` CLI operates in batch mode on pre-loaded golden-comment datasets. It works well for benchmarking and CI validation but has no facility for on-demand, per-PR review requests from external tools (GitHub webhooks, CI pipelines, IDE plugins, or UI dashboards).

A review server unlocks several use cases:

1. **Continuous review pipeline** — GitHub webhook sends PR data to the server, which clones the repo, gathers context, runs the multi-agent review, and returns findings as GitHub-compatible comments.
2. **On-demand review API** — CI scripts or developer tools can POST a diff + repo URL and get structured feedback with file/line annotations and severity classifications.
3. **Interactive exploration** — A web UI or dashboard can poll review status and browse findings grouped by agent role.
4. **Service composition** — The server's health check and review endpoints can be composed into larger automation workflows.

The architecture reuses the existing agent infrastructure (`crb-agents`, `crb-consensus`, `crb-judge`, `crb-reporting`) and extends it with repository context gathering, job-based async processing, and a clean REST API.

## Scope

- **In scope:**
  - HTTP server crate `crb-server` with axum-based routing
  - `POST /review` — submit a PR diff for review
  - `GET /review/{id}` — poll review status and findings
  - `GET /review/{id}/comments` — GitHub-compatible comment output
  - `GET /health` — health check endpoint
  - `POST /review/{id}/cancel` — cancel a running review
  - `GET /reviews` — list recent reviews with status
  - Repository context gathering (shallow clone, branch checkout, tech-stack detection, module mapping, CRG integration)
  - Template variable injection (`{repo}`, `{language}`, `{tech_stack}`, `{modules}`) into agent prompts
  - In-memory review job store with lifecycle states: `pending` -> `processing` -> `complete | failed`
  - CLI flags for port, prompts dir, model, concurrency, rules dir
  - CORS support via `tower-http`

- **Out of scope (initial release):**
  - Persistent review store (database-backed)
  - Authentication / API keys
  - Webhook signature verification
  - Batch review requests (async multiple PRs in one call)
  - Streaming response (SSE for real-time findings)
  - Rate limiting beyond the concurrency semaphore
  - HTTPS / TLS termination
  - OpenAPI / Swagger doc generation

## Key Design Decisions

1. **axum over actix-web** — axum is tokio-native (matching the workspace), lightweight, and works directly with rig-core's async agent calls. No need for a separate tokio runtime.
2. **In-memory store (HashMap + Arc<RwLock>)** — Simple, fast, matches the single-process deployment model. Persistence via restart is acceptable for MVP.
3. **Job-based async processing** — Reviews run as `tokio::spawn` tasks with status updates. The HTTP handler returns immediately with a `review_id`; the client polls for completion.
4. **Repo context as template variables** — Context is gathered before agent invocation and injected via `PromptLibrary::render()` with template variables `{repo}`, `{language}`, `{tech_stack}`, `{modules}`. Works with both built-in and file-based prompts.
5. **Reuse of existing crates** — `crb-agents::build_agent()`, `crb-consensus::run_consensus()`, `crb-judge`, `crb-reporting::GoldenCommentEntry` (adapted for server response). No duplication of agent or judge logic.
6. **Concurrency via existing semaphore** — The `--concurrency` flag controls how many LLM agent calls run simultaneously, reusing the same `tokio::sync::Semaphore` pattern from `crb-harness`.

## Directory Structure

```
review-harness/
├── Cargo.toml                     # [workspace] members = ["crates/*"]
└── crates/
    └── crb-server/                # HTTP review server
        ├── Cargo.toml             # deps: axum, tower-http, uuid, + workspace crates
        └── src/
            ├── main.rs            # axum server entry, route definitions, CLI parsing
            ├── routes/
            │   ├── mod.rs         # route module exports
            │   ├── review.rs     # POST /review, GET /review/{id}, GET /review/{id}/comments
            │   └── health.rs     # GET /health
            ├── state.rs          # AppState (shared client, prompt lib, store, semaphore)
            ├── store.rs          # ReviewJobStore — in-memory job lifecycle
            ├── context.rs        # Repo context gathering (clone, checkout, CRG, analysis)
            └── models.rs         # Request/response types, GitHub-compatible comment struct
```
