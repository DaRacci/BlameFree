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

/// Application configuration returned by the backend config endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Available model identifiers.
    #[serde(default)]
    pub models: Vec<String>,

    /// Available dataset identifiers.
    #[serde(default)]
    pub datasets: Vec<String>,

    /// Available reviewer roles/agents.
    #[serde(default)]
    pub roles: Vec<RoleInfo>,

    /// Whether OAuth authentication is configured server-side.
    #[serde(default)]
    pub auth_enabled: bool,
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

    /// Default reviewer roles.
    #[serde(default)]
    pub roles: Option<Vec<String>>,
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
#[deprecated]
pub struct ReasoningEffortsResponse {
    /// Available reasoning effort levels.
    pub levels: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_role_info_default_incompatible() {
        let json = r#"{"name":"Frontend","abbreviation":"FE"}"#;
        let role: RoleInfo = serde_json::from_str(json).unwrap();
        insta::assert_debug_snapshot!(role);
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

    #[test]
    fn test_dataset_defaults_default() {
        let defaults = DatasetDefaults::default();
        insta::assert_debug_snapshot!(defaults);
    }

    #[test]
    fn test_dataset_info_default_config() {
        let json = r#"{"id":"ds1","path":"/data/ds1","pr_count":50}"#;
        let info: DatasetInfo = serde_json::from_str(json).unwrap();
        insta::assert_debug_snapshot!(info);
    }
}
