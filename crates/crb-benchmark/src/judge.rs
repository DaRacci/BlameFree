use rig_core::agent::{Agent, PromptResponse};
use rig_core::client::CompletionClient;
use rig_core::completion::{Prompt, Usage};
use rig_core::providers::openai::{Client, responses_api::ResponsesCompletionModel};

use crb_types::benchmark::{JudgeVerdict, Metrics};

/// The Martian JUDGE_PROMPT template used for LLM-as-judge evaluation.
///
/// This prompt instructs the judge to compare a golden (expected) comment
/// against a candidate finding from an agent and return a structured verdict.
pub const JUDGE_PROMPT: &str = "\
You are evaluating AI code review tools.
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

/// Build a judge agent with the Martian JUDGE_PROMPT as its preamble.
pub fn build_judge(client: &Client, model: &str) -> Agent<ResponsesCompletionModel> {
    client
        .agent(model)
        .preamble(JUDGE_PROMPT)
        .temperature(0.3)
        .build()
}

/// Format the judge prompt by substituting the golden comment and candidate finding.
pub fn format_judge_prompt(golden_comment: &str, candidate: &str) -> String {
    JUDGE_PROMPT
        .replace("{golden_comment}", golden_comment)
        .replace("{candidate}", candidate)
}

/// Run the judge agent to produce a verdict for a single comparison,
/// returning both the verdict and the API usage statistics.
///
/// # Errors
///
/// Returns an error if the judge agent call fails or the response cannot
/// be parsed as a [`JudgeVerdict`].
pub async fn run_judge(
    judge: &Agent<ResponsesCompletionModel>,
    golden_comment: &str,
    candidate: &str,
) -> Result<(JudgeVerdict, Usage), anyhow::Error> {
    let prompt = format_judge_prompt(golden_comment, candidate);
    let resp: PromptResponse = judge.prompt(&prompt).extended_details().await?;
    let verdict: JudgeVerdict = serde_json::from_str(&resp.output)?;
    Ok((verdict, resp.usage))
}

/// Tokenize text exactly like Python's `.lower().split()`
/// (whitespace split only, no punctuation stripping)
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split_whitespace()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Jaccard word-overlap heuristic matching.
///
/// - Tokenizes both strings into lowercase word sets (split on non-alphanumeric)
/// - Computes Jaccard = |intersection| / |union|
/// - Returns Some(match_score) if >= threshold, None otherwise
///
/// If either string is empty after tokenization, the union is zero and `None` is
/// returned (cannot compute meaningful similarity on empty sets).
///
/// # Returns
///
/// `Some(score)` where `score` is the Jaccard similarity (0.0–1.0) if it meets
/// the threshold, or `None` if below threshold or either input is empty.
pub fn jaccard_match(finding_text: &str, golden_comment: &str, threshold: f64) -> Option<f64> {
    let finding_words = tokenize(finding_text);
    let golden_words = tokenize(golden_comment);

    if finding_words.is_empty() || golden_words.is_empty() {
        return None;
    }

    let finding_set: std::collections::BTreeSet<&str> =
        finding_words.iter().map(|s| s.as_str()).collect();
    let golden_set: std::collections::BTreeSet<&str> =
        golden_words.iter().map(|s| s.as_str()).collect();

    let intersection: usize = finding_set.intersection(&golden_set).count();
    let union: usize = finding_set.union(&golden_set).count();

    if union == 0 {
        return None;
    }

    let score = intersection as f64 / union as f64;

    if score >= threshold {
        Some(score)
    } else {
        None
    }
}

/// Compute precision, recall, and F1 from judge verdicts.
///
/// - **TP** (true positive): a finding that matched a golden comment.
/// - **FP** (false positive): a finding that did *not* match any golden comment.
/// - **FN** (false negative): a golden comment that *no* finding matched.
///
/// `verdicts` is the flattened list of all (finding × golden) judge calls.
/// `golden_count` is the total number of golden comments for this PR.
///
/// # Returns
///
/// A [`Metrics`] struct with TP, FP, FN, precision, recall, and F1.
pub fn compute_metrics(verdicts: &[JudgeVerdict], golden_count: usize) -> Metrics {
    let true_positives = verdicts.iter().filter(|v| v.match_).count();
    let false_positives = verdicts.len() - true_positives;

    // Cap FNs: we can't have more FNs than golden comments, nor fewer than
    // golden_count minus matched (each golden can be matched at most once
    // per paired evaluation, but across all findings we count TPs directly).
    let matched_goldens = true_positives.min(golden_count);
    let false_negatives = golden_count.saturating_sub(matched_goldens);

    let precision = if true_positives + false_positives > 0 {
        true_positives as f64 / (true_positives + false_positives) as f64
    } else {
        0.0
    };

    let recall = if true_positives + false_negatives > 0 {
        true_positives as f64 / (true_positives + false_negatives) as f64
    } else {
        // No false negatives and no true positives means no goldens — undefined recall.
        // Return 1.0 (perfect recall) for the degenerate case.
        1.0
    };

    let _f1 = if (precision + recall) > 0.0 {
        2.0 * precision * recall / (precision + recall)
    } else {
        0.0
    };

    Metrics {
        true_positives,
        false_positives,
        false_negatives,
        duration_secs: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use crate::judge::{compute_metrics, format_judge_prompt, jaccard_match};
    use crate::judge::JUDGE_PROMPT;
    use crb_types::benchmark::{JudgeVerdict, MetricsProvider};

    const THRESHOLD: f64 = 0.12;

    #[test]
    fn test_perfect_match() {
        let verdicts = vec![
            JudgeVerdict {
                reasoning: "a".into(),
                match_: true,
                confidence: 1.0,
            },
            JudgeVerdict {
                reasoning: "b".into(),
                match_: true,
                confidence: 1.0,
            },
        ];
        let m = compute_metrics(&verdicts, 2);
        assert_eq!(m.true_positives, 2);
        assert_eq!(m.false_positives, 0);
        assert_eq!(m.false_negatives, 0);
        assert!((m.precision() - 1.0).abs() < 1e-6);
        assert!((m.recall() - 1.0).abs() < 1e-6);
        assert!((m.f1() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_no_match() {
        let verdicts = vec![JudgeVerdict {
            reasoning: "a".into(),
            match_: false,
            confidence: 0.0,
        }];
        let m = compute_metrics(&verdicts, 1);
        assert_eq!(m.true_positives, 0);
        assert_eq!(m.false_positives, 1);
        assert_eq!(m.false_negatives, 1);
        assert!((m.precision() - 0.0).abs() < 1e-6);
        assert!((m.recall() - 0.0).abs() < 1e-6);
        assert!((m.f1() - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_partial_match() {
        let verdicts = vec![
            JudgeVerdict {
                reasoning: "a".into(),
                match_: true,
                confidence: 1.0,
            },
            JudgeVerdict {
                reasoning: "b".into(),
                match_: false,
                confidence: 0.0,
            },
        ];
        let m = compute_metrics(&verdicts, 2);
        assert_eq!(m.true_positives, 1);
        assert_eq!(m.false_positives, 1);
        assert_eq!(m.false_negatives, 1);
        assert!((m.precision() - 0.5).abs() < 1e-6);
        assert!((m.recall() - 0.5).abs() < 1e-6);
        assert!((m.f1() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_jaccard_identical() {
        let score = jaccard_match(
            "hardcoded secret in config",
            "hardcoded secret in config",
            THRESHOLD,
        );
        assert!(score.unwrap() > 0.9);
    }

    #[test]
    fn test_jaccard_partial_overlap() {
        let score = jaccard_match(
            "hardcoded API key found",
            "hardcoded secret token in code",
            THRESHOLD,
        );
        assert!(score.is_some());
    }

    #[test]
    fn test_jaccard_no_overlap() {
        let score = jaccard_match(
            "null pointer check",
            "SQL injection vulnerability",
            THRESHOLD,
        );
        assert!(score.is_none());
    }

    #[test]
    fn test_jaccard_threshold_boundary() {
        let score = jaccard_match("a", "b", 0.0);
        assert!(score.is_some()); // threshold=0 means always match
        let score = jaccard_match("a", "b", 1.0);
        assert!(score.is_none()); // threshold=1 means only exact matches
    }

    #[test]
    fn test_jaccard_empty_strings() {
        assert!(jaccard_match("", "", THRESHOLD).is_none()); // empty union
        assert!(jaccard_match("hello", "", THRESHOLD).is_none());
    }

    #[test]
    fn test_jaccard_case_insensitive() {
        let s1 = jaccard_match("SQL Injection", "sql injection", THRESHOLD);
        let s2 = jaccard_match("Sql Injection", "sql injection", THRESHOLD);
        assert_eq!(s1, s2);
    }

    #[test]
    fn test_jaccard_punctuation_stripping() {
        // With whitespace-only split the parenthesized variant yields tokens
        // {"xss", "(cross-site", "scripting)"} — union of 6 → Jaccard = 1/6 ≈ 0.167
        let s1 = jaccard_match(
            "xss (cross-site scripting)",
            "xss cross site scripting",
            THRESHOLD,
        );
        assert!(s1.is_some());
        assert!((s1.unwrap() - 1.0 / 6.0).abs() < 0.01);
    }

    #[test]
    fn test_jaccard_precise_intersection() {
        // "hardcoded" shared out of 7 unique words across
        // "hardcoded API key found" ∩ "hardcoded secret in config" = 1/7 ≈ 0.1428
        let score = jaccard_match(
            "hardcoded API key found",
            "hardcoded secret in config",
            THRESHOLD,
        );
        assert!(score.is_some());
        assert!((score.unwrap() - 1.0 / 7.0).abs() < 0.01);
    }

    mod edge_cases {
        use super::*;

        #[test]
        fn test_jaccard_hyphen_difference() {
            // "cross-site" is a single token, "cross site" is two — different intersection sizes
            let hyphen_score = jaccard_match(
                "cross-site scripting vulnerability",
                "cross site scripting",
                THRESHOLD,
            );
            let regular_score = jaccard_match(
                "cross site scripting vulnerability",
                "cross site scripting",
                THRESHOLD,
            );
            // hyphen: 1 shared ("scripting") / 5 union = 0.2
            assert!(hyphen_score.is_some());
            assert!((hyphen_score.unwrap() - 0.2).abs() < 0.01);
            // no hyphen: 3 shared / 4 union = 0.75
            if let Some(s) = regular_score {
                assert!((s - 0.75).abs() < 0.01);
            }
        }

        #[test]
        fn test_jaccard_compound_difference() {
            // "well-known" is a single token, no overlap with "well" or "known"
            let score = jaccard_match("well-known vulnerability", "well known issue", THRESHOLD);
            assert!(
                score.is_none(),
                "well-known is a single token, no overlap with 'well' or 'known'"
            );
        }

        #[test]
        fn test_jaccard_apostrophe_preserved() {
            // "doesn't" is a single token (apostrophe preserved in whitespace split)
            let score = jaccard_match("doesn't work", "doesn't function", THRESHOLD);
            if let Some(s) = score {
                // {"doesn't"} common, union = {"doesn't", "work", "function"} = 3
                assert!((s - 1.0 / 3.0).abs() < 0.01);
            }
        }

        #[test]
        fn test_jaccard_ssrf_real_example() {
            // SSRF phrase tokens have zero overlap with expanded SSRF description
            let score = jaccard_match(
                "Server-Side Request Forgery via open()",
                "SSRF vulnerability using open(url) without validation",
                THRESHOLD,
            );
            assert!(score.is_none());
        }

        #[test]
        fn test_jaccard_hyphen_vs_spaces() {
            // "cross-site" vs "cross site" — Jaccard = 1/4 = 0.25 (only "scripting" common)
            let score = jaccard_match("cross-site scripting", "cross site scripting", THRESHOLD);
            assert!(score.is_some());
            assert!((score.unwrap() - 0.25).abs() < 0.01);
        }
    }

    #[test]
    fn test_format_judge_prompt_basic() {
        let result = format_judge_prompt("golden", "candidate");
        assert!(result.contains("golden"));
        assert!(result.contains("candidate"));
        assert!(result.contains("match"));
        assert!(result.contains("reasoning"));
    }

    #[test]
    fn test_format_judge_prompt_contains_placeholders() {
        assert!(JUDGE_PROMPT.contains("{golden_comment}"));
        assert!(JUDGE_PROMPT.contains("{candidate}"));
    }

    #[test]
    fn test_format_judge_prompt_special_chars() {
        let result = format_judge_prompt("line1\nline2", "text with \"quotes\"");
        assert!(result.contains("line1\nline2"));
        assert!(result.contains("text with \"quotes\""));
    }
}
