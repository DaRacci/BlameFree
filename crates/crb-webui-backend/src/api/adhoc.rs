//! API handlers for ad-hoc PR reviews.
//!
//! Provides endpoints to submit a GitHub PR URL for ad-hoc review,
//! list previous ad-hoc reviews, and get their details.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
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
use crb_types::benchmark::judge::JudgeVerdict;
use crb_types::benchmark::metrics::{Metrics, MetricsProvider};
use crb_types::benchmark::result::PrResult;
use crb_types::cost::AnalyticsSnapshot;
use crb_types::review::{PullRequestReviewMetadata, Review, ReviewMetadata, ReviewStatus};
use crb_types::vcs::pr::PrMeta;
use crb_types::vcs::repository::{RemoteRepositoryMeta, VCSPlatform};
use crb_types::wrappers::Model;
use crb_webui_shared::config::AgentInfo;
use mti::prelude::{MagicTypeIdExt, V7};
use rig_core::client::ProviderClient;
use riv_stor::traits::Store;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{error, info, warn};

use crate::api::runs::RunDetailResponse;
use crate::api::runs::{self};
use crate::server::AppState;
use crb_webui_shared::review::RunConfig;

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
        Json(Review {
            id: run_id.as_str().create_type_id::<V7>(),
            agent_sessions: HashMap::new(),
            analytics: None,
            duration: None,
            status: ReviewStatus::Running,
            metadata: ReviewMetadata::Plain,
        }),
    )
        .into_response()
}

/// List all previous ad-hoc review runs.
pub async fn list_adhoc_runs(State(state): State<AppState>) -> impl IntoResponse {
    let store = &state.store;
    let mut runs: Vec<Review> = match store.list::<Review>(&()).await {
        Ok(r) => r,
        Err(_) => Vec::new(),
    };

    // Sort by id (adhoc IDs embed a Unix timestamp)
    runs.sort_by(|a, b| b.id.to_string().cmp(&a.id.to_string()));
    Json(runs).into_response()
}

/// Get details for a specific ad-hoc review run.
pub async fn get_adhoc_run(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> impl IntoResponse {
    let store = &state.store;
    let run_id_mtid = id.as_str().create_type_id::<V7>();

    // Try loading from store first
    if let Ok(Some(review)) = store.load::<Review>(&run_id_mtid).await {
        // Load PR results from store (lightweight — no children loaded)
        let results: Vec<PrResult> = store.list::<PrResult>(&()).await.unwrap_or_default();

        // Filter results matching this run (run_id is a prefix of pr_result.id)
        let run_results: Vec<PrResult> = results
            .into_iter()
            .filter(|pr| pr.id.to_string().starts_with(&id))
            .collect();

        let duration_secs = review.duration.map(|d| d.as_secs_f64()).unwrap_or(0.0);
        let mut aggregate_metrics = Metrics {
            true_positives: 0,
            false_positives: 0,
            false_negatives: 0,
            duration_secs,
        };
        for r in &run_results {
            aggregate_metrics.true_positives += r.metrics.true_positives;
            aggregate_metrics.false_positives += r.metrics.false_positives;
            aggregate_metrics.false_negatives += r.metrics.false_negatives;
        }

        let detail = RunDetailResponse {
            meta: review,
            results: run_results,
            aggregate: aggregate_metrics,
            config: None, // Config stored separately in summary; fine for now
        };

        return Json(detail).into_response();
    }

    // Fallback: read from filesystem (legacy runs not yet in store)
    let run_dir = state.output_dir.join("adhoc").join(&id);

    if !run_dir.exists() {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Ad-hoc run not found"})),
        )
            .into_response();
    }

    let (summary_data, _has_summary) = load_adhoc_summary(&run_dir);

    let model = summary_str(&summary_data, "model", "unknown");
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
    let duration_secs = summary_data
        .as_ref()
        .and_then(|s| s.get("duration_secs"))
        .and_then(|v| v.as_f64());

    let mut results: Vec<PrResult> = Vec::new();
    let mut aggregate_metrics = Metrics {
        true_positives: 0,
        false_positives: 0,
        false_negatives: 0,
        duration_secs: duration_secs.unwrap_or(0.0),
    };
    #[allow(deprecated)]
    for (file_path, _) in runs::iter_json_files(&run_dir) {
        if let Ok(content) = fs::read_to_string(&file_path) {
            if let Ok(pr_json) = serde_json::from_str::<PrResult>(&content) {
                let metrics = &pr_json.metrics;
                #[allow(deprecated)]
                results.push(PrResult {
                    id: pr_json.id,
                    golden_comments: vec![],
                    metrics: pr_json.metrics.clone(),
                    findings_with_verdicts: vec![],
                    cost: pr_json.cost.clone(),
                });

                aggregate_metrics.true_positives += metrics.true_positives;
                aggregate_metrics.false_positives += metrics.false_positives;
                aggregate_metrics.false_negatives += metrics.false_negatives;
            }
        }
    }

    let detail = RunDetailResponse {
        meta: Review {
            id: id.as_str().create_type_id::<V7>(),
            agent_sessions: HashMap::new(),
            analytics: None,
            duration: duration_secs.map(Duration::from_secs_f64),
            status: ReviewStatus::Completed,
            metadata: ReviewMetadata::Plain,
        },
        results,
        aggregate: aggregate_metrics,
        config: Some(RunConfig {
            model,
            dataset: String::new(),
            agents: roles
                .into_iter()
                .map(|abbr| AgentInfo {
                    abbreviation: abbr.clone(),
                    name: abbr,
                    incompatible_with_roles: vec![],
                })
                .collect(),
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

    let prs: Vec<PrMeta> = page
        .items
        .into_iter()
        .map(|pr| PrMeta {
            number: pr.number as u32,
            title: pr.title.unwrap_or_default(),
            url: pr.html_url.map(|u| u.to_string()).unwrap_or_default(),
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

    let client = rig_core::providers::openrouter::Client::from_env()
        .map_err(|e| anyhow::anyhow!("Failed to create OpenRouter client: {e}"))?;

    let prompt_lib = Arc::new(prompts::PromptLibrary::get_instance());

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

    let context = crb_harness::eval::EvalContext {
        repo_root: output_subdir.clone(),
        ruleset: None,
        repository: RemoteRepositoryMeta {
            platform: VCSPlatform::GitHub,
            owner: String::new(),
            name: String::new(),
        },
        pull_request: Some(crb_types::vcs::pr::PrMeta {
            title: pr_title.to_string(),
            url: pr_url.to_string(),
            number: 0,
        }),
    };

    let cfg = crb_harness::eval::EvalConfig {
        review_id: format!("adhoc-{}", run_id).create_type_id::<V7>(),
        context,
        strategy: crb_harness::eval::EvalStrategy::Panel,
        model: wrapped_model,
        reasoning_effort: None,
        client: Arc::new(client),
        cache: cache_backend,
        cost_tracker,
        dashboard_tx: None,
        agents,
        max_findings: 20,
        template_vars: None,
    };

    let findings = crb_harness::pipeline::evaluate(Diff::new(diff.raw.clone()), &cfg).await?;

    let metrics_for_summary = Metrics::default();

    let total_cost = 0.0;

    fs::create_dir_all(&output_subdir)?;

    let pr_result_path = output_subdir.join(format!("{}.json", pr_key));

    let pr_json = serde_json::json!({
        "id": format!("adhoc-{}", run_id),
        "pr_title": pr_title,
        "url": pr_url,
        "findings_count": findings.len(),
        "golden_count": 0,
        "metrics": metrics_for_summary,
        "verdicts": [],
        "cost": null,
        "findings": findings.iter().map(|f| serde_json::to_value(f)).collect::<Result<Vec<_>, _>>()?,
        "agent_responses": [],
    });

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

    // Save results to store
    let review_id = format!("adhoc-{}", run_id).create_type_id::<V7>();
    let pr_result = PrResult {
        id: review_id.clone(),
        golden_comments: Vec::new(),
        metrics: metrics_for_summary,
        findings_with_verdicts: findings
            .into_iter()
            .map(|f| {
                (
                    f,
                    crb_types::benchmark::judge::JudgeVerdict {
                        reasoning: "Pending judge evaluation".to_string(),
                        match_: false,
                        confidence: 0.0,
                    },
                )
            })
            .collect(),
        cost: AnalyticsSnapshot::default(),
    };
    let _ = state.store.save::<PrResult>(&pr_result).await;

    let review = Review {
        id: run_id.to_string().create_type_id::<V7>(),
        agent_sessions: HashMap::new(),
        analytics: None,
        duration: Some(elapsed),
        status: ReviewStatus::Completed,
        metadata: ReviewMetadata::Plain,
    };
    let _ = state.store.save::<Review>(&review).await;

    info!(
        run_id = %run_id,
        pr_title = %pr_title,
        findings = findings.len(),
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

/// Scan an ad-hoc run directory and produce a Review<PullRequestReviewMetadata>.
fn scan_adhoc_run_dir(path: &Path, run_id: &str) -> Option<Review> {
    let (summary_data, _has_summary) = load_adhoc_summary(path);

    let pr_title = summary_str(&summary_data, "pr_title", "Unknown");
    let pr_url = summary_str(&summary_data, "pr_url", "");
    let status_str = summary_str(&summary_data, "status", "unknown");
    let status = match status_str.as_str() {
        "completed" => ReviewStatus::Completed,
        "failed" => ReviewStatus::Failed,
        "cancelled" => ReviewStatus::Cancelled,
        "running" => ReviewStatus::Running,
        _ => ReviewStatus::Pending,
    };
    let model = summary_str(&summary_data, "model", "unknown");

    Some(Review {
        id: run_id.create_type_id::<V7>(),
        agent_sessions: HashMap::new(),
        analytics: None,
        duration: None,
        status,
        metadata: ReviewMetadata::PullRequest(PullRequestReviewMetadata {
            repository: RemoteRepositoryMeta {
                platform: VCSPlatform::GitHub,
                owner: String::new(),
                name: String::new(),
            },
            meta: PrMeta {
                title: pr_title,
                url: pr_url,
                number: 0,
            },
        }),
    })
}
