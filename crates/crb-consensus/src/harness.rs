//! Harness integration — bridge between crb-harness and the consensus pipeline.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use rig_core::agent::Agent;
use rig_core::completion::Usage;
use rig_core::providers::openai;
use rig_core::providers::openai::responses_api::ResponsesCompletionModel;

use crb_agents::prompts::PromptLibrary;
use crb_judge::JudgeVerdict;
use crb_reporting::PrResult;
use crb_reporting::golden::GoldenCommentEntry;

#[cfg(feature = "exp16_adaptive_agents")]
use crate::adaptive::should_use_single_agent;
use crate::pipeline::run_consensus;
use crate::{CacheBackend, GoldenComment, ReviewerConfig, Role};

/// Convenience function that matches the existing `evaluate_pr()` signature in
/// `crb-harness` but uses the full consensus pipeline internally.
///
/// Bridges between `crb-reporting`'s [`GoldenCommentEntry`] / [`PrResult`] types
/// and the consensus crate's richer golden-comment model so it can serve as a
/// drop-in replacement for the single-agent evaluation.
///
/// Because `crb-reporting::GoldenComment` lacks `file` / `line` fields, the
/// conversion uses an empty file, line 0, and the comment text wrapped in
/// [`regex::escape`] as the message regex.
///
/// If `cache` is provided, agent interactions and judge calls are cached.
#[allow(clippy::too_many_arguments)]
pub async fn evaluate_pr_with_consensus(
    pr: &GoldenCommentEntry,
    diff: &str,
    client: &openai::Client,
    model: &str,
    judge: &Agent<ResponsesCompletionModel>,
    rules_preamble: Option<&str>,
    template_vars: Option<&HashMap<String, serde_json::Value>>,
    roles: &[&str],
    max_findings: usize,
    cache: Option<Arc<dyn CacheBackend>>,
    // Content-addressed cache key components
    diff_hash: &str,
    prompt_hash: &str,
    rules_hash: &str,
    judge_prompt_hash: &str,
    judge_model: &str,
    tool_preamble: Option<&str>,
    workdir: Option<&str>,
    additional_params: Option<serde_json::Value>,
    dashboard_tx: Option<tokio::sync::broadcast::Sender<crb_types::RunEvent>>,
) -> Result<(PrResult, Usage, Usage, usize, usize, usize)> {
    // ── Adaptive agent dispatch (EXP-016) ──────────────────────────────
    #[cfg(feature = "exp16_adaptive_agents")]
    let roles: Vec<&str> = {
        if should_use_single_agent(diff, 3, 200) {
            tracing::info!(
                "EXP-016: adaptive dispatch — small PR detected, using single GEN agent"
            );
            vec!["GEN"]
        } else {
            roles.to_vec()
        }
    };
    #[cfg(not(feature = "exp16_adaptive_agents"))]
    let roles = roles;

    // Build one reviewer config per selected role.
    let reviewer_configs: Vec<ReviewerConfig> = roles
        .iter()
        .map(|role_str| ReviewerConfig {
            role: Role(role_str.to_string()),
            model: model.to_string(),
            max_findings,
        })
        .collect();

    // Convert crb-reporting GoldenComments to consensus GoldenComments.
    // crb-reporting's GoldenComment lacks file/line, so we use
    // empty file + line 0 and escape the comment text as a regex.
    let consensus_goldens: Vec<GoldenComment> = pr
        .comments
        .iter()
        .map(|gc| GoldenComment {
            file: String::new(),
            line: 0,
            message_regex: regex::escape(&gc.comment),
            severity: gc.severity.clone(),
            source: "any".to_string(),
        })
        .collect();

    let report = run_consensus(
        diff,
        consensus_goldens,
        reviewer_configs,
        client,
        judge,
        rules_preamble,
        template_vars,
        cache,
        diff_hash,
        prompt_hash,
        rules_hash,
        judge_prompt_hash,
        judge_model,
        tool_preamble,
        workdir,
        additional_params,
        dashboard_tx,
    )
    .await;
    // Build verdicts for compatibility with crb-reporting::PrResult.
    let mut verdicts = Vec::new();
    for _ in &report.true_positives {
        verdicts.push(JudgeVerdict {
            reasoning: "Matched via heuristic or LLM judge".into(),
            match_: true,
            confidence: 1.0,
        });
    }
    for _ in &report.false_positives {
        verdicts.push(JudgeVerdict {
            reasoning: "No matching golden comment".into(),
            match_: false,
            confidence: 0.0,
        });
    }

    let total_findings: usize = report
        .agents
        .iter()
        .map(|(_, findings)| findings.len())
        .sum();

    Ok((
        PrResult {
            pr_title: pr.pr_title.clone(),
            url: pr.url.clone(),
            findings_count: total_findings,
            golden_count: pr.comments.len(),
            metrics: crb_judge::Metrics {
                true_positives: report.true_positives.len(),
                false_positives: report.false_positives.len(),
                false_negatives: report.false_negatives.len(),
                precision: report.precision,
                recall: report.recall,
                f1: report.f1,
            },
            verdicts,
            cost: None,
        },
        report.agent_usage,
        report.judge_usage,
        report.agent_api_calls,
        report.judge_api_calls,
        report.judge_cache_hits,
    ))
}
