use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use riv_stor::traits::Store;
use riv_types::{capabilities::ReasoningEffort, wrappers::Model};
use riv_webui_shared::routes::{API_DISCOVERY_CAPABILITIES, API_DISCOVERY_MODELS};
use tracing::instrument;

use crate::{routes_register, server::AppState};

routes_register! {
  get API_DISCOVERY_MODELS => get_models,
  get API_DISCOVERY_CAPABILITIES => get_capabilities,
}

#[instrument(skip(state), name = API_DISCOVERY_MODELS)]
pub async fn get_models<S>(State(state): State<AppState<S>>) -> Response
where
    S: Store + Send + Sync + Clone + 'static,
{
    crate::services::list_models(&state)
        .await
        .map(|models| (StatusCode::OK, Json(models)).into_response())
        .unwrap_or_else(|_| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(Vec::<Model>::new())).into_response()
        })
}

#[instrument(skip(state), name = API_DISCOVERY_CAPABILITIES, fields(model_slug = %model_slug))]
pub async fn get_capabilities<S>(
    State(state): State<AppState<S>>,
    Path(model_slug): Path<String>,
) -> Response
where
    S: Store + Send + Sync + Clone + 'static,
{
    crate::services::list_reasoning_efforts(&state, &model_slug)
        .await
        .map(|capabilities| (StatusCode::OK, Json(capabilities)).into_response())
        .unwrap_or_else(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(Vec::<ReasoningEffort>::new()),
            )
                .into_response()
        })
}
