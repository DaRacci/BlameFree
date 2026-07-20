//! API handlers for benchmark runs: list, detail, start.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{fs, time};

use crate::api::not_found;
use crate::harness;
use crate::server::{ActiveRun, AppState};
use axum::Json;
use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;
use crb_harness::paths;
use crb_shared::fs::compute_duration_from_dir;
use crb_types::benchmark::metrics::Metrics;
use crb_types::benchmark::result::PrResult;
use crb_types::capabilities::ReasoningEffort;
use crb_types::cost::AnalyticsSnapshot;
use crb_types::review::{Review, ReviewMetadata, ReviewStatus};
use crb_types::vcs::pr::PrMeta;
use crb_types::wrappers::Model;
use crb_webui_shared::config::AgentInfo;
use crb_webui_shared::review::AgentLogResponse;
use crb_webui_shared::review::LogsListResponse;
use crb_webui_shared::review::PrAgentEntry;
use crb_webui_shared::review::PrAgentsResponse;
use crb_webui_shared::review::PrLogsEntry;
use crb_webui_shared::review::RunConfig;
use crb_webui_shared::routes::{API_RUNS_ID_DETAILS_KEY, API_RUNS_ID_LOGS_KEY_ROLE};
use mti::prelude::{MagicTypeIdExt, V7};
use riv_stor::traits::Store;
use rustls::pki_types::UnixTime;
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

/// Local response struct replacing the deprecated `RunDetail`.
#[derive(Debug, Clone, Serialize)]
pub struct RunDetailResponse {
    pub meta: Review,
    pub results: Vec<PrResult>,
    pub aggregate: Metrics,
    #[serde(default)]
    pub config: Option<RunConfig>,
}

/// Iterate over `.json` files in a directory, yielding (file_path, file_name) pairs.
/// Skips files whose name starts with `_` and the summary file.
#[deprecated = "This should be migrated to harness crate"]
pub fn iter_json_files(dir: &Path) -> impl Iterator<Item = (PathBuf, String)> {
    let iter: Box<dyn Iterator<Item = _>> = if let Ok(entries) = fs::read_dir(dir) {
        Box::new(entries.flatten().filter_map(|entry| {
            let path = entry.path();
            if path.extension().map_or(true, |e| e != "json") {
                return None;
            }
            let fname = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if fname.starts_with('_') || fname == paths::SUMMARY_FILE {
                return None;
            }
            Some((path, fname))
        }))
    } else {
        Box::new(std::iter::empty())
    };

    iter
}

/// Configuration for starting a new benchmark run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[deprecated = "There should be another type that is already used in the benchmark crate"]
pub struct BenchmarkConfig {
    pub model: String,

    #[serde(default = "crb_shared::default_model")]
    pub judge_model: String,

    #[serde(default = "default_dataset_dir", alias = "dataset")]
    pub dataset_dir: String,

    #[serde(default = "default_max_findings")]
    pub max_findings: usize,

    #[serde(default)]
    pub agents: Vec<AgentInfo>,

    #[serde(default)]
    pub pr_filter: Option<String>,

    /// Reasoning effort for supported models
    #[serde(default)]
    pub reasoning_effort: Option<ReasoningEffort>,
}

#[deprecated]
fn default_dataset_dir() -> String {
    "datasets/golden_comments".to_string()
}

#[deprecated]
fn default_max_findings() -> usize {
    20
}

impl From<&BenchmarkConfig> for RunConfig {
    fn from(cfg: &BenchmarkConfig) -> Self {
        RunConfig {
            model: cfg.model.clone(),
            dataset: cfg.dataset_dir.clone(),
            agents: cfg.agents.clone(),
        }
    }
}

/// List all benchmark runs, including active and completed runs.
#[instrument(skip(state), name = API_RUNS_ID_DETAILS_KEY)]
pub async fn list_runs(State(state): State<AppState>) -> impl IntoResponse {
    let store = &state.store;
    let mut runs: Vec<Review> = match store.list::<Review>(&()).await {
        Ok(r) => r,
        Err(_) => Vec::new(),
    };

    // Add active runs that aren't yet persisted to the store
    {
        let active = state.active_runs.read().await;
        for (id, _ar) in active.iter() {
            if runs.iter().any(|r| r.id == *id) {
                continue;
            }

            runs.push(Review {
                id: id.as_str().create_type_id::<V7>(),
                agent_sessions: HashMap::new(),
                analytics: None,
                duration: Some(Duration::from_secs_f64(0.0)),
                status: ReviewStatus::Running,
                metadata: ReviewMetadata::Plain,
            });
        }
    }

    runs.sort_by(|a, b| {
        let a_running = a.status == ReviewStatus::Running;
        let b_running = b.status == ReviewStatus::Running;
        // Active runs come first
        a_running.cmp(&b_running).reverse()
    });

    Json(runs).into_response()
}

/// Scan a run directory and compute summary metrics.
fn scan_run_dir(path: &Path, name: &str) -> Result<Review, String> {
    let entries = fs::read_dir(path).map_err(|e| e.to_string())?;
    let mut results = Vec::new();
    let mut duration_secs = 0.0f64;
    let mut has_summary = false;

    for entry in entries.flatten() {
        let file_path = entry.path();
        if file_path.extension().map_or(true, |e| e != "json") {
            continue;
        }
        let file_name = file_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // TODO: HOLY FUCK CLEAN THIS SHIT UP
        if file_name == paths::SUMMARY_FILE {
            if let Ok(content) = fs::read_to_string(&file_path) {
                if let Ok(summary) =
                    serde_json::from_str::<HashMap<String, serde_json::Value>>(&content)
                {
                    has_summary = true;
                    if let Some(metrics) = summary.get("aggregate_metrics") {
                        if let Some(_) = metrics.as_object() {
                            duration_secs = summary
                                .get("duration_secs")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.0);

                            return Ok(Review {
                                id: name.to_string().create_type_id::<V7>(),
                                agent_sessions: HashMap::new(),
                                analytics: None,
                                duration: Some(Duration::from_secs_f64(duration_secs)),
                                status: ReviewStatus::Completed,
                                metadata: ReviewMetadata::Plain,
                            });
                        }
                    }
                }
            }
            continue;
        }

        if let Ok(content) = fs::read_to_string(&file_path) {
            if let Ok(pr_result) = serde_json::from_str::<PrResult>(&content) {
                results.push(pr_result);
            }
        }
    }

    if results.is_empty() {
        return Err("no results found".to_string());
    }

    // Fallback: compute duration from file timestamps if not found in summary
    if duration_secs == 0.0 && !has_summary {
        duration_secs = compute_duration_from_dir(path);
    }

    Ok(Review {
        id: name.to_string().create_type_id::<V7>(),
        agent_sessions: HashMap::new(),
        analytics: None,
        duration: Some(Duration::from_secs_f64(duration_secs)),
        status: ReviewStatus::Completed,
        metadata: ReviewMetadata::Plain,
    })
}

fn get_file_modified(path: &Path) -> String {
    if let Ok(metadata) = fs::metadata(path) {
        if let Ok(modified) = metadata.modified() {
            if let Ok(duration) = modified.duration_since(time::UNIX_EPOCH) {
                let secs = duration.as_secs();
                let naive = chrono::DateTime::from_timestamp(secs as i64, 0)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_else(|| "unknown".to_string());
                return naive;
            }
        }
    }
    "unknown".to_string()
}

/// Build a Review for a run still in memory (not yet written to disk).
fn format_running_response(id: &str, active_run: &ActiveRun) -> impl IntoResponse {
    let review = Review {
        id: id.to_string().create_type_id::<V7>(),
        agent_sessions: HashMap::new(),
        analytics: None,
        duration: None,
        status: ReviewStatus::Running,
        metadata: ReviewMetadata::Plain,
    };
    Json(review).into_response()
}

/// Read `_summary.json` from a run directory, if it exists.
fn read_summary_from_dir(run_path: &Path) -> Option<(String, f64, f64, usize)> {
    let summary_path = run_path.join(paths::SUMMARY_FILE);
    let content = fs::read_to_string(&summary_path).ok()?;
    let summary: HashMap<String, serde_json::Value> = serde_json::from_str(&content).ok()?;
    Some((
        summary
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        summary
            .get("duration_secs")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        summary
            .get("total_cost_usd")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        summary
            .get("total_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize,
    ))
}

/// Read all PR result files from a run directory.
fn read_pr_results_from_dir(run_path: &Path, cache_dir: &Option<PathBuf>) -> Vec<PrResult> {
    let mut results = Vec::new();
    let entries = match fs::read_dir(run_path) {
        Ok(e) => e,
        Err(_) => return results,
    };

    for entry in entries.flatten() {
        let file_path = entry.path();
        if file_path.extension().map_or(true, |e| e != "json") {
            continue;
        }
        let file_name = file_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // Skip metadata files (anything starting with _)
        if file_name.starts_with('_') {
            continue;
        }

        let content = match fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        if let Ok(pr_result) = serde_json::from_str::<PrResult>(&content) {
            results.push(pr_result);
        }
    }

    results
}

/// Compute aggregate metrics from PR results.
fn compute_aggregate_metrics(
    _results: &[PrResult],
    _total_cost: f64,
    duration_secs: f64,
) -> Metrics {
    Metrics {
        true_positives: 0,
        false_positives: 0,
        false_negatives: 0,
        duration_secs,
    }
}

/// Get detailed run results.
pub async fn get_run(State(state): State<AppState>, AxumPath(id): AxumPath<String>) -> Response {
    tracing::info!("GET /api/runs/{}", id);

    // Check if run is still in progress (in active_runs before store has it)
    let active_run_config = {
        let runs = state.active_runs.read().await;
        runs.get(&id.as_str().create_type_id::<V7>()).cloned()
    };

    if active_run_config.is_some() {
        if let Some(ref active_run) = active_run_config {
            return format_running_response(&id, active_run).into_response();
        }
    }

    let store = &state.store;
    let run_id_mtid = id.as_str().create_type_id::<V7>();

    // Load Review from store
    let review = match store.load::<Review>(&run_id_mtid.to_string()).await {
        Ok(r) => r,
        Err(_) => {
            tracing::error!("Run not found: {}", id);
            return not_found(format!("Run not found: {}", id)).into_response();
        }
    };

    // Load PR results from store (all results; filtering by run is a future refinement)
    let results: Vec<PrResult> = store.list::<PrResult>(&()).await.unwrap_or_default();

    let duration_secs = review.duration.map(|d| d.as_secs_f64()).unwrap_or(0.0);

    // Merge config from active run state if available (it isn't stored on disk)
    let config = active_run_config
        .as_ref()
        .map(|ar| RunConfig::from(&ar.run_config));

    let aggregate = compute_aggregate_metrics(&results, 0.0, duration_secs);

    let detail = RunDetailResponse {
        meta: review,
        results,
        aggregate,
        config,
    };

    Json(detail).into_response()
}

/// Start a new benchmark run.
pub async fn start_run(
    State(state): State<AppState>,
    Json(config): Json<BenchmarkConfig>,
) -> impl IntoResponse {
    tracing::info!(
        "POST /api/runs — model={}, dataset={}, roles={:?}",
        config.model,
        config.dataset_dir,
        config.agents,
    );
    let run_id = format!(
        "run-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    );

    // Calculate total PRs before constructing ActiveRun so the frontend can see it immediately when polling
    // Resolve dataset directory: the config stores just the dataset ID (e.g. "golden_comments"),
    // but the actual path is relative to the server's base dataset_dir (e.g. "datasets/golden_comments").
    let dataset_dir = state.config.server.dataset_dir.join(&config.dataset_dir);
    let total_prs = count_prs_in_dataset(&dataset_dir);

    let (tx, _rx) = tokio::sync::broadcast::channel::<crb_types::RunEvent>(1024);

    let active_run = ActiveRun {
        created_at: UnixTime::now(),
        run_config: config.clone(),
        tx: tx.clone(),
    };

    {
        let mut runs = state.active_runs.write().await;
        runs.insert(run_id.as_str().create_type_id::<V7>(), active_run);
    }

    let output_dir = state.output_dir.clone();
    let run_id_clone = run_id.clone();
    let active_runs = state.active_runs.clone();
    let config_clone = config.clone();
    let benchmark_dir = state.config.server.benchmark_dir.clone();
    let dataset_dir_clone = dataset_dir.clone();
    let store = state.store.clone();

    tokio::spawn(async move {
        if let Err(e) = harness::run_harness(
            &run_id_clone,
            &config_clone,
            &output_dir,
            benchmark_dir.as_deref(),
            tx,
            active_runs,
            &dataset_dir_clone,
            store,
        )
        .await
        {
            tracing::error!("Harness run {} failed: {}", run_id_clone, e);
        }
    });

    let response = Review {
        id: run_id.as_str().create_type_id::<V7>(),
        agent_sessions: HashMap::new(),
        analytics: None,
        duration: None,
        status: ReviewStatus::Running,
        metadata: ReviewMetadata::Plain,
    };

    (StatusCode::CREATED, Json(response))
}

/// Count PR entries in a dataset directory.
fn count_prs_in_dataset(dataset_dir: &Path) -> usize {
    if !dataset_dir.exists() {
        return 0;
    }
    let mut count = 0;
    if let Ok(entries) = fs::read_dir(dataset_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    // Try parsing as an object with "entries" key first
                    if let Ok(val) =
                        serde_json::from_str::<HashMap<String, serde_json::Value>>(&content)
                    {
                        if let Some(entries) = val.get("entries").and_then(|v| v.as_array()) {
                            count += entries.len();
                            continue;
                        }
                    }
                    // Try parsing as a raw array
                    if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&content) {
                        count += arr.len();
                    }
                }
            }
        }
    }
    count
}

/// Scan a PR cache directory for agent log files and return deduplicated agents.
fn scan_agent_roles(pr_cache_dir: &Path) -> Vec<AgentInfo> {
    use std::collections::BTreeSet;
    let mut roles = BTreeSet::new();

    // Try content-addressed layout first: agents/*.agent_{role}_prompt.txt
    let agents_dir = pr_cache_dir.join("agents");
    if agents_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&agents_dir) {
            for entry in entries.flatten() {
                let fname = entry.file_name().to_string_lossy().to_string();
                // Match: <hash>.agent_{role}_prompt.txt or <hash>.agent_{role}_response.txt
                if let Some(rest) = fname.strip_suffix("_prompt.txt") {
                    if let Some(role) = rest.rsplit(".agent_").next() {
                        roles.insert(role.to_string());
                    }
                } else if let Some(rest) = fname.strip_suffix("_response.txt") {
                    if let Some(role) = rest.rsplit(".agent_").next() {
                        roles.insert(role.to_string());
                    }
                }
            }
        }
    }

    // Also check simple layout: agent_{role}_prompt.txt / agent_{role}_response.txt
    if let Ok(entries) = fs::read_dir(pr_cache_dir) {
        for entry in entries.flatten() {
            let fname = entry.file_name().to_string_lossy().to_string();
            if let Some(rest) = fname.strip_prefix("agent_") {
                if let Some(role) = rest
                    .strip_suffix("_prompt.txt")
                    .or_else(|| rest.strip_suffix("_response.txt"))
                {
                    roles.insert(role.to_string());
                }
            }
        }
    }

    roles
        .into_iter()
        .map(|abbr| AgentInfo {
            abbreviation: abbr.clone(),
            name: abbr,
            incompatible_with_roles: vec![],
        })
        .collect()
}

/// Try to read an agent log file, returning the contents lossy-decoded.
fn read_agent_log_file(cache_dir: &Path, pr_key: &str, role: &str, suffix: &str) -> Option<String> {
    let pr_dir = cache_dir.join(pr_key);

    // Content-addressed layout: agents/*.agent_{role}_{suffix}.txt
    let agents_dir = pr_dir.join("agents");
    if agents_dir.is_dir() {
        let pattern = format!(".agent_{}_{}.txt", role, suffix);
        if let Ok(entries) = fs::read_dir(&agents_dir) {
            for entry in entries.flatten() {
                let fname = entry.file_name().to_string_lossy().to_string();
                if fname.ends_with(&pattern) {
                    if let Ok(content) = fs::read(entry.path()) {
                        return Some(String::from_utf8_lossy(&content).to_string());
                    }
                }
            }
        }
    }

    // Simple layout: agent_{role}_{suffix}.txt
    let simple_path = pr_dir.join(format!("agent_{}_{}.txt", role, suffix));
    if simple_path.is_file() {
        if let Ok(content) = fs::read(&simple_path) {
            return Some(String::from_utf8_lossy(&content).to_string());
        }
    }

    None
}

/// Resolve the actual cache directory for a given run, trying multiple layouts:
/// 1. `output_dir/<run_id>/cache/` (harness writes agents here)
/// 2. `output_dir.parent()/cache/<run_id>/` (nested by run_id)
/// 3. `output_dir.parent()/cache/` (flat, no run_id subdirectory)
fn resolve_cache_dir(output_dir: &Path, _run_id: &str) -> Option<PathBuf> {
    let base_dir = output_dir.parent().unwrap_or(Path::new("."));
    let candidates = [
        // New layout: output/_cache/ (flat, shared across runs)
        output_dir.join(crb_cache::paths::CACHE_DIR_NAME),
        // Legacy layouts (backward compat):
        output_dir.join(_run_id).join("cache"),
        base_dir.join("cache").join(_run_id),
        base_dir.join("cache"),
    ];
    for path in &candidates {
        if path.is_dir() {
            return Some(path.clone());
        }
    }
    None
}

/// List available log files for a run
///
/// Merges PRs from the output directory (canonical source) with cache entries.
/// All PRs with output files are shown; cache entries add agent roles where available.
pub async fn list_logs(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> impl IntoResponse {
    tracing::info!("GET /api/runs/{}/logs", id);

    let run_path = state.output_dir.join(&id);
    let cache_dir = resolve_cache_dir(&state.output_dir, &id);

    // 1. Collect PR keys from the output directory (canonical source)
    let mut output_prs: Vec<(String, String)> = Vec::new(); // (pr_key, pr_title)
    if run_path.is_dir() {
        if let Ok(entries) = fs::read_dir(&run_path) {
            for entry in entries.flatten() {
                let file_path = entry.path();
                if file_path.extension().map_or(true, |e| e != "json") {
                    continue;
                }
                let fname = file_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                if fname == paths::SUMMARY_FILE || fname.starts_with("candidates") {
                    continue;
                }

                let stem = file_path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                if stem.is_empty() || stem.starts_with('_') || stem.starts_with('.') {
                    continue;
                }

                let title = if let Ok(content) = fs::read_to_string(&file_path) {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                        val.get("pr_title")
                            .and_then(|v| v.as_str())
                            .unwrap_or(&stem)
                            .to_string()
                    } else {
                        stem.clone()
                    }
                } else {
                    stem.clone()
                };
                output_prs.push((stem, title));
            }
        }
    }

    // 2. Collect PR keys from the cache directory (supplementary)
    let mut cached_prs: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    if let Some(ref cd) = cache_dir {
        if let Ok(entries) = fs::read_dir(cd) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let pr_key = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                if pr_key.starts_with('_') || pr_key.starts_with('.') {
                    continue;
                }
                cached_prs.insert(pr_key);
            }
        }
    }

    // 3. Merge: use output PRs as canonical list, supplement with cache-only PRs
    let mut all_pr_keys: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for (key, _) in &output_prs {
        all_pr_keys.insert(key.clone());
    }
    for key in &cached_prs {
        all_pr_keys.insert(key.clone());
    }

    let mut prs: Vec<PrLogsEntry> = Vec::new();
    for pr_key in &all_pr_keys {
        // Resolve title: first from output PRs, then from cache (via resolve_pr_title)
        #[allow(deprecated)]
        let pr_title = output_prs
            .iter()
            .find(|(k, _)| k == pr_key)
            .map(|(_, t)| t.clone())
            .unwrap_or_else(|| resolve_pr_title(&state.output_dir, &id, pr_key));

        // TODO: Domain-type path
        // When a `Review` with populated `agent_sessions` is available,
        // construct PrLogsEntry via:
        //   From<(PrMeta, &HashMap<MagicTypeId, AgentSession>)> for PrLogsEntry
        //
        // Example (commented out until Review is threaded through):
        //   let pr_meta = PrMeta { title: pr_title, url: ..., number: ... };
        //   let entry = PrLogsEntry::from((pr_meta, &review.agent_sessions));

        // Scan agents from cache if available
        let agents = if let Some(ref cd) = cache_dir {
            let pr_dir = cd.join(pr_key);
            if pr_dir.is_dir() {
                scan_agent_roles(&pr_dir)
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        prs.push(PrLogsEntry {
            meta: PrMeta {
                title: pr_title,
                url: String::new(),
                number: pr_key.parse().unwrap_or(0),
            },
            agents,
        });
    }

    Json(LogsListResponse { run_id: id, prs }).into_response()
}

/// Try to resolve a PR title from the run's output files.
#[deprecated = "This should be migrated to benchmark crate"]
fn resolve_pr_title(output_dir: &Path, run_id: &str, pr_key: &str) -> String {
    // The pr_key could be a number or URL fragment; try to find a matching result file
    let run_path = output_dir.join(run_id);
    if !run_path.is_dir() {
        return pr_key.to_string();
    }

    let Ok(entries) = fs::read_dir(&run_path) else {
        return pr_key.to_string();
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map_or(true, |e| e != "json") {
            continue;
        }

        let fname = path.file_name().unwrap_or_default().to_string_lossy();
        if fname == paths::SUMMARY_FILE {
            continue;
        }

        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };

        let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) else {
            continue;
        };

        let pr_title = val
            .get("pr_title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let pr_url = val
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Match by PR number from URL being equal to parsed pr_key
        let pr_num_from_url = pr_url
            .rsplit('/')
            .next()
            .and_then(|s| s.parse::<u32>().ok());
        let pr_num_from_key = pr_key.parse::<u32>().ok();
        if pr_num_from_url.is_some() && pr_num_from_url == pr_num_from_key {
            if !pr_title.is_empty() {
                return pr_title;
            }
        }
        // Also match if the filename contains the pr_key (filename is often {pr_number}.json)
        if fname.contains(pr_key) && !pr_title.is_empty() {
            return pr_title;
        }
    }

    pr_key.to_string()
}

/// Get specific agent log
#[instrument(skip(state), name = API_RUNS_ID_LOGS_KEY_ROLE)]
pub async fn get_agent_log(
    State(state): State<AppState>,
    AxumPath((id, pr_key, role)): AxumPath<(String, String, String)>,
) -> impl IntoResponse {
    let cache_dir = match resolve_cache_dir(&state.output_dir, &id) {
        Some(d) => d,
        None => {
            // TODO: Domain-type path
            // When an `AgentSession` is available, construct via:
            //   AgentLogResponse::from((&run_id, &agent_session))
            return Json(AgentLogResponse {
                run_id: id.clone(),
                prompt: None,
                response: None,
                reasoning: None,
                available: false,
            })
            .into_response();
        }
    };

    let pr_dir = cache_dir.join(&pr_key);
    if !pr_dir.exists() || !pr_dir.is_dir() {
        return Json(AgentLogResponse {
            run_id: id,
            prompt: None,
            response: None,
            reasoning: None,
            available: false,
        })
        .into_response();
    }

    let prompt = read_agent_log_file(&cache_dir, &pr_key, &role, "prompt");
    let response = read_agent_log_file(&cache_dir, &pr_key, &role, "response");
    let reasoning = read_agent_log_file(&cache_dir, &pr_key, &role, "reasoning");
    let available = prompt.is_some() || response.is_some() || reasoning.is_some();

    // TOOD: When an AgentSession is available, replace the above file reads with:
    //   AgentLogResponse::from((&id.as_str(), &agent_session))

    Json(AgentLogResponse {
        run_id: id,
        prompt,
        response,
        reasoning,
        available,
    })
    .into_response()
}

/// Get agent availability info for a single PR
///
/// Returns the PR title and which agents have cached log files.
pub async fn get_pr_agents(
    State(state): State<AppState>,
    AxumPath((id, pr_key)): AxumPath<(String, String)>,
) -> impl IntoResponse {
    tracing::info!("GET /api/runs/{}/prs/{}", id, pr_key);

    let cache_dir = resolve_cache_dir(&state.output_dir, &id);
    #[allow(deprecated)]
    let pr_title = resolve_pr_title(&state.output_dir, &id, &pr_key);

    // Scan agents from cache
    let agents = if let Some(ref cd) = cache_dir {
        let pr_dir = cd.join(&pr_key);
        if pr_dir.is_dir() {
            let roles = scan_agent_roles(&pr_dir);
            // For each role, check which log files exist
            let mut entries: Vec<PrAgentEntry> = Vec::new();
            for role in roles {
                let has_prompt =
                    read_agent_log_file(cd, &pr_key, &role.abbreviation, "prompt").is_some();
                let has_response =
                    read_agent_log_file(cd, &pr_key, &role.abbreviation, "response").is_some();
                let has_reasoning =
                    read_agent_log_file(cd, &pr_key, &role.abbreviation, "reasoning").is_some();
                entries.push(PrAgentEntry {
                    role: role.abbreviation,
                    has_prompt,
                    has_response,
                    has_reasoning,
                });
            }
            entries
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    // TODO: Domain-type path
    // When agent sessions are available, construct PrAgentEntry via:
    //   PrAgentEntry::from((&role_abbreviation, &agent_session))
    // And PrAgentsResponse via:
    //   PrAgentsResponse::from((&run_id, &pr_key, &pr_title, &agent_sessions))
    // The file-based scanning above is the fallback until Review.agent_sessions
    // is threaded through these handlers.
    //
    // Example:
    //   let resp = PrAgentsResponse::from((
    //       &id, &pr_key, &pr_title, &review.agent_sessions,
    //   ));

    // Check if output file exists for this PR
    let has_output = {
        let run_path = state.output_dir.join(&id);
        if run_path.is_dir() {
            let pr_key_lower = pr_key.to_lowercase();
            let mut found = false;
            if let Ok(entries) = fs::read_dir(&run_path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map_or(true, |e| e != "json") {
                        continue;
                    }
                    let fname = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_lowercase();
                    if fname == paths::SUMMARY_FILE || fname.starts_with("candidates") {
                        continue;
                    }
                    if fname.contains(&pr_key_lower) {
                        found = true;
                        break;
                    }
                }
            }
            found
        } else {
            false
        }
    };

    Json(PrAgentsResponse {
        run_id: id,
        pr_key,
        pr_title,
        agents,
        has_output,
    })
    .into_response()
}

/// Get full details for a specific PR from its result file
#[instrument(skip(state), name = API_RUNS_ID_DETAILS_KEY)]
pub async fn get_pr_detail(
    State(state): State<AppState>,
    AxumPath((id, pr_key)): AxumPath<(String, String)>,
) -> impl IntoResponse {
    let run_path = state.output_dir.join(&id);
    if !run_path.exists() || !run_path.is_dir() {
        return not_found(format!("Run not found: {id}"));
    }

    use crate::api::runs::iter_json_files;

    let pr_key_lower = pr_key.to_lowercase();
    #[allow(deprecated)]
    for (file_path, fname) in iter_json_files(&run_path) {
        // Match by filename containing pr_key, or by PR number extracted from URL
        let content = match fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let val: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Match by filename containing pr_key (normalize spaces to underscores for filename matching),
        // or by PR number extracted from URL, or by PR title containing pr_key
        let pr_url = val
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let pr_title = val
            .get("pr_title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let pr_num_from_url = pr_url
            .rsplit('/')
            .next()
            .and_then(|s| s.parse::<u32>().ok());
        let pr_num_from_key = pr_key.parse::<u32>().ok();
        // Normalize spaces to underscores in the pr_key for filename matching (files use underscores)
        let pr_key_normalized = pr_key_lower.replace(' ', "_");
        let fname_lower = fname.to_lowercase();
        let matches = fname_lower.contains(&pr_key_lower)
            || fname_lower.contains(&pr_key_normalized)
            || (pr_num_from_url.is_some() && pr_num_from_url == pr_num_from_key)
            || pr_title.to_lowercase().contains(&pr_key_lower);

        debug!(
            "pr-detail matching: pr_key='{}', fname='{}', fname_lower='{}', pr_key_normalized='{}', matches={}",
            pr_key, fname, fname_lower, pr_key_normalized, matches
        );

        if matches {
            let metrics = val
                .get("metrics")
                .and_then(|v| serde_json::from_value::<Metrics>(v.clone()).ok())
                .unwrap_or_default();
            let cost = val
                .get("cost")
                .and_then(|v| serde_json::from_value::<AnalyticsSnapshot>(v.clone()).ok());
            #[allow(deprecated)]
            return Json(PrResult {
                id: pr_key.as_str().create_type_id::<V7>(),
                golden_comments: vec![],
                benchmark_id: None,
                findings_with_verdicts: vec![],
            })
            .into_response();
        }
    }

    not_found(format!("PR not found: {pr_key}"))
}
