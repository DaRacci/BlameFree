//! Judging a golden comment — LLM-as-judge with Jaccard fallback.

use std::sync::Arc;

use rig_core::agent::Agent;
use rig_core::providers::openai::responses_api::ResponsesCompletionModel;

use crb_judge::run_judge;
use crb_shared::cache::compute_judge_cache_key;
use crb_shared::finding::Finding;
use crb_shared::jaccard::jaccard_similarity;

use crate::{CacheBackend, GoldenComment, MatchResult};

/// Judge a single golden comment against a set of candidate findings using
/// an **LLM-as-judge first, Jaccard word-overlap fallback** pipeline (matching
/// the Python step3_judge_comments.py order).
///
/// **Algorithm:**
/// 1. **Pre-filter** candidates by exact `file` + `line` match (fast, cheap).
/// 2. **LLM judge** — for each pre-filtered candidate, ask the judge agent
///    whether the finding matches the golden.  Uses content-addressed caching
///    (via `cache` / `judge_prompt_hash` / `judge_model`) to avoid redundant
///    API calls.  Returns `TruePositive` on the **first** LLM match.
/// 3. **Jaccard fallback** — if the LLM found no match, run Jaccard word-overlap
///    with threshold **0.3** (matching Python).  Returns `TruePositive` on the
///    **first** candidate scoring ≥ 0.3.
/// 4. **FalseNegative** — no candidate matched.
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
    // Step 1: pre-filter candidates by exact file + line match
    let file_matches: Vec<&Finding> = candidates
        .iter()
        .filter(|f| golden.matches_candidate(f))
        .collect();

    if file_matches.is_empty() {
        return MatchResult::FalseNegative;
    }

    // Step 2: LLM judge on each pre-filtered candidate (with cache)
    for finding in &file_matches {
        let judge_key = compute_judge_cache_key(
            judge_prompt_hash,
            &finding.message,
            &golden.message_regex,
            judge_model,
        );

        // Check judge cache first
        if let Some(ref c) = cache {
            if let Some(cached_verdict) = c.lookup_judge_by_key(&judge_key) {
                tracing::info!("CACHE HIT for judge (key={})", &judge_key[..12]);
                *judge_cache_hits += 1;
                if cached_verdict.match_ {
                    return MatchResult::TruePositive;
                }
                // Cache says no match — skip this finding
                continue;
            }
        }

        // Cache miss — make the API call
        tracing::info!("CACHE MISS for judge (key={})", &judge_key[..12]);
        *judge_api_calls += 1;
        match run_judge(judge, &golden.message_regex, &finding.message).await {
            Ok((verdict, _usage)) => {
                // Write-through cache
                if let Some(ref c) = cache {
                    let verdict_json = serde_json::to_string(&verdict).unwrap_or_default();
                    c.save_judge_with_key(
                        &judge_key,
                        &golden.message_regex,
                        &finding.message,
                        &verdict_json,
                    );
                }
                if verdict.match_ {
                    return MatchResult::TruePositive;
                }
            }
            Err(e) => {
                tracing::warn!("Judge call failed: {e}");
            }
        }
    }

    // Step 3: LLM missed — try Jaccard word-overlap fallback (threshold 0.3)
    for finding in &file_matches {
        if jaccard_similarity(&finding.message, &golden.message_regex, false) >= 0.3 {
            return MatchResult::TruePositive;
        }
    }

    // Step 4: no match at all
    MatchResult::FalseNegative
}
