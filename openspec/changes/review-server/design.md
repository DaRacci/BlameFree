# Design: HTTP Review Server (crb-server)

## 1. Overview

The review server exposes the multi-agent code review pipeline as a RESTful HTTP API. When a client submits a review request (repo URL + PR number + diff), the server:

1. Acknowledges the request immediately with a `review_id` (status: `pending`).
2. Gathers repository context (shallow clone, branch checkout, tech-stack detection, CRG analysis).
3. Injects the context as template variables into agent prompts via `PromptLibrary`.
4. Runs the multi-agent consensus pipeline (SA, CL, AR, SEC agents + judge) concurrently.
5. Stores structured findings in the job record.
6. The client polls `GET /review/{id}` or fetches `GET /review/{id}/comments` for GitHub-compatible output.

## 2. Architecture

```
┌──────────────┐     POST /review      ┌─────────────────────────────────────────┐
│   Client     │ ──────────────────▶   │              axum Server                │
│ (Webhook/CLI │                       │  ┌──────────┐  ┌──────────────────────┐  │
│  /Dashboard) │ ◀──────────────────  │  │  Routes  │  │    AppState          │  │
│              │    GET /review/{id}   │  │          │  │  ┌────────────────┐  │  │
└──────────────┘                       │  │ review.rs│  │  │ ReviewJobStore │  │  │
                                       │  │ health.rs│  │  │ (Arc<RwLock>)  │  │  │
                                       │  └────┬─────┘  │  └────────────────┘  │  │
                                       │       │        │  ┌────────────────┐  │  │
                                       │       │        │  │ LLM Client     │  │  │
                                       │       │        │  │ (rig Provider) │  │  │
                                       │       │        │  └────────────────┘  │  │
                                       │       │        │  ┌────────────────┐  │  │
                                       │       │        │  │ PromptLibrary  │  │  │
                                       │       │        │  │ (Arc)          │  │  │
                                       │       │        │  └────────────────┘  │  │
                                       │       │        │  ┌────────────────┐  │  │
                                       │       │        │  │ Semaphore      │  │  │
                                       │       │        │  └────────────────┘  │  │
                                       └───────┼──────────────────────────────┘  │
                                               │
                                    ┌──────────▼──────────┐
                                    │  repo context        │
                                    │  gathering           │
                                    │  (context.rs)        │
                                    │                      │
                                    │  1. git clone/check  │
                                    │  2. tech-stack detect│
                                    │  3. module analysis  │
                                    │  4. CRG detect(opt)  │
                                    └──────────┬───────────┘
                                               │ context as
                                               │ template vars
                                               ▼
                                    ┌──────────────────────────┐
                                    │  crb-consensus::         │
                                    │  run_consensus()         │
                                    │                          │
                                    │  ┌──▶ SA Agent ──┐       │
                                    │  │──▶ CL Agent ──┤       │
                                    │  │──▶ AR Agent ──┤       │
                                    │  │──▶ SEC Agent ─┤       │
                                    │  └───────────────┘       │
                                    │         │                │
                                    │         ▼                │
                                    │  Judge + Aggregation     │
                                    └──────────┬───────────────┘
                                               │ findings
                                               ▼
                                    ┌──────────────────────────┐
                                    │  ReviewJobStore          │
                                    │  status -> "complete"     │
                                    │  findings -> [...results] │
                                    └──────────────────────────┘
```

## 3. API Design

### 3.1 Endpoint Contracts

| Method | Path | Request Body | Response | Description |
|--------|------|-------------|----------|-------------|
| POST | `/review` | `ReviewRequest` JSON | `ReviewResponse` (202) | Submit a PR for review. Returns immediately with `review_id`. |
| GET | `/review/{id}` | — | `ReviewStatusResponse` (200) | Check review status and get findings when complete. |
| GET | `/review/{id}/comments` | — | `Vec<ReviewComment>` (200) | Get findings in GitHub-compatible format. |
| GET | `/health` | — | `HealthResponse` (200) | Server health check. |
| POST | `/review/{id}/cancel` | — | `CancelResponse` (200) | Cancel a running review. |
| GET | `/reviews` | — | `Vec<ReviewSummary>` (200) | List recent reviews. |

### 3.2 Request Types

```rust
/// Request to submit a PR for review.
#[derive(Debug, Deserialize)]
pub struct ReviewRequest {
    /// GitHub repo URL: https://github.com/owner/repo
    pub repo_url: String,
    /// PR number to review.
    pub pr_number: u32,
    /// Optional pre-fetched diff (if not provided, server fetches it).
    #[serde(default)]
    pub diff: Option<String>,
    /// Optional base branch for diff generation (default: main).
    #[serde(default = "default_base_branch")]
    pub base_branch: String,
}

fn default_base_branch() -> String {
    "main".to_string()
}
```

### 3.3 Response Types

```rust
/// Response to a successful review submission.
#[derive(Debug, Serialize)]
pub struct ReviewResponse {
    pub review_id: Uuid,
    pub status: ReviewStatus,
}

/// Status of a review job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReviewStatus {
    Pending,
    Processing,
    Complete,
    Failed(String),  // error message
    Cancelled,
}

/// Full status response for a review.
#[derive(Debug, Serialize)]
pub struct ReviewStatusResponse {
    pub review_id: Uuid,
    pub status: ReviewStatus,
    pub findings: Vec<ReviewFinding>,
    pub metrics: ReviewMetrics,
    pub context: RepoContext,
}

/// A single review finding from the pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewFinding {
    pub file: Option<String>,
    pub line: Option<u32>,
    pub body: String,
    pub severity: String,
    pub rule_code: Option<String>,
    pub suggestion: Option<String>,
    /// Which agent role found this (SA/CL/AR/SEC).
    pub source_role: String,
}

/// Aggregated review metrics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReviewMetrics {
    pub total_findings: usize,
    pub critical_count: usize,
    pub high_count: usize,
    pub medium_count: usize,
    pub low_count: usize,
    pub by_role: HashMap<String, usize>,
}

/// GitHub-compatible comment format.
#[derive(Debug, Serialize)]
pub struct ReviewComment {
    pub file: String,
    pub line: u32,
    pub body: String,
    pub severity: String,
    pub rule_code: Option<String>,
    pub suggestion: Option<String>,
}

/// Health check response.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_secs: u64,
}

/// Summary of a review for listing.
#[derive(Debug, Serialize)]
pub struct ReviewSummary {
    pub review_id: Uuid,
    pub repo_url: String,
    pub pr_number: u32,
    pub status: ReviewStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub findings_count: usize,
}
```

### 3.4 Error Responses

All errors return JSON with a consistent structure:

```rust
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub detail: Option<String>,
}
```

HTTP status codes:
- `400 Bad Request` — malformed input or missing required fields
- `404 Not Found` — unknown `review_id`
- `409 Conflict` — trying to cancel a completed review
- `500 Internal Server Error` — unexpected server error

## 4. State Management

### 4.1 AppState

```rust
/// Shared application state injected into all axum route handlers.
#[derive(Clone)]
pub struct AppState {
    /// LLM client from rig-core (OpenRouter/OpenAI/Anthropic).
    pub client: Arc<openai::Client>,
    /// Prompt library for agent role prompts.
    pub prompt_lib: Arc<PromptLibrary>,
    /// In-memory review job store.
    pub store: Arc<ReviewJobStore>,
    /// Concurrency semaphore for LLM API calls.
    pub sem: Arc<Semaphore>,
    /// Repos cache directory for cloned repos.
    pub repos_cache: PathBuf,
    /// Prompts directory path.
    pub prompts_dir: Option<PathBuf>,
    /// Model name for reviewer agents.
    pub model: String,
    /// Judge model name.
    pub judge_model: String,
    /// Agent roles to use (SA, CL, AR, SEC).
    pub roles: Vec<String>,
    /// Max findings per agent.
    pub max_findings: usize,
    /// Optional ruleset.
    pub ruleset: Option<RuleSet>,
    /// Server start timestamp.
    pub start_time: tokio::time::Instant,
}
```

### 4.2 Review Job Lifecycle

```
Pending ──▶ Processing ──▶ Complete
                │
                ├──▶ Failed
                │
                └──▶ Cancelled
```

- **Pending:** Request received, waiting for semaphore or queued.
- **Processing:** Repo context gathering and/or agent execution in progress.
- **Complete:** All agents finished, findings stored.
- **Failed:** Unrecoverable error (e.g., repo clone failure, auth error).
- **Cancelled:** Client sent `POST /review/{id}/cancel`.

### 4.3 ReviewJobStore

```rust
/// In-memory store for review jobs, backed by a HashMap.
pub struct ReviewJobStore {
    jobs: Arc<RwLock<HashMap<Uuid, ReviewJob>>>,
    order: Arc<RwLock<Vec<Uuid>>>,  // insertion order for listing
    max_jobs: usize,
}

pub struct ReviewJob {
    pub id: Uuid,
    pub request: ReviewRequest,
    pub status: ReviewStatus,
    pub findings: Vec<ReviewFinding>,
    pub context: Option<RepoContext>,
    pub metrics: ReviewMetrics,
    pub created_at: SystemTime,
    pub completed_at: Option<SystemTime>,
}
```

## 5. Repo Context Gathering

### 5.1 Workflow

When a review request arrives with `repo_url` and `pr_number`:

1. **Parse URL** — Extract `owner`, `repo` from `https://github.com/{owner}/{repo}/pull/{num}`.
2. **Clone or fetch** — Shallow clone (`--depth 1`) to `{repos_cache}/{owner}_{repo}`. If already cached, `git fetch` to update.
3. **Checkout PR** — Fetch the PR branch: `git fetch origin pull/{pr_number}/head:{branch}` and check it out.
4. **Detect tech stack** — Scan for language indicators: `Cargo.toml` -> Rust, `package.json` -> Node.js, `requirements.txt` -> Python, `go.mod` -> Go, `.csproj` -> C#, etc.
5. **Analyze modules** — List top-level source directories and key entry points.
6. **CRG integration (optional)** — If `code-review-graph` is available on `PATH`, run `code-review-graph detect-changes` to get call-graph context for changed files.
7. **Gather diff** — Run `git diff {base_branch}...HEAD` to get the diff (or use the provided diff from the request).

### 5.2 RepoContext Struct

```rust
/// Context information gathered from the repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoContext {
    pub owner: String,
    pub repo: String,
    pub pr_number: u32,
    pub language: String,
    pub tech_stack: Vec<String>,
    pub modules: Vec<String>,
    pub changed_files: Vec<String>,
    pub diff: String,
    pub call_graph: Option<String>,
}

impl RepoContext {
    /// Convert to template variables for PromptLibrary rendering.
    pub fn to_template_vars(&self) -> HashMap<&str, &str> {
        let mut vars = HashMap::new();
        vars.insert("repo", &format!("{}/{}", self.owner, self.repo));
        vars.insert("language", &self.language);
        vars.insert("tech_stack", &self.tech_stack.join(", "));
        vars.insert("modules", &self.modules.join(", "));
        vars.insert("changed_files", &self.changed_files.join(", "));
        vars
    }
}
```

### 5.3 Tech-Stack Detection

Simple file-based heuristics (no external analyzer required):

| File | Language | Tech Stack |
|------|----------|------------|
| `Cargo.toml` | Rust | Tokio, Axum, Serde, etc. (from dependencies) |
| `package.json` | JavaScript/TypeScript | React, Express, etc. |
| `requirements.txt` / `pyproject.toml` | Python | Django, FastAPI, etc. |
| `go.mod` | Go | Gin, Echo, etc. |
| `pom.xml` / `build.gradle` | Java | Spring, Micronaut, etc. |
| `.csproj` | C# | ASP.NET, etc. |
| `Gemfile` | Ruby | Rails, Sinatra, etc. |

## 6. Prompt Template Integration

### 6.1 Template Variables

The server injects the following variables into `PromptLibrary::render()`:

| Variable | Source | Example |
|----------|--------|---------|
| `{repo}` | `owner/repo` from URL | `facebook/react` |
| `{language}` | Tech-stack detection | `Rust` |
| `{tech_stack}` | Dependency analysis | `Tokio, Axum, Serde` |
| `{modules}` | Source directory analysis | `src/routes, src/models, src/db` |
| `{changed_files}` | Git diff output | `src/routes/review.rs, src/models.rs` |

### 6.2 Prompt Example

A custom SA prompt file (`prompts/experiments/EXP-013/sa.md`) might contain:

```
You are a static analysis specialist reviewing {repo} ({language}).
The project uses: {tech_stack}
Key modules changed: {modules}

Focus your analysis on the changed files listed above.
Analyze the provided code diff for potential bugs, code smells,
and violations of best practices. Respond with a JSON array of findings.
```

## 7. CLI Arguments

```bash
crb-server \
    --port 8080 \
    --prompts-dir prompts/experiments/EXP-013 \
    --model deepseek/deepseek-v4-flash \
    --judge-model deepseek/deepseek-v4-flash \
    --concurrency 4 \
    --rules-dir .crb/rules/ \
    --repos-cache /tmp/crb-repos \
    --max-jobs 100
```

| Flag | Env | Default | Description |
|------|-----|---------|-------------|
| `--port` | `CRB_PORT` | `8080` | HTTP listen port |
| `--host` | `CRB_HOST` | `0.0.0.0` | Listen address |
| `--model` | `MODEL` | `gpt-4o` | LLM model for reviewers |
| `--judge-model` | `JUDGE_MODEL` | `gpt-4o-mini` | LLM model for judge |
| `--prompts-dir` | `PROMPTS_DIR` | `prompts/builtin` | Prompt library directory |
| `--concurrency` | `CONCURRENCY` | `4` | Max concurrent LLM calls |
| `--rules-dir` | `RULES_DIR` | `.crb/rules/` | Rule directory |
| `--repos-cache` | `REPOS_CACHE` | `/tmp/crb-repos` | Repo clone cache |
| `--max-jobs` | `MAX_JOBS` | `100` | Max in-memory review jobs |
| `--roles` | `ROLES` | `SA,CL,AR,SEC` | Agent roles to use |
| `--max-findings` | `MAX_FINDINGS` | `20` | Max findings per agent |

## 8. Dependencies

### crb-server/Cargo.toml

```toml
[package]
name = "crb-server"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "crb-server"
path = "src/main.rs"

[dependencies]
crb-agents = { path = "../crb-agents" }
crb-judge = { path = "../crb-judge" }
crb-consensus = { path = "../crb-consensus" }
crb-reporting = { path = "../crb-reporting" }
crb-tools = { path = "../crb-tools" }
crb-rules = { path = "../crb-rules" }
rig-core = { workspace = true }
tokio = { workspace = true, features = ["rt-multi-thread", "macros", "sync"] }
axum = "0.7"
tower-http = { version = "0.5", features = ["cors", "trace"] }
uuid = { version = "1", features = ["v4", "serde"] }
serde = { workspace = true }
serde_json = { workspace = true }
clap = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true, features = ["env-filter"] }
anyhow = { workspace = true }
chrono = { version = "0.4", features = ["serde"] }
tokio-util = "0.7"
```

### Workspace root additions

```toml
[workspace]
members = ["crates/*"]

[workspace.dependencies]
# ... existing deps ...
# axum and uuid are NOT workspace-level — they're specific to crb-server
```

## 9. Error Handling

| Scenario | HTTP Status | Behavior |
|----------|-------------|----------|
| Missing `repo_url` | 400 | Return error with field validation message |
| Invalid `pr_number` | 400 | Return error for non-positive integers |
| Repo clone fails (no network) | 202 (job failed) | Job enters `Failed` status with error detail |
| Repo not found (404 from git) | 202 (job failed) | Job enters `Failed` status |
| LLM API call fails | Transient -> retry | Agent returns empty findings with warning; job completes |
| Review not found | 404 | Return error `"review not found"` |
| Cancel already-completed review | 409 | Return error `"review already completed"` |
| Malformed UUID in path | 400 | Return error `"invalid review id"` |

## 10. CORS Configuration

The server enables CORS for all origins (configurable via flag in future) using `tower-http::cors::CorsLayer`:

```rust
let cors = CorsLayer::new()
    .allow_origin(Any)
    .allow_methods([Method::GET, Method::POST])
    .allow_headers([header::CONTENT_TYPE]);
```

## 11. Key Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| HTTP framework | `axum 0.7` | Tokio-native, works with rig-core's async. Lightweight, well-documented |
| CORS | `tower-http` | Standard tower middleware, no extra deps |
| Review IDs | `uuid v4` | Unforgeable, no collision, standard format |
| Job store | In-memory `HashMap<RwLock>` | Simple, no DB dependency. Acceptable for single-instance |
| Async processing | `tokio::spawn` + status update | Non-blocking request handlers. Client polls for completion |
| Repo cache | Git shallow clone to temp dir | Avoids re-cloning for repeated reviews of same repo |
| Diff source | Inline `diff` field in request | Client can provide diff directly; server falls back to git |
| Template injection | `PromptLibrary::render()` | Reuses existing template infrastructure. No new prompt system |
| Concurrency | `tokio::sync::Semaphore` | Reuses same pattern from crb-harness. Single shared throttle |
