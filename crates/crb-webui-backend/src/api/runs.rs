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
    #[deprecated = "Call get config on its own"]
    pub config: Option<RunConfig>,
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
pub async fn list_runs(State(state): State<AppState<impl Store>>) -> impl IntoResponse {
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

/// Get detailed run results.
#[deprecated = "This will become part of the review endpoints."]
pub async fn get_run(
    State(state): State<AppState<impl Store>>,
    AxumPath(id): AxumPath<String>,
) -> Response {
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
    State(state): State<AppState<impl Store>>,
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

/// List available log files for a run
///
/// Merges PRs from the output directory (canonical source) with cache entries.
/// All PRs with output files are shown; cache entries add agent roles where available.
pub async fn list_logs(
    State(state): State<AppState<impl Store>>,
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
    State(state): State<AppState<impl Store>>,
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
    State(state): State<AppState<impl Store>>,
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
    State(state): State<AppState<impl Store>>,
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
