//! The full consensus pipeline
//! Run reviewers, judge against goldens, compute metrics.

use std::collections::HashMap;
use std::sync::Arc;

use crb_reporting::cost::AnalyticsSnapshot;
use crb_reporting::golden::GoldenComment;
use crb_tools::build_tool_server;
use rig_core::agent::Agent;
use rig_core::completion::Usage;
use rig_core::providers::openai;
use rig_core::providers::openai::responses_api::ResponsesCompletionModel;

use crb_shared::finding::Finding;

use crate::execution::run_reviewers;
use crate::judge::judge_comment;
use crate::{ConsensusReport, MatchResult};
use crate::Role;
use crb_cache::traits::CacheBackend;

/// Run the full multi-agent consensus pipeline.
///
/// 1. Concurrently run all reviewer agents via [`run_reviewers`].
/// 2. For each golden comment, attempt heuristic matching ([`judge_comment`]) against all findings.
/// 3. Goldens that do not match heuristically fall back to the LLM judge.
/// 4. Remaining unmatched findings are classified as false positives.
/// 5. Compute precision / recall / F1 metrics.
///
/// If `cache` is provided, agent interactions and judge calls are cached
/// using content-addressed keys derived from prompt hashes, diff hash, etc.
#[allow(clippy::too_many_arguments)]
#[deprecated = "Needs rewrite to use new types, apis and patterns."]
pub async fn run_consensus(
    diff: &str,
    goldens: Vec<GoldenComment>,
    reviewer_configs: Vec<(Role, String, usize)>,
    client: &openai::Client,
    judge: &Agent<ResponsesCompletionModel>,
    rules_preamble: Option<&str>,
    template_vars: Option<&HashMap<String, serde_json::Value>>,
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
) -> ConsensusReport {
    let tool_server = build_tool_server(workdir).run();
    let (agents, agent_api_calls, agent_usage) = run_reviewers(
        reviewer_configs,
        diff,
        diff_hash,
        client,
        rules_preamble,
        template_vars,
        cache.clone(),
        prompt_hash,
        rules_hash,
        tool_preamble,
        additional_params,
        dashboard_tx,
        tool_server.clone(),
    )
    .await;

    let judge_usage = Usage::new();
    let mut unmatched: Vec<Finding> = agents
        .iter()
        .flat_map(|(_, findings)| findings.iter())
        .cloned()
        .collect();
    unmatched.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.message.cmp(&b.message))
    });

    let mut true_positives: Vec<(GoldenComment, Finding)> = Vec::new();
    let mut false_negatives: Vec<GoldenComment> = Vec::new();
    let mut judge_api_calls: usize = 0;
    let mut judge_cache_hits: usize = 0;

    for golden in &goldens {
        let result = judge_comment(
            golden,
            &unmatched,
            judge,
            judge_model,
            cache.clone(),
            judge_prompt_hash,
            &mut judge_api_calls,
            &mut judge_cache_hits,
        )
        .await;

        match result {
            MatchResult::TruePositive => {
                // Remove the first file+line matched finding from the pool
                // (judge_comment returns on the first match, so the first
                // candidate in iteration order is the one that was matched).
                if !unmatched.is_empty() {
                    let matched = unmatched.remove(0);
                    true_positives.push((golden.clone(), matched));
                }
            }
            MatchResult::FalseNegative => {
                false_negatives.push(golden.clone());
            }
            MatchResult::FalsePositive => {
                // This variant isn't returned by judge_comment (it checks a golden
                // against candidates, so it only yields TP or FN).
                // Defensively treat as FN.
                false_negatives.push(golden.clone());
            }
        }
    }

    let false_positives = unmatched;
    let tp_count = true_positives.len();
    let fp_count = false_positives.len();
    let fn_count = false_negatives.len();

    ConsensusReport {
        agents,
        true_positives,
        false_positives,
        false_negatives,
        analytics: AnalyticsSnapshot {
            ..Default::default() // TODO
        },
        ..Default::default()
    }
}

/// Run the consensus judging step on already-completed review findings.
/// This is the post-step that follows the review pipeline.
///
/// 1. Accepts already-completed review results from each agent.
/// 2. For each golden comment, attempts heuristic matching ([`judge_comment`]) against pooled findings.
/// 3. Remaining unmatched findings are classified as false positives.
/// 4. Computes precision / recall / F1 metrics.
///
/// If `cache` is provided, judge calls are cached using content-addressed keys
/// derived from prompt hashes.
#[allow(clippy::too_many_arguments)]
pub async fn run_consensus_post(
    agents: Vec<(Role, Vec<Finding>)>,
    goldens: Vec<GoldenComment>,
    judge: &Agent<ResponsesCompletionModel>,
    judge_model: &str,
    cache: Option<Arc<dyn CacheBackend>>,
    judge_prompt_hash: &str,
) -> ConsensusReport {
    let mut unmatched: Vec<Finding> = agents
        .iter()
        .flat_map(|(_, findings)| findings.iter())
        .cloned()
        .collect();
    unmatched.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.message.cmp(&b.message))
    });

    let mut true_positives: Vec<(GoldenComment, Finding)> = Vec::new();
    let mut false_negatives: Vec<GoldenComment> = Vec::new();
    let mut judge_api_calls: usize = 0;
    let mut judge_cache_hits: usize = 0;

    for golden in &goldens {
        let result = judge_comment(
            golden,
            &unmatched,
            judge,
            judge_model,
            cache.clone(),
            judge_prompt_hash,
            &mut judge_api_calls,
            &mut judge_cache_hits,
        )
        .await;

        match result {
            MatchResult::TruePositive => {
                // Remove the first file+line matched finding from the pool
                // (judge_comment returns on the first match, so the first
                // candidate in iteration order is the one that was matched).
                if !unmatched.is_empty() {
                    let matched = unmatched.remove(0);
                    true_positives.push((golden.clone(), matched));
                }
            }
            MatchResult::FalseNegative => {
                false_negatives.push(golden.clone());
            }
            MatchResult::FalsePositive => {
                // This variant isn't returned by judge_comment (it checks a golden
                // against candidates, so it only yields TP or FN).
                // Defensively treat as FN.
                false_negatives.push(golden.clone());
            }
        }
    }

    let false_positives = unmatched;

    ConsensusReport {
        agents,
        true_positives,
        false_positives,
        false_negatives,
        analytics: AnalyticsSnapshot {
            ..Default::default() // TODO
        },
        ..Default::default()
    }
}
