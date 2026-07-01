//! API handlers for ad-hoc PR reviews.
//!
//! Provides endpoints to submit a GitHub PR URL for ad-hoc review (read-only,
//! no GitHub commenting), list previous ad-hoc reviews, and get their details.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::runs::{RunDetail, PrResultResponse, AggregateMetricsResponse, RunConfigResponse, MetricsJson, PrResultJson, VerdictJson, CostJson};
use crate::harness;
use crate::server::AppState;
use rig_core::client::ProviderClient;

/// Request body for POST /api/adhoc/review
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdhocReviewRequest {
    pub url: String,
    #[serde(default = "default_adhoc_model")]
    pub model: String,
    #[serde(default = "default_adhoc_roles")]
    pub roles: Vec<String>,
}

fn default_adhoc_model() -> String {
    "deepseek/deepseek-v4-flash".to_string()
}

fn default_adhoc_roles() -> Vec<String> {
    vec!["SA".to_string(), "CL".to_string()]
}

/// Response from POST /api/adhoc/review
#[derive(Debug, Clone, Serialize)]
pub struct AdhocReviewResponse {
    pub run_id: String,
    pub pr_title: String,
    pub status: String,
}

/// Summary of an ad-hoc review run (for the list endpoint)
#[derive(Debug, Clone, Serialize)]
pub struct AdhocRunSummary {
    pub id: String,
    pub pr_url: String,
    pub pr_title: String,
    pub status: String,
    pub created_at: String,
    pub model: String,
    pub roles: Vec<String>,
    pub findings_count: usize,
    pub total_cost: f64,
}

/// ── POST /api/adhoc/review ──────────────────────────────────────────────
///
/// Submit a GitHub PR URL for ad-hoc review. Fetches the PR diff + metadata
/// from the GitHub API, runs the harness agents, and stores results.
pub async fn start_adhoc_review(
    State(state): State<AppState>,
    Json(req): Json<AdhocReviewRequest>,
) -> impl IntoResponse {
    tracing::info!(
        "POST /api/adhoc/review url={} model={} roles={:?}",
        req.url,
        req.model,
        req.roles,
    );

    // Validate URL
    let (owner, repo, pr_number) = match parse_github_url(&req.url) {
        Some(info) => info,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "Invalid GitHub PR URL. Expected format: https://github.com/owner/repo/pull/123"
                })),
            )
                .into_response();
        }
    };

    // Fetch PR metadata and diff from GitHub API
    let (pr_title, diff) = match fetch_pr_diff(&state, &owner, &repo, pr_number).await {
        Ok(result) => result,
        Err(e) => {
            tracing::error!("Failed to fetch PR {}: {}", req.url, e);
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "error": format!("Failed to fetch PR: {}", e)
                })),
            )
                .into_response();
        }
    };

    // Create a unique run ID
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let run_id = format!("adhoc-{timestamp}");

    // Spawn a background task for the actual review
    let state_clone = state.clone();
    let roles_str = req.roles.join(",");
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
            &roles_str,
        )
        .await
        {
            tracing::error!("Ad-hoc review {run_id_bg} failed: {e}");
        }
    });

    (
        StatusCode::OK,
        Json(AdhocReviewResponse {
            run_id,
            pr_title,
            status: "running".to_string(),
        }),
    )
        .into_response()
}

/// ── GET /api/adhoc/runs ─────────────────────────────────────────────────
///
/// List all previous ad-hoc review runs.
pub async fn list_adhoc_runs(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let adhoc_dir = state.output_dir.join("adhoc");
    let mut runs: Vec<AdhocRunSummary> = Vec::new();

    if adhoc_dir.exists() {
        let entries = match std::fs::read_dir(&adhoc_dir) {
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

    // Sort by created_at descending (most recent first)
    runs.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Json(runs).into_response()
}

/// ── GET /api/adhoc/runs/:id ─────────────────────────────────────────────
///
/// Get details for a specific ad-hoc review run.
pub async fn get_adhoc_run(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> impl IntoResponse {
    let run_dir = state.output_dir.join("adhoc").join(&id);

    if !run_dir.exists() {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Ad-hoc run not found"})),
        )
            .into_response();
    }

    let summary_path = run_dir.join("_summary.json");
    let summary_data: Option<serde_json::Value> = std::fs::read_to_string(&summary_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());

    let model = summary_data
        .as_ref()
        .and_then(|s| s.get("model"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let roles: Vec<String> = summary_data
        .as_ref()
        .and_then(|s| s.get("roles"))
        .and_then(|v| v.as_str())
        .map(|s| s.split(',').map(|r| r.trim().to_string()).collect())
        .unwrap_or_default();

    let status = summary_data
        .as_ref()
        .and_then(|s| s.get("status"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let duration_secs = summary_data
        .as_ref()
        .and_then(|s| s.get("duration_secs"))
        .and_then(|v| v.as_f64());

    let total_cost = summary_data
        .as_ref()
        .and_then(|s| s.get("total_cost_usd"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    // Read per-PR result files
    let mut results: Vec<PrResultResponse> = Vec::new();
    let mut aggregate_metrics = AggregateMetricsResponse {
        avg_f1: 0.0,
        avg_precision: 0.0,
        avg_recall: 0.0,
        total_tp: 0,
        total_fp: 0,
        total_fn: 0,
        total_cost,
        total_prs: 0,
        duration_secs: duration_secs.unwrap_or(0.0),
    };

    if let Ok(entries) = std::fs::read_dir(&run_dir) {
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
                continue;
            }

            if let Ok(content) = std::fs::read_to_string(&file_path) {
                if let Ok(pr_json) = serde_json::from_str::<PrResultJson>(&content) {
                    let metrics = &pr_json.metrics;
                    results.push(PrResultResponse {
                        pr_number: 0,
                        pr_key: file_name.trim_end_matches(".json").to_string(),
                        title: pr_json.pr_title,
                        f1: Some(metrics.f1),
                        precision: Some(metrics.precision),
                        recall: Some(metrics.recall),
                        cost: pr_json.cost.as_ref().map(|c| c.total_usd),
                        status: Some("done".to_string()),
                        has_agents: false,
                    });

                    aggregate_metrics.total_tp += metrics.true_positives;
                    aggregate_metrics.total_fp += metrics.false_positives;
                    aggregate_metrics.total_fn += metrics.false_negatives;
                }
            }
        }
    }

    // Compute averages
    let n = results.len() as f64;
    if n > 0.0 {
        let sum_f1: f64 = results.iter().filter_map(|r| r.f1).sum();
        let sum_prec: f64 = results.iter().filter_map(|r| r.precision).sum();
        let sum_recall: f64 = results.iter().filter_map(|r| r.recall).sum();
        aggregate_metrics.avg_f1 = sum_f1 / n;
        aggregate_metrics.avg_precision = sum_prec / n;
        aggregate_metrics.avg_recall = sum_recall / n;
        aggregate_metrics.total_prs = n as u32;
    }

    let detail = RunDetail {
        id: id.clone(),
        name: id,
        pr_count: results.len(),
        results,
        aggregate: Some(aggregate_metrics),
        total_cost: Some(total_cost),
        total_tokens: 0,
        duration_secs,
        model: model.clone(),
        status,
        config: Some(RunConfigResponse {
            model,
            dataset: String::new(),
            roles,
        }),
    };

    Json(detail).into_response()
}

/// ── GET /api/adhoc/prs/:owner/:repo ──────────────────────────────────────────
///
/// List open PRs from a GitHub repo (proxied to avoid CORS).
#[derive(Debug, Serialize)]
pub struct GithubPrListItem {
    pub number: u32,
    pub title: String,
    pub html_url: String,
}

pub async fn list_repo_prs(
    State(state): State<AppState>,
    AxumPath((owner, repo)): AxumPath<(String, String)>,
) -> impl IntoResponse {
    tracing::info!("GET /api/adhoc/prs/{}/{}", owner, repo);

    let token = state.github_token.clone().unwrap_or_default();
    let client = reqwest::Client::new();
    let url = format!("https://api.github.com/repos/{owner}/{repo}/pulls?state=open&per_page=100");

    let mut req = client
        .get(&url)
        .header("User-Agent", "review-harness/1.0")
        .header("Accept", "application/json");
    if !token.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", token));
    }

    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": format!("HTTP error: {e}") })),
            )
                .into_response();
        }
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return (
            status,
            Json(serde_json::json!({ "error": format!("GitHub API returned {status}: {body}") })),
        )
            .into_response();
    }

    let items: Vec<serde_json::Value> = match resp.json().await {
        Ok(items) => items,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": format!("Failed to parse response: {e}") })),
            )
                .into_response();
        }
    };

    let prs: Vec<GithubPrListItem> = items
        .into_iter()
        .filter_map(|item| {
            let number = item.get("number")?.as_u64()? as u32;
            let title = item.get("title")?.as_str()?.to_string();
            let html_url = item.get("html_url")?.as_str()?.to_string();
            Some(GithubPrListItem { number, title, html_url })
        })
        .collect();

    Json(prs).into_response()
}

// ─── Internal helpers ─────────────────────────────────────────────────────

/// Parse a GitHub PR URL into (owner, repo, pr_number)
fn parse_github_url(url: &str) -> Option<(String, String, u32)> {
    let re = regex::Regex::new(r"^https://github\.com/([^/]+)/([^/]+)/pull/(\d+)$").ok()?;
    let caps = re.captures(url)?;
    let owner = caps.get(1)?.as_str().to_string();
    let repo = caps.get(2)?.as_str().to_string();
    let pr_number: u32 = caps.get(3)?.as_str().parse().ok()?;
    Some((owner, repo, pr_number))
}

/// GitHub API response for a PR (metadata endpoint)
#[derive(Debug, Deserialize)]
struct GithubPrResponse {
    title: String,
    #[serde(default)]
    body: Option<String>,
}

/// Fetch PR title and raw diff from the GitHub API.
async fn fetch_pr_diff(
    state: &AppState,
    owner: &str,
    repo: &str,
    pr_number: u32,
) -> Result<(String, String), String> {
    let token = state.github_token.clone().unwrap_or_default();

    let client = reqwest::Client::new();

    // Fetch PR metadata
    let pr_url = format!("https://api.github.com/repos/{owner}/{repo}/pulls/{pr_number}");
    let mut pr_req = client.get(&pr_url)
        .header("User-Agent", "review-harness/1.0")
        .header("Accept", "application/json");
    if !token.is_empty() {
        pr_req = pr_req.header("Authorization", format!("Bearer {}", token));
    }

    let pr_resp = pr_req.send().await.map_err(|e| format!("HTTP error: {e}"))?;
    if !pr_resp.status().is_success() {
        let status = pr_resp.status();
        let body = pr_resp.text().await.unwrap_or_default();
        return Err(format!("GitHub API returned {status}: {body}"));
    }

    let pr_data: GithubPrResponse = pr_resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse PR response: {e}"))?;

    // Fetch PR diff (raw text, using application/vnd.github.v3.diff)
    let diff_url = format!("https://api.github.com/repos/{owner}/{repo}/pulls/{pr_number}");
    let mut diff_req = client.get(&diff_url)
        .header("User-Agent", "review-harness/1.0")
        .header("Accept", "application/vnd.github.v3.diff");
    if !token.is_empty() {
        diff_req = diff_req.header("Authorization", format!("Bearer {}", token));
    }

    let diff_resp = diff_req.send().await.map_err(|e| format!("HTTP error: {e}"))?;
    if !diff_resp.status().is_success() {
        let status = diff_resp.status();
        let body = diff_resp.text().await.unwrap_or_default();
        return Err(format!("GitHub API returned {status}: {body}"));
    }

    let diff = diff_resp.text().await.map_err(|e| format!("Failed to read diff: {e}"))?;

    Ok((pr_data.title, diff))
}

/// Run the actual review pipeline for an ad-hoc PR.
async fn run_adhoc_review_inner(
    state: &AppState,
    run_id: &str,
    pr_url: &str,
    pr_title: &str,
    diff: &str,
    model: &str,
    roles: &str,
) -> anyhow::Result<()> {
    let output_subdir = state.output_dir.join("adhoc").join(run_id);
    let cache_dir = output_subdir.join("cache");

    tracing::info!(
        run_id = %run_id,
        pr_title = %pr_title,
        model = %model,
        roles = %roles,
        "Starting ad-hoc review"
    );

    // ── Setup clients ────────────────────────────────────────────────────
    let client = rig_core::providers::openai::Client::from_env()
        .map_err(|e| anyhow::anyhow!("Failed to create OpenAI client: {e}"))?;

    let judge = crb_judge::build_judge(&client, model);

    // ── Prompt library (built-in defaults) ────────────────────────────
    let prompt_lib = Arc::new({
        let mut lib = crb_agents::prompts::PromptLibrary::new();
        let prompts_dir = Path::new("prompts/builtin");
        if prompts_dir.exists() {
            match lib.load_from_dir(prompts_dir) {
                Ok(()) => tracing::info!("Loaded prompts from: {}", prompts_dir.display()),
                Err(e) => tracing::warn!("Failed to load prompts from {}: {e}", prompts_dir.display()),
            }
        }
        lib
    });

    // ── Create cache directory ────────────────────────────────────────
    std::fs::create_dir_all(&cache_dir)?;

    // ── Create a GoldenCommentEntry with empty comments ────────────────
    // (no golden data to compare against — just running agents on the diff)
    use crb_reporting::GoldenCommentEntry;
    let pr = GoldenCommentEntry {
        pr_title: pr_title.to_string(),
        url: pr_url.to_string(),
        original_url: None,
        az_comment: None,
        comments: vec![],
    };

    // ── Cache instance ────────────────────────────────────────────────
    let pr_key = crb_harness::utils::sanitize_filename(pr_title);
    let cache: Arc<crb_harness::LlmCache> = Arc::new(
        crb_harness::LlmCache::new(&cache_dir, &pr_key)
            .expect("Failed to create LLM cache directory"),
    );

    // ── Cost tracker ──────────────────────────────────────────────────
    let cost_tracker = Arc::new(crb_harness::CostTracker::new());

    // ── Preprocess diff ───────────────────────────────────────────────
    let diff = crb_harness::preprocess_diff(diff);

    if diff.is_empty() {
        tracing::warn!("Empty diff for PR: {}", pr_title);
    }

    // ── Rules preamble (none) ─────────────────────────────────────────
    let rules_preamble: Option<String> = None;

    // ── Agent evaluation (consensus pipeline) ─────────────────────────
    tracing::info!(
        "Running ad-hoc review with roles={}, model={}",
        roles,
        model
    );

    let (all_findings, verdicts) = crb_harness::evaluate_pr_consensus(
        &pr,
        &client,
        model,
        &judge,
        &diff,
        vec![], // no linter findings
        rules_preamble.as_deref(),
        &prompt_lib,
        roles,
        20, // max_findings
        Some(cache.clone()),
        cost_tracker.clone(),
        None, // workdir
    )
    .await?;

    let processed_findings = crb_harness::post_process_findings(&all_findings);

    // ── Compute metrics ───────────────────────────────────────────────
    // Since there are no golden comments, metrics will be 0
    let metrics = crb_judge::compute_metrics(&verdicts, 0);
    let metrics_for_summary = metrics.clone();

    let cost_summary = cost_tracker.to_summary();
    let total_cost = cost_summary.total_usd;

    // ── Build PrResult ────────────────────────────────────────────────
    use crb_reporting::PrResult;
    let result = PrResult {
        pr_title: pr_title.to_string(),
        url: pr_url.to_string(),
        findings_count: processed_findings.len(),
        golden_count: 0,
        metrics,
        verdicts,
        cost: Some(cost_summary),
    };

    // ── Write output ──────────────────────────────────────────────────
    std::fs::create_dir_all(&output_subdir)?;

    // Write per-PR result as JSON
    let pr_result_path = output_subdir.join(format!("{}.json", pr_key));
    let pr_json = PrResultJson {
        pr_title: result.pr_title.clone(),
        url: result.url.clone(),
        findings_count: result.findings_count,
        golden_count: 0,
        metrics: MetricsJson {
            true_positives: result.metrics.true_positives,
            false_positives: result.metrics.false_positives,
            false_negatives: result.metrics.false_negatives,
            precision: result.metrics.precision,
            recall: result.metrics.recall,
            f1: result.metrics.f1,
        },
        verdicts: result.verdicts.iter().map(|v| VerdictJson {
            reasoning: v.reasoning.clone(),
            match_: v.match_,
            confidence: v.confidence,
        }).collect(),
        cost: result.cost.map(|c| CostJson {
            total_usd: c.total_usd,
            agent_tokens_in: c.agent_tokens_in as u64,
            agent_tokens_out: c.agent_tokens_out as u64,
            judge_tokens_in: c.judge_tokens_in as u64,
            judge_tokens_out: c.judge_tokens_out as u64,
            agent_call_count: c.agent_call_count as u64,
            judge_call_count: c.judge_call_count as u64,
        }),
        findings: serde_json::json!({
            "findings": processed_findings.iter().map(|f| {
                serde_json::json!({
                    "message": f.message,
                    "severity": f.severity,
                    "file": f.file,
                    "line": f.line,
                    "rule_code": f.rule_code,
                })
            }).collect::<Vec<_>>()
        }),
        agent_responses: vec![],
    };

    let pr_json_str = serde_json::to_string_pretty(&pr_json)?;
    std::fs::write(&pr_result_path, &pr_json_str)?;

    // Write _summary.json
    let elapsed = std::time::Instant::now().elapsed();
    let summary = serde_json::json!({
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
            "avg_f1": metrics_for_summary.f1,
            "avg_precision": metrics_for_summary.precision,
            "avg_recall": metrics_for_summary.recall,
            "total_true_positives": metrics_for_summary.true_positives,
            "total_false_positives": metrics_for_summary.false_positives,
            "total_false_negatives": metrics_for_summary.false_negatives,
        },
    });

    let summary_str = serde_json::to_string_pretty(&summary)?;
    std::fs::write(output_subdir.join("_summary.json"), &summary_str)?;

    tracing::info!(
        run_id = %run_id,
        pr_title = %pr_title,
        findings = processed_findings.len(),
        cost = total_cost,
        elapsed_secs = elapsed.as_secs_f64(),
        "Ad-hoc review completed"
    );

    Ok(())
}

/// Scan an ad-hoc run directory and produce a summary.
fn scan_adhoc_run_dir(path: &Path, run_id: &str) -> Option<AdhocRunSummary> {
    let summary_path = path.join("_summary.json");
    let content = std::fs::read_to_string(&summary_path).ok()?;
    let data: serde_json::Value = serde_json::from_str(&content).ok()?;

    let pr_title = data
        .get("pr_title")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown")
        .to_string();

    let pr_url = data
        .get("pr_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let status = data
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let model = data
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let roles: Vec<String> = data
        .get("roles")
        .and_then(|v| v.as_str())
        .map(|s| s.split(',').map(|r| r.trim().to_string()).collect())
        .unwrap_or_default();

    let total_cost = data
        .get("total_cost_usd")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let findings_count = data
        .get("aggregate_metrics")
        .and_then(|m| m.as_object())
        .and_then(|m| {
            // Count total findings from per-PR file
            None::<usize>
        })
        .unwrap_or(0);

    // Try to get created_at from the directory name (adhoc-{timestamp})
    let created_at = if let Some(ts_str) = run_id.strip_prefix("adhoc-") {
        if let Ok(ts) = ts_str.parse::<u64>() {
            chrono::DateTime::from_timestamp(ts as i64, 0)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| "unknown".to_string())
        } else {
            "unknown".to_string()
        }
    } else {
        "unknown".to_string()
    };

    // Count findings from the per-PR result file
    let findings_count = if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let fpath = entry.path();
            if fpath.extension().map_or(true, |e| e != "json") {
                continue;
            }
            if fpath.file_name().map_or(true, |n| n == "_summary.json") {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(&fpath) {
                if let Ok(pr_json) = serde_json::from_str::<PrResultJson>(&content) {
                    return Some(AdhocRunSummary {
                        id: run_id.to_string(),
                        pr_url,
                        pr_title,
                        status,
                        created_at,
                        model,
                        roles,
                        findings_count: pr_json.findings_count,
                        total_cost,
                    });
                }
            }
        }
        // Fallback: count from aggregate metrics if no per-PR file
        0
    } else {
        0
    };

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
