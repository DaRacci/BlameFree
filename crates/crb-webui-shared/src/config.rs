use serde::{Deserialize, Serialize};

/// Information about an available role/agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    /// Human-readable role name.
    pub name: String,

    /// Role abbreviation.
    pub abbreviation: String,

    /// Roles that are incompatible with this role.
    #[serde(default)]
    pub incompatible_with_roles: Vec<String>,
}

impl AgentInfo {
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
    pub agents: Vec<AgentInfo>,

    /// Whether OAuth authentication is configured server-side.
    #[serde(default)]
    pub auth_enabled: bool,
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
}

/// A single PR entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[deprecated = "Migrate to new crb-types interfaces using [`crb_types::review::Review`]"]
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
