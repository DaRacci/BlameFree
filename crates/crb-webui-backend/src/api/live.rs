//! SSE live streaming handler.
//!
//! Server-Sent Events for real-time agent monitoring during active review runs.

use axum::Json;
use axum::extract::Path as AxumPath;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::sse::KeepAlive;
use axum::response::sse::{Event, Sse};
use crb_webui_shared::routes::API_RUNS_ID_LIVE;
use mti::prelude::MagicTypeId;
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;
use tracing::instrument;

use crate::server::AppState;

const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(15);

/// Get an SSE stream of live agent outputs.
#[instrument(skip(state), fields(run_id = %id), name = API_RUNS_ID_LIVE)]
pub async fn live_stream(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<MagicTypeId>,
) -> impl IntoResponse {
    let tx = {
        let runs = state.active_runs.read().await;
        runs.get(&id).map(|run| run.tx.clone())
    };

    match tx {
        Some(tx) => {
            let stream = BroadcastStream::new(tx.subscribe()).filter_map(|result| match result {
                Ok(event) => {
                    let json = serde_json::to_string(&event).ok()?;
                    Some(Ok::<Event, Infallible>(Event::default().data(json)))
                }
                Err(_) => None, // Client missed; TODO:Should we resend the last event?
            });

            Sse::new(stream)
                .keep_alive(
                    KeepAlive::new()
                        .interval(KEEP_ALIVE_INTERVAL)
                        .text("keep-alive"),
                )
                .into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("No active run: {}", id)})),
        )
            .into_response(),
    }
}
