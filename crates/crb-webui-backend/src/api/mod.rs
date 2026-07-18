//! API route handlers for the web UI dashboard.

use axum::Json;
use axum::response::{IntoResponse, Response};
use reqwest::StatusCode;

pub mod adhoc;
pub mod admin;
pub mod config;
pub mod live;
pub mod runs;

/// Helper to build a JSON error response with a given status code and message.
pub(crate) fn err_json(status: StatusCode, msg: impl std::fmt::Display) -> Response {
    (status, Json(serde_json::json!({"error": msg.to_string()}))).into_response()
}

/// Helper to return a `NotFound` response.
pub(crate) fn not_found(msg: impl std::fmt::Display) -> Response {
    err_json(StatusCode::NOT_FOUND, msg)
}

/// Helper to return a `500` response.
pub(crate) fn internal_err(msg: impl std::fmt::Display) -> Response {
    err_json(StatusCode::INTERNAL_SERVER_ERROR, msg)
}
