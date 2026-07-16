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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logs_response_serde_roundtrip() {
        let orig = LogsResponse {
            logs: "INFO: starting review\nINFO: completed".into(),
            available: true,
            message: Some("Logs available".into()),
        };
        let json = serde_json::to_string(&orig).unwrap();
        let deserialized: LogsResponse = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&orig);
        let _ = deserialized;
    }

    #[test]
    fn test_logs_response_unavailable_no_message() {
        let orig = LogsResponse {
            logs: String::new(),
            available: false,
            message: None,
        };
        let json = serde_json::to_string(&orig).unwrap();
        let deserialized: LogsResponse = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&orig);
        let _ = deserialized;
    }

    #[test]
    fn test_logs_response_default_message_field() {
        // message has #[serde(default)] so omitting it should work
        let json = r#"{"logs":"test logs","available":true}"#;
        let resp: LogsResponse = serde_json::from_str(json).unwrap();
        insta::assert_debug_snapshot!(resp);
    }
}
