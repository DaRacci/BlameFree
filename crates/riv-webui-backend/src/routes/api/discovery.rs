use axum::{
    extract::{Path, State},
    response::IntoResponse,
};
use riv_stor::traits::Store;
use riv_webui_shared::routes::{API_DISCOVERY_CAPABILITIES, API_DISCOVERY_MODELS};
use tracing::instrument;

use crate::{routes_register, server::AppState};

routes_register! {
  get API_DISCOVERY_MODELS => get_models,
  get API_DISCOVERY_CAPABILITIES => get_capabilities,
}

#[instrument(skip(state), name = API_DISCOVERY_MODELS)]
#[allow(unused_variables)]
pub async fn get_models<S>(State(state): State<AppState<S>>) -> impl IntoResponse
where
    S: Store + Send + Sync + Clone + 'static,
{
    //TODO
    axum::http::StatusCode::NOT_IMPLEMENTED
}

#[instrument(skip(state), name = API_DISCOVERY_CAPABILITIES, fields(model_slug = %model_slug))]
#[allow(unused_variables)]
pub async fn get_capabilities<S>(
    State(state): State<AppState<S>>,
    Path(model_slug): Path<String>,
) -> impl IntoResponse
where
    S: Store + Send + Sync + Clone + 'static,
{
    //TODO
    axum::http::StatusCode::NOT_IMPLEMENTED
}
