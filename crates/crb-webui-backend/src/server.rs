//! Axum server setup, shared state, and router.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use crb_types::RunEvent;
use crb_types::capabilities::ReasoningEffort;
use crb_webui_shared::routes;
use mti::prelude::MagicTypeId;
use reqwest::header;
use rustls::pki_types::UnixTime;
use strum::VariantArray;
use tokio::sync::{RwLock, broadcast};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::api::{adhoc, admin, config, runs};
use crate::auth::{self, SessionStore};
use crate::config::WebUiConfig;
use crate::static_assets::StaticAssets;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    /// Directory containing per-PR JSON result files.
    pub output_dir: PathBuf,

    /// Directory containing datasets.
    #[deprecated = "This should be managed by the benchmark config instead of being a global state."]
    pub dataset_dir: PathBuf,

    /// Comma-separated list of available models.
    #[deprecated = "This is a dynamic list not a app state."]
    pub models: String,

    /// Path to the code-review-benchmark directory (must contain offline/).
    #[deprecated = "This should be managed by the benchmark config instead of being a global state."]
    pub benchmark_dir: Option<PathBuf>,

    /// Active review sessions.
    pub active_runs: Arc<RwLock<HashMap<MagicTypeId, ActiveRun>>>,

    /// Web UI configuration.
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
    #[deprecated = "A run isnt always a benchmark."]
    pub config: runs::BenchmarkConfig,

    /// Broadcast channel for SSE events.
    pub tx: broadcast::Sender<RunEvent>,

    /// Number of completed PRs.
    #[deprecated]
    pub completed_prs: usize,

    /// Total number of PRs.
    #[deprecated]
    pub total_prs: usize,

    /// Whether the run has finished.
    #[deprecated]
    pub finished: bool,
}

impl AppState {
    pub fn new(
        output_dir: PathBuf,
        dataset_dir: PathBuf,
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

pub async fn start(state: AppState, port: u16) -> anyhow::Result<()> {
    let app = Router::new()
        .route(routes::API_RUNS, get(runs::list_runs).post(runs::start_run))
        .route(routes::API_RUNS_ID, get(runs::get_run))
        .route(routes::API_RUNS_ID_LIVE, get(crate::api::live::live_stream))
        .route(routes::API_RUNS_ID_LOGS, get(runs::list_logs))
        .route(routes::API_RUNS_ID_PRS_KEY, get(runs::get_pr_agents))
        .route(routes::API_RUNS_ID_LOGS_KEY_ROLE, get(runs::get_agent_log))
        .route(routes::API_RUNS_ID_DETAILS_KEY, get(runs::get_pr_detail))
        .route(routes::API_CONFIG, get(config::get_config))
        .route(routes::API_CONFIG_DATASETS, get(config::list_datasets))
        .route(routes::API_CONFIG_REASONING, get(ReasoningEffort::VARIANTS))
        .route(routes::API_DATASETS_ID_PRS, get(config::list_dataset_prs))
        .route(routes::API_ADHOC_REVIEW, post(adhoc::start_adhoc_review))
        .route(routes::API_ADHOC_RUNS, get(adhoc::list_adhoc_runs))
        .route(routes::API_ADHOC_RUNS_ID, get(adhoc::get_adhoc_run))
        .route(routes::API_ADHOC_PRS_OWNER_REPO, get(adhoc::list_repo_prs))
        .route(routes::API_ADMIN_LOGS, get(admin::get_logs))
        .route(routes::API_ADMIN_LOGS_STREAM, get(admin::get_logs_stream))
        .route(routes::AUTH_LOGIN, get(auth::login))
        .route(routes::AUTH_LOGOUT, get(auth::logout))
        .route(routes::AUTH_CALLBACK, get(auth::callback))
        .route(routes::AUTH_ME, get(auth::me));

    let app = app
        .fallback(static_or_index)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    info!("Listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Serve static files or fall back to index.html for SPA routing.
async fn static_or_index(State(_): State<AppState>, uri: Uri) -> Response {
    const INDEX_HTML: &str = "index.html";

    let path = uri.path().trim_start_matches('/');
    let asset_path = if path.is_empty() { INDEX_HTML } else { path };

    if let Some(asset) = StaticAssets::get(asset_path) {
        let content_type =
            mime_type_from_extension(Path::new(asset_path).extension().and_then(|e| e.to_str()));
        return Response::builder()
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CONTENT_LENGTH, asset.data.len().to_string())
            .body(Body::from(asset.data.to_vec()))
            .unwrap();
    }

    // If the path has an extension and wasn't found, return 404
    if Path::new(path).extension().is_some() {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    }

    //TODO: 404 for SPA fallback if index.html is not found in embedded assets or disk
    // SPA fallback: serve embedded index.html for any unrecognized path
    if let Some(index) = StaticAssets::get("index.html") {
        return Response::builder()
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(index.data.to_vec()))
            .unwrap();
    }

    (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("There was an error serving the {} or index.html. Please check the server logs for more information.", path),
    )
        .into_response()
}

/// Serve index.html from a disk directory.
async fn serve_index_from_disk(static_dir: &Path) -> Response {
    let index_path = static_dir.join("index.html");
    match tokio::fs::read(&index_path).await {
        Ok(data) => Response::builder()
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mime_type_from_extension_html() {
        insta::assert_debug_snapshot!(mime_type_from_extension(Some("html")));
    }

    #[test]
    fn test_mime_type_from_extension_js() {
        insta::assert_debug_snapshot!(mime_type_from_extension(Some("js")));
    }

    #[test]
    fn test_mime_type_from_extension_wasm() {
        insta::assert_debug_snapshot!(mime_type_from_extension(Some("wasm")));
    }

    #[test]
    fn test_mime_type_from_extension_css() {
        insta::assert_debug_snapshot!(mime_type_from_extension(Some("css")));
    }

    #[test]
    fn test_mime_type_from_extension_json() {
        insta::assert_debug_snapshot!(mime_type_from_extension(Some("json")));
    }

    #[test]
    fn test_mime_type_from_extension_png() {
        insta::assert_debug_snapshot!(mime_type_from_extension(Some("png")));
    }

    #[test]
    fn test_mime_type_from_extension_svg() {
        insta::assert_debug_snapshot!(mime_type_from_extension(Some("svg")));
    }

    #[test]
    fn test_mime_type_from_extension_ico() {
        insta::assert_debug_snapshot!(mime_type_from_extension(Some("ico")));
    }

    #[test]
    fn test_mime_type_from_extension_txt() {
        insta::assert_debug_snapshot!(mime_type_from_extension(Some("txt")));
    }

    #[test]
    fn test_mime_type_from_extension_fallback() {
        insta::assert_debug_snapshot!(mime_type_from_extension(Some("unknown")));
    }

    #[test]
    fn test_mime_type_from_extension_fallback_none() {
        insta::assert_debug_snapshot!(mime_type_from_extension(None));
    }

    #[test]
    fn test_mime_type_from_extension_empty_string() {
        insta::assert_debug_snapshot!(mime_type_from_extension(Some("")));
    }
}
