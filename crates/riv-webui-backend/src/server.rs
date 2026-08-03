//! Axum server setup, shared state, and router.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use leptos::config::LeptosOptions;
use leptos::context::provide_context;
use leptos_axum::{LeptosRoutes, generate_route_list};
use mti::prelude::MagicTypeId;
use riv_stor::traits::Store;
use riv_types::RunEvent;
use tokio::sync::{RwLock, broadcast};
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
    pub octocrab: octocrab::Octocrab,

    /// Path to the server log file.
    pub log_file: PathBuf,

    /// Directory containing run output and store fallback locations.
    pub output_dir: PathBuf,

    /// Active reviews tracked in-memory.
    pub active_reviews: Arc<RwLock<Vec<MagicTypeId>>>,

    /// Broadcast channels keyed by review id for live events.
    pub review_channels: Arc<RwLock<HashMap<MagicTypeId, broadcast::Sender<RunEvent>>>>,

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
        output_dir: PathBuf,
        store: Arc<S>,
    ) -> Self {
        Self {
            leptos_options,
            config,
            session_store,
            octocrab,
            log_file,
            output_dir,
            active_reviews: Arc::new(RwLock::new(Vec::new())),
            review_channels: Arc::new(RwLock::new(HashMap::new())),
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

pub async fn start<S>(state: AppState<S>) -> anyhow::Result<()>
where
    S: Store + Send + Sync + Clone + 'static,
{
    let leptos_options = state.leptos_options.clone();
    let routes = generate_route_list(riv_webui_app::App);
    let site_root = leptos_options.site_root.to_string();
    let host = state.config.server.host.clone();
    let port = state.config.server.port;

    let app_services = riv_webui_app::AppServices {
        list_reviews: Arc::new({
            let state = state.clone();
            move |()| {
                let state = state.clone();
                Box::pin(async move { crate::services::list_reviews(&state).await })
            }
        }),
        read_admin_logs: Arc::new({
            let log_file = state.log_file.clone();
            move |()| {
                let log_file = log_file.clone();
                Box::pin(
                    async move { Ok(crate::routes::api::admin::load_logs_response(&log_file)) },
                )
            }
        }),
        list_repo_prs: Arc::new({
            let state = state.clone();
            move |(owner, repo)| {
                let state = state.clone();
                Box::pin(async move { crate::services::list_repo_prs(&state, &owner, &repo).await })
            }
        }),
        fetch_pr_diff: Arc::new({
            let state = state.clone();
            move |(owner, repo, pr_number)| {
                let state = state.clone();
                Box::pin(async move {
                    crate::services::fetch_pr_diff(&state, &owner, &repo, pr_number).await
                })
            }
        }),
        list_datasets: Arc::new({
            let state = state.clone();
            move |()| {
                let state = state.clone();
                Box::pin(async move { crate::services::list_datasets(&state).await })
            }
        }),
        list_dataset_prs: Arc::new({
            let state = state.clone();
            move |(dataset_id,)| {
                let state = state.clone();
                Box::pin(
                    async move { crate::services::list_dataset_prs(&state, &dataset_id).await },
                )
            }
        }),
        list_models: Arc::new({
            let state = state.clone();
            move |()| {
                let state = state.clone();
                Box::pin(async move { crate::services::list_models(&state).await })
            }
        }),
        list_reasoning_efforts: Arc::new({
            let state = state.clone();
            move |(model,)| {
                let state = state.clone();
                Box::pin(
                    async move { crate::services::list_reasoning_efforts(&state, &model).await },
                )
            }
        }),
        list_agents: Arc::new({
            let state = state.clone();
            move |()| {
                let state = state.clone();
                Box::pin(async move { crate::services::list_agents(&state).await })
            }
        }),
        get_review: Arc::new({
            let state = state.clone();
            move |(review_id,)| {
                let state = state.clone();
                Box::pin(async move { crate::services::get_review(&state, &review_id).await })
            }
        }),
        list_pr_results: Arc::new({
            let state = state.clone();
            move |(review_id,)| {
                let state = state.clone();
                Box::pin(async move { crate::services::list_pr_results(&state, &review_id).await })
            }
        }),
        list_agent_logs: Arc::new({
            let state = state.clone();
            move |(review_id,)| {
                let state = state.clone();
                Box::pin(async move { crate::services::list_agent_logs(&state, &review_id).await })
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
