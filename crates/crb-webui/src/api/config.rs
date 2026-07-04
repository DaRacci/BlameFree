//! API handler for configuration endpoints.

use std::path::Path;

use axum::extract::{Path as AxumPath, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::server::AppState;

/// Information about an available role/agent.
#[derive(Debug, Clone, Serialize)]
pub struct RoleInfo {
    pub abbreviation: String,
    #[serde(default)]
    pub incompatible_with_roles: Vec<String>,
}

/// Available configuration options.
#[derive(Debug, Clone, Serialize)]
pub struct ConfigResponse {
    pub models: Vec<String>,
    pub datasets: Vec<String>,
    pub roles: Vec<RoleInfo>,
    /// Whether reduce-diff mode is enabled (compile-time feature flag).
    pub reduce_diff_enabled: bool,
    /// Whether OAuth authentication is configured.
    #[serde(default)]
    pub auth_enabled: bool,
}

/// Response for GET /api/config/reasoning-efforts.
#[derive(Debug, Clone, Serialize)]
pub struct ReasoningEffortsResponse {
    pub levels: Vec<String>,
}

/// Information about an available model.
#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
}

/// Per-dataset config loaded from dataset.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize)]
pub struct DatasetInfo {
    pub id: String,
    pub path: String,
    pub pr_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<DatasetConfig>,
}

/// GET /api/config — list available models, datasets, and roles.
pub async fn get_config(State(state): State<AppState>) -> Json<ConfigResponse> {
    tracing::info!("GET /api/config");
    let models: Vec<String> = state
        .models
        .split(',')
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
        .collect();

    let datasets: Vec<String> = scan_datasets(&state.dataset_dir)
        .into_iter()
        .map(|d| d.id)
        .collect();

    let roles: Vec<RoleInfo> = crb_agents::prompts::PromptLibrary::new()
        .map(|lib| {
            let mut role_infos: Vec<RoleInfo> = lib
                .roles()
                .iter()
                .map(|abbr| {
                    let config = lib.config(abbr);
                    RoleInfo {
                        abbreviation: abbr.to_string(),
                        incompatible_with_roles: config
                            .map(|c| c.incompatible_with_roles.clone())
                            .unwrap_or_default(),
                    }
                })
                .collect();
            role_infos.sort_by(|a, b| a.abbreviation.cmp(&b.abbreviation));
            role_infos
        })
        .unwrap_or_else(|_| {
            vec![
                RoleInfo { abbreviation: "ARCH".to_string(), incompatible_with_roles: vec![] },
                RoleInfo { abbreviation: "CL".to_string(), incompatible_with_roles: vec![] },
                RoleInfo { abbreviation: "GEN".to_string(), incompatible_with_roles: vec!["SA".to_string(), "CL".to_string(), "ARCH".to_string(), "SEC".to_string()] },
                RoleInfo { abbreviation: "SA".to_string(), incompatible_with_roles: vec![] },
                RoleInfo { abbreviation: "SEC".to_string(), incompatible_with_roles: vec![] },
            ]
        });

    Json(ConfigResponse {
        models,
        datasets,
        roles,
        reduce_diff_enabled: cfg!(feature = "reduce-diff"),
        auth_enabled: state.config.oauth.is_some(),
    })
}

/// GET /api/config/datasets — list available datasets with PR counts.
pub async fn list_datasets(State(state): State<AppState>) -> Json<Vec<DatasetInfo>> {
    tracing::info!("GET /api/config/datasets");
    Json(scan_datasets(&state.dataset_dir))
}

fn scan_datasets(dataset_dir: &Path) -> Vec<DatasetInfo> {
    let mut datasets = Vec::new();

    if !dataset_dir.exists() {
        return datasets;
    }

    if let Ok(entries) = std::fs::read_dir(dataset_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let id = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let pr_count = count_prs_in_dir(&path);

                // Try to load dataset.toml config
                let config = load_dataset_config(&path);

                datasets.push(DatasetInfo {
                    id,
                    path: path.to_string_lossy().to_string(),
                    pr_count,
                    config,
                });
            }
        }
    }

    datasets.sort_by(|a, b| b.pr_count.cmp(&a.pr_count));
    datasets
}

fn load_dataset_config(dir: &Path) -> Option<DatasetConfig> {
    let config_path = dir.join("dataset.toml");
    if !config_path.exists() {
        return None;
    }
    match std::fs::read_to_string(&config_path) {
        Ok(content) => match toml::from_str::<DatasetConfig>(&content) {
            Ok(cfg) => Some(cfg),
            Err(e) => {
                tracing::warn!(
                    "Failed to parse dataset.toml in {}: {e}",
                    dir.display()
                );
                None
            }
        },
        Err(e) => {
            tracing::warn!(
                "Failed to read dataset.toml in {}: {e}",
                dir.display()
            );
            None
        }
    }
}

/// GET /api/config/reasoning-efforts — list available reasoning effort levels.
pub async fn list_reasoning_efforts() -> Json<ReasoningEffortsResponse> {
    let levels: Vec<String> = crb_harness::model_capabilities::REASONING_EFFORT_LEVELS
        .iter()
        .map(|s| s.to_string())
        .collect();
    Json(ReasoningEffortsResponse { levels })
}

fn count_prs_in_dir(dir: &Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "json") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    // Dataset files are JSON arrays of PR entries
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                        match val {
                            serde_json::Value::Array(arr) => {
                                count += arr.len();
                            }
                            serde_json::Value::Object(obj) => {
                                // Also support {"entries": [...]} format
                                if let Some(entries) = obj.get("entries").and_then(|v| v.as_array()) {
                                    count += entries.len();
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    count
}

/// A single PR entry returned by GET /api/datasets/:id/prs.
#[derive(Debug, Clone, Serialize)]
pub struct PrEntry {
    pub key: String,
    pub url: String,
    pub title: String,
    pub repo: String,
    pub pr_number: u32,
}

/// GET /api/datasets/:id/prs — list all PRs in a dataset.
pub async fn list_dataset_prs(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Json<Vec<PrEntry>> {
    tracing::info!("GET /api/datasets/{id}/prs");
    let dataset_dir = state.dataset_dir.join(&id);

    if !dataset_dir.exists() || !dataset_dir.is_dir() {
        tracing::warn!("Dataset directory not found: {}", dataset_dir.display());
        return Json(Vec::new());
    }

    let mut prs = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&dataset_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "json") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    read_prs_from_json(&path, &content, &mut prs);
                }
            }
        }
    }

    Json(prs)
}

/// Parse PR entries from a dataset JSON file and append them to `prs`.
fn read_prs_from_json(path: &Path, content: &str, prs: &mut Vec<PrEntry>) {
    // Try parsing as array first
    let items: Vec<serde_json::Value> = match serde_json::from_str(content) {
        Ok(val) => {
            match val {
                serde_json::Value::Array(arr) => arr,
                serde_json::Value::Object(obj) => {
                    obj.get("entries")
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default()
                }
                _ => return,
            }
        }
        Err(_) => return,
    };

    // Pre-compile regex to extract owner/repo/pull/N from GitHub URLs
    let re = regex::Regex::new(r"github\.com/[^/]+/([^/]+)/pull/(\d+)").unwrap();

    for item in items {
        let url = item
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let title = item
            .get("pr_title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Extract PR number from URL (last path segment)
        let pr_number: u32 = url
            .rsplit('/')
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        // Derive key from URL so it's a substring of the URL itself.
        // This ensures the harness's pr.url.to_lowercase().contains(key) match works.
        // URL format: https://github.com/owner/repo/pull/N
        // Key format: repo/pull/N  (e.g., "discourse-graphite/pull/1")
        let repo = re
            .captures(&url)
            .map(|caps| caps[1].to_string())
            .unwrap_or_else(|| {
                // Fallback: derive repo from filename (e.g., "discourse.json" -> "discourse")
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string()
            });
        let key = format!("{}/pull/{}", repo, pr_number);

        prs.push(PrEntry {
            key,
            url,
            title,
            repo,
            pr_number,
        });
    }
}
