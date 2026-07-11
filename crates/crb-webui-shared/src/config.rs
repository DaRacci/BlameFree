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

    /// Default reviewer roles.
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
