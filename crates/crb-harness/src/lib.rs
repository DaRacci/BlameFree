use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use crb_agents::build_agent;
use crb_agents::prompts::{AgentEntry, PromptLibrary};
use crb_auditor::apply_severity_auditor;
use crb_cache::sha256::sha256_hex;
use crb_cache::traits::CacheBackend;
use crb_consensus::harness::evaluate_pr_with_consensus;
use crb_reporting::cost::AnalyticsTracker;
use crb_reporting::golden::GoldenCommentEntry;
use crb_reporting::history::{RunHistoryEntry, append_run_history};
use crb_reporting::{Metrics, PrResult};
use crb_shared::deduplicate::semantic_dedup;
use crb_shared::finding::Finding;
use crb_shared::jaccard::jaccard_similarity;
use crb_shared::url::parse_github_url;
use crb_shared::{diff, sanitize_filename};
use crb_tools::linters::tool::LinterArgs;
use crb_tools::{build_tool_server, create_linter_tool};
use crb_types::RunEvent;
use crb_types::benchmark::JudgeVerdict;
use crb_types::wrappers::{Diff, Model};
use regex::Regex;
use rig_core::agent::{Agent, PromptResponse};
use rig_core::client::ProviderClient;
use rig_core::completion::{Prompt, Usage};
use rig_core::providers::openai;
use rig_core::providers::openai::responses_api::ResponsesCompletionModel;
use rig_core::tool::Tool;
use rig_core::tool::server::{ToolServer, ToolServerHandle};
use tokio::sync::broadcast;
use tracing::{info, info_span, warn};

use crate::config::ReviewArgs;
use crate::eval::{EvalConfig, EvalStrategy};
use crate::model_capabilities::ReasoningEffort;

pub mod config;
pub mod eval;
pub mod finding;
pub mod history;
pub mod model_capabilities;
pub mod paths;
pub mod pipeline;
pub mod review;
pub mod runner;
pub mod test_utils;

/// Describes which kind of diff to review.
pub enum ReviewMode {
    /// Review a commit range `base..head`.
    Commits { base: String, head: String },

    /// Review the current working tree (unstaged + staged).
    Working,
}

/// Parameters for a full PR review.
#[deprecated = "Use EvalConfig and evaluate_pr() instead"]
pub struct ReviewParams {
    /// Unified diff of the PR to review.
    pub diff: String,

    /// Model identifier.
    pub model: String,

    /// Title of the PR being reviewed.
    pub pr_title: String,

    /// Reviewer role abbreviations.
    pub roles: Vec<String>,

    /// Maximum number of findings to return per agent.
    pub max_findings: usize,

    /// Optional cache directory for LLM response caching.
    pub cache_dir: Option<PathBuf>,
}

/// Load the diff for a PR from pre-extracted cached diff files.
///
/// Cached diffs live at `{benchmark_dir}/diffs/{owner}_{repo}_{pr_num}.diff`.
pub fn load_cached_diff(
    benchmark_dir: &Path,
    owner: &str,
    repo: &str,
    pr_num: u32,
) -> Option<String> {
    let diffs_dir = benchmark_dir.join("diffs");
    let diff_path = diffs_dir.join(format!("{}_{}_{}.diff", owner, repo, pr_num));
    match fs::read_to_string(&diff_path) {
        Ok(content) => {
            info!(
                "Loaded cached diff ({} bytes) from {}",
                content.len(),
                diff_path.display()
            );
            Some(content)
        }
        Err(e) => {
            warn!(
                "Cached diff not found at {}: {}. Using empty diff.",
                diff_path.display(),
                e
            );
            None
        }
    }
}

/// Call an async function with exponential backoff retry.
#[doc(hidden)]
pub async fn with_retry<F, Fut, T, E>(f: F, max_retries: usize, base_delay_ms: u64) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut attempt = 0usize;
    loop {
        match f().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                attempt += 1;
                if attempt >= max_retries {
                    return Err(e);
                }
                let delay = Duration::from_millis(base_delay_ms * 2u64.pow(attempt as u32));
                warn!(
                    "Attempt {}/{} failed: {}. Retrying in {}ms",
                    attempt,
                    max_retries,
                    e,
                    delay.as_millis()
                );
                tokio::time::sleep(delay).await;
            }
        }
    }
}

/// Spawn a single agent task for one role, with caching and retry.
#[allow(clippy::too_many_arguments)]
#[deprecated = "Use typed agents with output_schema and EvalConfig.cache instead."]
fn spawn_agent_task(
    role: String,
    client: openai::Client,
    model: Arc<String>,
    diff: Arc<String>,
    diff_hash: String,
    rules_hash: String,
    rules_preamble: Option<String>,
    cache: Option<Arc<dyn CacheBackend>>,
    cost_tracker: Arc<AnalyticsTracker>,
    dashboard_tx: Option<broadcast::Sender<RunEvent>>,
    additional_params: Option<serde_json::Value>,
    tool_server_handle: ToolServerHandle,
) -> impl std::future::Future<Output = Result<Vec<Finding>, String>> {
    async move {
        let prompt_library = PromptLibrary::get_instance();
        let span = info_span!("agent", role = %role);
        let _guard = span.enter();

        let prompt_hash = sha256_hex(prompt_library.get(&role).unwrap_or(""));
        let agent_cache_key = sha256_hex(&format!(
            "{prompt_hash}:{diff_hash}:{}:{}:{}",
            model.as_str(),
            role,
            rules_hash,
        ));

        // Check cache first
        if let Some(ref c) = cache {
            let raw = c.load_raw(&agent_cache_key);
            if !raw.is_empty() {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
                    let cached_response = val["response"].as_str().unwrap_or("").to_string();
                    let cached_usage = val.get("usage").and_then(|u| {
                        if u.is_null() || !u.is_object() {
                            None
                        } else {
                            Some(Usage {
                                input_tokens: u["input_tokens"].as_u64().unwrap_or(0),
                                output_tokens: u["output_tokens"].as_u64().unwrap_or(0),
                                total_tokens: u["total_tokens"].as_u64().unwrap_or(0),
                                cached_input_tokens: u["cached_input_tokens"].as_u64().unwrap_or(0),
                                cache_creation_input_tokens: u["cache_creation_input_tokens"]
                                    .as_u64()
                                    .unwrap_or(0),
                                reasoning_tokens: u["reasoning_tokens"].as_u64().unwrap_or(0),
                                tool_use_prompt_tokens: u["tool_use_prompt_tokens"]
                                    .as_u64()
                                    .unwrap_or(0),
                            })
                        }
                    });
                    info!(
                        "CACHE HIT for agent role={} (key={})",
                        role,
                        &agent_cache_key[..12]
                    );
                    let usage = cached_usage.unwrap_or_default();
                    cost_tracker.record_agent(&usage, true);
                    if let Some(ref tx) = dashboard_tx {
                        // Ignore; receiver may have disconnected
                        let _ = tx.send(RunEvent::AgentChunk {
                            identifier: role.clone(),
                            chunk: cached_response.clone(),
                        });
                        let result = parse_agent_findings(&cached_response);
                        let findings_count = result.as_ref().map(|v| v.len()).unwrap_or(0);
                        // Ignore; receiver may have disconnected
                        let _ = tx.send(RunEvent::AgentFinished {
                            identifier: role,
                            findings: findings_count,
                            success: result.is_ok(),
                        });
                    }
                    let result = parse_agent_findings(&cached_response);
                    return result;
                }
            }
        }
        info!(
            "CACHE MISS for agent role={} (key={})",
            role,
            &agent_cache_key[..12]
        );

        let agent = build_agent(
            &client,
            model.as_str(),
            &role,
            rules_preamble.as_deref(),
            None,
            None,
            additional_params.clone(),
            tool_server_handle.clone(),
        );

        let result: Result<Vec<Finding>, String> = with_retry(
            || {
                let agent = agent.clone();
                let role = role.clone();
                let diff = Arc::clone(&diff);
                let agent_cache_key = agent_cache_key.clone();
                let cache = cache.clone();
                let ct = cost_tracker.clone();
                let tx = dashboard_tx.clone();
                async move {
                    let resp: PromptResponse = agent
                        .prompt(diff.as_str())
                        .extended_details()
                        .await
                        .map_err(|e| e.to_string())?;
                    let response = resp.output;
                    let usage = resp.usage;

                    ct.record_agent(&usage, false);

                    if let Some(ref tx) = tx {
                        let _ = tx.send(RunEvent::AgentChunk {
                            // Ignore — receiver may have disconnected
                            identifier: role.clone(),
                            chunk: response.clone(),
                        });
                    }

                    if let Some(ref c) = cache {
                        let cache_data = serde_json::json!({
                            "response": response,
                            "usage": {
                                "input_tokens": usage.input_tokens,
                                "output_tokens": usage.output_tokens,
                                "total_tokens": usage.total_tokens,
                                "cached_input_tokens": usage.cached_input_tokens,
                                "cache_creation_input_tokens": usage.cache_creation_input_tokens,
                                "reasoning_tokens": usage.reasoning_tokens,
                                "tool_use_prompt_tokens": usage.tool_use_prompt_tokens,
                            },
                        });
                        c.store_raw(
                            &agent_cache_key,
                            &serde_json::to_string(&cache_data).unwrap(),
                        );
                    }

                    let findings = parse_agent_findings(&response);
                    if let Some(ref tx) = tx {
                        let findings_count = findings.as_ref().map(|v| v.len()).unwrap_or(0);
                        let _ = tx.send(RunEvent::AgentFinished {
                            // Ignore — receiver may have disconnected
                            identifier: role.clone(),
                            findings: findings_count,
                            success: findings.is_ok(),
                        });
                    }
                    findings
                }
            },
            3,
            1000,
        )
        .await;

        if result.is_err() {
            if let Some(ref tx) = dashboard_tx {
                let _ = tx.send(RunEvent::AgentFinished {
                    // Ignore — receiver may have disconnected
                    identifier: role.clone(),
                    findings: 0,
                    success: false,
                });
            }
        }
        result
    }
}

/// Run the original single-agent evaluation with finding collection.
/// (private) used by evaluate_pr
#[doc(hidden)]
#[deprecated = "Use EvalConfig-based evaluate_pr() instead."]
#[allow(trivial_casts)]
async fn evaluate_pr_single_agent(
    pr: &GoldenCommentEntry,
    client: &openai::Client,
    model: &str,
    diff: &str,
    linter_findings: Vec<Finding>,
    rules_preamble: Option<&str>,
    cache: Option<Arc<dyn CacheBackend>>,
    cost_tracker: Arc<AnalyticsTracker>,
    dashboard_tx: Option<&broadcast::Sender<RunEvent>>,
    additional_params: Option<serde_json::Value>,
) -> Result<(Vec<Finding>, Vec<JudgeVerdict>)> {
    let diff_hash = sha256_hex(diff);
    let rules_hash = sha256_hex(rules_preamble.unwrap_or(""));

    // ── Phase 1: spawn one agent per role ─────────────────────────────────
    let mut agent_set = tokio::task::JoinSet::new();
    let dashboard_tx_owned = dashboard_tx.map(|t| t.clone());
    let prompt_lib = PromptLibrary::get_instance();
    let tool_server_handle = ToolServer::new().run();
    let diff_arc = Arc::new(diff.to_string());
    let model_arc = Arc::new(model.to_string());
    let rules_preamble_owned = rules_preamble.map(String::from);
    for role in prompt_lib.agents() {
        let cache_arc: Option<Arc<dyn CacheBackend>> = cache.clone();
        agent_set.spawn(spawn_agent_task(
            role.role_abbreviation.to_string(),
            client.clone(),
            Arc::clone(&model_arc),
            Arc::clone(&diff_arc),
            diff_hash.clone(),
            rules_hash.clone(),
            rules_preamble_owned.clone(),
            cache_arc,
            cost_tracker.clone(),
            dashboard_tx_owned.clone(),
            additional_params.clone(),
            tool_server_handle.clone(),
        ));
    }

    // ── Phase 2: collect agent findings ──────────────────────────────────
    let mut all_findings: Vec<Finding> = linter_findings;
    while let Some(res) = agent_set.join_next().await {
        match res {
            Ok(Ok(mut findings)) => all_findings.append(&mut findings),
            Ok(Err(e)) => warn!("Agent failed: {e}"),
            Err(e) => warn!("Agent join error: {e}"),
        }
    }

    // ── Phase 3: return with empty verdicts (no judging) ─────────────────
    Ok((all_findings, Vec::new()))
}


/// Load the diff for a PR from pre-extracted cached diff files.
///
/// Tries the persistent worktree first, then falls back to cached diff files
/// at `{benchmark_dir}/diffs/{owner}_{repo}_{pr_num}.diff`.
#[deprecated = "Use load_cached_diff directly."]
pub async fn load_pr_diff(pr: &GoldenCommentEntry, benchmark_dir: &Path) -> Result<String> {
    match parse_github_url(&pr.url) {
        Ok((owner, repo, pr_num)) => {
            let worktree_path = benchmark_dir
                .join("worktrees")
                .join(format!("{owner}_{repo}_{pr_num}"));
            if worktree_path.join(".git").exists() {
                info!(
                    "Using persistent worktree at {} for PR #{}",
                    worktree_path.display(),
                    pr_num
                );
            }

            let d = load_cached_diff(benchmark_dir, &owner, &repo, pr_num).unwrap_or_default();
            if d.is_empty() {
                warn!("Empty diff for PR: {} (url: {})", pr.pr_title, pr.url);
            } else {
                info!("Loaded diff ({} bytes) for PR: {}", d.len(), pr.pr_title);
            }
            Ok(d)
        }
        Err(_) => {
            warn!(
                "Could not extract PR info from URL '{}'. Using empty diff.",
                pr.url
            );
            Ok(String::new())
        }
    }
}

/// Unified evaluation of a single PR.
///
/// This function runs the steps:
/// - diff preprocessing
/// - linter collection
/// - strategy dispatch
/// - post-processing (dedup / severity auditor / capping)
/// - metrics computation
/// - dashboard events
/// - metadata
/// - caching
#[deprecated = "Use crate::finding::evaluate_pr() instead."]
pub async fn evaluate_pr(
    pr: &GoldenCommentEntry,
    diff: &Diff,
    config: &EvalConfig,
) -> Result<PrResult> {
    // let bench_dir = config
    //     .benchmark_dir
    //     .as_deref()
    //     .unwrap_or_else(|| Path::new("."));

    // let cache: Option<Arc<dyn CacheBackend>> = config.cache.clone();

    // let diff = crb_shared::diff::preprocess_diff(diff);

    // let mut linter_findings: Vec<Finding> = Vec::new();
    // if let Some(ref configs) = config.linter_configs {
    //     let host_repo_path = bench_dir.to_string_lossy().to_string();
    //     let mut linter_set = tokio::task::JoinSet::new();
    //     for (_name, lconfig) in configs {
    //         let tool = create_linter_tool(lconfig);
    //         let args = LinterArgs {
    //             repo_path: host_repo_path.clone(),
    //         };
    //         linter_set.spawn(async move {
    //             let result = tool.call(args).await;
    //             result
    //         });
    //     }

    //     while let Some(res) = linter_set.join_next().await {
    //         match res {
    //             Ok(Ok(findings)) => linter_findings.extend(findings),
    //             Ok(Err(e)) => warn!("Linter failed: {e}"),
    //             Err(e) => warn!("Linter join error: {e}"),
    //         }
    //     }

    //     info!(
    //         "Found {} linter finding(s) for PR: {}",
    //         linter_findings.len(),
    //         pr.pr_title
    //     );
    // }

    // if config.linters_only {
    //     return Ok(PrResult {
    //         pr_title: pr.pr_title.clone(),
    //         url: pr.url.clone(),
    //         findings_count: linter_findings.len(),
    //         golden_count: pr.comments.len(),
    //         metrics: Metrics::default(),
    //         verdicts: vec![],
    //         cost: Some(config.cost_tracker.to_summary()),
    //     });
    // }

    let rules_preamble = config.ruleset.as_ref().map(|rs| rs.format_preamble(&[]));

    let pr_key = sanitize_filename(&pr.pr_title);
    if let Some(ref tx) = config.dashboard_tx {
        for role in config
            .roles
            .split(',')
            .map(|r| r.trim())
            .filter(|r| !r.is_empty())
        {
            let _ = tx.send(RunEvent::AgentStarted {
                identifier: pr_key.clone(),
                agent: role.to_string(),
            });
        }
    }

    let (all_findings, verdicts) = match config.strategy {
        EvalStrategy::Single => {
            let reasoning_params = config.reasoning_effort.and_then(|re| {
                model_capabilities::make_additional_params(&Model(config.model.clone()), Some(re))
            });
            evaluate_pr_single_agent(
                pr,
                &config.client,
                &config.model,
                &diff,
                linter_findings,
                rules_preamble.as_deref(),
                cache.clone(),
                config.cost_tracker.clone(),
                config.dashboard_tx.as_ref(),
                reasoning_params,
            )
            .await?
        }
        EvalStrategy::Panel => {
            let reasoning_params = config.reasoning_effort.and_then(|re| {
                model_capabilities::make_additional_params(&Model(config.model.clone()), Some(re))
            });
            evaluate_pr_single_agent(
                pr,
                &config.client,
                &config.model,
                &diff,
                linter_findings,
                rules_preamble.as_deref(),
                cache.clone(),
                config.cost_tracker.clone(),
                config.dashboard_tx.as_ref(),
                reasoning_params,
            )
            .await?
        }
    };

    // let processed_findings = post_process_findings(&all_findings);

    if let Some(ref tx) = config.dashboard_tx {
        for (_i, role) in ["SA", "CL", "AR", "SEC"].iter().enumerate() {
            let _ = tx.send(RunEvent::AgentFinished {
                identifier: role.to_string(),
                findings: processed_findings.len() / 4,
                success: true,
            });
        }
    }

    // ── Phase 10: Metrics ────────────────────────────────────────────────
    let metrics = Metrics::default();

    // ── Phase 11: Dashboard PrCompleted event ──────────────────────────
    if let Some(ref tx) = config.dashboard_tx {
        let tokens = config.cost_tracker.total_tokens();
        let total_tokens = tokens.0 + tokens.1;
        let cost_usd = config.cost_tracker.total_cost_usd();
        let total_agent_calls = 4;
        let _ = tx.send(RunEvent::ReviewCompleted {
            identifier: pr_key,
            metrics: crb_types::MetricsData {
                true_positives: metrics.true_positives,
                false_positives: metrics.false_positives,
                false_negatives: metrics.false_negatives,
                precision: metrics.precision,
                recall: metrics.recall,
                f1: metrics.f1,
            },
            cost: cost_usd,
            total_tokens,
            agent_calls: total_agent_calls,
            findings_count: verdicts.len(),
        });
    }

    // ── Phase 12: Cache metadata ────────────────────────────────────────
    let metadata = serde_json::json!({
        "pr_title": pr.pr_title,
        "url": pr.url,
        "model": config.model,
        "strategy": format!("{:?}", config.strategy),
        "timestamp": format!("{:?}", std::time::SystemTime::now()),
        "findings_count": verdicts.len(),
        "golden_count": pr.comments.len(),
        "metrics": {
            "true_positives": metrics.true_positives,
            "false_positives": metrics.false_positives,
            "false_negatives": metrics.false_negatives,
            "precision": metrics.precision,
            "recall": metrics.recall,
            "f1": metrics.f1,
        },
    });
    if let Some(ref cache) = cache {
        match serde_json::to_string(&metadata) {
            Ok(json_str) => cache.store_raw("run_metadata", &json_str),
            Err(e) => warn!("Failed to serialize cache metadata: {e}"),
        }
    }

    // ── Phase 13: Return result ─────────────────────────────────────────
    Ok(PrResult {
        pr_title: pr.pr_title.clone(),
        url: pr.url.clone(),
        findings_count: verdicts.len(),
        golden_count: pr.comments.len(),
        metrics,
        verdicts,
        cost: Some(config.cost_tracker.to_summary()),
    })
}

/// Write the `_summary.json` aggregate statistics file to the cache directory.
#[doc(hidden)]
#[deprecated = "Use AnalyticsSnapshot / Metrics serialization instead."]
pub fn write_summary(
    cache_dir: &PathBuf,
    model: &str,
    judge_model: &str,
    results: &[PrResult],
    duration: Duration,
) -> Result<()> {
    let total_llm_calls: usize = results.iter().map(|r| r.findings_count).sum();
    let total_judge_calls: usize = results.iter().map(|r| r.verdicts.len()).sum();

    let total_tokens: usize = results
        .iter()
        .filter_map(|r| r.cost.as_ref())
        .map(|c| c.agent_tokens_in + c.agent_tokens_out + c.judge_tokens_in + c.judge_tokens_out)
        .sum();
    let total_cost_usd: f64 = results
        .iter()
        .filter_map(|r| r.cost.as_ref())
        .map(|c| c.total_usd)
        .sum();
    let avg_agent_cache_hit_rate = if results.is_empty() {
        0.0
    } else {
        results
            .iter()
            .filter_map(|r| r.cost.as_ref())
            .map(|c| c.agent_cache_hit_rate)
            .sum::<f64>()
            / results.len() as f64
    };
    let avg_judge_cache_hit_rate = if results.is_empty() {
        0.0
    } else {
        results
            .iter()
            .filter_map(|r| r.cost.as_ref())
            .map(|c| c.judge_cache_hit_rate)
            .sum::<f64>()
            / results.len() as f64
    };

    let aggregate_metrics = if results.is_empty() {
        serde_json::json!({})
    } else {
        let avg_precision =
            results.iter().map(|r| r.metrics.precision).sum::<f64>() / results.len() as f64;
        let avg_recall =
            results.iter().map(|r| r.metrics.recall).sum::<f64>() / results.len() as f64;
        let avg_f1 = results.iter().map(|r| r.metrics.f1).sum::<f64>() / results.len() as f64;
        serde_json::json!({
            "avg_precision": avg_precision,
            "avg_recall": avg_recall,
            "avg_f1": avg_f1,
            "total_true_positives": results.iter().map(|r| r.metrics.true_positives).sum::<usize>(),
            "total_false_positives": results.iter().map(|r| r.metrics.false_positives).sum::<usize>(),
            "total_false_negatives": results.iter().map(|r| r.metrics.false_negatives).sum::<usize>(),
        })
    };

    let summary = serde_json::json!({
        "run_id": std::env::current_dir()
            .ok()
            .and_then(|d| d.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_default(),
        "model": model,
        "judge_model": judge_model,
        "total_prs": results.len(),
        "total_llm_calls": total_llm_calls,
        "total_judge_calls": total_judge_calls,
        "duration_secs": duration.as_secs_f64(),
        "aggregate_metrics": aggregate_metrics,
        "total_tokens": total_tokens,
        "total_cost_usd": total_cost_usd,
        "agent_cache_hit_rate": avg_agent_cache_hit_rate,
        "judge_cache_hit_rate": avg_judge_cache_hit_rate,
    });

    let summary_path = cache_dir.join(crate::paths::SUMMARY_FILE);
    fs::write(&summary_path, serde_json::to_string_pretty(&summary)?)?;
    info!("Cache summary written to: {}", summary_path.display());

    let run_entry = RunHistoryEntry {
        run_id: summary["run_id"].as_str().unwrap_or("").to_string(),
        timestamp: format!("{:?}", std::time::SystemTime::now()),
        model: model.to_string(),
        judge_model: judge_model.to_string(),
        total_prs: results.len(),
        duration_secs: duration.as_secs_f64(),
        total_cost_usd,
        total_tokens,
        agent_cache_hit_rate: avg_agent_cache_hit_rate,
        judge_cache_hit_rate: avg_judge_cache_hit_rate,
    };
    append_run_history(cache_dir, &run_entry)?;

    Ok(())
}
