//! API handlers for ad-hoc PR reviews.
//!
//! Provides endpoints to submit a GitHub PR URL for ad-hoc review,
//! list previous ad-hoc reviews, and get their details.

use std::path::Path;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use std::{env, fs};

use axum::Json;
use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use crb_agents::AgentEntry;
use crb_agents::prompts;
use crb_cache::filesystem::FilesystemBackend;
use crb_cache::traits::CacheBackend;
use crb_reporting::cost::AnalyticsTracker;
use crb_shared::DEFAULT_MODEL;
use crb_shared::diff::Diff;
use crb_shared::sanitize_filename;
use crb_shared::url::parse_github_url;
use crb_types::benchmark::metrics::{Metrics, MetricsProvider};
use crb_types::benchmark::result::PrResult;
use crb_types::vcs::repository::RemoteRepositoryMeta;
use crb_types::wrappers::Model;
use crb_webui_shared::adhoc::{AdhocReviewResponse, AdhocRunSummary, GithubPrListItem};
use crb_webui_shared::runs::RunStatus;
use rig_core::tool::server::ToolServer;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{error, info, warn};

use crate::api::runs::{self};
use crate::server::AppState;
use crb_webui_shared::runs::{PrResultRow, RunConfig, RunDetail, RunMeta};
use rig_core::client::CompletionClient;
use rig_core::client::ProviderClient;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdhocReviewRequest {
    pub url: String,

    #[serde(default = "default_adhoc_model")]
    pub model: String,

    #[serde(default = "default_adhoc_roles")]
    pub roles: Vec<String>,
}

fn default_adhoc_model() -> String {
    DEFAULT_MODEL.to_string()
}

// TODO: No string defaults for dynamic roles.
#[deprecated(note = "Use PromptLibrary::get_instance().abbreviations() instead")]
fn default_adhoc_roles() -> Vec<String> {
    crb_agents::prompts::PromptLibrary::get_instance()
        .abbreviations()
        .into_iter()
        .map(|s| s.to_string())
        .collect()
}

/// Submit a GitHub PR URL for ad-hoc review. Fetches the PR diff + metadata
/// from the GitHub API, runs the harness agents, and stores results.
pub async fn start_adhoc_review(
    State(state): State<AppState>,
    Json(req): Json<AdhocReviewRequest>,
) -> impl IntoResponse {
    info!(
        "POST /api/adhoc/review url={} model={} roles={:?}",
        req.url, req.model, req.roles,
    );

    let (owner, repo, pr_number) = match parse_github_url(&req.url) {
        Ok(info) => info,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "Invalid GitHub PR URL. Expected format: https://github.com/owner/repo/pull/123"
                })),
            )
                .into_response();
        }
    };

    let (pr_title, diff) = match fetch_pr_diff(&state, &owner, &repo, pr_number).await {
        Ok(result) => result,
        Err(e) => {
            error!("Failed to fetch PR {}: {}", req.url, e);
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({
                    "error": format!("Failed to fetch PR: {}", e)
                })),
            )
                .into_response();
        }
    };

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let run_id = format!("adhoc-{timestamp}");

    let state_clone = state.clone();
    let model = req.model.clone();
    let run_id_bg = run_id.clone();
    let pr_title_bg = pr_title.clone();
    tokio::spawn(async move {
        if let Err(e) = run_adhoc_review_inner(
            &state_clone,
            &run_id_bg,
            &req.url,
            &pr_title_bg,
            &diff,
            &model,
            &req.roles,
        )
        .await
        {
            error!("Ad-hoc review {run_id_bg} failed: {e}");
        }
    });

    (
        StatusCode::OK,
        Json(AdhocReviewResponse {
            run_id,
            pr_title,
            status: RunStatus::Running,
        }),
    )
        .into_response()
}

/// List all previous ad-hoc review runs.
pub async fn list_adhoc_runs(State(state): State<AppState>) -> impl IntoResponse {
    let adhoc_dir = state.output_dir.join("adhoc");
    let mut runs: Vec<AdhocRunSummary> = Vec::new();

    if adhoc_dir.exists() {
        let entries = match fs::read_dir(&adhoc_dir) {
            Ok(entries) => entries,
            Err(_) => return Json(Vec::<AdhocRunSummary>::new()).into_response(),
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let run_id = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            if let Some(summary) = scan_adhoc_run_dir(&path, &run_id) {
                runs.push(summary);
            }
        }
    }

    runs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Json(runs).into_response()
}

/// Get details for a specific ad-hoc review run.
pub async fn get_adhoc_run(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> impl IntoResponse {
    let run_dir = state.output_dir.join("adhoc").join(&id);

    if !run_dir.exists() {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Ad-hoc run not found"})),
        )
            .into_response();
    }

    let summary_path = run_dir.join(crb_harness::paths::SUMMARY_FILE);
    let summary_data: Option<serde_json::Value> = fs::read_to_string(&summary_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());

    let summary_str = |key: &str, default: &str| -> String {
        summary_data
            .as_ref()
            .and_then(|s| s.get(key))
            .and_then(|v| v.as_str())
            .unwrap_or(default)
            .to_string()
    };

    let summary_f64 = |key: &str, default: f64| -> f64 {
        summary_data
            .as_ref()
            .and_then(|s| s.get(key))
            .and_then(|v| v.as_f64())
            .unwrap_or(default)
    };

    let model = summary_str("model", "unknown");
    let roles: Vec<String> = summary_data
        .as_ref()
        .and_then(|s| s.get("roles"))
        .and_then(|v| {
            if let Some(arr) = v.as_array() {
                Some(
                    arr.iter()
                        .filter_map(|r| r.as_str().map(String::from))
                        .collect(),
                )
            } else if let Some(s) = v.as_str() {
                // Backward compat: old format stored comma-separated string
                Some(s.split(',').map(|r| r.trim().to_string()).collect())
            } else {
                None
            }
        })
        .unwrap_or_default();
    let status = summary_str("status", "unknown");
    let duration_secs = match summary_data
        .as_ref()
        .and_then(|s| s.get("duration_secs"))
        .and_then(|v| v.as_f64())
    {
        Some(v) => Some(v),
        None => None,
    };
    let total_cost = summary_f64("total_cost_usd", 0.0);

    let mut results: Vec<PrResultRow> = Vec::new();
    let mut aggregate_metrics = Metrics {
        true_positives: 0,
        false_positives: 0,
        false_negatives: 0,
        duration_secs: duration_secs.unwrap_or(0.0),
    };

    for (file_path, fname) in runs::iter_json_files(&run_dir) {
        if let Ok(content) = fs::read_to_string(&file_path) {
            if let Ok(pr_json) = serde_json::from_str::<PrResult>(&content) {
                let metrics = &pr_json.metrics;
                results.push(PrResultRow {
                    pr_number: 0,
                    pr_key: fname.trim_end_matches(".json").to_string(),
                    title: pr_json.pr_title,
                    f1: Some(metrics.f1()),
                    precision: Some(metrics.precision()),
                    recall: Some(metrics.recall()),
                    cost: pr_json.cost.as_ref().map(|c| c.total_cost()),
                    status: Some("done".to_string()),
                    has_agents: false,
                });

                aggregate_metrics.true_positives += metrics.true_positives;
                aggregate_metrics.false_positives += metrics.false_positives;
                aggregate_metrics.false_negatives += metrics.false_negatives;
            }
        }
    }

    let detail = RunDetail {
        meta: RunMeta {
            id: id.clone(),
            name: id,
            pr_count: results.len(),
            total_cost: Some(total_cost),
            total_tokens: 0,
            duration_secs,
            model: Some(model.clone()),
            status: RunStatus::Completed,
        },
        results,
        aggregate: Some(aggregate_metrics),
        config: Some(RunConfig {
            model,
            dataset: String::new(),
            roles,
        }),
    };

    Json(detail).into_response()
}

/// List open PRs from a GitHub repo
pub async fn list_repo_prs(
    State(state): State<AppState>,
    AxumPath((owner, repo)): AxumPath<(String, String)>,
) -> impl IntoResponse {
    info!("GET /api/adhoc/prs/{}/{}", owner, repo);

    let page = match state
        .octocrab
        .pulls(&owner, &repo)
        .list()
        .state(octocrab::params::State::Open)
        .per_page(100)
        .send()
        .await
    {
        Ok(page) => page,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("GitHub API error: {e}") })),
            )
                .into_response();
        }
    };

    let prs: Vec<GithubPrListItem> = page
        .items
        .into_iter()
        .map(|pr| GithubPrListItem {
            number: pr.number as u32,
            title: pr.title.unwrap_or_default(),
            html_url: pr.html_url.map(|u| u.to_string()).unwrap_or_default(),
        })
        .collect();

    Json(prs).into_response()
}

/// Fetch PR title and raw diff from the GitHub API via octocrab.
async fn fetch_pr_diff(
    state: &AppState,
    owner: &str,
    repo: &str,
    pr_number: u32,
) -> Result<(String, String), String> {
    let pr = state
        .octocrab
        .pulls(owner, repo)
        .get(pr_number as u64)
        .await
        .map_err(|e| format!("Failed to fetch PR metadata: {e}"))?;

    let title = pr.title.unwrap_or_default();

    // Fetch PR diff (raw text, using application/vnd.github.v3.diff custom Accept header).
    // octocrab's typed methods don't support raw text responses, so we use reqwest directly
    // for this single endpoint. Auth is injected from GITHUB_TOKEN env var.
    let diff_client = reqwest::Client::new();
    let token = env::var("GITHUB_TOKEN").ok();
    let mut diff_req = diff_client
        .get(format!(
            "https://api.github.com/repos/{owner}/{repo}/pulls/{pr_number}"
        ))
        .header("Accept", "application/vnd.github.v3.diff")
        .header("User-Agent", "review-harness/1.0");
    if let Some(ref t) = token {
        diff_req = diff_req.header("Authorization", format!("Bearer {t}"));
    }
    let diff = diff_req
        .send()
        .await
        .map_err(|e| format!("Failed to fetch PR diff: {e}"))?
        .text()
        .await
        .map_err(|e| format!("Failed to read diff text: {e}"))?;

    Ok((title, diff))
}

/// Run the actual review pipeline for an ad-hoc PR.
async fn run_adhoc_review_inner(
    state: &AppState,
    run_id: &str,
    pr_url: &str,
    pr_title: &str,
    diff: &str,
    model: &str,
    roles: &[String],
) -> anyhow::Result<()> {
    let output_subdir = state.output_dir.join("adhoc").join(run_id);
    let cache_dir = output_subdir.join(crb_cache::paths::CACHE_DIR_NAME);

    info!(
        run_id = %run_id,
        pr_title = %pr_title,
        model = %model,
        roles = ?roles,
        "Starting ad-hoc review"
    );

    let client = rig_core::providers::openai::Client::from_env()
        .map_err(|e| anyhow::anyhow!("Failed to create OpenAI client: {e}"))?;

    let judge = client
        .agent(model)
        .preamble(
            "You are evaluating AI code review tools.\n\
            Determine if the candidate issue matches the golden (expected) comment.\n\
            \n\
            Golden Comment (the issue we're looking for):\n\
            {golden_comment}\n\
            \n\
            Candidate Issue (from the tool's review):\n\
            {candidate}\n\
            \n\
            Instructions:\n\
            - Determine if the candidate identifies the SAME underlying issue as the golden comment\n\
            - Accept semantic matches - different wording is fine if it's the same problem\n\
            - Focus on whether they point to the same bug, concern, or code issue\n\
            \n\
            Respond with ONLY a JSON object:\n\
            {\"reasoning\": \"brief explanation\", \"match\": true/false, \"confidence\": 0.0-1.0}",
        )
        .temperature(0.3)
        .build();

    let prompt_lib = Arc::new(prompts::PromptLibrary::get_instance());

    use crb_reporting::golden::GoldenCommentEntry;
    let pr = GoldenCommentEntry {
        pr_title: pr_title.to_string(),
        url: pr_url.to_string(),
        comments: vec![],
    };

    let pr_key = sanitize_filename(pr_title);
    let cost_tracker = Arc::new(AnalyticsTracker::new());
    let diff = Diff::new(diff.to_string());

    if diff.sections.is_empty() {
        warn!("Empty diff for PR: {}", pr_title);
    }

    info!(
        "Running ad-hoc review with roles={:?}, model={}",
        roles, model
    );

    let cache_backend: Option<Arc<dyn CacheBackend>> =
        Some(Arc::new(FilesystemBackend::new(&cache_dir)));

    let agents: Vec<&'static AgentEntry> = if roles.is_empty() {
        prompt_lib.agents().into_iter().collect()
    } else {
        roles
            .iter()
            .filter_map(|r| prompt_lib.config(r.trim()))
            .collect()
    };
    let agents: &'static [&'static AgentEntry] = Box::leak(agents.into_boxed_slice());

    let wrapped_model = Model(model.to_string());

    let cfg = crb_harness::eval::EvalConfig {
        review_id: format!("adhoc-{}", run_id),
        strategy: crb_harness::eval::EvalStrategy::Panel,
        model: wrapped_model,
        reasoning_effort: None,
        client: Arc::new(client),
        cache: cache_backend,
        cost_tracker,
        dashboard_tx: None,
        agents,
        repo_root: output_subdir.clone(),
        max_findings: 20,
        judge_model: model.to_string(),
        judge,
        linters_only: false,
        linter_configs: None,
        ruleset: None,
        template_vars: None,
    };

    let findings = crb_harness::pipeline::evaluate(diff, &cfg).await?;
    let result = crb_harness::pipeline::build_pr_result(
        &findings,
        &cfg,
        &pr.pr_title,
        &pr.url,
        pr.comments.len(),
    )
    .await;

    let metrics_for_summary = result.metrics.clone();

    let total_cost = result.cost.as_ref().map(|c| c.total_cost()).unwrap_or(0.0);

    fs::create_dir_all(&output_subdir)?;

    let pr_result_path = output_subdir.join(format!("{}.json", pr_key));
    let cost_snapshot = result.cost.clone();
    let pr_json = PrResult {
        meta: PrMeta {
            title: result.pr_title.clone(),
            url: result.url.clone(),
        },
        golden_count: 0,
        metrics: result.metrics.clone(),
        verdicts: result.verdicts.clone(),
        cost: cost_snapshot,
        findings: serde_json::json!([]),
        agent_responses: vec![],
    };

    let pr_json_str = serde_json::to_string_pretty(&pr_json)?;
    fs::write(&pr_result_path, &pr_json_str)?;

    let elapsed = Instant::now().elapsed();
    //TOOD: DISGUSTING!
    let summary = json!({
        "model": model,
        "judge_model": model,
        "roles": roles,
        "status": "completed",
        "pr_url": pr_url,
        "pr_title": pr_title,
        "total_prs": 1,
        "total_cost_usd": total_cost,
        "duration_secs": elapsed.as_secs_f64(),
        "aggregate_metrics": {
            "avg_f1": metrics_for_summary.f1(),
            "avg_precision": metrics_for_summary.precision(),
            "avg_recall": metrics_for_summary.recall(),
            "total_true_positives": metrics_for_summary.true_positives,
            "total_false_positives": metrics_for_summary.false_positives,
            "total_false_negatives": metrics_for_summary.false_negatives,
        },
    });

    let summary_str = serde_json::to_string_pretty(&summary)?;
    fs::write(
        output_subdir.join(crb_harness::paths::SUMMARY_FILE),
        &summary_str,
    )?;

    info!(
        run_id = %run_id,
        pr_title = %pr_title,
        findings = result.findings_count,
        cost = total_cost,
        elapsed_secs = elapsed.as_secs_f64(),
        "Ad-hoc review completed"
    );

    Ok(())
}

/// Extract a string field from a JSON summary value.
fn summary_str(data: &Option<serde_json::Value>, key: &str, default: &str) -> String {
    data.as_ref()
        .and_then(|s| s.get(key))
        .and_then(|v| v.as_str())
        .unwrap_or(default)
        .to_string()
}

/// Extract an f64 field from a JSON summary value.
fn summary_f64(data: &Option<serde_json::Value>, key: &str, default: f64) -> f64 {
    data.as_ref()
        .and_then(|s| s.get(key))
        .and_then(|v| v.as_f64())
        .unwrap_or(default)
}

/// Load the JSON summary file for an ad-hoc run, if it exists.
fn load_adhoc_summary(path: &Path) -> (Option<serde_json::Value>, bool) {
    let summary_path = path.join(crb_harness::paths::SUMMARY_FILE);
    match fs::read_to_string(&summary_path) {
        Ok(content) => match serde_json::from_str(&content) {
            Ok(val) => (Some(val), true),
            Err(_) => (None, false),
        },
        Err(_) => (None, false),
    }
}

/// Scan an ad-hoc run directory and produce a summary.
fn scan_adhoc_run_dir(path: &Path, run_id: &str) -> Option<AdhocRunSummary> {
    let (summary_data, _has_summary) = load_adhoc_summary(path);

    let pr_title = summary_str(&summary_data, "pr_title", "Unknown");
    let pr_url = summary_str(&summary_data, "pr_url", "");
    let status_str = summary_str(&summary_data, "status", "unknown");
    let status = match status_str.as_str() {
        "completed" => RunStatus::Completed,
        "failed" => RunStatus::Failed,
        "cancelled" => RunStatus::Cancelled,
        "running" => RunStatus::Running,
        _ => RunStatus::Pending,
    };
    let model = summary_str(&summary_data, "model", "unknown");
    let total_cost = summary_f64(&summary_data, "total_cost_usd", 0.0);
    let roles: Vec<String> = summary_data
        .as_ref()
        .and_then(|s| s.get("roles"))
        .and_then(|v| {
            if let Some(arr) = v.as_array() {
                Some(
                    arr.iter()
                        .filter_map(|r| r.as_str().map(String::from))
                        .collect(),
                )
            } else if let Some(s) = v.as_str() {
                // Backward compat: old format stored comma-separated string
                Some(s.split(',').map(|r| r.trim().to_string()).collect())
            } else {
                None
            }
        })
        .unwrap_or_default();
    let created_at = created_at_from_run_id(run_id);
    let findings_count = count_adhoc_findings(path);

    Some(AdhocRunSummary {
        id: run_id.to_string(),
        pr_url,
        pr_title,
        status,
        created_at,
        model,
        roles,
        findings_count,
        total_cost,
    })
}

/// Parse `created_at` from an ad-hoc run ID of the form `adhoc-{timestamp}`.
fn created_at_from_run_id(run_id: &str) -> String {
    if let Some(ts_str) = run_id.strip_prefix("adhoc-") {
        if let Ok(ts) = ts_str.parse::<u64>() {
            chrono::DateTime::from_timestamp(ts as i64, 0)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| "unknown".to_string())
        } else {
            "unknown".to_string()
        }
    } else {
        "unknown".to_string()
    }
}

/// Count findings from the per-PR result file in an ad-hoc run directory.
fn count_adhoc_findings(path: &Path) -> usize {
    let entries = match fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return 0,
    };

    for entry in entries.flatten() {
        let fpath = entry.path();
        if fpath.extension().map_or(true, |e| e != "json") {
            continue;
        }
        if fpath
            .file_name()
            .map_or(true, |n| n == crb_harness::paths::SUMMARY_FILE)
        {
            continue;
        }
        if let Ok(content) = fs::read_to_string(&fpath) {
            if let Ok(pr_json) = serde_json::from_str::<PrResult>(&content) {
                return pr_json.findings_count;
            }
        }
    }
    0
}
