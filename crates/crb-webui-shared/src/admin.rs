use serde::{Deserialize, Serialize};

/// GET /api/admin/logs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogsResponse {
    /// Raw log text content.
    pub logs: String,

    /// Whether logs are available for the requested run.
    pub available: bool,

    /// Optional message explaining why logs are unavailable.
    #[serde(default)]
    pub message: Option<String>,
}
