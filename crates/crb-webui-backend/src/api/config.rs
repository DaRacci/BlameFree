//! API handler for configuration endpoints.

use std::fs;
use std::path::Path;

use axum::Json;
use axum::extract::{Path as AxumPath, State};
use crb_webui_shared::routes::{API_CONFIG, API_CONFIG_DATASETS, API_DATASETS_ID_PRS};
use serde::Serialize;
use tracing::{error, instrument, warn};

use crate::server::AppState;
use crb_webui_shared::config::AppConfig;
use crb_webui_shared::config::DatasetInfo;
use crb_webui_shared::config::PrEntry;
use crb_webui_shared::config::RoleInfo;

/// Information about an available model.
#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
}

/// List available models, datasets, and roles.
#[instrument(skip_all, name = API_CONFIG, fields(models = %state.models, dataset_dir = %state.dataset_dir.display()))]
pub async fn get_config(State(state): State<AppState>) -> Json<AppConfig> {
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

    let lib = crb_agents::prompts::PromptLibrary::get_instance();
    let mut roles: Vec<RoleInfo> = lib
        .agents()
        .iter()
        .map(|agent| RoleInfo {
            abbreviation: agent.role_abbreviation.clone(),
            name: agent.role_abbreviation.clone(),
            incompatible_with_roles: agent.incompatible_with_roles.clone(),
        })
        .collect();
    roles.sort_by(|a, b| a.abbreviation.cmp(&b.abbreviation));

    Json(AppConfig {
        models,
        datasets,
        roles,
        auth_enabled: state.config.oauth.is_some(),
    })
}

/// List available datasets with PR counts.
#[instrument(skip_all, name = API_CONFIG_DATASETS, fields(dataset_dir = %state.dataset_dir.display()))]
pub async fn list_datasets(State(state): State<AppState>) -> Json<Vec<DatasetInfo>> {
    Json(scan_datasets(&state.dataset_dir))
}

fn scan_datasets(dataset_dir: &Path) -> Vec<DatasetInfo> {
    let mut datasets = Vec::new();

    if !dataset_dir.exists() {
        return datasets;
    }

    let Ok(entries) = fs::read_dir(dataset_dir) else {
        error!(
            "Failed to read dataset directory: {}",
            dataset_dir.display()
        );
        return datasets;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

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

    datasets.sort_by(|a, b| b.pr_count.cmp(&a.pr_count));
    datasets
}

fn count_prs_in_dir(dir: &Path) -> usize {
    let mut count = 0;
    let Ok(entries) = fs::read_dir(dir) else {
        return count;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.extension().map_or(false, |e| e == "json") {
            continue;
        }

        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };

        let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) else {
            continue;
        };

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
    count
}

/// List all PRs in a dataset.
#[instrument(skip_all, name = API_DATASETS_ID_PRS, fields(dataset_id = %id))]
pub async fn list_dataset_prs(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Json<Vec<PrEntry>> {
    const EMPTY_VEC: Vec<PrEntry> = Vec::new();
    let dataset_dir = state.dataset_dir.join(&id);

    if !dataset_dir.exists() || !dataset_dir.is_dir() {
        warn!("Dataset directory not found: {}", dataset_dir.display());
        return Json(EMPTY_VEC);
    }

    let Ok(entries) = fs::read_dir(&dataset_dir) else {
        error!(
            "Failed to read dataset directory: {}",
            dataset_dir.display()
        );
        return Json(EMPTY_VEC);
    };

    let mut prs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "json") {
            if let Ok(content) = fs::read_to_string(&path) {
                read_prs_from_json(&path, &content, &mut prs);
            }
        }
    }

    Json(prs)
}

/// Parse PR entries from a dataset JSON file and append them to `prs`.
fn read_prs_from_json(path: &Path, content: &str, prs: &mut Vec<PrEntry>) {
    // Try parsing as array first
    let items: Vec<serde_json::Value> = match serde_json::from_str(content) {
        Ok(val) => match val {
            serde_json::Value::Array(arr) => arr,
            serde_json::Value::Object(obj) => obj
                .get("entries")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default(),
            _ => return,
        },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_prs_in_dir_empty() {
        let dir = tempfile::tempdir().expect("temp dir");
        let count = count_prs_in_dir(dir.path());
        insta::assert_debug_snapshot!(count);
    }

    #[test]
    fn test_count_prs_in_dir_single_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file_path = dir.path().join("test.json");
        let json = r#"[
            {"url": "https://github.com/owner/repo/pull/1", "pr_title": "Fix bug"},
            {"url": "https://github.com/owner/repo/pull/2", "pr_title": "Add feature"}
        ]"#;
        std::fs::write(&file_path, json).expect("write test file");
        insta::assert_debug_snapshot!(count_prs_in_dir(dir.path()));
    }

    #[test]
    fn test_count_prs_in_dir_object_format() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file_path = dir.path().join("test.json");
        let json = r#"{
            "entries": [
                {"url": "https://github.com/owner/repo/pull/1", "pr_title": "Fix"},
                {"url": "https://github.com/owner/repo/pull/2", "pr_title": "Feature"},
                {"url": "https://github.com/owner/repo/pull/3", "pr_title": "Docs"}
            ]
        }"#;
        std::fs::write(&file_path, json).expect("write test file");
        insta::assert_debug_snapshot!(count_prs_in_dir(dir.path()));
    }

    #[test]
    fn test_count_prs_in_dir_multiple_files() {
        let dir = tempfile::tempdir().expect("temp dir");
        let f1 = dir.path().join("a.json");
        std::fs::write(
            &f1,
            r#"[{"url":"https://github.com/a/b/pull/1","pr_title":"x"}]"#,
        )
        .expect("write");
        let f2 = dir.path().join("b.json");
        std::fs::write(
            &f2,
            r#"[{"url":"https://github.com/c/d/pull/2","pr_title":"y"}]"#,
        )
        .expect("write");
        insta::assert_debug_snapshot!(count_prs_in_dir(dir.path()));
    }

    #[test]
    fn test_count_prs_in_dir_non_json_ignored() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("readme.txt"), "hello").expect("write");
        std::fs::write(dir.path().join("config.toml"), "[section]").expect("write");
        insta::assert_debug_snapshot!(count_prs_in_dir(dir.path()));
    }

    #[test]
    fn test_count_prs_in_dir_nonexistent() {
        insta::assert_debug_snapshot!(count_prs_in_dir(Path::new("/nonexistent/path")));
    }

    #[test]
    fn test_read_prs_from_json_array() {
        let path = Path::new("discourse.json");
        let json = r#"[
            {"url": "https://github.com/discourse/discourse/pull/100", "pr_title": "Fix"},
            {"url": "https://github.com/discourse/discourse/pull/200", "pr_title": "Enhance"}
        ]"#;
        let mut prs = Vec::new();
        read_prs_from_json(path, json, &mut prs);
        insta::assert_debug_snapshot!(prs);
    }

    #[test]
    fn test_read_prs_from_json_object_with_entries() {
        let path = Path::new("test.json");
        let json = r#"{
            "entries": [
                {"url": "https://github.com/rust-lang/rust/pull/1", "pr_title": "PR #1"},
                {"url": "https://github.com/rust-lang/rust/pull/2", "pr_title": "PR #2"}
            ]
        }"#;
        let mut prs = Vec::new();
        read_prs_from_json(path, json, &mut prs);
        insta::assert_debug_snapshot!(prs);
    }

    #[test]
    fn test_read_prs_from_json_empty() {
        let path = Path::new("test.json");
        let json = r#"[]"#;
        let mut prs = Vec::new();
        read_prs_from_json(path, json, &mut prs);
        insta::assert_debug_snapshot!(prs);
    }

    #[test]
    fn test_read_prs_from_json_invalid() {
        let path = Path::new("test.json");
        let json = "not valid json";
        let mut prs = Vec::new();
        read_prs_from_json(path, json, &mut prs);
        insta::assert_debug_snapshot!(prs);
    }

    #[test]
    fn test_read_prs_from_json_missing_fields() {
        let path = Path::new("test.json");
        let json = r#"[
            {"url": "https://github.com/owner/repo/pull/5"}
        ]"#;
        let mut prs = Vec::new();
        read_prs_from_json(path, json, &mut prs);
        insta::assert_debug_snapshot!(prs);
    }

    #[test]
    fn test_scan_datasets_nonexistent_dir() {
        let result = scan_datasets(Path::new("/nonexistent/dataset/path"));
        insta::assert_debug_snapshot!(result.is_empty());
    }

    #[test]
    fn test_scan_datasets_empty_dir() {
        let dir = tempfile::tempdir().expect("temp dir");
        let result = scan_datasets(dir.path());
        insta::assert_debug_snapshot!(result.is_empty());
    }

    #[test]
    fn test_scan_datasets_with_subdirs() {
        let dir = tempfile::tempdir().expect("temp dir");
        let ds1 = dir.path().join("dataset-a");
        std::fs::create_dir(&ds1).expect("create ds1");
        // Add a PR file in the dataset
        std::fs::write(
            ds1.join("prs.json"),
            r#"[{"url":"https://github.com/a/b/pull/1","pr_title":"x"}]"#,
        )
        .expect("write prs");

        let ds2 = dir.path().join("dataset-b");
        std::fs::create_dir(&ds2).expect("create ds2");
        // Add a PR file
        std::fs::write(
            ds2.join("prs.json"),
            r#"[{"url":"https://github.com/c/d/pull/2","pr_title":"y"}]"#,
        )
        .expect("write prs");

        let result = scan_datasets(dir.path());
        insta::assert_debug_snapshot!(result.len());
        insta::assert_debug_snapshot!(result[0].pr_count);
        insta::assert_debug_snapshot!(result[1].pr_count);
    }
}
