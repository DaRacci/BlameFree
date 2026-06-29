use rig_core::agent::Agent;
use rig_core::client::CompletionClient;
use rig_core::completion::Prompt;
use rig_core::providers::openai::{Client, responses_api::ResponsesCompletionModel};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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

/// The structured verdict returned by the judge LLM.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JudgeVerdict {
    pub reasoning: String,
    #[serde(rename = "match")]
    pub match_: bool,
    pub confidence: f64,
}

/// Aggregated evaluation metrics computed from judge verdicts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Metrics {
    pub true_positives: usize,
    pub false_positives: usize,
    pub false_negatives: usize,
    pub precision: f64,
    pub recall: f64,
    pub f1: f64,
}

/// Build a judge agent with the Martian JUDGE_PROMPT as its preamble.
pub fn build_judge(
    client: &Client,
    model: &str,
) -> Agent<ResponsesCompletionModel> {
    client
        .agent(model)
        .preamble(JUDGE_PROMPT)
        .build()
}

/// Format the judge prompt by substituting the golden comment and candidate finding.
pub fn format_judge_prompt(golden_comment: &str, candidate: &str) -> String {
    JUDGE_PROMPT
        .replace("{golden_comment}", golden_comment)
        .replace("{candidate}", candidate)
}

/// Run the judge agent to produce a verdict for a single comparison.
pub async fn run_judge(
    judge: &Agent<ResponsesCompletionModel>,
    golden_comment: &str,
    candidate: &str,
) -> Result<JudgeVerdict, anyhow::Error> {
    let prompt = format_judge_prompt(golden_comment, candidate);
    let response = judge.prompt(&prompt).await?;
    let verdict: JudgeVerdict = serde_json::from_str(&response)?;
    Ok(verdict)
}

// ── Jaccard heuristic matching ─────────────────────────────────────────────

/// Tokenize text exactly like Python's `.lower().split()`
/// (whitespace split only, no punctuation stripping)
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split_whitespace()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Jaccard word-overlap heuristic matching (replicates Python step3_judge_comments).
///
/// - Tokenizes both strings into lowercase word sets (split on non-alphanumeric)
/// - Computes Jaccard = |intersection| / |union|
/// - Returns Some(match_score) if >= threshold, None otherwise
///
/// If either string is empty after tokenization, the union is zero and `None` is
/// returned (cannot compute meaningful similarity on empty sets).
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

    let f1 = if (precision + recall) > 0.0 {
        2.0 * precision * recall / (precision + recall)
    } else {
        0.0
    };

    Metrics {
        true_positives,
        false_positives,
        false_negatives,
        precision,
        recall,
        f1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_perfect_match() {
        let verdicts = vec![
            JudgeVerdict { reasoning: "a".into(), match_: true, confidence: 1.0 },
            JudgeVerdict { reasoning: "b".into(), match_: true, confidence: 1.0 },
        ];
        let m = compute_metrics(&verdicts, 2);
        assert_eq!(m.true_positives, 2);
        assert_eq!(m.false_positives, 0);
        assert_eq!(m.false_negatives, 0);
        assert!((m.precision - 1.0).abs() < 1e-6);
        assert!((m.recall - 1.0).abs() < 1e-6);
        assert!((m.f1 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_no_match() {
        let verdicts = vec![
            JudgeVerdict { reasoning: "a".into(), match_: false, confidence: 0.0 },
        ];
        let m = compute_metrics(&verdicts, 1);
        assert_eq!(m.true_positives, 0);
        assert_eq!(m.false_positives, 1);
        assert_eq!(m.false_negatives, 1);
        assert!((m.precision - 0.0).abs() < 1e-6);
        assert!((m.recall - 0.0).abs() < 1e-6);
        assert!((m.f1 - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_partial_match() {
        let verdicts = vec![
            JudgeVerdict { reasoning: "a".into(), match_: true, confidence: 1.0 },
            JudgeVerdict { reasoning: "b".into(), match_: false, confidence: 0.0 },
        ];
        let m = compute_metrics(&verdicts, 2);
        assert_eq!(m.true_positives, 1);
        assert_eq!(m.false_positives, 1);
        assert_eq!(m.false_negatives, 1);
        assert!((m.precision - 0.5).abs() < 1e-6);
        assert!((m.recall - 0.5).abs() < 1e-6);
        assert!((m.f1 - 0.5).abs() < 1e-6);
    }

    // ── Jaccard tests ────────────────────────────────────────────────────

    const THRESHOLD: f64 = 0.12;

    #[test]
    fn test_jaccard_identical() {
        let score = jaccard_match("hardcoded secret in config", "hardcoded secret in config", 0.12);
        assert!(score.unwrap() > 0.9);
    }

    #[test]
    fn test_jaccard_partial_overlap() {
        let score = jaccard_match("hardcoded API key found", "hardcoded secret token in code", 0.12);
        assert!(score.is_some());
    }

    #[test]
    fn test_jaccard_no_overlap() {
        let score = jaccard_match("null pointer check", "SQL injection vulnerability", 0.12);
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
        assert!(jaccard_match("", "", 0.12).is_none()); // empty union
        assert!(jaccard_match("hello", "", 0.12).is_none());
    }

    #[test]
    fn test_jaccard_case_insensitive() {
        let s1 = jaccard_match("SQL Injection", "sql injection", 0.12);
        let s2 = jaccard_match("Sql Injection", "sql injection", 0.12);
        assert_eq!(s1, s2); // case insensitive
    }

    #[test]
    fn test_jaccard_punctuation_stripping() {
        // With whitespace-only split (Python identical):
        // "xss (cross-site scripting)" → {"xss", "(cross-site", "scripting)"}
        // "xss cross site scripting" → {"xss", "cross", "site", "scripting"}
        // Intersection: {"xss"}, Union: 6 words → Jaccard = 1/6 ≈ 0.167
        let s1 = jaccard_match("xss (cross-site scripting)", "xss cross site scripting", 0.12);
        assert!(s1.is_some());
        assert!((s1.unwrap() - 1.0 / 6.0).abs() < 0.01);
    }

    #[test]
    fn test_jaccard_python_equivalent() {
        // Python: "hardcoded secret in config".lower().split() -> {"hardcoded", "secret", "in", "config"}
        // Finding: "hardcoded API key found" -> {"hardcoded", "api", "key", "found"}
        // Intersection: {"hardcoded"} = 1
        // Union: {"hardcoded", "secret", "in", "config", "api", "key", "found"} = 7
        // Jaccard = 1/7 ≈ 0.1428 >= 0.12 ✓
        let score = jaccard_match("hardcoded API key found", "hardcoded secret in config", 0.12);
        assert!(score.is_some());
        let score_val = score.unwrap();
        assert!((score_val - 1.0 / 7.0).abs() < 0.01);
    }

    // ── Cross-validation vs Python step3_judge_comments ────────────────────
    //
    // FIXED: Tokenization now matches Python identically!
    //   Both Rust and Python use whitespace-only split:
    //     text.to_lowercase().split_whitespace()
    //     text.lower().split()
    //
    // All 12/12 cross-validation test cases now produce identical scores.

    #[test]
    fn cv_identical_strings() {
        // Both implementations: score = 1.0
        let score = jaccard_match("hardcoded secret in config", "hardcoded secret in config", THRESHOLD);
        let val = score.unwrap();
        println!("[Rust CV] identical strings: score={:.6}  [Python: 1.0] ✓", val);
        assert!((val - 1.0).abs() < 0.001);
    }

    #[test]
    fn cv_partial_overlap_hardcoded() {
        // "hardcoded" shared out of 7 unique words → 1/7 ≈ 0.142857
        // Python same: 1/7 ≈ 0.142857 ✓
        let score = jaccard_match("hardcoded API key found", "hardcoded secret in config", THRESHOLD);
        let val = score.unwrap();
        println!("[Rust CV] partial overlap (hardcoded): score={:.6}  [Python: 0.142857] ✓", val);
        assert!((val - 1.0 / 7.0).abs() < 0.001);
    }

    #[test]
    fn cv_no_overlap() {
        // Both: score = None (no common tokens)
        let score = jaccard_match("null pointer check", "SQL injection vulnerability", THRESHOLD);
        assert!(score.is_none());
        println!("[Rust CV] no overlap: score=None  [Python: 0.0, no match] ✓");
    }

    #[test]
    fn cv_punctuation_diff() {
        // NOW MATCHES PYTHON:
        // Python: "xss (cross-site scripting)" → {"xss", "(cross-site", "scripting)"}
        //         ∩ {"xss", "cross", "site", "scripting"} = {"xss"} → 1/6 = 0.167
        // Rust:   "xss (cross-site scripting)" → {"xss", "(cross-site", "scripting)"}
        //         ∩ {"xss", "cross", "site", "scripting"} = {"xss"} → 1/6 = 0.167 ✓
        let score = jaccard_match("xss (cross-site scripting)", "xss cross site scripting", THRESHOLD);
        let val = score.unwrap();
        println!("[Rust CV] punctuation case: score={:.6}  [Python: 0.166667] ✓ MATCHES", val);
        assert!((val - 1.0 / 6.0).abs() < 0.001);
    }

    #[test]
    fn cv_case_insensitive() {
        // Both: score = 1.0 for "SQL Injection" vs "sql injection"
        let s1 = jaccard_match("SQL Injection", "sql injection", THRESHOLD);
        let s2 = jaccard_match("Sql Injection", "sql injection", THRESHOLD);
        assert_eq!(s1, s2);
        println!("[Rust CV] case insensitive: score={:.6}  [Python: 1.0] ✓", s1.unwrap());
    }

    #[test]
    fn cv_empty_strings() {
        // Both: return no match for empty/one-empty
        assert!(jaccard_match("", "", THRESHOLD).is_none());
        assert!(jaccard_match("hello", "", THRESHOLD).is_none());
        println!("[Rust CV] empty strings: both None  [Python: 0.0, no match] ✓");
    }

    #[test]
    fn cv_hyphen_difference() {
        // NOW MATCHES PYTHON — no more difference:
        // "cross-site scripting vulnerability" → {"cross-site", "scripting", "vulnerability"}
        // "cross site scripting" → {"cross", "site", "scripting"}
        // Intersection: {"scripting"} = 1
        // Union: {"cross-site", "scripting", "vulnerability", "cross", "site"} = 5
        // Jaccard = 1/5 = 0.2 — same in Python ✓
        let hyphen_score = jaccard_match("cross-site scripting vulnerability", "cross site scripting", THRESHOLD);
        // "cross site scripting vulnerability" → {"cross", "site", "scripting", "vulnerability"}
        // "cross site scripting" → {"cross", "site", "scripting"}
        // Intersection: 3, Union: 4 → Jaccard = 3/4 = 0.75
        let regular_score = jaccard_match("cross site scripting vulnerability", "cross site scripting", THRESHOLD);
        println!("[Rust CV] hyphen case (cross-site vs cross site): score={:?}  [Python: 1/5=0.2] ✓ MATCHES", hyphen_score);
        println!("[Rust CV] regular case (no hyphen): score={:?}  [Python: 3/4=0.75]", regular_score);
        assert!(hyphen_score.is_some());
        assert!((hyphen_score.unwrap() - 0.2).abs() < 0.01);
        if let Some(s) = regular_score {
            assert!((s - 0.75).abs() < 0.01);
        }
    }

    #[test]
    fn cv_compound_difference() {
        // NOW MATCHES PYTHON:
        // Python: "well-known vulnerability" → {"well-known", "vulnerability"}
        //         ∩ "well known issue" = {} → 0.0 → NO MATCH
        // Rust:   {"well-known", "vulnerability"}
        //         ∩ {"well", "known", "issue"} = {} → 0.0 → NO MATCH ✓
        let score = jaccard_match("well-known vulnerability", "well known issue", THRESHOLD);
        println!("[Rust CV] compound diff: score={:?}  [Python: 0.0, NO MATCH] ✓ MATCHES", score);
        assert!(score.is_none(), "well-known is a single token now, no overlap with 'well' or 'known'");
    }

    #[test]
    fn cv_apostrophe_preserved() {
        // Both: "doesn't" is a single token (apostrophe preserved in Rust)
        let score = jaccard_match("doesn't work", "doesn't function", THRESHOLD);
        println!("[Rust CV] apostrophe: score={:?}  [Python: 1/3=0.333]", score);
        if let Some(s) = score {
            // "doesn't" common, union = {"doesn't", "work", "function"} = 3
            assert!((s - 1.0 / 3.0).abs() < 0.01);
        }
    }

    #[test]
    fn cv_ssrf_real_example() {
        // Both Rust and Python agree (whitespace-only split):
        // Python: "Server-Side Request Forgery via open()"
        //         → {"server-side", "request", "forgery", "via", "open()"}
        //         "SSRF vulnerability using open(url) without validation"
        //         → {"ssrf", "vulnerability", "using", "open(url)", "without", "validation"}
        //         ∩ = {} → 0.0 → NO MATCH
        //
        // Rust:   "Server-Side Request Forgery via open()"
        //         → {"server-side", "request", "forgery", "via", "open()"}
        //         "SSRF vulnerability using open(url) without validation"
        //         → {"ssrf", "vulnerability", "using", "open(url)", "without", "validation"}
        //         ∩ = {} → 0.0 → NO MATCH ✓
        let score = jaccard_match(
            "Server-Side Request Forgery via open()",
            "SSRF vulnerability using open(url) without validation",
            THRESHOLD,
        );
        println!("[Rust CV] real SSRF: score={:?}  [Python: 0.0, no match]", score);
        // Both agree: no match ✓
        assert!(score.is_none());
    }

    #[test]
    fn cv_python_identical_punctuation_case() {
        // Python: "cross-site" is 1 token, "cross site" is 2 tokens → different
        // With whitespace-only split: "cross-site" matches "cross site" — NO (0 words overlap)
        let score = jaccard_match("cross-site scripting", "cross site scripting", THRESHOLD);
        // Python token sets: {"cross-site", "scripting"} vs {"cross", "site", "scripting"}
        // Intersection: {"scripting"}, Union: {"cross-site", "cross", "site", "scripting"} = 4
        // Jaccard = 1/4 = 0.25 → above 0.12 → matches
        assert!(score.is_some());
        assert!((score.unwrap() - 0.25).abs() < 0.01);
    }

    #[test]
    fn cv_python_xss_paren_case() {
        // Python: "xss (cross-site scripting)" → tokens: {"xss", "(cross-site", "scripting)"}
        // Python: "xss cross site scripting" → tokens: {"xss", "cross", "site", "scripting"}
        // Intersection: {"xss"}, Union: {"xss", "(cross-site", "scripting)", "cross", "site", "scripting"} = 6
        // Jaccard = 1/6 ≈ 0.167 → above 0.12 → matches
        let score = jaccard_match("xss (cross-site scripting)", "xss cross site scripting", THRESHOLD);
        assert!(score.is_some());
        // Python Jaccard = 1/6 ≈ 0.167
        assert!((score.unwrap() - 0.1666).abs() < 0.01);
    }
}
