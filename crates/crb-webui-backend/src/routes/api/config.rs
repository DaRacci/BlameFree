use axum::{Json, extract::State, response::IntoResponse};
use crb_webui_shared::routes::API_CONFIG;
use riv_stor::traits::Store;
use tracing::instrument;

use crate::{routes_register, server::AppState};

routes_register! {
  get API_CONFIG => get_config,
}

#[instrument(skip(state), name = API_CONFIG)]
pub async fn get_config<S>(State(state): State<AppState<S>>) -> impl IntoResponse
where
    S: Store + Send + Sync + Clone + 'static,
{
    Json(state.config.clone())
}
