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

/// Summary of a past benchmark run.
#[derive(Debug, Clone, Serialize)]
pub struct RunSummary {
    pub id: String,
    pub name: String,
    pub pr_count: usize,
    pub avg_f1: f64,
    pub avg_precision: f64,
    pub avg_recall: f64,
    pub total_cost: f64,
    pub total_tokens: usize,
    pub duration_secs: f64,
    pub created_at: String,
    pub model: String,
    pub status: String,
}

/// Detailed run result with per-PR data (API response shape matching frontend expectations).
#[derive(Debug, Clone, Serialize)]
pub struct RunDetail {
    pub id: String,
    pub name: String,
    pub pr_count: usize,
    pub results: Vec<PrResultResponse>,
    pub aggregate: Option<AggregateMetricsResponse>,
    pub total_cost: Option<f64>,
    pub total_tokens: usize,
    pub duration_secs: Option<f64>,
    pub model: String,
    pub status: String,
    pub config: Option<RunConfigResponse>,
}

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
}

/// API response shape for a single PR result (matching frontend PrResult).
#[derive(Debug, Clone, Serialize)]
pub struct PrResultResponse {
    pub pr_number: u32,
    pub title: String,
    pub f1: Option<f64>,
    pub precision: Option<f64>,
    pub recall: Option<f64>,
    pub cost: Option<f64>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MetricsJson {
    #[serde(default)]
    pub true_positives: usize,
    #[serde(default)]
    pub false_positives: usize,
    #[serde(default)]
    pub false_negatives: usize,
    #[serde(default)]
    pub precision: f64,
    #[serde(default)]
    pub recall: f64,
    #[serde(default)]
    pub f1: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerdictJson {
    #[serde(default)]
    pub reasoning: String,
    #[serde(default)]
    pub match_: bool,
    #[serde(default)]
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateMetricsResponse {
    pub avg_f1: f64,
    pub avg_precision: f64,
    pub avg_recall: f64,
    pub total_tp: usize,
    pub total_fp: usize,
    pub total_fn: usize,
    #[serde(default)]
    pub total_cost: f64,
    #[serde(default)]
    pub total_prs: u32,
    #[serde(default)]
    pub duration_secs: f64,
}

/// Run config returned in the run detail response.
#[derive(Debug, Clone, Serialize)]
pub struct RunConfigResponse {
    pub model: String,
    pub dataset: String,
    pub roles: Vec<String>,
}

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

/// Response from GET /api/runs/:id/logs
#[derive(Debug, Clone, Serialize)]
pub struct LogsListResponse {
    pub run_id: String,
    pub cache_available: bool,
    pub prs: Vec<PrLogsEntry>,
}

/// A single PR's available log entries
#[derive(Debug, Clone, Serialize)]
pub struct PrLogsEntry {
    pub pr_key: String,
    pub pr_title: String,
    pub agents: Vec<String>,
}

/// Response from GET /api/runs/:id/logs/:pr_key/:role
#[derive(Debug, Clone, Serialize)]
pub struct AgentLogResponse {
    pub run_id: String,
    pub pr_key: String,
    pub role: String,
    pub prompt: Option<String>,
    pub response: Option<String>,
    pub available: bool,
}

/// Response from POST /api/runs/:id/replay
#[derive(Debug, Clone, Serialize)]
pub struct ReplayStartResponse {
    pub run_id: String,
    pub status: String,
    pub cache_available: bool,
}

/// Response from GET /api/runs/:id/replay/status
#[derive(Debug, Clone, Serialize)]
pub struct ReplayStatusResponse {
    pub run_id: String,
    pub status: String,
    pub progress_pct: u32,
    pub completed_prs: u32,
    pub total_prs: u32,
    pub message: String,
}

// ── Handlers ────────────────────────────────────────────────────────────────

/// GET /api/runs — list all completed benchmark runs.
pub async fn list_runs(State(state): State<AppState>) -> impl IntoResponse {
    tracing::info!("GET /api/runs");
    let output_dir = state.output_dir.clone();
    let mut runs = Vec::new();

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

    runs.sort_by(|a, b| b.name.cmp(&a.name));
    Json(runs).into_response()
}

/// Scan a run directory and compute summary metrics.
fn scan_run_dir(path: &Path, name: &str) -> Result<RunSummary, String> {
    use std::fs;

    let entries = fs::read_dir(path).map_err(|e| e.to_string())?;
    let mut results = Vec::new();
    let mut total_cost = 0.0f64;
    let mut total_tokens = 0usize;
    let mut duration_secs = 0.0f64;

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

        if file_name == "_summary.json" {
            if let Ok(content) = fs::read_to_string(&file_path) {
                if let Ok(summary) =
                    serde_json::from_str::<HashMap<String, serde_json::Value>>(&content)
                {
                    if let Some(metrics) = summary.get("aggregate_metrics") {
                        if let Some(am) = metrics.as_object() {
                            let ag = AggregateMetricsResponse {
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
                                pr_count,
                                avg_f1: ag.avg_f1,
                                avg_precision: ag.avg_precision,
                                avg_recall: ag.avg_recall,
                                total_cost,
                                total_tokens,
                                duration_secs,
                                created_at: get_file_modified(path),
                                model,
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

    Ok(RunSummary {
        id: name.to_string(),
        name: name.to_string(),
        pr_count,
        avg_f1,
        avg_precision,
        avg_recall,
        total_cost,
        total_tokens,
        duration_secs,
        created_at: get_file_modified(path),
        model: "unknown".to_string(),
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

    let mut results: Vec<PrResultResponse> = Vec::new();
    let mut total_cost = 0.0f64;
    let mut total_tokens = 0usize;
    let mut model = "unknown".to_string();
    let mut duration_secs = 0.0f64;

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

        if file_name == "_summary.json" {
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

                    results.push(PrResultResponse {
                        pr_number,
                        title: pr.pr_title,
                        f1: Some(pr.metrics.f1),
                        precision: Some(pr.metrics.precision),
                        recall: Some(pr.metrics.recall),
                        cost: None,
                        status: Some("done".to_string()),
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

    let detail = RunDetail {
        id: id.clone(),
        name: id.clone(),
        pr_count,
        results,
        aggregate: Some(AggregateMetricsResponse {
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
        config: None,
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

    let (tx, _rx) = tokio::sync::broadcast::channel::<crate::events::DashboardEvent>(1024);

    let active_run = ActiveRun {
        created_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        config: config.clone(),
        tx: tx.clone(),
        completed_prs: 0,
        total_prs: 0,
        finished: false,
    };

    {
        let mut runs = state.active_runs.write().await;
        runs.insert(run_id.clone(), active_run);
    }

    let harness_path = state.harness_path.clone();
    let output_dir = state.output_dir.clone();
    let run_id_clone = run_id.clone();
    let active_runs = state.active_runs.clone();
    let config_clone = config.clone();

    tokio::spawn(async move {
        if let Err(e) = harness::run_harness(
            &harness_path,
            &run_id_clone,
            &config_clone,
            &output_dir,
            tx,
            active_runs,
        )
        .await
        {
            tracing::error!("Harness run {} failed: {}", run_id_clone, e);
        }
    });

    let dataset_dir = PathBuf::from(&config.dataset_dir);
    let total_prs = count_prs_in_dataset(&dataset_dir);

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
                    if let Ok(val) =
                        serde_json::from_str::<HashMap<String, serde_json::Value>>(&content)
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
                if let Some(role) = rest.strip_suffix("_prompt.txt")
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

/// GET /api/runs/:id/logs — list available log files for a run
pub async fn list_logs(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> impl IntoResponse {
    tracing::info!("GET /api/runs/{}/logs", id);

    let base_dir = state.output_dir.parent().unwrap_or(Path::new("."));
    let cache_dir = base_dir.join("cache").join(&id);

    if !cache_dir.exists() || !cache_dir.is_dir() {
        return Json(LogsListResponse {
            run_id: id,
            cache_available: false,
            prs: vec![],
        })
        .into_response();
    }

    let mut prs = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&cache_dir) {
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

            // Try to find a PR title from the output directory
            let pr_title = resolve_pr_title(&state.output_dir, &id, &pr_key);

            let agents = scan_agent_roles(&path);

            if !agents.is_empty() {
                prs.push(PrLogsEntry {
                    pr_key,
                    pr_title,
                    agents,
                });
            }
        }
    }

    Json(LogsListResponse {
        run_id: id,
        cache_available: true,
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
            if fname == "_summary.json" {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(pr) = serde_json::from_str::<PrResultJson>(&content) {
                    // Match by PR number from URL being equal to parsed pr_key
                    let pr_num_from_url = pr.url.rsplit('/').next().and_then(|s| s.parse::<u32>().ok());
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

    let base_dir = state.output_dir.parent().unwrap_or(Path::new("."));
    let cache_dir = base_dir.join("cache").join(&id);
    let pr_dir = cache_dir.join(&pr_key);

    if !pr_dir.exists() || !pr_dir.is_dir() {
        return Json(AgentLogResponse {
            run_id: id,
            pr_key,
            role,
            prompt: None,
            response: None,
            available: false,
        })
        .into_response();
    }

    let prompt = read_agent_log_file(&cache_dir, &pr_key, &role, "prompt");
    let response = read_agent_log_file(&cache_dir, &pr_key, &role, "response");
    let available = prompt.is_some() || response.is_some();

    Json(AgentLogResponse {
        run_id: id,
        pr_key,
        role,
        prompt,
        response,
        available,
    })
    .into_response()
}

/// POST /api/runs/:id/replay — start replay from cache
pub async fn start_replay(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> impl IntoResponse {
    tracing::info!("POST /api/runs/{}/replay", id);

    let base_dir = state.output_dir.parent().unwrap_or(Path::new("."));
    let cache_dir = base_dir.join("cache").join(&id);
    let run_output_dir = state.output_dir.join(&id);
    let summary_path = run_output_dir.join("_summary.json");

    if !cache_dir.exists() || !cache_dir.is_dir() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": format!("Cache not available for run: {}", id)})),
        )
            .into_response();
    }

    if !summary_path.exists() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": format!("Summary not found for run: {}", id)})),
        )
            .into_response();
    }

    // Read summary to get original run config
    let summary_content = match std::fs::read_to_string(&summary_path) {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to read summary: {}", e)})),
            )
                .into_response();
        }
    };

    let summary: HashMap<String, serde_json::Value> = match serde_json::from_str(&summary_content) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Invalid summary JSON: {}", e)})),
            )
                .into_response();
        }
    };

    let model = summary
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let dataset_dir = summary
        .get("dataset_dir")
        .and_then(|v| v.as_str())
        .or_else(|| summary.get("dataset").and_then(|v| v.as_str()))
        .unwrap_or("datasets/golden_comments")
        .to_string();
    let roles = summary
        .get("roles")
        .and_then(|v| v.as_str())
        .unwrap_or("SA,CL,AR,SEC")
        .to_string();

    // Create replay output directory
    let replay_output = base_dir.join("output").join(format!("{}-replay", id));

    // Create replay state
    let replay_state = crate::server::ReplayState {
        status: "running".to_string(),
        progress_pct: 0,
        completed_prs: 0,
        total_prs: 0,
        message: "Starting replay...".to_string(),
        output_dir: replay_output.clone(),
    };

    {
        let mut replays = state.replays.write().await;
        replays.insert(id.clone(), replay_state);
    }

    // Spawn crb-harness in background with --cache-dir
    let harness_path = state.harness_path.clone();
    let id_clone = id.clone();
    let replays = state.replays.clone();
    let cache_arg = cache_dir.to_string_lossy().to_string();
    let replay_output_arg = replay_output.to_string_lossy().to_string();

    tokio::spawn(async move {
        let status = tokio::process::Command::new(&harness_path)
            .arg("--model")
            .arg(&model)
            .arg("--dataset-dir")
            .arg(&dataset_dir)
            .arg("--roles")
            .arg(&roles)
            .arg("--cache-dir")
            .arg(&cache_arg)
            .arg("--output-dir")
            .arg(&replay_output_arg)
            .status()
            .await;

        let mut runs = replays.write().await;
        if let Some(rstate) = runs.get_mut(&id_clone) {
            match status {
                Ok(s) if s.success() => {
                    rstate.status = "completed".to_string();
                    rstate.progress_pct = 100;
                    rstate.completed_prs = rstate.total_prs;
                    rstate.message = "Replay completed successfully".to_string();
                }
                Ok(s) => {
                    rstate.status = "failed".to_string();
                    rstate.message = format!("Harness exited with: {}", s);
                }
                Err(e) => {
                    rstate.status = "failed".to_string();
                    rstate.message = format!("Failed to run harness: {}", e);
                }
            }
        }
    });

    Json(ReplayStartResponse {
        run_id: id,
        status: "started".to_string(),
        cache_available: true,
    })
    .into_response()
}

/// GET /api/runs/:id/replay/status — poll replay progress
pub async fn replay_status(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> impl IntoResponse {
    tracing::info!("GET /api/runs/{}/replay/status", id);

    let replays = state.replays.read().await;
    if let Some(rstate) = replays.get(&id) {
        Json(ReplayStatusResponse {
            run_id: id,
            status: rstate.status.clone(),
            progress_pct: rstate.progress_pct,
            completed_prs: rstate.completed_prs,
            total_prs: rstate.total_prs,
            message: rstate.message.clone(),
        })
        .into_response()
    } else {
        Json(ReplayStatusResponse {
            run_id: id,
            status: "idle".to_string(),
            progress_pct: 0,
            completed_prs: 0,
            total_prs: 0,
            message: "No replay in progress".to_string(),
        })
        .into_response()
    }
}

/// POST /api/runs/:id/convert — convert findings to candidates.json format
///
/// Reads per-PR JSON files from the run directory, converts findings to
/// `candidates.json`, and returns conversion statistics.
pub async fn convert_to_candidates(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> impl IntoResponse {
    tracing::info!("POST /api/runs/{}/convert", id);
    let run_dir = state.output_dir.join(&id);

    match crate::converter::convert_run(&run_dir) {
        Ok(stats) => {
            tracing::info!(
                "Converted run '{}': {} PRs, {} findings → candidates.json",
                id,
                stats.pr_count,
                stats.finding_count
            );
            (StatusCode::OK, Json(serde_json::to_value(&stats).unwrap())).into_response()
        }
        Err(e) => {
            tracing::error!("Conversion failed for run '{}': {}", id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response()
        }
    }
}

/// POST /api/runs/:id/judge — run the Python step3 judge on converted candidates
///
/// Expects `candidates.json` to exist in the run directory (run convert first).
/// Locates the `offline/` directory and runs `uv run python -m code_review_benchmark.step3_judge_comments --tool hermes`.
pub async fn run_judge(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> impl IntoResponse {
    tracing::info!("POST /api/runs/{}/judge", id);
    let run_dir = state.output_dir.join(&id);

    let result = crate::converter::run_judge(&run_dir, state.benchmark_dir.as_deref()).await;
    tracing::info!("Judge for run '{}': success={}, message={}", id, result.success, result.message);

    let status = if result.success {
        StatusCode::OK
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    };

    (status, Json(serde_json::to_value(&result).unwrap())).into_response()
}
