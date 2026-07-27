//! Heuristic matching between golden comments and candidate findings.
//!
//! The core function [`judge_comment`] performs content-based matching:
//! a candidate finding matches a golden comment if their text overlaps
//! (substring containment in either direction, case-insensitive).

use std::sync::Arc;

use rig_core::agent::Agent;
use rig_core::providers::openai::responses_api::ResponsesCompletionModel;
use riv_cache::traits::CacheBackend;
use riv_types::benchmark::golden::GoldenComment;
use riv_types::finding::Finding;

use crate::MatchResult;

/// Heuristically match a golden comment against a pool of candidate findings.
///
/// Returns [`MatchResult::TruePositive`] if any candidate finding's message
/// textually overlaps with the golden comment (substring containment,
/// case-insensitive). Returns [`MatchResult::FalseNegative`] otherwise.
///
/// The `judge` / `judge_model` / `cache` / `judge_prompt_hash` parameters are
/// reserved for a future LLM-based fallback judge. Currently only the
/// heuristic path is active.
pub async fn judge_comment(
    golden: &GoldenComment,
    candidates: &[Finding],
    _judge: &Agent<ResponsesCompletionModel>,
    _judge_model: &str,
    _cache: Arc<dyn CacheBackend>,
    _judge_prompt_hash: &str,
    _judge_api_calls: &mut usize,
) -> MatchResult {
    let golden_lower = golden.comment.to_lowercase();

    for finding in candidates {
        let finding_lower = finding.message.to_lowercase();

        // Substring overlap in either direction is considered a match.
        if finding_lower.contains(&golden_lower) || golden_lower.contains(&finding_lower) {
            return MatchResult::TruePositive;
        }
    }

    MatchResult::FalseNegative
}
