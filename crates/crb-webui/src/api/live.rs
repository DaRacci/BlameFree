//! SSE live streaming handler.
//!
//! Server-Sent Events for real-time agent monitoring during
//! active benchmark runs.

use axum::extract::Path as AxumPath;
use axum::extract::State;
use axum::response::sse::{Event, Sse};
use axum::response::IntoResponse;
use axum::Json;
use std::convert::Infallible;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::server::AppState;

/// GET /api/runs/:id/live — SSE stream of live agent outputs.
pub async fn live_stream(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> impl IntoResponse {
    tracing::info!("GET /api/runs/{}/live", id);
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
                Err(_) => None, // Lagged — skip
            });

            Sse::new(stream)
                .keep_alive(
                    axum::response::sse::KeepAlive::new()
                        .interval(std::time::Duration::from_secs(15))
                        .text("keep-alive"),
                )
                .into_response()
        }
        None => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("No active run: {}", id)})),
        )
            .into_response(),
    }
}
