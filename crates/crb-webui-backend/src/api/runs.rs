//! API handlers for benchmark runs: list, detail, start.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::harness;
use crate::server::{ActiveRun, AppState};

pub use crb_shared::RunSummary;

pub use crb_shared::RunDetail;

pub use crb_shared::CostJson;

/// A single PR result as it appears in the JSON files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrResultJson {
    #[serde(default)]
    pub pr_title: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub findings_count: usize,
    #[serde(default)]
    pub golden_count: usize,
    #[serde(default)]
    pub metrics: MetricsJson,
    #[serde(default)]
    pub verdicts: Vec<VerdictJson>,
    #[serde(default)]
    pub cost: Option<CostJson>,
    /// Raw findings JSON from agents (optional, may not exist in older files)
    #[serde(default)]
    pub findings: serde_json::Value,
    /// Raw agent response texts (optional, may not exist in older files)
    #[serde(default)]
    pub agent_responses: Vec<String>,
}

pub use crb_shared::PrResult;

pub use crb_shared::MetricsJson;

pub use crb_shared::VerdictJson;

pub use crb_shared::AggregateMetrics;

pub use crb_shared::RunConfig;

/// Configuration for starting a new benchmark run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    pub model: String,

    #[serde(default = "default_judge_model")]
    pub judge_model: String,

    #[serde(default = "default_dataset_dir", alias = "dataset")]
    pub dataset_dir: String,

    #[serde(default = "default_concurrency")]
    pub concurrency: usize,

    #[serde(default = "default_max_findings")]
    pub max_findings: usize,

    #[serde(default = "default_prompts_dir")]
    pub prompts_dir: String,

    pub cache_dir: Option<String>,

    #[serde(default = "default_roles", deserialize_with = "deserialize_roles")]
    pub roles: String,

    #[serde(default)]
    pub skip_consensus: bool,

    #[serde(default)]
    pub skip_linters: bool,

    #[serde(default)]
    pub pr_filter: Option<String>,

    #[serde(default = "default_use_cache")]
    pub use_cache: bool,

    /// Reasoning effort for supported models (None = disabled, Some = low/medium/high).
    #[serde(default)]
    pub reasoning_effort: Option<String>,
}

fn default_use_cache() -> bool {
    true
}

/// Deserialize `roles` from either a comma-separated string or a Vec<String>.
fn deserialize_roles<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    use serde::de;
    struct RolesVisitor;
    impl<'de> de::Visitor<'de> for RolesVisitor {
        type Value = String;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a string or array of strings")
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<String, E> {
            Ok(v.to_string())
        }
        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<String, A::Error> {
            let mut parts = Vec::new();
            while let Some(s) = seq.next_element::<String>()? {
                parts.push(s);
            }
            Ok(parts.join(","))
        }
    }
    d.deserialize_any(RolesVisitor)
}

fn default_judge_model() -> String {
    "deepseek/deepseek-v4-flash".to_string()
}

fn default_dataset_dir() -> String {
    "datasets/golden_comments".to_string()
}

fn default_concurrency() -> usize {
    4
}

fn default_max_findings() -> usize {
    20
}

fn default_prompts_dir() -> String {
    "prompts/builtin".to_string()
}

fn default_roles() -> String {
    "SA,CL,AR,SEC".to_string()
}

/// Response returned when a benchmark is started.
#[derive(Debug, Clone, Serialize)]
pub struct StartRunResponse {
    pub run_id: String,
    pub status: String,
    pub total_prs: usize,
}

pub use crb_shared::{
    AgentLogResponse, LogsListResponse, PrAgentEntry, PrAgentsResponse, PrDetailResponse,
    PrLogsEntry,
};

/// GET /api/runs — list all benchmark runs (both completed and active).
pub async fn list_runs(State(state): State<AppState>) -> impl IntoResponse {
    tracing::info!("GET /api/runs");
    let output_dir = state.output_dir.clone();
    let mut runs: Vec<RunSummary> = Vec::new();

    // 1) Read completed runs from disk
    if output_dir.exists() {
        let entries = match std::fs::read_dir(&output_dir) {
            Ok(entries) => entries,
            Err(_) => return Json(Vec::<RunSummary>::new()).into_response(),
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            if let Ok(summary) = scan_run_dir(&path, &name) {
                runs.push(summary);
            }
        }
    }

    // 2) Include active (in-memory) runs that haven't been written to disk yet
    {
        let active = state.active_runs.read().await;
        for (id, ar) in active.iter() {
            // Skip if already in the completed list (duplicate)
            if runs.iter().any(|r| r.id == *id) {
                continue;
            }
            runs.push(RunSummary {
                id: id.clone(),
                name: id.clone(),
                pr_count: ar.total_prs as u32,
                avg_f1: Some(0.0),
                avg_precision: Some(0.0),
                avg_recall: Some(0.0),
                total_cost: Some(0.0),
                total_tokens: 0,
                duration_secs: Some(0.0),
                created_at: format_timestamp(ar.created_at),
                model: Some(ar.config.model.clone()),
                status: if ar.finished {
                    "completed".to_string()
                } else {
                    "running".to_string()
                },
            });
        }
    }

    // 3) Sort: active (running) first by creation time, then completed by time
    runs.sort_by(|a, b| {
        let a_running = a.status == "running";
        let b_running = b.status == "running";
        // Active runs come first
        a_running
            .cmp(&b_running)
            .reverse()
            // Within same group, most recent first (created_at is RFC 3339, lexicographically sortable)
            .then_with(|| b.created_at.cmp(&a.created_at))
    });

    Json(runs).into_response()
}

/// Format a Unix timestamp seconds as an RFC 3339 string.
fn format_timestamp(secs: u64) -> String {
    chrono::DateTime::from_timestamp(secs as i64, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Compute duration from the newest and oldest file timestamps in a directory.
fn compute_duration_from_timestamps(path: &Path) -> f64 {
    use std::fs;
    let mut oldest = f64::MAX;
    let mut newest = 0.0f64;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.path().metadata() {
                if let Ok(modified) = meta.modified() {
                    let secs = modified
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs_f64();
                    if secs > 0.0 {
                        if secs < oldest {
                            oldest = secs;
                        }
                        if secs > newest {
                            newest = secs;
                        }
                    }
                }
            }
        }
    }
    if newest > oldest && oldest < f64::MAX {
        newest - oldest
    } else {
        0.0
    }
}

/// Scan a run directory and compute summary metrics.
fn scan_run_dir(path: &Path, name: &str) -> Result<RunSummary, String> {
    use std::fs;

    let entries = fs::read_dir(path).map_err(|e| e.to_string())?;
    let mut results = Vec::new();
    let mut total_cost = 0.0f64;
    let mut total_tokens = 0usize;
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

        if file_name == crb_harness::paths::SUMMARY_FILE {
            if let Ok(content) = fs::read_to_string(&file_path) {
                if let Ok(summary) =
                    serde_json::from_str::<HashMap<String, serde_json::Value>>(&content)
                {
                    has_summary = true;
                    if let Some(metrics) = summary.get("aggregate_metrics") {
                        if let Some(am) = metrics.as_object() {
                            let ag = AggregateMetrics {
                                avg_f1: am.get("avg_f1").and_then(|v| v.as_f64()).unwrap_or(0.0),
                                avg_precision: am
                                    .get("avg_precision")
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.0),
                                avg_recall: am
                                    .get("avg_recall")
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.0),
                                total_tp: am
                                    .get("total_true_positives")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0)
                                    as usize,
                                total_fp: am
                                    .get("total_false_positives")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0)
                                    as usize,
                                total_fn: am
                                    .get("total_false_negatives")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0)
                                    as usize,
                                total_cost: summary
                                    .get("total_cost_usd")
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.0),
                                total_prs: summary
                                    .get("total_prs")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0) as u32,
                                duration_secs: summary
                                    .get("duration_secs")
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.0),
                            };
                            let pr_count = summary
                                .get("total_prs")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0) as usize;
                            let model = summary
                                .get("model")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown")
                                .to_string();
                            duration_secs = summary
                                .get("duration_secs")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.0);
                            total_cost = summary
                                .get("total_cost_usd")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.0);
                            total_tokens = summary
                                .get("total_tokens")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0) as usize;

                            return Ok(RunSummary {
                                id: name.to_string(),
                                name: name.to_string(),
                                pr_count: pr_count as u32,
                                avg_f1: Some(ag.avg_f1),
                                avg_precision: Some(ag.avg_precision),
                                avg_recall: Some(ag.avg_recall),
                                total_cost: Some(total_cost),
                                total_tokens,
                                duration_secs: Some(duration_secs),
                                created_at: get_file_modified(path),
                                model: Some(model),
                                status: "completed".to_string(),
                            });
                        }
                    }
                }
            }
            continue;
        }

        if let Ok(content) = fs::read_to_string(&file_path) {
            if let Ok(pr_result) = serde_json::from_str::<PrResultJson>(&content) {
                results.push(pr_result);
            }
        }
    }

    if results.is_empty() {
        return Err("no results found".to_string());
    }

    let pr_count = results.len();
    let avg_f1 = results.iter().map(|r| r.metrics.f1).sum::<f64>() / pr_count as f64;
    let avg_precision = results.iter().map(|r| r.metrics.precision).sum::<f64>() / pr_count as f64;
    let avg_recall = results.iter().map(|r| r.metrics.recall).sum::<f64>() / pr_count as f64;

    // Aggregate per-PR cost if available
    if total_cost == 0.0 {
        total_cost = results
            .iter()
            .filter_map(|r| r.cost.as_ref().map(|c| c.total_usd))
            .sum();
    }

    // Fallback: compute duration from file timestamps if not found in summary
    if duration_secs == 0.0 && !has_summary {
        duration_secs = compute_duration_from_timestamps(path);
    }

    Ok(RunSummary {
        id: name.to_string(),
        name: name.to_string(),
        pr_count: pr_count as u32,
        avg_f1: Some(avg_f1),
        avg_precision: Some(avg_precision),
        avg_recall: Some(avg_recall),
        total_cost: Some(total_cost),
        total_tokens,
        duration_secs: Some(duration_secs),
        created_at: get_file_modified(path),
        model: Some("unknown".to_string()),
        status: "completed".to_string(),
    })
}

fn get_file_modified(path: &Path) -> String {
    if let Ok(metadata) = std::fs::metadata(path) {
        if let Ok(modified) = metadata.modified() {
            if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
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

/// GET /api/runs/:id — get detailed run results.
pub async fn get_run(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> impl IntoResponse {
    tracing::info!("GET /api/runs/{}", id);

    // Check if run is still in progress (in active_runs before output dir exists)
    let active_run_config = {
        let runs = state.active_runs.read().await;
        runs.get(&id).cloned()
    };
    if let Some(ref active_run) = active_run_config {
        if !active_run.finished {
            // Running — return in-memory state
            let roles: Vec<String> = active_run
                .config
                .roles
                .split(',')
                .map(|s| s.trim().to_string())
                .collect();
            let detail = RunDetail {
                id: id.clone(),
                name: id.clone(),
                pr_count: active_run.total_prs,
                results: vec![],
                aggregate: None,
                total_cost: None,
                total_tokens: 0,
                duration_secs: None,
                model: active_run.config.model.clone(),
                status: "running".to_string(),
                config: Some(RunConfig {
                    model: active_run.config.model.clone(),
                    dataset: active_run.config.dataset_dir.clone(),
                    roles,
                }),
            };
            return Json(detail).into_response();
        }
        // Finished — fall through to disk reading below (do NOT return early with empty results)
    }

    let run_path = state.output_dir.join(&id);

    if !run_path.exists() || !run_path.is_dir() {
        tracing::error!("Run directory not found: {}", run_path.display());
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("Run not found: {}", id)})),
        )
            .into_response();
    }

    let entries = match std::fs::read_dir(&run_path) {
        Ok(entries) => entries,
        Err(e) => {
            tracing::error!("Failed to read run dir {}: {}", run_path.display(), e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response();
        }
    };

    let mut results: Vec<PrResult> = Vec::new();
    let mut total_cost = 0.0f64;
    let mut total_tokens = 0usize;
    let mut model = "unknown".to_string();
    let mut duration_secs = 0.0f64;

    // Resolve cache dir once for agent checking
    let cache_dir = resolve_cache_dir(&state.output_dir, &id);

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
            if file_name == crb_harness::paths::SUMMARY_FILE {
                if let Ok(content) = std::fs::read_to_string(&file_path) {
                    if let Ok(summary) =
                        serde_json::from_str::<HashMap<String, serde_json::Value>>(&content)
                    {
                        model = summary
                            .get("model")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        duration_secs = summary
                            .get("duration_secs")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0);
                        total_cost = summary
                            .get("total_cost_usd")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0);
                        total_tokens = summary
                            .get("total_tokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as usize;
                    }
                }
            }
            continue;
        }

        if let Ok(content) = std::fs::read_to_string(&file_path) {
            match serde_json::from_str::<PrResultJson>(&content) {
                Ok(pr) => {
                    // Extract PR number from URL or filename
                    let pr_number = pr
                        .url
                        .rsplit('/')
                        .next()
                        .and_then(|s| s.parse::<u32>().ok())
                        .unwrap_or(0);
                    // pr_key is the output filename stem (sanitized PR title)
                    let pr_key = file_path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let pr_key_for_agents = pr_key.clone();

                    results.push(PrResult {
                        pr_number,
                        pr_key,
                        title: pr.pr_title,
                        f1: Some(pr.metrics.f1),
                        precision: Some(pr.metrics.precision),
                        recall: Some(pr.metrics.recall),
                        cost: pr.cost.as_ref().map(|c| c.total_usd),
                        status: Some("done".to_string()),
                        has_agents: cache_dir.as_ref().map_or(false, |cd| {
                            let pr_dir = cd.join(&pr_key_for_agents);
                            pr_dir.is_dir() && pr_dir.join("agents").is_dir()
                        }),
                    });
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to parse PR result file {}: {}",
                        file_path.display(),
                        e
                    );
                }
            }
        }
    }

    let pr_count = results.len();
    let _total_tp: usize = 0;
    let _total_fp: usize = 0;
    let _total_fn: usize = 0;

    let avg_f1 = if pr_count > 0 {
        results.iter().filter_map(|r| r.f1).sum::<f64>() / pr_count as f64
    } else {
        0.0
    };
    let avg_precision = if pr_count > 0 {
        results.iter().filter_map(|r| r.precision).sum::<f64>() / pr_count as f64
    } else {
        0.0
    };
    let avg_recall = if pr_count > 0 {
        results.iter().filter_map(|r| r.recall).sum::<f64>() / pr_count as f64
    } else {
        0.0
    };

    // Fallback: compute duration from file timestamps if not found in summary
    if duration_secs == 0.0 {
        let run_path = state.output_dir.join(&id);
        duration_secs = compute_duration_from_timestamps(&run_path);
    }

    // Merge config from active run state if available (it isn't stored on disk)
    let config = active_run_config.as_ref().map(|ar| RunConfig {
        model: ar.config.model.clone(),
        dataset: ar.config.dataset_dir.clone(),
        roles: ar
            .config
            .roles
            .split(',')
            .map(|s| s.trim().to_string())
            .collect(),
    });

    let detail = RunDetail {
        id: id.clone(),
        name: id.clone(),
        pr_count,
        results,
        aggregate: Some(AggregateMetrics {
            avg_f1,
            avg_precision,
            avg_recall,
            total_tp: _total_tp,
            total_fp: _total_fp,
            total_fn: _total_fn,
            total_cost,
            total_prs: pr_count as u32,
            duration_secs,
        }),
        total_cost: Some(total_cost),
        total_tokens,
        duration_secs: Some(duration_secs),
        model,
        status: "completed".to_string(),
        config,
    };

    Json(detail).into_response()
}

/// POST /api/runs — start a new benchmark run.
pub async fn start_run(
    State(state): State<AppState>,
    Json(config): Json<BenchmarkConfig>,
) -> impl IntoResponse {
    tracing::info!(
        "POST /api/runs — model={}, dataset={}, roles={}",
        config.model,
        config.dataset_dir,
        config.roles
    );
    let run_id = format!(
        "run-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    );

    // Calculate total PRs before constructing ActiveRun so the frontend
    // can see it immediately when polling GET /api/runs/:id
    // Resolve dataset directory: the config stores just the dataset ID (e.g. "golden_comments"),
    // but the actual path is relative to the server's base dataset_dir (e.g. "datasets/golden_comments").
    let dataset_dir = state.dataset_dir.join(&config.dataset_dir);
    let total_prs = count_prs_in_dataset(&dataset_dir);

    let (tx, _rx) = tokio::sync::broadcast::channel::<crate::events::DashboardEvent>(1024);

    let active_run = ActiveRun {
        created_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        config: config.clone(),
        tx: tx.clone(),
        completed_prs: 0,
        total_prs,
        finished: false,
    };

    {
        let mut runs = state.active_runs.write().await;
        runs.insert(run_id.clone(), active_run);
    }

    let output_dir = state.output_dir.clone();
    let run_id_clone = run_id.clone();
    let active_runs = state.active_runs.clone();
    let config_clone = config.clone();
    let benchmark_dir = state.benchmark_dir.clone();
    let dataset_dir_clone = dataset_dir.clone();

    tokio::spawn(async move {
        if let Err(e) = harness::run_harness(
            &run_id_clone,
            &config_clone,
            &output_dir,
            benchmark_dir.as_deref(),
            tx,
            active_runs,
            &dataset_dir_clone,
        )
        .await
        {
            tracing::error!("Harness run {} failed: {}", run_id_clone, e);
        }
    });

    let response = StartRunResponse {
        run_id,
        status: "started".to_string(),
        total_prs,
    };

    (StatusCode::CREATED, Json(response))
}

/// Count PR entries in a dataset directory.
fn count_prs_in_dataset(dataset_dir: &Path) -> usize {
    if !dataset_dir.exists() {
        return 0;
    }
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(dataset_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "json") {
                if let Ok(content) = std::fs::read_to_string(&path) {
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

// ── Log viewing handlers ────────────────────────────────────────────────────

/// Scan a PR cache directory for agent log files and return deduplicated roles.
fn scan_agent_roles(pr_cache_dir: &Path) -> Vec<String> {
    use std::collections::BTreeSet;
    let mut roles = BTreeSet::new();

    // Try content-addressed layout first: agents/*.agent_{role}_prompt.txt
    let agents_dir = pr_cache_dir.join("agents");
    if agents_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&agents_dir) {
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
    if let Ok(entries) = std::fs::read_dir(pr_cache_dir) {
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

    roles.into_iter().collect()
}

/// Try to read an agent log file, returning the contents lossy-decoded.
fn read_agent_log_file(cache_dir: &Path, pr_key: &str, role: &str, suffix: &str) -> Option<String> {
    let pr_dir = cache_dir.join(pr_key);

    // Content-addressed layout: agents/*.agent_{role}_{suffix}.txt
    let agents_dir = pr_dir.join("agents");
    if agents_dir.is_dir() {
        let pattern = format!(".agent_{}_{}.txt", role, suffix);
        if let Ok(entries) = std::fs::read_dir(&agents_dir) {
            for entry in entries.flatten() {
                let fname = entry.file_name().to_string_lossy().to_string();
                if fname.ends_with(&pattern) {
                    if let Ok(content) = std::fs::read(entry.path()) {
                        return Some(String::from_utf8_lossy(&content).to_string());
                    }
                }
            }
        }
    }

    // Simple layout: agent_{role}_{suffix}.txt
    let simple_path = pr_dir.join(format!("agent_{}_{}.txt", role, suffix));
    if simple_path.is_file() {
        if let Ok(content) = std::fs::read(&simple_path) {
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
        output_dir.join(crb_harness::paths::CACHE_DIR_NAME),
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

/// GET /api/runs/:id/logs — list available log files for a run
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
        if let Ok(entries) = std::fs::read_dir(&run_path) {
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
                if fname == crb_harness::paths::SUMMARY_FILE || fname.starts_with("candidates") {
                    continue;
                }
                // Filename stem is the pr_key
                let stem = file_path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                if stem.is_empty() || stem.starts_with('_') || stem.starts_with('.') {
                    continue;
                }
                // Try to get a more descriptive title from the JSON content
                let title = if let Ok(content) = std::fs::read_to_string(&file_path) {
                    if let Ok(pr) = serde_json::from_str::<PrResultJson>(&content) {
                        if !pr.pr_title.is_empty() {
                            pr.pr_title
                        } else {
                            stem.clone()
                        }
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
        if let Ok(entries) = std::fs::read_dir(cd) {
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
        let pr_title = output_prs
            .iter()
            .find(|(k, _)| k == pr_key)
            .map(|(_, t)| t.clone())
            .unwrap_or_else(|| resolve_pr_title(&state.output_dir, &id, pr_key));

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
            pr_key: pr_key.clone(),
            pr_title,
            agents,
        });
    }

    Json(LogsListResponse {
        run_id: id,
        cache_available: cache_dir.is_some(),
        prs,
    })
    .into_response()
}

/// Try to resolve a PR title from the run's output files.
fn resolve_pr_title(output_dir: &Path, run_id: &str, pr_key: &str) -> String {
    // The pr_key could be a number or URL fragment; try to find a matching result file
    let run_path = output_dir.join(run_id);
    if !run_path.is_dir() {
        return pr_key.to_string();
    }
    if let Ok(entries) = std::fs::read_dir(&run_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(true, |e| e != "json") {
                continue;
            }
            let fname = path.file_name().unwrap_or_default().to_string_lossy();
            if fname == crb_harness::paths::SUMMARY_FILE {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(pr) = serde_json::from_str::<PrResultJson>(&content) {
                    // Match by PR number from URL being equal to parsed pr_key
                    let pr_num_from_url = pr
                        .url
                        .rsplit('/')
                        .next()
                        .and_then(|s| s.parse::<u32>().ok());
                    let pr_num_from_key = pr_key.parse::<u32>().ok();
                    if pr_num_from_url.is_some() && pr_num_from_url == pr_num_from_key {
                        if !pr.pr_title.is_empty() {
                            return pr.pr_title;
                        }
                    }
                    // Also match if the filename contains the pr_key (filename is often {pr_number}.json)
                    if fname.contains(pr_key) && !pr.pr_title.is_empty() {
                        return pr.pr_title;
                    }
                }
            }
        }
    }
    pr_key.to_string()
}

/// GET /api/runs/:id/logs/:pr_key/:role — get specific agent log
pub async fn get_agent_log(
    State(state): State<AppState>,
    AxumPath((id, pr_key, role)): AxumPath<(String, String, String)>,
) -> impl IntoResponse {
    tracing::info!("GET /api/runs/{}/logs/{}/{}", id, pr_key, role);

    let cache_dir = match resolve_cache_dir(&state.output_dir, &id) {
        Some(d) => d,
        None => {
            return Json(AgentLogResponse {
                run_id: id.clone(),
                pr_key,
                role,
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
            pr_key,
            role,
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

    Json(AgentLogResponse {
        run_id: id,
        pr_key,
        role,
        prompt,
        response,
        reasoning,
        available,
    })
    .into_response()
}

/// GET /api/runs/:id/prs/:pr_key — get agent availability info for a single PR
///
/// Returns the PR title and which agents have cached log files.
pub async fn get_pr_agents(
    State(state): State<AppState>,
    AxumPath((id, pr_key)): AxumPath<(String, String)>,
) -> impl IntoResponse {
    tracing::info!("GET /api/runs/{}/prs/{}", id, pr_key);

    let cache_dir = resolve_cache_dir(&state.output_dir, &id);
    let pr_title = resolve_pr_title(&state.output_dir, &id, &pr_key);

    // Scan agents from cache
    let agents = if let Some(ref cd) = cache_dir {
        let pr_dir = cd.join(&pr_key);
        if pr_dir.is_dir() {
            let roles = scan_agent_roles(&pr_dir);
            // For each role, check which log files exist
            let mut entries: Vec<PrAgentEntry> = Vec::new();
            for role in roles {
                let has_prompt = read_agent_log_file(cd, &pr_key, &role, "prompt").is_some();
                let has_response = read_agent_log_file(cd, &pr_key, &role, "response").is_some();
                let has_reasoning = read_agent_log_file(cd, &pr_key, &role, "reasoning").is_some();
                entries.push(PrAgentEntry {
                    role,
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

    // Check if output file exists for this PR
    let has_output = {
        let run_path = state.output_dir.join(&id);
        if run_path.is_dir() {
            let pr_key_lower = pr_key.to_lowercase();
            let mut found = false;
            if let Ok(entries) = std::fs::read_dir(&run_path) {
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
                    if fname == crb_harness::paths::SUMMARY_FILE || fname.starts_with("candidates")
                    {
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

/// GET /api/runs/:id/pr-detail/:pr_key — get full details for a specific PR from its result file
pub async fn get_pr_detail(
    State(state): State<AppState>,
    AxumPath((id, pr_key)): AxumPath<(String, String)>,
) -> impl IntoResponse {
    tracing::info!("GET /api/runs/{}/pr-detail/{}", id, pr_key);

    let run_path = state.output_dir.join(&id);
    if !run_path.exists() || !run_path.is_dir() {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("Run not found: {}", id) })),
        )
            .into_response();
    }

    // Find the matching PR result file — pr_key could be a filename fragment or PR number
    let entries = match std::fs::read_dir(&run_path) {
        Ok(e) => e,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Cannot read run directory"})),
            )
                .into_response()
        }
    };

    let pr_key_lower = pr_key.to_lowercase();
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
        if fname == crb_harness::paths::SUMMARY_FILE || fname.starts_with("candidates") {
            continue;
        }

        // Match by filename containing pr_key, or by PR number extracted from URL
        let content = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let pr: PrResultJson = match serde_json::from_str(&content) {
            Ok(p) => p,
            Err(_) => continue,
        };

        // Match by filename containing pr_key (normalize spaces to underscores for filename matching),
        // or by PR number extracted from URL, or by PR title containing pr_key
        let pr_num_from_url = pr
            .url
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
            || pr.pr_title.to_lowercase().contains(&pr_key_lower);

        tracing::debug!(
            "pr-detail matching: pr_key='{}', fname='{}', fname_lower='{}', pr_key_normalized='{}', matches={}",
            pr_key, fname, fname_lower, pr_key_normalized, matches
        );

        if matches {
            return Json(PrDetailResponse {
                run_id: id,
                pr_title: pr.pr_title,
                url: pr.url,
                findings_count: pr.findings_count,
                golden_count: pr.golden_count,
                metrics: pr.metrics,
                verdicts: pr.verdicts,
                cost: pr.cost,
                findings: pr.findings,
                agent_responses: pr.agent_responses,
            })
            .into_response();
        }
    }

    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"error": format!("PR not found: {}", pr_key) })),
    )
        .into_response()
}
