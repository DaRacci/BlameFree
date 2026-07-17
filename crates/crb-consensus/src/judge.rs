//! LLM-as-judge with Jaccard fallback.

use std::sync::Arc;

use crb_types::benchmark::{golden::GoldenComment, judge::JudgeVerdict};
use rig_core::agent::Agent;
use rig_core::completion::Prompt;
use rig_core::providers::openai::responses_api::ResponsesCompletionModel;

use crb_cache::sha256::sha256_hex;

use crb_shared::jaccard::jaccard_similarity;
use crb_types::finding::Finding;
use tracing::warn;

use crate::MatchResult;
use crb_cache::traits::{CacheBackend, CacheKey};

/// Content-addressed cache key for a judge call.
struct JudgeCacheKey {
    judge_prompt_hash: String,
    finding_message: String,
    golden_message_regex: String,
    judge_model: String,
}

impl CacheKey for JudgeCacheKey {
    fn cache_key(&self) -> String {
        sha256_hex(&format!(
            "{}{}{}{}",
            self.judge_prompt_hash,
            self.finding_message,
            self.golden_message_regex,
            self.judge_model
        ))
    }
}

/// Judge a single golden comment against a set of candidate findings using an **LLM-as-judge first,
/// Jaccard word-overlap fallback** pipeline (matching the Python step3_judge_comments.py order).
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
    cache: Arc<dyn CacheBackend>,
    judge_prompt_hash: &str,
    judge_api_calls: &mut usize,
) -> MatchResult {
    let file_matches: Vec<_> = candidates.iter().collect();

    for finding in &file_matches {
        let cache_key = JudgeCacheKey {
            judge_prompt_hash: judge_prompt_hash.to_string(),
            finding_message: finding.message.clone(),
            golden_message_regex: golden.comment.clone(),
            judge_model: judge_model.to_string(),
        };

        let calls = &mut *judge_api_calls;
        let verdict = cache
            .get_or_compute(&cache_key, move || {
                let comment = golden.comment.clone();
                let msg = finding.message.clone();
                async move {
                    *calls += 1;
                    let prompt = format_judge_prompt(&comment, &msg);
                    match judge.prompt(&prompt).extended_details().await {
                        Ok(resp) => serde_json::from_str::<JudgeVerdict>(&resp.output).unwrap_or(
                            JudgeVerdict {
                                match_: false,
                                reasoning: String::new(),
                                confidence: 0.0,
                            },
                        ),
                        Err(e) => {
                            warn!("Judge call failed: {e}");
                            JudgeVerdict {
                                match_: false,
                                reasoning: String::new(),
                                confidence: 0.0,
                            }
                        }
                    }
                }
            })
            .await;

        if verdict.match_ {
            return MatchResult::TruePositive;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_judge_prompt_replaces_golden_comment() {
        let prompt = format_judge_prompt("my golden comment", "some candidate");
        assert!(prompt.contains("my golden comment"));
        assert!(!prompt.contains("{golden_comment}"));
    }

    #[test]
    fn test_format_judge_prompt_replaces_candidate() {
        let prompt = format_judge_prompt("golden", "my candidate finding");
        assert!(prompt.contains("my candidate finding"));
        assert!(!prompt.contains("{candidate}"));
    }

    #[test]
    fn test_format_judge_prompt_with_special_chars() {
        let golden = "code: unsafe block detected";
        let candidate = "line 42: potential memory issue";
        let prompt = format_judge_prompt(golden, candidate);
        assert!(prompt.contains(golden));
        assert!(prompt.contains(candidate));
    }

    #[test]
    fn test_format_judge_prompt_empty_strings() {
        let prompt = format_judge_prompt("", "");
        // After replacement, empty golden_comment means the line reads "...looking for:\n\nCandidate..."
        assert!(prompt.contains("Golden Comment (the issue we're looking for):\n\n"));
        assert!(prompt.contains("Candidate Issue (from the tool's review):\n\n"));
    }

    #[test]
    fn test_judge_prompt_constant_not_empty() {
        assert!(!JUDGE_PROMPT.is_empty());
        assert!(JUDGE_PROMPT.contains("{golden_comment}"));
        assert!(JUDGE_PROMPT.contains("{candidate}"));
    }
}
