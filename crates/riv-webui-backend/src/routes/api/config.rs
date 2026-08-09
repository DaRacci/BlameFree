use axum::{Json, extract::State, response::IntoResponse};
use riv_stor::traits::Store;
use riv_webui_shared::routes::API_CONFIG;

use crate::{routes_register, server::AppState};

#[derive(serde::Serialize)]
struct PublicConfigResponse {
    auth_enabled: bool,
}

routes_register! {
  get API_CONFIG => get_config,
}

pub async fn get_config<S>(State(state): State<AppState<S>>) -> impl IntoResponse
where
    S: Store + Send + Sync + Clone + 'static,
{
    Json(PublicConfigResponse {
        auth_enabled: state.config.oauth.is_some(),
    })
}
