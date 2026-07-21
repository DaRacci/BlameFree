use axum::{extract::State, response::IntoResponse};
use crb_webui_shared::routes::API_BENCHMARK_DATASETS;
use riv_stor::traits::Store;

use crate::{routes_register, server::AppState};

routes_register! {
  get API_BENCHMARK_DATASETS => get_datasets
}

pub async fn get_datasets<S>(State(state): State<AppState<S>>) -> impl IntoResponse
where
    S: Store + Send + Sync + Clone + 'static,
{
    //TODO
    axum::http::StatusCode::NOT_IMPLEMENTED
}
