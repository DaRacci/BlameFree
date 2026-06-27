//! API handler for configuration endpoints.

use std::path::Path;

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::server::AppState;

/// Available configuration options.
#[derive(Debug, Clone, Serialize)]
pub struct ConfigResponse {
    pub models: Vec<String>,
    pub datasets: Vec<String>,
    pub roles: Vec<String>,
}

/// Information about an available model.
#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
}

/// Information about an available dataset.
#[derive(Debug, Clone, Serialize)]
pub struct DatasetInfo {
    pub id: String,
    pub path: String,
    pub pr_count: usize,
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

    let roles = vec![
        "SA".to_string(),
        "CL".to_string(),
        "AR".to_string(),
        "SEC".to_string(),
    ];

    Json(ConfigResponse {
        models,
        datasets,
        roles,
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
                datasets.push(DatasetInfo {
                    id,
                    path: path.to_string_lossy().to_string(),
                    pr_count,
                });
            }
        }
    }

    datasets.sort_by(|a, b| b.pr_count.cmp(&a.pr_count));
    datasets
}

fn count_prs_in_dir(dir: &Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "json") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(val) =
                        serde_json::from_str::<std::collections::HashMap<String, serde_json::Value>>(
                            &content,
                        )
                    {
                        if let Some(entries) = val.get("entries").and_then(|v| v.as_array()) {
                            count += entries.len();
                        }
                    }
                }
            }
        }
    }
    count
}
