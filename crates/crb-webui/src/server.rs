//! Axum server setup, shared state, and router.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{State};
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::body::Body;
use axum::routing::{get, post};
use axum::Router;
use tokio::sync::{broadcast, RwLock};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::auth::SessionStore;
use crate::config::WebUiConfig;
use crate::events::DashboardEvent;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    /// Directory containing per-PR JSON result files.
    pub output_dir: PathBuf,
    /// Directory containing datasets.
    pub dataset_dir: PathBuf,
    /// Directory of the static frontend files.
    pub static_dir: PathBuf,
    /// Comma-separated list of available models.
    pub models: String,
    /// Path to the code-review-benchmark directory (must contain offline/).
    pub benchmark_dir: Option<PathBuf>,
    /// Active (running) benchmark runs.
    pub active_runs: Arc<RwLock<HashMap<String, ActiveRun>>>,
    /// Active replay operations.
    pub replays: Arc<RwLock<HashMap<String, ReplayState>>>,
    /// Web UI configuration (includes optional OAuth).
    pub config: WebUiConfig,
    /// Session store for OAuth-authenticated users.
    pub session_store: SessionStore,
    /// Octocrab GitHub API client (authenticated via GITHUB_TOKEN env var).
    pub octocrab: octocrab::Octocrab,
    /// Path to the server log file.
    pub log_file: PathBuf,
}

/// State for an actively running benchmark.
pub struct ActiveRun {
    /// When the run was started (Unix timestamp).
    pub created_at: u64,
    /// The config used to start this run.
    pub config: crate::api::BenchmarkConfig,
    /// Broadcast channel for SSE events.
    pub tx: broadcast::Sender<DashboardEvent>,
    /// Number of completed PRs.
    pub completed_prs: usize,
    /// Total number of PRs.
    pub total_prs: usize,
    /// Whether the run has finished.
    pub finished: bool,
}

/// State of a replay operation.
pub struct ReplayState {
    pub status: String,        // "running", "completed", "failed"
    pub progress_pct: u32,
    pub completed_prs: u32,
    pub total_prs: u32,
    pub message: String,
    pub output_dir: PathBuf,
}

impl AppState {
    pub fn new(
        output_dir: PathBuf,
        dataset_dir: PathBuf,
        static_dir: PathBuf,
        models: String,
        benchmark_dir: Option<PathBuf>,
        config: WebUiConfig,
        octocrab: octocrab::Octocrab,
        session_store: SessionStore,
        log_file: PathBuf,
    ) -> Self {
        Self {
            output_dir,
            dataset_dir,
            static_dir,
            models,
            benchmark_dir,
            active_runs: Arc::new(RwLock::new(HashMap::new())),
            replays: Arc::new(RwLock::new(HashMap::new())),
            config,
            session_store,
            octocrab,
            log_file,
        }
    }
}

/// Start the axum HTTP server.
pub async fn start(state: AppState, port: u16) -> anyhow::Result<()> {
    let api_router = Router::new()
        .route("/api/runs", get(crate::api::list_runs).post(crate::api::start_run))
        .route("/api/runs/:id", get(crate::api::get_run))
        .route("/api/runs/:id/live", get(crate::api::live_stream))
        .route("/api/config", get(crate::api::get_config))
        .route("/api/config/datasets", get(crate::api::list_datasets))
        .route("/api/runs/:id/logs", get(crate::api::list_logs))
        .route("/api/runs/:id/logs/:pr_key/:role", get(crate::api::get_agent_log))
        .route("/api/runs/:id/prs/:pr_key", get(crate::api::get_pr_agents))
        .route("/api/runs/:id/replay", post(crate::api::start_replay))
        .route("/api/runs/:id/replay/status", get(crate::api::replay_status))
        .route("/api/runs/:id/pr-detail/:pr_key", get(crate::api::get_pr_detail))
        .route("/api/datasets/:id/prs", get(crate::api::list_dataset_prs))
        // Ad-hoc review endpoints
        .route("/api/adhoc/review", post(crate::api::start_adhoc_review))
        .route("/api/adhoc/runs", get(crate::api::list_adhoc_runs))
        .route("/api/adhoc/runs/:id", get(crate::api::get_adhoc_run))
        .route("/api/adhoc/prs/:owner/:repo", get(crate::api::list_repo_prs))
        // Admin endpoints
        .route("/api/admin/logs", get(crate::api::get_logs))
        .route("/api/admin/logs/stream", get(crate::api::get_logs_stream));

    // Build router: merge all routes first, then apply state and layers
    let mut app = Router::new().merge(api_router);

    // If OAuth is configured, add authentication routes
    if state.config.oauth.is_some() {
        tracing::info!("OAuth is enabled — adding authentication routes");
        app = app.merge(crate::auth::router());
    } else {
        tracing::info!("OAuth is disabled — skipping authentication routes");
    }

    let app = app
        .fallback(static_or_index)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("Listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Serve static files or fall back to index.html for SPA routing.
async fn static_or_index(
    State(state): State<AppState>,
    uri: Uri,
) -> Response {
    let static_dir = state.static_dir.clone();
    let file_path = static_dir.join(uri.path().trim_start_matches('/'));

    // Try to serve the file directly
    if file_path.exists() && file_path.is_file() {
        match tokio::fs::read(&file_path).await {
            Ok(data) => {
                // Determine content type from extension
                let content_type = match file_path.extension().and_then(|e| e.to_str()) {
                    Some("html") => "text/html; charset=utf-8",
                    Some("js") => "application/javascript",
                    Some("wasm") => "application/wasm",
                    Some("css") => "text/css",
                    Some("json") => "application/json",
                    Some("png") => "image/png",
                    Some("svg") => "image/svg+xml",
                    Some("ico") => "image/x-icon",
                    _ => "application/octet-stream",
                };
                return Response::builder()
                    .header("Content-Type", content_type)
                    .body(Body::from(data))
                    .unwrap();
            }
            Err(_) => {
                return StatusCode::NOT_FOUND.into_response();
            }
        }
    }

    // SPA fallback: serve index.html
    let index_path = static_dir.join("index.html");
    match tokio::fs::read(&index_path).await {
        Ok(data) => Response::builder()
            .header("Content-Type", "text/html; charset=utf-8")
            .body(Body::from(data))
            .unwrap(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            format!(
                "Static directory '{}' not found. Build the frontend or set --static-dir.",
                static_dir.display()
            ),
        )
            .into_response(),
    }
}
