use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use riv_stor::traits::Store;
use riv_webui_shared::{config::DatasetInfo, routes::API_BENCHMARK_DATASETS};

use crate::{routes_register, server::AppState};

routes_register! {
  get API_BENCHMARK_DATASETS => get_datasets
}

pub async fn get_datasets<S>(State(state): State<AppState<S>>) -> Response
where
    S: Store + Send + Sync + Clone + 'static,
{
    crate::services::list_datasets(&state)
        .await
        .map(|datasets| (StatusCode::OK, Json(datasets)).into_response())
        .unwrap_or_else(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(Vec::<DatasetInfo>::new()),
            )
                .into_response()
        })
}
