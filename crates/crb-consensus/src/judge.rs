//! LLM-as-judge with Jaccard fallback.

use std::sync::Arc;

use crb_reporting::golden::GoldenComment;
use crb_types::benchmark::JudgeVerdict;
use rig_core::agent::Agent;
use rig_core::completion::Prompt;
use rig_core::providers::openai::responses_api::ResponsesCompletionModel;

use crb_cache::sha256::sha256_hex;

use crb_shared::finding::Finding;
use crb_shared::jaccard::jaccard_similarity;
use tracing::{info, warn};

use crate::MatchResult;
use crb_cache::traits::CacheBackend;

/// Compute a content-addressed cache key for a judge call.
#[deprecated = "Use [`crb_shared::cache`]"]
fn compute_judge_cache_key(
    judge_prompt_hash: &str,
    finding_message: &str,
    golden_message_regex: &str,
    judge_model: &str,
) -> String {
    sha256_hex(&format!(
        "{judge_prompt_hash}{finding_message}{golden_message_regex}{judge_model}"
    ))
}

/// Judge a single golden comment against a set of candidate findings using
/// an **LLM-as-judge first, Jaccard word-overlap fallback** pipeline (matching
/// the Python step3_judge_comments.py order).
///
/// **Algorithm:**
/// 1. **Pre-filter** candidates by exact `file` + `line` match (fast, cheap).
/// 2. **LLM judge**: for each pre-filtered candidate, ask the judge agent whether the finding matches the golden.
///   Uses content-addressed caching (via `cache` / `judge_prompt_hash` / `judge_model`) to avoid redundant API calls.
///   Returns `TruePositive` on the **first** LLM match.
/// 3. **Jaccard fallback**: if the LLM found no match, run Jaccard word-overlap with threshold **0.3** (matching Python).
///   Returns `TruePositive` on the **first** candidate scoring ≥ 0.3.
/// 4. **FalseNegative**: no candidate matched.
#[allow(clippy::too_many_arguments, clippy::cognitive_complexity)]
pub async fn judge_comment(
    golden: &GoldenComment,
    candidates: &[Finding],
    judge: &Agent<ResponsesCompletionModel>,
    judge_model: &str,
    cache: Option<Arc<dyn CacheBackend>>,
    judge_prompt_hash: &str,
    judge_api_calls: &mut usize,
    judge_cache_hits: &mut usize,
) -> MatchResult {
    // pre-filter candidates by exact file + line match
    let file_matches: Vec<_> = candidates
        .iter()
        .filter(|f| golden.matches_candidate(f))
        .collect();
    if file_matches.is_empty() {
        return MatchResult::FalseNegative;
    }

    // LLM judge on each pre-filtered candidate
    for finding in &file_matches {
        let judge_key = compute_judge_cache_key(
            judge_prompt_hash,
            &finding.message,
            &golden.comment,
            judge_model,
        );

        if let Some(ref c) = cache {
            let cached = c.load_raw(&judge_key);
            if !cached.is_empty() {
                if let Ok(cached_verdict) = serde_json::from_str::<JudgeVerdict>(&cached) {
                    info!("CACHE HIT for judge (key={})", &judge_key[..12]);
                    *judge_cache_hits += 1;
                    if cached_verdict.match_ {
                        return MatchResult::TruePositive;
                    }
                    continue;
                }
            }
        }

        info!("CACHE MISS for judge (key={})", &judge_key[..12]);
        *judge_api_calls += 1;
        let prompt = format_judge_prompt(&golden.comment, &finding.message);
        match judge.prompt(&prompt).extended_details().await {
            Ok(resp) => {
                if let Ok(verdict) = serde_json::from_str::<JudgeVerdict>(&resp.output) {
                    if let Some(ref c) = cache {
                        c.store_raw(
                            &judge_key,
                            &serde_json::to_string(&verdict).unwrap_or_default(),
                        );
                    }
                    if verdict.match_ {
                        return MatchResult::TruePositive;
                    }
                }
            }
            Err(e) => {
                warn!("Judge call failed: {e}");
            }
        }
    }

    // Try Jaccard word-overlap fallback
    for finding in &file_matches {
        if jaccard_similarity(&finding.message, &golden.comment, false) >= 0.3 {
            return MatchResult::TruePositive;
        }
    }

    MatchResult::FalseNegative
}

fn format_judge_prompt(golden_comment: &str, candidate: &str) -> String {
    JUDGE_PROMPT
        .replace("{golden_comment}", golden_comment)
        .replace("{candidate}", candidate)
}

pub const JUDGE_PROMPT: &str = "You are evaluating AI code review tools.
Determine if the candidate issue matches the golden (expected) comment.

Golden Comment (the issue we're looking for):
{golden_comment}

Candidate Issue (from the tool's review):
{candidate}

Instructions:
- Determine if the candidate identifies the SAME underlying issue as the golden comment
- Accept semantic matches - different wording is fine if it's the same problem
- Focus on whether they point to the same bug, concern, or code issue

Respond with ONLY a JSON object:
{\"reasoning\": \"brief explanation\", \"match\": true/false, \"confidence\": 0.0-1.0}";
