use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use crb_agents::agent::AgentEntry;
use crb_shared::diff::Diff;
use crb_types::benchmark::Metrics;
use crb_types::wrappers::WrappedData;
use rig_core::agent::Agent;
use rig_core::model::Model;
use rig_core::providers::openai;
use rig_core::providers::openai::responses_api::ResponsesCompletionModel;

use crb_reporting::PrResult;
use crb_reporting::golden::GoldenCommentEntry;
use crb_types::benchmark::JudgeVerdict;

use crate::adaptive::get_agents_for_diff;
use crate::pipeline::run_consensus;
use crate::{CacheBackend, ReviewerConfig, Role};

/// Convenience function that matches the existing `evaluate_pr()` signature in `crb-harness`
/// but uses the full consensus pipeline internally.
///
/// Bridges between `crb-reporting`'s [`GoldenCommentEntry`] / [`PrResult`] types
/// and the consensus crate's richer golden-comment model so it can serve as a
/// drop-in replacement for the single-agent evaluation.
///
/// Because `crb-reporting::GoldenComment` lacks `file` / `line` fields,
/// the conversion uses an empty file, line 0, and the comment text wrapped in [`regex::escape`] as the message regex.
///
/// If `cache` is provided, agent interactions and judge calls are cached.
#[allow(clippy::too_many_arguments)]
#[deprecated = "Run a normal review first and then implement a new function to evaluate the results only."]
pub async fn evaluate_pr_with_consensus(
    pr: &GoldenCommentEntry,
    diff: &Diff,
    client: &openai::Client,
    model: &Model,
    judge: &Agent<ResponsesCompletionModel>,
    rules_preamble: Option<&str>,
    template_vars: Option<&HashMap<String, serde_json::Value>>,
    selected_agents: &[&'static AgentEntry],
    max_findings: usize,
    cache: Option<Arc<dyn CacheBackend>>,
    diff_hash: &str,
    prompt_hash: &str,
    rules_hash: &str,
    judge_prompt_hash: &str,
    judge_model: &str,
    tool_preamble: Option<&str>,
    workdir: Option<&str>,
    additional_params: Option<serde_json::Value>,
    dashboard_tx: Option<tokio::sync::broadcast::Sender<crb_types::RunEvent>>,
) -> Result<PrResult> {
    let roles = get_agents_for_diff(diff, selected_agents);

    let reviewer_configs: Vec<ReviewerConfig> = roles
        .iter()
        .map(|agent| ReviewerConfig {
            role: Role(agent.role_abbreviation.to_string()),
            model: model.to_string(),
            max_findings,
        })
        .collect();

    let report = run_consensus(
        diff.get(),
        pr.comments.clone(),
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

    Ok(PrResult {
        pr_title: pr.pr_title.clone(),
        url: pr.url.clone(),
        findings_count: total_findings,
        golden_count: pr.comments.len(),
        metrics: Metrics {
            true_positives: report.true_positives.len(),
            false_positives: report.false_positives.len(),
            false_negatives: report.false_negatives.len(),
            duration_secs: 0.0, // TODO: Add timing metrics to the consensus report
        },
        verdicts,
        cost: None, // TODO: Add cost metrics to the consensus report
    })
}
