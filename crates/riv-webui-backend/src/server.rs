//! Axum server setup, shared state, and router.

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use leptos::config::LeptosOptions;
use leptos::context::provide_context;
use leptos_axum::{LeptosRoutes, generate_route_list};
use riv_stor::traits::Store;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::auth::SessionStore;
use crate::config::WebUiConfig;

/// Shared application state.
#[derive(Clone)]
pub struct AppState<S: Store + Send + Sync + Clone>
where
    Self: Send + Sync,
{
    /// Leptos SSR options (site root, pkg dir, etc.).
    pub leptos_options: LeptosOptions,

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
        leptos_options: LeptosOptions,
        config: WebUiConfig,
        octocrab: octocrab::Octocrab,
        session_store: SessionStore,
        log_file: PathBuf,
        store: Arc<S>,
    ) -> Self {
        Self {
            leptos_options,
            config,
            session_store,
            octocrab,
            log_file,
            store,
        }
    }
}

impl<S: Store + Send + Sync + Clone + 'static> axum::extract::FromRef<AppState<S>>
    for LeptosOptions
{
    fn from_ref(state: &AppState<S>) -> LeptosOptions {
        state.leptos_options.clone()
    }
}

pub async fn start(state: AppState<impl Store + 'static>) -> anyhow::Result<()> {
    let leptos_options = state.leptos_options.clone();
    let routes = generate_route_list(riv_webui_app::App);
    let site_root = leptos_options.site_root.to_string();
    let host = state.config.server.host.clone();
    let port = state.config.server.port;

    let app_services = riv_webui_app::AppServices {
        list_reviews: std::sync::Arc::new({
            let state = state.clone();
            move || {
                let state = state.clone();
                Box::pin(async move { crate::routes::api::reviews::load_reviews(&state).await })
            }
        }),
        read_admin_logs: std::sync::Arc::new({
            let log_file = state.log_file.clone();
            move || {
                let log_file = log_file.clone();
                Box::pin(
                    async move { Ok(crate::routes::api::admin::load_logs_response(&log_file)) },
                )
            }
        }),
    };

    let app = Router::new()
        .merge(crate::routes::auth::register_routes(&state))
        .merge(crate::routes::api::reviews::register_routes(&state))
        .merge(crate::routes::api::config::register_routes(&state))
        .merge(crate::routes::api::admin::register_routes(&state))
        .merge(crate::routes::api::benchmark::register_routes(&state))
        .merge(crate::routes::api::discovery::register_routes(&state))
        .leptos_routes_with_context(
            &state,
            routes,
            {
                let app_services = app_services.clone();
                move || provide_context(app_services.clone())
            },
            {
                let options = leptos_options.clone();
                move || riv_webui_app::shell(options.clone())
            },
        )
        .fallback_service(ServeDir::new(&site_root))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("{host}:{port}");
    info!("Listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
