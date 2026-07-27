//! Axum server setup, shared state, and router.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use mti::prelude::MagicTypeId;
use reqwest::header;
use riv_stor::traits::Store;
use riv_types::RunEvent;
use riv_types::capabilities::ReasoningEffort;
use rustls::pki_types::UnixTime;
use strum::VariantArray;
use tokio::sync::{RwLock, broadcast};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::auth::SessionStore;
use crate::config::WebUiConfig;
use crate::static_assets::StaticAssets;

async fn list_reasoning_efforts() -> Json<Vec<ReasoningEffort>> {
    Json(ReasoningEffort::VARIANTS.to_vec())
}

/// Shared application state.
#[derive(Clone)]
pub struct AppState<S: Store + Send + Sync + Clone>
where
    Self: Send + Sync,
{
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

    /// Store for data persistence.
    pub store: Arc<S>,
}

/// State for an actively running benchmark.
#[derive(Clone)]
pub struct ActiveRun {
    /// When the run was started.
    pub created_at: UnixTime,

    /// Broadcast channel for SSE events.
    pub tx: broadcast::Sender<RunEvent>,
}

impl<S: Store + Send + Sync + Clone> AppState<S> {
    pub fn new(
        config: WebUiConfig,
        octocrab: octocrab::Octocrab,
        session_store: SessionStore,
        log_file: PathBuf,
        store: Arc<S>,
    ) -> Self {
        Self {
            active_runs: Arc::new(RwLock::new(HashMap::new())),
            config,
            session_store,
            octocrab,
            log_file,
            store,
        }
    }
}

pub async fn start(state: AppState<impl Store + 'static>) -> anyhow::Result<()> {
    let app = Router::new()
        .merge(crate::routes::auth::register_routes(&state))
        .merge(crate::routes::api::reviews::register_routes(&state))
        .merge(crate::routes::api::config::register_routes(&state))
        .merge(crate::routes::api::admin::register_routes(&state))
        .merge(crate::routes::api::benchmark::register_routes(&state))
        .merge(crate::routes::api::discovery::register_routes(&state));

    let host = state.config.server.host.clone();
    let port = state.config.server.port.clone();

    let app = app
        .fallback(static_or_index)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("{host}:{port}");
    info!("Listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Serve static files or fall back to index.html for SPA routing.
async fn static_or_index(State(_): State<AppState<impl Store>>, uri: Uri) -> Response {
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
