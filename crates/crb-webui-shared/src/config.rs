use serde::{Deserialize, Serialize};

/// Information about an available role/agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleInfo {
    pub abbreviation: String,
    #[serde(default)]
    pub incompatible_with_roles: Vec<String>,
}

/// Per-dataset config loaded from dataset.toml
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DatasetConfig {
    #[serde(default)]
    pub defaults: DatasetDefaults,
}

/// Default values that auto-fill the New Run form when a dataset is selected.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DatasetDefaults {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub concurrency: Option<usize>,
    #[serde(default)]
    pub max_findings: Option<usize>,
    #[serde(default)]
    pub roles: Option<String>,
}

/// Information about an available dataset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetInfo {
    pub id: String,
    pub path: String,
    pub pr_count: usize,
    #[serde(default)]
    pub config: Option<DatasetConfig>,
}

/// A single PR entry returned by GET /api/datasets/:id/prs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrEntry {
    pub key: String,
    pub url: String,
    pub title: String,
    pub repo: String,
    pub pr_number: u32,
}

/// Response for GET /api/config/reasoning-efforts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningEffortsResponse {
    pub levels: Vec<String>,
}
