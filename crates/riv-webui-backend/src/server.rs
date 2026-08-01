//! Axum server setup, shared state, and router.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use reqwest::header;
use riv_stor::traits::Store;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::auth::SessionStore;
use crate::config::WebUiConfig;
#[cfg(feature = "embed-frontend")]
use crate::static_assets::StaticAssets;

/// Shared application state.
#[derive(Clone)]
pub struct AppState<S: Store + Send + Sync + Clone>
where
    Self: Send + Sync,
{
    /// Web UI configuration.
    pub config: WebUiConfig,

    /// Session store for OAuth-authenticated users.
    pub session_store: SessionStore,

    /// Octocrab GitHub API client (authenticated via GITHUB_TOKEN env var).
    #[allow(unused)]
    pub octocrab: octocrab::Octocrab,

    /// Path to the server log file.
    pub log_file: PathBuf,

    /// Store for data persistence.
    pub store: Arc<S>,
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

    #[cfg(feature = "embed-frontend")]
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
    #[cfg(feature = "embed-frontend")]
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
