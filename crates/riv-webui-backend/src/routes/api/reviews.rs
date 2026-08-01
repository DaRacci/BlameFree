use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use mti::prelude::MagicTypeId;
use riv_stor::traits::Store;
use riv_types::review::Review;
use riv_webui_shared::routes::{
    API_REVIEWS_AGENTS, API_REVIEWS_DETAILS, API_REVIEWS_LIST, API_REVIEWS_LOGS,
    API_REVIEWS_STREAM, API_REVIEWS_SUBMIT,
};
use tracing::{error, instrument};

use crate::{routes_register, server::AppState};

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
    state.store.list::<Review>(&()).await.map_err(|err| {
        error!("Failed to list reviews: {}", err);
        err.to_string()
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
pub async fn submit_review<S>(State(state): State<AppState<S>>) -> Response
where
    S: Store + Send + Sync + Clone + 'static,
{
    let _ = state;
    (StatusCode::NOT_IMPLEMENTED, Json(None::<String>)).into_response()
}

#[instrument(skip(state), name = API_REVIEWS_LOGS)]
pub async fn get_review_logs<S>(
    State(state): State<AppState<S>>,
    Path((review_id, agent_id)): Path<(MagicTypeId, MagicTypeId)>,
) -> Response
where
    S: Store + Send + Sync + Clone + 'static,
{
    let _ = (state, review_id, agent_id);
    (StatusCode::NOT_IMPLEMENTED, Json(None::<String>)).into_response()
}

/// Returns a list of agent IDs that are participating in the review.
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
            error!("Failed to get review {}: {}", review_id, "Review not found");
            return (StatusCode::NOT_FOUND, Json(None::<Vec<MagicTypeId>>)).into_response();
        }
    };

    let agents = review
        .agent_sessions
        .iter()
        .map(|s| s.id.clone())
        .collect::<Vec<_>>();
    (StatusCode::OK, Json(Some(agents))).into_response()
}

/// Stream a running review.
///
/// Includes logs, status updates, and other events related to the review.
///
/// Returns an Error if the review is not running or does not exist.
#[instrument(skip(state), name = API_REVIEWS_STREAM)]
pub async fn stream_review<S>(
    State(state): State<AppState<S>>,
    Path(review_id): Path<MagicTypeId>,
) -> Response
where
    S: Store + Send + Sync + Clone + 'static,
{
    let _ = (state, review_id);
    (StatusCode::NOT_IMPLEMENTED, Json(None::<String>)).into_response()
}
