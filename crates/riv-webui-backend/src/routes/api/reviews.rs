use std::{convert::Infallible, time::Duration};

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    response::{IntoResponse, Response},
};
use mti::prelude::MagicTypeId;
use riv_stor::traits::Store;
use riv_types::review::Review;
use riv_webui_shared::{
    review::ReviewAgentLog,
    routes::{
        API_REVIEWS_AGENTS, API_REVIEWS_DETAILS, API_REVIEWS_LIST, API_REVIEWS_LOGS,
        API_REVIEWS_STREAM, API_REVIEWS_SUBMIT,
    },
};
use tokio_stream::{StreamExt, wrappers::BroadcastStream};
use tracing::{error, instrument, warn};

use crate::{routes_register, server::AppState};

const REVIEW_SUBMIT_BLOCKED_REASON: &str = "Review launch blocked for human review: route contract still expects review id before launch, and `riv_harness::pipeline::run_linters()` is still `todo!()`.";

routes_register! {
  get API_REVIEWS_LIST => list_reviews,
  get API_REVIEWS_DETAILS => get_review,
  post API_REVIEWS_SUBMIT => submit_review,
  get API_REVIEWS_LOGS => get_review_logs,
  get API_REVIEWS_AGENTS => get_review_agents,
  get API_REVIEWS_STREAM => stream_review,
}

pub async fn load_reviews<S>(state: &AppState<S>) -> Result<Vec<Review>, String>
where
    S: Store + Send + Sync + Clone + 'static,
{
    crate::services::list_reviews(state).await.map_err(|error| {
        error!("Failed to list reviews: {}", error);
        error
    })
}

#[instrument(skip(state), name = API_REVIEWS_LIST)]
pub async fn list_reviews<S>(State(state): State<AppState<S>>) -> Response
where
    S: Store + Send + Sync + Clone + 'static,
{
    load_reviews(&state)
        .await
        .map(|reviews| (StatusCode::OK, Json(reviews)).into_response())
        .unwrap_or_else(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(Vec::<Review>::new()),
            )
                .into_response()
        })
}

#[instrument(skip(state), name = API_REVIEWS_DETAILS)]
pub async fn get_review<S>(
    State(state): State<AppState<S>>,
    Path(review_id): Path<MagicTypeId>,
) -> Response
where
    S: Store + Send + Sync + Clone + 'static,
{
    state
        .store
        .load::<Review>(&review_id)
        .await
        .map(|review| {
            if let Some(review) = review {
                (StatusCode::OK, Json(Some(review))).into_response()
            } else {
                (StatusCode::NOT_FOUND, Json(None::<Review>)).into_response()
            }
        })
        .unwrap_or_else(|err| {
            error!("Failed to get review {}: {}", review_id, err);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(None::<Review>)).into_response()
        })
}

#[instrument(skip(state), name = API_REVIEWS_SUBMIT)]
pub async fn submit_review<S>(
    State(state): State<AppState<S>>,
    Path(review_id): Path<MagicTypeId>,
) -> Response
where
    S: Store + Send + Sync + Clone + 'static,
{
    let _ = state;
    (
        StatusCode::FAILED_DEPENDENCY,
        Json(serde_json::json!({
            "error": REVIEW_SUBMIT_BLOCKED_REASON,
            "review_id": review_id.to_string(),
        })),
    )
        .into_response()
}

#[instrument(skip(state), name = API_REVIEWS_LOGS)]
pub async fn get_review_logs<S>(
    State(state): State<AppState<S>>,
    Path((review_id, agent_id)): Path<(MagicTypeId, MagicTypeId)>,
) -> Response
where
    S: Store + Send + Sync + Clone + 'static,
{
    match crate::services::list_agent_logs(&state, &review_id).await {
        Ok(logs) => {
            let selected = logs.into_iter().find(|log| log.agent_id == agent_id);
            if let Some(log) = selected {
                (StatusCode::OK, Json(Some(log))).into_response()
            } else {
                (StatusCode::NOT_FOUND, Json(None::<ReviewAgentLog>)).into_response()
            }
        }
        Err(error) if error.contains("not found") => {
            (StatusCode::NOT_FOUND, Json(None::<ReviewAgentLog>)).into_response()
        }
        Err(error) => {
            error!(
                "Failed to get review log {} / {}: {}",
                review_id, agent_id, error
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(None::<ReviewAgentLog>),
            )
                .into_response()
        }
    }
}

/// Returns list of agent IDs participating in review.
#[instrument(skip(state), name = API_REVIEWS_AGENTS)]
pub async fn get_review_agents<S>(
    State(state): State<AppState<S>>,
    Path(review_id): Path<MagicTypeId>,
) -> Response
where
    S: Store + Send + Sync + Clone + 'static,
{
    let review = match state.store.load::<Review>(&review_id).await {
        Ok(Some(review)) => review,
        Err(err) => {
            error!("Failed to get review {}: {}", review_id, err);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(None::<Vec<MagicTypeId>>),
            )
                .into_response();
        }
        Ok(None) => {
            return (StatusCode::NOT_FOUND, Json(None::<Vec<MagicTypeId>>)).into_response();
        }
    };

    let agents = review
        .agent_sessions
        .iter()
        .map(|session| session.id.clone())
        .collect::<Vec<_>>();
    (StatusCode::OK, Json(Some(agents))).into_response()
}

/// Stream running review events over SSE.
#[instrument(skip(state), name = API_REVIEWS_STREAM)]
pub async fn stream_review<S>(
    State(state): State<AppState<S>>,
    Path(review_id): Path<MagicTypeId>,
) -> Response
where
    S: Store + Send + Sync + Clone + 'static,
{
    let is_active = {
        let active_reviews = state.active_reviews.read().await;
        active_reviews.contains(&review_id)
    };

    if !is_active {
        return match state.store.load::<Review>(&review_id).await {
            Ok(Some(_)) => (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "error": "Review exists but is not running",
                    "review_id": review_id.to_string(),
                })),
            )
                .into_response(),
            Ok(None) => (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": "Review not found",
                    "review_id": review_id.to_string(),
                })),
            )
                .into_response(),
            Err(error) => {
                error!(
                    "Failed to look up review {} for stream: {}",
                    review_id, error
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": "Failed to load review stream state",
                        "review_id": review_id.to_string(),
                    })),
                )
                    .into_response()
            }
        };
    }

    let sender = {
        let channels = state.review_channels.read().await;
        channels.get(&review_id).cloned()
    };

    let Some(sender) = sender else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "No live channel registered for review",
                "review_id": review_id.to_string(),
            })),
        )
            .into_response();
    };

    let stream_review_id = review_id.to_string();
    let receiver = sender.subscribe();
    let stream = BroadcastStream::new(receiver).filter_map(move |message| match message {
        Ok(event) => match serde_json::to_string(&event) {
            Ok(payload) => Some(Ok::<Event, Infallible>(Event::default().data(payload))),
            Err(error) => {
                warn!(
                    "Failed to serialize run event for {}: {}",
                    stream_review_id, error
                );
                None
            }
        },
        Err(error) => {
            warn!(
                "Review stream recv error for {}: {}",
                stream_review_id, error
            );
            None
        }
    });

    Sse::new(stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keep-alive"),
        )
        .into_response()
}
