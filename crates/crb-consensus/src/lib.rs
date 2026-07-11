//! Multi-agent consensus orchestration for code review evaluation.
//!
//! Orchestrates multiple LLM reviewer agents concurrently,
//! then aggregates their structured findings via heuristic matching and LLM
//! judge fallback against golden comments.

pub mod adaptive;
pub mod agent;
pub mod execution;
pub mod harness;
pub mod judge;
pub mod pipeline;

use std::sync::LazyLock;

pub use crb_shared::cache::CacheBackend;
use crb_shared::finding::Finding;
use regex::Regex;
use rig_core::completion::{AssistantContent, Message, Usage};
use serde::{Deserialize, Serialize};

/// Regex to extract JSON from markdown code blocks.
#[allow(clippy::unwrap_used)]
static RE_CODEBLOCK_JSON: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)```(?:json)?\s*\n(.*?)\n\s*```").unwrap());

/// Regex to find any JSON array in a response.
#[allow(clippy::unwrap_used)]
static RE_JSON_ARRAY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[[\s\S]*\]").unwrap());

/// The role of a reviewer agent.
///
/// This is a dynamic newtype around a string abbreviation.
/// Valid values are loaded at runtime from the agent manifest (`prompts/agents/*.md`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize, Serialize)]
pub struct Role(pub String);

impl Role {
    /// Convert to the string identifier used by `crb_agents::build_agent`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for Role {
    fn from(s: &str) -> Self {
        Role(s.to_uppercase())
    }
}

impl From<String> for Role {
    fn from(s: String) -> Self {
        Role(s.to_uppercase())
    }
}

/// Configuration for a single reviewer agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewerConfig {
    /// The reviewer role (SA, CL, AR, SEC, etc.).
    pub role: Role,

    /// Model identifier for this reviewer (e.g. "deepseek/deepseek-v4-flash").
    pub model: String,

    /// Maximum number of findings this agent should produce.
    pub max_findings: usize,
}

/// A golden (expected) comment against which findings are judged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenComment {
    /// Source file path where the golden comment applies.
    pub file: String,

    /// Line number in the source file.
    pub line: u32,

    /// Regex pattern matched against `Finding::message`.
    pub message_regex: String,

    /// Expected severity for this golden comment.
    pub severity: String,

    /// Which role(s) should catch this: "SA", "CL", "AR", "SEC", or "any".
    pub source: String,
}

impl GoldenComment {
    /// Check whether a candidate finding matches this golden comment's
    /// file and line (exact match on both).
    pub fn matches_candidate(&self, f: &Finding) -> bool {
        f.file.as_deref() == Some(&self.file) && f.line == Some(self.line)
    }
}

/// Result of matching a golden comment against candidate findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchResult {
    /// A candidate finding matches the golden comment.
    TruePositive,
    /// A candidate finding has no matching golden comment.
    FalsePositive,
    /// A golden comment has no matching candidate finding.
    FalseNegative,
}

/// Output of a full consensus run.
#[derive(Debug, Clone, Serialize)]
pub struct ConsensusReport {
    /// Findings from each agent, grouped by role.
    pub agents: Vec<(Role, Vec<Finding>)>,
    /// Goldens that were matched by at least one finding.
    pub true_positives: Vec<(GoldenComment, Finding)>,
    /// Findings that matched no golden.
    pub false_positives: Vec<Finding>,
    /// Goldens that matched no finding.
    pub false_negatives: Vec<GoldenComment>,
    /// TP / (TP + FP)
    pub precision: f64,
    /// TP / (TP + FN)
    pub recall: f64,
    /// F1 = harmonic mean of precision and recall
    pub f1: f64,
    /// Number of agent LLM calls that were cache misses (actual API calls made).
    pub agent_api_calls: usize,
    /// Number of judge LLM calls that were cache misses (actual API calls made).
    pub judge_api_calls: usize,
    /// Number of judge LLM calls that were cache hits (served from cache).
    pub judge_cache_hits: usize,
    /// Aggregate token usage from all agent API calls (real + cached).
    pub agent_usage: Usage,
    /// Aggregate token usage from all judge API calls (real + cached).
    pub judge_usage: Usage,
}

// ── Shared utility functions ──────────────────────────────────────────────────

/// Attempt to parse findings from an agent response using a 3-strategy
/// fallback:
///
/// 1. Direct JSON parse of the full response
/// 2. Extract JSON from markdown code blocks via [`RE_CODEBLOCK_JSON`]
/// 3. Find any JSON array via [`RE_JSON_ARRAY`]
///
/// If `context` is non-empty, a warning is logged with that context on
/// failure (e.g. `"CACHED"`, `""` for silent failure).
pub fn parse_findings_from_response(response: &str, role: &Role, context: &str) -> Vec<Finding> {
    serde_json::from_str(response).unwrap_or_else(|_| {
        if let Some(caps) = RE_CODEBLOCK_JSON.captures(response) {
            #[allow(clippy::unwrap_used)]
            let inner = caps.get(1).unwrap().as_str().trim();
            if let Ok(f) = serde_json::from_str::<Vec<Finding>>(inner) {
                return f;
            }
        }
        if let Some(m) = RE_JSON_ARRAY.find(response) {
            if let Ok(f) = serde_json::from_str::<Vec<Finding>>(m.as_str()) {
                return f;
            }
        }
        if !context.is_empty() {
            tracing::warn!(
                "Failed to parse {} findings for role {:?}. Response (truncated): {}",
                context,
                role,
                &response[..std::cmp::min(200, response.len())],
            );
        }
        Vec::new()
    })
}

/// Try to extract the last assistant text message from a chat history.
///
/// When [`PromptError::MaxTurnsError`] fires, the agent's accumulated
/// conversation is available in `chat_history`.  This function walks it in
/// reverse to find the most recent `Message::Assistant` whose content includes
/// an `AssistantContent::Text` variant - that text is often a partial or
/// complete JSON findings array the model produced before being cut off.
pub fn extract_last_assistant_text(history: &[Message]) -> Option<String> {
    for msg in history.iter().rev() {
        if let Message::Assistant { content, .. } = msg {
            for item in content.iter() {
                if let AssistantContent::Text(text) = item {
                    let t = text.text.trim().to_string();
                    if !t.is_empty() {
                        return Some(t);
                    }
                }
            }
        }
    }
    None
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crb_shared::cache::sha256_hex;
    use crb_shared::cache::{compute_agent_cache_key, compute_judge_cache_key};

    /// Build a minimal single-hunk diff for the given file path and content.
    /// Content should include the `-` and `+` prefix lines (e.g. "-old\n+new\n").
    fn minimal_diff(file_path: &str, content: &str) -> String {
        format!(
            "\
diff --git a/{fp} b/{fp}
--- a/{fp}
+++ b/{fp}
@@ -1 +1 @@
{content}",
            fp = file_path,
            content = content
        )
    }

    /// Shorthand for `minimal_diff("src/main.rs", content)`.
    fn diff_main(content: &str) -> String {
        minimal_diff("src/main.rs", content)
    }

    /// Assert that the precision, recall, and F1 metrics of a report
    /// are all equal to the given expected value (within 1e-6).
    fn assert_metrics(report: &ConsensusReport, expected: f64) {
        let eps = 1e-6;
        assert!((report.precision - expected).abs() < eps);
        assert!((report.recall - expected).abs() < eps);
        assert!((report.f1 - expected).abs() < eps);
    }

    #[test]
    fn test_role_as_str() {
        assert_eq!(Role("SA".into()).as_str(), "SA");
        assert_eq!(Role("CL".into()).as_str(), "CL");
        assert_eq!(Role("AR".into()).as_str(), "AR");
        assert_eq!(Role("SEC".into()).as_str(), "SEC");
    }

    #[test]
    fn test_role_variants_are_distinct() {
        assert_ne!(Role("SA".into()), Role("CL".into()));
        assert_ne!(Role("CL".into()), Role("AR".into()));
        assert_ne!(Role("AR".into()), Role("SEC".into()));
    }

    #[test]
    fn test_match_result_serialization() {
        let tp = serde_json::to_value(MatchResult::TruePositive).unwrap();
        let fp = serde_json::to_value(MatchResult::FalsePositive).unwrap();
        let fn_ = serde_json::to_value(MatchResult::FalseNegative).unwrap();
        assert!(tp.is_string());
        assert!(fp.is_string());
        assert!(fn_.is_string());
        assert_ne!(tp, fp);
        assert_ne!(fp, fn_);
    }

    #[test]
    fn test_compute_agent_cache_key_deterministic() {
        let key1 = compute_agent_cache_key("abc", "def", "gpt-4o", "SA", "rules123");
        let key2 = compute_agent_cache_key("abc", "def", "gpt-4o", "SA", "rules123");
        assert_eq!(key1, key2);
        // Different input should produce different key
        let key3 = compute_agent_cache_key("abc", "xyz", "gpt-4o", "SA", "rules123");
        assert_ne!(key1, key3);
    }

    #[test]
    fn test_compute_judge_cache_key_deterministic() {
        let key1 = compute_judge_cache_key("jph", "finding msg", "golden comment", "gpt-4o-mini");
        let key2 = compute_judge_cache_key("jph", "finding msg", "golden comment", "gpt-4o-mini");
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_sha256_hex() {
        let h = sha256_hex("hello");
        assert_eq!(h.len(), 64); // SHA256 hex is 64 chars
    }

    // ── judge_comment tests ─────────────────────────────────────────

    #[test]
    fn test_judge_comment_no_candidates() {
        // Empty candidates → no file+line match → FalseNegative
        let golden = GoldenComment {
            file: "src/main.rs".into(),
            line: 42,
            message_regex: r".*".into(),
            severity: "error".into(),
            source: "SA".into(),
        };
        // We can't call judge_comment directly in unit tests because it requires
        // a real LLM agent.  Instead we test that empty candidates produce FN.
        // The file+line pre-filter returns empty → FalseNegative.
        let candidates: Vec<Finding> = vec![];
        let file_matches: Vec<&Finding> = candidates
            .iter()
            .filter(|f| golden.matches_candidate(f))
            .collect();
        assert!(file_matches.is_empty());
    }

    #[test]
    fn test_consensus_report_empty() {
        // No goldens, no findings -> perfect metrics
        let report = ConsensusReport {
            agents: vec![],
            true_positives: vec![],
            false_positives: vec![],
            false_negatives: vec![],
            precision: 1.0,
            recall: 1.0,
            f1: 1.0,
            agent_api_calls: 0,
            judge_api_calls: 0,
            judge_cache_hits: 0,
            agent_usage: Usage::new(),
            judge_usage: Usage::new(),
        };
        assert_metrics(&report, 1.0);
    }

    #[test]
    fn test_consensus_report_perfect() {
        // All findings match all goldens
        let report = ConsensusReport {
            agents: vec![],
            true_positives: vec![(
                GoldenComment {
                    file: "a.rs".into(),
                    line: 1,
                    message_regex: "foo".into(),
                    severity: "error".into(),
                    source: "any".into(),
                },
                Finding {
                    file: Some("a.rs".into()),
                    line: Some(1),
                    message: "foo".into(),
                    severity: "error".into(),
                    ..Default::default()
                },
            )],
            false_positives: vec![],
            false_negatives: vec![],
            precision: 1.0,
            recall: 1.0,
            f1: 1.0,
            agent_api_calls: 0,
            judge_api_calls: 0,
            judge_cache_hits: 0,
            agent_usage: Usage::new(),
            judge_usage: Usage::new(),
        };
        assert_eq!(report.true_positives.len(), 1);
        assert_eq!(report.false_positives.len(), 0);
        assert_eq!(report.false_negatives.len(), 0);
        assert_metrics(&report, 1.0);
    }

    #[test]
    fn test_consensus_report_no_matches() {
        let report = ConsensusReport {
            agents: vec![],
            true_positives: vec![],
            false_positives: vec![Finding {
                file: Some("a.rs".into()),
                line: Some(1),
                message: "unexpected".into(),
                severity: "warning".into(),
                severity_audited: false,
                ..Default::default()
            }],
            false_negatives: vec![GoldenComment {
                file: "a.rs".into(),
                line: 1,
                message_regex: "expected".into(),
                severity: "error".into(),
                source: "any".into(),
            }],
            precision: 0.0,
            recall: 0.0,
            f1: 0.0,
            agent_api_calls: 0,
            judge_api_calls: 0,
            judge_cache_hits: 0,
            agent_usage: Usage::new(),
            judge_usage: Usage::new(),
        };
        assert_eq!(report.true_positives.len(), 0);
        assert_eq!(report.false_positives.len(), 1);
        assert_eq!(report.false_negatives.len(), 1);
        assert_metrics(&report, 0.0);
    }

    #[test]
    fn test_reviewer_config_serialization() {
        let config = ReviewerConfig {
            role: Role("SEC".into()),
            model: "gpt-4o".into(),
            max_findings: 15,
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("SEC"));
        assert!(json.contains("gpt-4o"));
        assert!(json.contains("15"));
        let deserialized: ReviewerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.role, Role("SEC".into()));
        assert_eq!(deserialized.model, "gpt-4o");
        assert_eq!(deserialized.max_findings, 15);
    }

    #[test]
    fn test_golden_comment_serialization() {
        let gc = GoldenComment {
            file: "src/lib.rs".into(),
            line: 100,
            message_regex: r"unsafe\s+fn".into(),
            severity: "warning".into(),
            source: "SEC".into(),
        };
        let json = serde_json::to_string(&gc).unwrap();
        let deserialized: GoldenComment = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.file, "src/lib.rs");
        assert_eq!(deserialized.line, 100);
    }

    #[test]
    fn test_count_diff_files_empty() {
        assert_eq!(count_diff_files(""), 0);
    }

    #[test]
    fn test_count_diff_files_single() {
        let diff = "\
diff --git a/src/main.rs b/src/main.rs
index abc..def 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,4 @@
 fn main() {
-    println!(\"hello\");
+    println!(\"hello world\");
 }
";
        assert_eq!(count_diff_files(diff), 1);
    }

    #[test]
    fn test_count_diff_files_multiple() {
        let diff = "\
diff --git a/src/main.rs b/src/main.rs
index a..b
--- a/src/main.rs
+++ b/src/main.rs
@@ -1 +1 @@
-foo
+bar
diff --git a/src/lib.rs b/src/lib.rs
index c..d
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1 +1 @@
-baz
+qux
diff --git a/Cargo.toml b/Cargo.toml
index e..f
--- a/Cargo.toml
+++ b/Cargo.toml
@@ -1 +1 @@
-old
+new
";
        assert_eq!(count_diff_files(diff), 3);
    }

    #[test]
    fn test_count_diff_lines_empty() {
        assert_eq!(count_diff_lines(""), 0);
    }

    #[test]
    fn test_count_diff_lines_counts_additions_and_deletions() {
        let diff = "\
diff --git a/src/main.rs b/src/main.rs
index abc..def 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,5 +1,6 @@
 fn main() {
-    let x = 1;
-    let y = 2;
+    let x = 10;
+    let y = 20;
+    let z = 30;
     println!(\"done\");
 }
";
        assert_eq!(count_diff_lines(diff), 5);
    }

    #[test]
    fn test_count_diff_lines_excludes_headers() {
        let diff = diff_main("-foo\n+bar\n");
        assert_eq!(count_diff_lines(&diff), 2);
    }

    #[test]
    fn test_diff_touches_full_panel_languages_no_match() {
        let diff = "\
diff --git a/src/main.py b/src/main.py
diff --git a/README.md b/README.md
";
        assert!(!diff_touches_full_panel_languages(diff));
    }

    #[test]
    fn test_diff_touches_full_panel_languages_rust() {
        let diff = minimal_diff("src/main.rs", "-old\n+new\n");
        assert!(diff_touches_full_panel_languages(&diff));
    }

    #[test]
    fn test_diff_touches_full_panel_languages_typescript() {
        let diff = minimal_diff("src/foo.ts", "-old\n+new\n");
        assert!(diff_touches_full_panel_languages(&diff));
    }

    #[test]
    fn test_diff_touches_full_panel_languages_go() {
        let diff = minimal_diff("server.go", "-old\n+new\n");
        assert!(diff_touches_full_panel_languages(&diff));
    }

    #[test]
    fn test_diff_touches_full_panel_languages_java() {
        let diff = minimal_diff("Main.java", "-old\n+new\n");
        assert!(diff_touches_full_panel_languages(&diff));
    }

    #[test]
    fn test_diff_touches_full_panel_languages_cpp() {
        let diff = minimal_diff("main.cpp", "-old\n+new\n");
        assert!(diff_touches_full_panel_languages(&diff));
    }

    #[test]
    fn test_should_use_single_agent_small_pr() {
        let diff = minimal_diff("README.md", "-old\n+new\n");
        assert!(should_use_single_agent(&diff, 3, 200));
    }

    #[test]
    fn test_should_use_single_agent_too_many_files() {
        let file_count = 4;
        let diff = (0..file_count)
            .map(|i| {
                let fname = format!("a{}.txt", i);
                minimal_diff(&fname, "-old\n+new\n")
            })
            .collect::<Vec<_>>()
            .join("");
        assert!(!should_use_single_agent(&diff, 3, 200));
    }

    #[test]
    fn test_should_use_single_agent_too_many_lines() {
        let diff = "\
diff --git a/a.txt b/a.txt
--- a/a.txt
+++ b/a.txt
@@ -1,100 +1,300 @@
"
        .to_string()
            + &(0..250)
                .map(|i| format!("+line_{}\n", i))
                .collect::<String>();
        assert!(!should_use_single_agent(&diff, 3, 200));
    }

    #[test]
    fn test_should_use_single_agent_safety_override_rust() {
        let diff = diff_main("-old\n+new\n");
        assert!(!should_use_single_agent(&diff, 3, 200));
    }

    #[test]
    fn test_should_use_single_agent_safety_override_go() {
        let diff = minimal_diff("server.go", "-old\n+new\n");
        assert!(!should_use_single_agent(&diff, 3, 200));
    }

    #[test]
    fn test_role_gen_variant() {
        let role = Role("GEN".into());
        assert_eq!(role.as_str(), "GEN");
    }

    #[test]
    fn test_role_gen_serialization() {
        let json = serde_json::to_string(&Role("GEN".into())).unwrap();
        assert_eq!(json, "\"GEN\"");
        let deserialized: Role = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, Role("GEN".into()));
    }
}
