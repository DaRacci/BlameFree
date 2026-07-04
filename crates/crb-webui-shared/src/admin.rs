use serde::{Deserialize, Serialize};

/// Response from GET /api/admin/logs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogsResponse {
    pub logs: String,
    pub available: bool,
    #[serde(default)]
    pub message: Option<String>,
}
