use serde::{Deserialize, Serialize};

/// Information about an available role/agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleInfo {
    /// Human-readable role name.
    pub name: String,

    /// Role abbreviation.
    pub abbreviation: String,

    /// Roles that are incompatible with this role.
    #[serde(default)]
    pub incompatible_with_roles: Vec<String>,
}

impl RoleInfo {
    /// Combined display string: "Name (ABBR)".
    pub fn display_name(&self) -> String {
        format!("{} ({})", self.name, self.abbreviation)
    }
}

/// Per-dataset config loaded from dataset.toml.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DatasetConfig {
    /// Default values for the New Run form when this dataset is selected.
    #[serde(default)]
    pub defaults: DatasetDefaults,
}

/// Default values that auto-fill the New Run form when a dataset is selected.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DatasetDefaults {
    /// Default model for this dataset.
    #[serde(default)]
    pub model: Option<String>,

    /// Default concurrency level.
    #[serde(default)]
    pub concurrency: Option<usize>,

    /// Default max_findings limit.
    #[serde(default)]
    pub max_findings: Option<usize>,

    /// Default reviewer roles (comma-separated for legacy frontend compat).
    /// TODO: Change to `Option<Vec<String>>` when frontend is updated.
    #[serde(default)]
    pub roles: Option<String>,
}

/// Information about an available dataset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetInfo {
    /// Dataset identifier (used in API paths).
    pub id: String,

    /// Filesystem path to the dataset directory.
    pub path: String,

    /// Number of PRs in this dataset.
    pub pr_count: usize,

    /// Optional per-dataset configuration.
    #[serde(default)]
    pub config: Option<DatasetConfig>,
}

/// GET /api/datasets/:id/prs.
///
/// A single PR entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrEntry {
    /// PR key (e.g. "owner/repo/pull/N").
    pub key: String,

    /// Full URL to the PR.
    pub url: String,

    /// PR title.
    pub title: String,

    /// Repository name (owner/repo).
    pub repo: String,

    /// PR number.
    pub pr_number: u32,
}

/// Response for GET /api/config/reasoning-efforts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningEffortsResponse {
    /// Available reasoning effort levels.
    pub levels: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── RoleInfo ──────────────────────────────────────────────────────────

    #[test]
    fn test_role_info_serde_roundtrip() {
        let orig = RoleInfo {
            name: "Frontend Engineer".into(),
            abbreviation: "FE".into(),
            incompatible_with_roles: vec![],
        };
        let json = serde_json::to_string(&orig).unwrap();
        let deserialized: RoleInfo = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&orig);
        let _ = deserialized;
    }

    #[test]
    fn test_role_info_display_name() {
        let role = RoleInfo {
            name: "Backend Engineer".into(),
            abbreviation: "BE".into(),
            incompatible_with_roles: vec![],
        };
        insta::assert_debug_snapshot!(role.display_name());
    }

    #[test]
    fn test_role_info_with_incompatible_roles() {
        let role = RoleInfo {
            name: "Full Stack".into(),
            abbreviation: "FS".into(),
            incompatible_with_roles: vec!["FE".into(), "BE".into()],
        };
        let json = serde_json::to_string(&role).unwrap();
        let deserialized: RoleInfo = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&role);
        let _ = deserialized;
    }

    #[test]
    fn test_role_info_default_incompatible() {
        let json = r#"{"name":"Frontend","abbreviation":"FE"}"#;
        let role: RoleInfo = serde_json::from_str(json).unwrap();
        insta::assert_debug_snapshot!(role);
    }

    // ── DatasetConfig ─────────────────────────────────────────────────────

    #[test]
    fn test_dataset_config_serde_roundtrip() {
        let orig = DatasetConfig {
            defaults: DatasetDefaults {
                model: Some("gpt-4o".into()),
                concurrency: Some(5),
                max_findings: Some(10),
                roles: Some("FE,BE".into()),
            },
        };
        let json = serde_json::to_string(&orig).unwrap();
        let deserialized: DatasetConfig = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&orig);
        let _ = deserialized;
    }

    #[test]
    fn test_dataset_config_default() {
        let config = DatasetConfig::default();
        insta::assert_debug_snapshot!(config);
    }

    #[test]
    fn test_dataset_config_empty_json() {
        let json = "{}";
        let config: DatasetConfig = serde_json::from_str(json).unwrap();
        insta::assert_debug_snapshot!(config);
    }

    // ── DatasetDefaults ───────────────────────────────────────────────────

    #[test]
    fn test_dataset_defaults_serde_roundtrip() {
        let orig = DatasetDefaults {
            model: Some("claude-3-opus".into()),
            concurrency: Some(3),
            max_findings: Some(20),
            roles: Some("SEC".into()),
        };
        let json = serde_json::to_string(&orig).unwrap();
        let deserialized: DatasetDefaults = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&orig);
        let _ = deserialized;
    }

    #[test]
    fn test_dataset_defaults_default() {
        let defaults = DatasetDefaults::default();
        insta::assert_debug_snapshot!(defaults);
    }

    // ── DatasetInfo ───────────────────────────────────────────────────────

    #[test]
    fn test_dataset_info_serde_roundtrip() {
        let orig = DatasetInfo {
            id: "benchmark-v2".into(),
            path: "/data/datasets/benchmark-v2".into(),
            pr_count: 150,
            config: None,
        };
        let json = serde_json::to_string(&orig).unwrap();
        let deserialized: DatasetInfo = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&orig);
        let _ = deserialized;
    }

    #[test]
    fn test_dataset_info_with_config() {
        let config = DatasetConfig {
            defaults: DatasetDefaults {
                model: Some("gpt-4".into()),
                ..Default::default()
            },
        };
        let orig = DatasetInfo {
            id: "test-set".into(),
            path: "/tmp/datasets/test".into(),
            pr_count: 10,
            config: Some(config),
        };
        let json = serde_json::to_string(&orig).unwrap();
        let deserialized: DatasetInfo = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&orig);
        let _ = deserialized;
    }

    #[test]
    fn test_dataset_info_default_config() {
        let json = r#"{"id":"ds1","path":"/data/ds1","pr_count":50}"#;
        let info: DatasetInfo = serde_json::from_str(json).unwrap();
        insta::assert_debug_snapshot!(info);
    }

    // ── PrEntry ───────────────────────────────────────────────────────────

    #[test]
    fn test_pr_entry_serde_roundtrip() {
        let orig = PrEntry {
            key: "owner/repo/pull/42".into(),
            url: "https://github.com/owner/repo/pull/42".into(),
            title: "Add CI pipeline".into(),
            repo: "owner/repo".into(),
            pr_number: 42,
        };
        let json = serde_json::to_string(&orig).unwrap();
        let deserialized: PrEntry = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&orig);
        let _ = deserialized;
    }

    // ── ReasoningEffortsResponse ──────────────────────────────────────────

    #[test]
    fn test_reasoning_efforts_response_serde_roundtrip() {
        let orig = ReasoningEffortsResponse {
            levels: vec!["low".into(), "medium".into(), "high".into()],
        };
        let json = serde_json::to_string(&orig).unwrap();
        let deserialized: ReasoningEffortsResponse = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&orig);
        let _ = deserialized;
    }

    #[test]
    fn test_reasoning_efforts_response_empty() {
        let orig = ReasoningEffortsResponse { levels: vec![] };
        let json = serde_json::to_string(&orig).unwrap();
        let deserialized: ReasoningEffortsResponse = serde_json::from_str(&json).unwrap();
        insta::assert_json_snapshot!(&orig);
        let _ = deserialized;
    }
}
