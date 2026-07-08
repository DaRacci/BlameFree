//! Axum server setup, shared state, and router.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use rustls::pki_types::UnixTime;
use tokio::sync::{broadcast, RwLock};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::auth::SessionStore;
use crate::config::WebUiConfig;
use crate::events::DashboardEvent;
use crate::static_assets::StaticAssets;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    /// Directory containing per-PR JSON result files.
    pub output_dir: PathBuf,

    /// Directory containing datasets.
    pub dataset_dir: PathBuf,

    /// Directory of the static frontend files. `None` uses embedded assets.
    pub static_dir: Option<PathBuf>,

    /// Comma-separated list of available models.
    pub models: String,

    /// Path to the code-review-benchmark directory (must contain offline/).
    pub benchmark_dir: Option<PathBuf>,

    /// Active (running) benchmark runs.
    pub active_runs: Arc<RwLock<HashMap<String, ActiveRun>>>,

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
#[derive(Clone)]
pub struct ActiveRun {
    /// When the run was started.
    pub created_at: UnixTime,

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

impl AppState {
    pub fn new(
        output_dir: PathBuf,
        dataset_dir: PathBuf,
        static_dir: Option<PathBuf>,
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
        .route(
            "/api/runs",
            get(crate::api::list_runs).post(crate::api::start_run),
        )
        .route("/api/runs/:id", get(crate::api::get_run))
        .route("/api/runs/:id/live", get(crate::api::live_stream))
        .route("/api/config", get(crate::api::get_config))
        .route("/api/config/datasets", get(crate::api::list_datasets))
        .route(
            "/api/config/reasoning-efforts",
            get(crate::api::list_reasoning_efforts),
        )
        .route("/api/runs/:id/logs", get(crate::api::list_logs))
        .route(
            "/api/runs/:id/logs/:pr_key/:role",
            get(crate::api::get_agent_log),
        )
        .route("/api/runs/:id/prs/:pr_key", get(crate::api::get_pr_agents))
        .route(
            "/api/runs/:id/pr-detail/:pr_key",
            get(crate::api::get_pr_detail),
        )
        .route("/api/datasets/:id/prs", get(crate::api::list_dataset_prs))
        // Ad-hoc review endpoints
        .route("/api/adhoc/review", post(crate::api::start_adhoc_review))
        .route("/api/adhoc/runs", get(crate::api::list_adhoc_runs))
        .route("/api/adhoc/runs/:id", get(crate::api::get_adhoc_run))
        .route(
            "/api/adhoc/prs/:owner/:repo",
            get(crate::api::list_repo_prs),
        )
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
///
/// When `--static-dir` is set, serves from disk (dev mode).
/// Otherwise, serves from assets embedded at build time via `rust-embed`.
async fn static_or_index(State(state): State<AppState>, uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    // Try disk-based serving if a static directory is configured
    if let Some(static_dir) = &state.static_dir {
        let file_path = static_dir.join(path);

        // If path is empty or points to a directory, serve index.html
        if path.is_empty() || path.ends_with('/') || !file_path.extension().is_some() {
            return serve_index_from_disk(static_dir).await;
        }

        // Try to serve the file directly from disk
        if file_path.exists() && file_path.is_file() {
            match tokio::fs::read(&file_path).await {
                Ok(data) => {
                    let content_type =
                        mime_type_from_extension(file_path.extension().and_then(|e| e.to_str()));
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

        // SPA fallback: serve index.html from disk
        return serve_index_from_disk(static_dir).await;
    }

    // Embedded asset serving (no --static-dir)
    let asset_path = if path.is_empty() { "index.html" } else { path };

    if let Some(asset) = StaticAssets::get(asset_path) {
        let content_type = mime_type_from_extension(
            std::path::Path::new(asset_path)
                .extension()
                .and_then(|e| e.to_str()),
        );
        return Response::builder()
            .header("Content-Type", content_type)
            .header("Content-Length", asset.data.len().to_string())
            .body(Body::from(asset.data.to_vec()))
            .unwrap();
    }

    // If the path has an extension and wasn't found, return 404
    if std::path::Path::new(path).extension().is_some() {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    }

    // SPA fallback: serve embedded index.html for any unrecognized path
    if let Some(index) = StaticAssets::get("index.html") {
        return Response::builder()
            .header("Content-Type", "text/html; charset=utf-8")
            .body(Body::from(index.data.to_vec()))
            .unwrap();
    }

    (
        StatusCode::NOT_FOUND,
        "Frontend assets not found. Build the frontend or use --static-dir.".to_string(),
    )
        .into_response()
}

/// Serve index.html from a disk directory.
async fn serve_index_from_disk(static_dir: &std::path::Path) -> Response {
    let index_path = static_dir.join("index.html");
    match tokio::fs::read(&index_path).await {
        Ok(data) => Response::builder()
            .header("Content-Type", "text/html; charset=utf-8")
            .body(Body::from(data))
            .unwrap(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            format!(
                "Static directory '{}' not found or index.html missing. Build the frontend or set --static-dir.",
                static_dir.display()
            ),
        )
            .into_response(),
    }
}

/// Determine MIME type from a file extension.
fn mime_type_from_extension(ext: Option<&str>) -> &'static str {
    match ext {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "application/javascript",
        Some("wasm") => "application/wasm",
        Some("css") => "text/css",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}
