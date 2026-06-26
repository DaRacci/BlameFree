# Tasks: HTTP Review Server (crb-server)

## Phase 1 — Project Setup & Types

- [ ] **1.1 Create crate scaffolding**
  - Create `crates/crb-server/` directory structure
  - Write `Cargo.toml` with dependencies (axum, tower-http, uuid, chrono, + workspace crates)
  - Create `src/main.rs`, `src/routes/mod.rs`, `src/routes/review.rs`, `src/routes/health.rs`
  - Create `src/state.rs`, `src/store.rs`, `src/context.rs`, `src/models.rs`
  - Update workspace `Cargo.toml` (already has `members = ["crates/*"]`, no change needed)

- [ ] **1.2 Define models in `src/models.rs`**
  - `ReviewRequest` — input struct with `repo_url`, `pr_number`, optional `diff`, `base_branch`
  - `ReviewResponse` — output with `review_id: Uuid`, `status: ReviewStatus`
  - `ReviewStatus` — enum: `Pending`, `Processing`, `Complete`, `Failed(String)`, `Cancelled`
  - `ReviewStatusResponse` — full status with findings, metrics, context
  - `ReviewFinding` — file, line, body, severity, rule_code, suggestion, source_role
  - `ReviewMetrics` — count totals by severity, breakdown by role
  - `ReviewComment` — GitHub-compatible comment format
  - `HealthResponse` — status, version, uptime
  - `ReviewSummary` — for list endpoint
  - `ErrorResponse` — consistent error structure
  - `RepoContext` — owner, repo, pr_number, language, tech_stack, modules, changed_files, diff

- [ ] **1.3 Implement serde Serialize/Deserialize for all types**
  - Derive `Serialize`, `Deserialize`, `Debug`, `Clone` as appropriate
  - Add `#[serde(default)]` for optional fields
  - `chrono` time fields with correct format annotations

## Phase 2 — In-Memory Job Store

- [ ] **2.1 Implement `ReviewJobStore` in `src/store.rs`**
  - `ReviewJob` struct with id, request, status, findings, context, metrics, timestamps
  - `ReviewJobStore::new(max_jobs: usize)` constructor
  - `insert(request) -> Uuid` — create new pending job, return its UUID
  - `get(id) -> Option<ReviewJob>` — lookup by UUID
  - `update_status(id, status)` — transition job status
  - `set_findings(id, findings, context, metrics)` — store results on completion
  - `cancel(id) -> Result<(), Error>` — cancel a pending/processing job
  - `list(limit: usize) -> Vec<ReviewSummary>` — list recent jobs ordered by creation time
  - Enforce `max_jobs` with LRU eviction of oldest completed jobs when limit exceeded

- [ ] **2.2 Unit tests for store**
  - Job creation and UUID uniqueness
  - Status transitions (Pending → Processing → Complete)
  - Invalid transitions (Complete → Processing should fail)
  - Cancel of pending vs completed jobs
  - Max jobs eviction
  - Thread safety under concurrent access

## Phase 3 — AppState & Route Setup

- [ ] **3.1 Implement `AppState` in `src/state.rs`**
  - Fields: client, prompt_lib, store, sem, repos_cache, prompts_dir, model, judge_model, roles, max_findings, ruleset, start_time
  - `AppState::from_cli(args: &CliArgs) -> Result<Self>` constructor
  - Load `PromptLibrary` from `--prompts-dir` (same pattern as crb-harness)
  - Create LLM client from environment (`rig-core::providers::openai::Client::from_env()`)
  - Create `tokio::sync::Semaphore` with `--concurrency`
  - Load `RuleSet` from `--rules-dir` if not skipped

- [ ] **3.2 Define CLI args in `src/main.rs`**
  - `CliArgs` struct with clap derive (port, host, model, judge_model, prompts_dir, concurrency, rules_dir, repos_cache, max_jobs, roles, max_findings)
  - Implement `FromStr` for `ReviewStatus` if needed for query params

- [ ] **3.3 Set up axum router in `src/main.rs`**
  - `GET /health` → `health::health_check`
  - `POST /review` → `review::submit_review`
  - `GET /review/{id}` → `review::get_review`
  - `GET /review/{id}/comments` → `review::get_comments`
  - `POST /review/{id}/cancel` → `review::cancel_review`
  - `GET /reviews` → `review::list_reviews`
  - Attach `CorsLayer` (allow all origins for MVP)
  - Attach `TraceLayer` for request logging
  - Start server with `tokio::net::TcpListener` + `axum::serve`

- [ ] **3.4 Implement health route in `src/routes/health.rs`**
  - Return `HealthResponse` with status "ok", crate version, uptime seconds
  - Simple GET handler, no async work needed

## Phase 4 — Review Submit & Process

- [ ] **4.1 Implement `POST /review` in `src/routes/review.rs`**
  - Parse `ReviewRequest` from JSON body
  - Validate: `repo_url` must be non-empty, `pr_number` > 0
  - Create review job in store (status: Pending)
  - Spawn async task for processing:
    1. Update status to Processing
    2. Gather repo context (context.rs)
    3. Run consensus pipeline with context-injected prompts
    4. Store findings and metrics
    5. Update status to Complete (or Failed)
  - Return `ReviewResponse` with 202 Accepted
  - Use `tracing::info_span!` for job lifecycle logging

- [ ] **4.2 Implement review processing pipeline**
  - Extract `owner`, `repo`, `pr_number` from URL (reuse `extract_pr_info` or similar)
  - Clone/cache repo via `context::gather_repo_context()`
  - Detect tech stack and modules
  - Run `crb-consensus::run_consensus()` or `crb-agents::build_agent()` + `crb-judge`
  - Convert `ConsensusReport` / agent findings to `Vec<ReviewFinding>`
  - Compute `ReviewMetrics` from findings

- [ ] **4.3 Implement `GET /review/{id}`**
  - Extract UUID from path (`Uuid::parse_str`)
  - Look up in store
  - If not found, return 404
  - Return `ReviewStatusResponse` with current status, findings (if complete), metrics, context

- [ ] **4.4 Implement `GET /review/{id}/comments`**
  - Extract UUID from path
  - Look up in store, return 404 if not found
  - If not complete, return 409 Conflict with message "review still processing"
  - Convert `ReviewFinding` list to `Vec<ReviewComment>` (drop source_role, ensure file/line are present)
  - Return JSON array of comments

- [ ] **4.5 Implement `POST /review/{id}/cancel`**
  - Extract UUID
  - Look up and check status
  - Can only cancel `Pending` or `Processing` jobs
  - Update status to `Cancelled`
  - Return success message

- [ ] **4.6 Implement `GET /reviews`**
  - Read `?limit=20` query parameter (default 10, max 100)
  - Call `store.list(limit)`
  - Return JSON array of `ReviewSummary`

## Phase 5 — Repo Context Gathering

- [ ] **5.1 Implement `context.rs` — `gather_repo_context()`**
  - Parse `owner`, `repo` from GitHub URL
  - Construct cache path: `{repos_cache}/{owner}_{repo}`
  - Check if repo already cached:
    - If yes: `git fetch origin` to update
    - If no: `git clone --depth 1 https://github.com/{owner}/{repo}.git {cache_path}`
  - Fetch PR branch: `git fetch origin pull/{pr_number}/head:{branch_name}`
  - Checkout PR branch
  - Run `git diff {base_branch}...HEAD` for diff (or use provided diff)
  - Generate list of changed files from diff
  - Detect language and tech stack from filesystem
  - Analyze key modules (source directories, entry points)
  - Optionally run CRG: `code-review-graph detect-changes --repo {path}`

- [ ] **5.2 Implement tech-stack detection**
  - Scan repo root for well-known files:
    - `Cargo.toml` → Rust + parse key dependencies
    - `package.json` → Node.js + parse dependencies
    - `pyproject.toml` / `requirements.txt` → Python
    - `go.mod` → Go
    - `pom.xml` → Java/Maven
    - `build.gradle` → Java/Gradle
    - `.csproj` → C#
    - `Gemfile` → Ruby
    - `CMakeLists.txt` → C/C++
  - Return detected language as string, tech stack as Vec<String>

- [ ] **5.3 Implement module analysis**
  - List top-level directories under `src/`, `lib/`, `app/`, `cmd/` etc.
  - Identify entry points (`main.rs`, `main.go`, `index.js`, `app.py`)
  - Return module names as Vec<String>

- [ ] **5.4 Implement CRG integration (optional)**
  - Check if `code-review-graph` is on `PATH`
  - If yes, run: `code-review-graph detect-changes --repo {path}`
  - Capture stdout as call graph context string
  - If CRG not found, log info and continue without it

- [ ] **5.5 Template variable injection**
  - Convert `RepoContext` to `HashMap<&str, &str>` for `PromptLibrary::render()`
  - Variables: `{repo}`, `{language}`, `{tech_stack}`, `{modules}`, `{changed_files}`
  - Inject into build_agent calls when creating reviewer agents

## Phase 6 — Integration & Pipeline Wiring

- [ ] **6.1 Wire consensus pipeline into server**
  - Create reviewer configs from `AppState.roles` and model settings
  - Create `crb-consensus::ReviewerConfig` for each role
  - Call `run_consensus()` with diff, goldens (empty for server — no golden comparison in API mode), reviewer configs, client, judge, rules preamble, prompt lib with template vars
  - Convert `ConsensusReport` agents findings into `Vec<ReviewFinding>`

- [ ] **6.2 Handle the "no golden comments" case**
  - In server mode, there are no golden comments to compare against
  - The consensus pipeline should run agents only, skip judge evaluation
  - Alternatively, pass empty goldens list → all findings are FPs → still get structured findings
  - Decision: run agents directly via `run_reviewers()`, skip consensus judge, collect findings as-is

- [ ] **6.3 Implement agent results → ReviewFinding conversion**
  - Map each `Finding` from agents to `ReviewFinding`
  - Add `source_role` based on which agent produced it
  - Group findings by severity for metrics
  - Compute `ReviewMetrics` counts

## Phase 7 — Testing

- [ ] **7.1 Unit tests for `store.rs`**
  - Concurrent inserts (stress test with 100 tasks)
  - Correct LRU eviction when max_jobs exceeded
  - Status transition validation
  - Cancel behavior

- [ ] **7.2 Unit tests for `context.rs`**
  - URL parsing: valid GitHub URLs, invalid URLs, edge cases
  - Tech-stack detection with mock filesystem (temp dirs)
  - Template variable generation from RepoContext

- [ ] **7.3 Integration tests for HTTP endpoints**
  - `POST /review` returns 202 with valid UUID
  - `GET /review/{id}` returns pending/processing/complete status
  - `GET /health` returns 200 with status "ok"
  - `GET /review/{nonexistent}` returns 404
  - `POST /review` with missing fields returns 400
  - `POST /review/{id}/cancel` works on pending jobs
  - `GET /reviews` returns list (empty or populated)

- [ ] **7.4 Integration test with mock LLM**
  - Create a mock `rig-core` client or agent that returns predefined findings
  - Submit a review with a known diff and repo URL
  - Verify findings are stored and returned correctly
  - Verify template variables are injected into prompts

## Phase 8 — Polish & Documentation

- [ ] **8.1 Tracing and observability**
  - Add `tracing` spans per review job, per agent call, per context-gathering step
  - Structured fields: `review_id`, `repo`, `pr_number`, `status`, `duration_ms`
  - Log agent findings and errors at appropriate levels
  - `TraceLayer` for HTTP request/response logging

- [ ] **8.2 Error handling hardening**
  - Graceful shutdown: catch SIGINT/SIGTERM, drain in-flight reviews, close listener
  - Timeout for repo cloning (60s)
  - Timeout for agent calls (120s, matching crb-consensus)
  - Retry logic for transient LLM failures (optional, could reuse from crb-harness)

- [ ] **8.3 Documentation**
  - Module-level doc comments on all public types and functions
  - README.md for crb-server with:
    - Quick start: `cargo run -p crb-server`
    - Example curl commands for each endpoint
    - Environment variable reference
    - Prompt template variable documentation
  - Add to workspace root README.md

- [ ] **8.4 CLI polish**
  - `--help` output with all flags described
  - Version flag (`--version`)
  - Sensible defaults for all flags
  - `--dry-run` flag that prints config and exits without starting the server

## Phase 9 — Future (Post-MVP)

- [ ] **9.1 Persistent storage** — SQLite or PostgreSQL backend for review jobs
- [ ] **9.2 Authentication** — Bearer token or API key validation
- [ ] **9.3 Webhook integration** — GitHub webhook receiver with signature verification
- [ ] **9.4 Streaming** — SSE endpoint for real-time findings as agents complete
- [ ] **9.5 Rate limiting** — Per-IP or per-token rate limiter
- [ ] **9.6 OpenAPI spec** — Auto-generated OpenAPI 3.0 documentation
- [ ] **9.7 Caching** — Cache repo context by `owner/repo` key to avoid re-cloning on repeated requests
- [ ] **9.8 Review templates** — Pre-defined review profiles (quick check, full audit, security-only)
