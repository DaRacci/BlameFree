//! Multi-agent consensus orchestration for code review evaluation.
//!
//! Orchestrates multiple LLM reviewer agents (SA, CL, AR, SEC) concurrently,
//! then aggregates their structured findings via heuristic matching and LLM
//! judge fallback against golden (expected) comments.
//!
//! Provides a [`evaluate_pr_with_consensus`] convenience function that matches
//! the existing `evaluate_pr()` signature in `crb-harness` for drop-in use.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use regex::Regex;
use rig_core::agent::Agent;
use rig_core::completion::Prompt;
use rig_core::providers::openai;
use rig_core::providers::openai::responses_api::ResponsesCompletionModel;
use serde::{Deserialize, Serialize};
use tokio::task::JoinSet;

use crb_agents::prompts::PromptLibrary;
use crb_agents::{build_agent, Finding};
use crb_judge::{run_judge, JudgeVerdict};
use crb_reporting::{GoldenCommentEntry, PrResult};

// ── Cache backend trait ──────────────────────────────────────────────────────

/// Interface for caching LLM interactions (prompts, responses, judge calls).
///
/// This is a trait so that the harness can inject its own cache implementation
/// without creating a circular dependency between `crb-consensus` and
/// `crb-harness`.
pub trait CacheBackend: Send + Sync {
    /// Save an agent prompt+response pair for the given role.
    fn save_agent(&self, role: &str, prompt: &str, response: &str);

    /// Append a judge call entry (golden comment, finding message, verdict JSON).
    fn save_judge(&self, golden: &str, finding: &str, verdict_json: &str);
}

// ── Types ────────────────────────────────────────────────────────────────────

/// The role of a reviewer agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    SA,
    CL,
    AR,
    SEC,
}

impl Role {
    /// Convert to the string identifier used by `crb_agents::build_agent`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::SA => "SA",
            Role::CL => "CL",
            Role::AR => "AR",
            Role::SEC => "SEC",
        }
    }
}

/// Configuration for a single reviewer agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewerConfig {
    pub role: Role,
    pub model: String,
    pub max_findings: usize,
}

/// A golden (expected) comment against which findings are judged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenComment {
    pub file: String,
    pub line: u32,
    /// Regex pattern matched against `Finding::message`.
    pub message_regex: String,
    pub severity: String,
    /// Which role(s) should catch this: "SA", "CL", "AR", "SEC", or "any".
    pub source: String,
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
    /// Harmonic mean of precision and recall.
    pub f1: f64,
}

// ── Agent construction ──────────────────────────────────────────────────────

/// Build a reviewer agent for the given role.
///
/// Delegates to [`crb_agents::build_agent`] with the role's string identifier
/// and an optional rules preamble.  The returned agent should be prompted with
/// the diff to produce structured findings (parsed via `serde_json`).
///
/// `prompt_lib` and `template_vars` are forwarded to [`crb_agents::build_agent`]
/// to support file-based prompt loading and template substitution.
pub fn build_reviewer_agent(
    client: &openai::Client,
    config: &ReviewerConfig,
    rules_preamble: Option<&str>,
    prompt_lib: Option<&PromptLibrary>,
    template_vars: Option<&HashMap<&str, &str>>,
) -> Agent<ResponsesCompletionModel> {
    build_agent(client, &config.model, config.role.as_str(), rules_preamble, prompt_lib, template_vars)
}

// ── Concurrent execution ────────────────────────────────────────────────────

/// Spawn all reviewer agents concurrently and collect their findings.
///
/// Each agent is run with a 120-second timeout.  Findings are capped at
/// `config.max_findings`.  Agents that time out or return errors yield an
/// empty finding list with a warning — no hard failure.
///
/// If `cache` is provided, the prompt (diff) and response for each agent
/// are saved via [`CacheBackend::save_agent`].
pub async fn run_reviewers(
    configs: Vec<ReviewerConfig>,
    diff: &str,
    client: &openai::Client,
    rules_preamble: Option<&str>,
    prompt_lib: Option<&PromptLibrary>,
    template_vars: Option<&HashMap<&str, &str>>,
    cache: Option<Arc<dyn CacheBackend>>,
) -> Vec<(Role, Vec<Finding>)> {
    let mut set = JoinSet::new();

    for config in configs {
        let client = client.clone();
        let diff = diff.to_string();
        let role = config.role;
        let max_findings = config.max_findings;
        let preamble = rules_preamble.map(String::from);
        let agent = build_reviewer_agent(&client, &config, preamble.as_deref(), prompt_lib, template_vars);
        let cache = cache.clone();

        set.spawn(async move {
            let outcome = tokio::time::timeout(Duration::from_secs(120), async {
                let response = agent.prompt(&diff).await?;

                // Cache the prompt+response if cache is active
                if let Some(ref cache) = cache {
                    cache.save_agent(role.as_str(), &diff, &response);
                }

                // Log raw response for debugging
                let preview_len = std::cmp::min(500, response.len());
                tracing::info!("Agent raw response (first 500 chars): {}", &response[..preview_len]);

                // Strategy 1: Try direct JSON array parse
                let mut findings: Vec<Finding> =
                    serde_json::from_str(&response).unwrap_or_else(|_| {
                        // Strategy 2: Extract JSON from markdown code blocks
                        let re = Regex::new(r"(?s)```(?:json)?\s*\n(.*?)\n\s*```").unwrap();
                        if let Some(caps) = re.captures(&response) {
                            let inner = caps.get(1).unwrap().as_str().trim();
                            if let Ok(f) = serde_json::from_str::<Vec<Finding>>(inner) {
                                return f;
                            }
                        }
                        // Strategy 3: Find any JSON array in the response
                        let array_re = Regex::new(r"\[[\s\S]*\]").unwrap();
                        if let Some(m) = array_re.find(&response) {
                            if let Ok(f) = serde_json::from_str::<Vec<Finding>>(m.as_str()) {
                                return f;
                            }
                        }
                        tracing::warn!(
                            "Failed to parse findings for role {:?}. Raw response (truncated): {}",
                            role,
                            &response[..std::cmp::min(200, response.len())],
                        );
                        Vec::new()
                    });
                if findings.len() > max_findings {
                    tracing::warn!(
                        "Role {:?} produced {} findings, capping at {}",
                        role,
                        findings.len(),
                        max_findings,
                    );
                    findings.truncate(max_findings);
                }
                Ok::<_, anyhow::Error>((role, findings))
            })
            .await;

            match outcome {
                Ok(Ok(pair)) => pair,
                Ok(Err(e)) => {
                    tracing::warn!("Role {:?} agent failed: {e}", role);
                    (role, Vec::new())
                }
                Err(_) => {
                    tracing::warn!("Role {:?} timed out after 120s", role);
                    (role, Vec::new())
                }
            }
        });
    }

    let mut results: Vec<(Role, Vec<Finding>)> = Vec::new();
    while let Some(res) = set.join_next().await {
        match res {
            Ok(pair) => results.push(pair),
            Err(e) => tracing::warn!("Agent join error: {e}"),
        }
    }
    results
}

// ── Heuristic judge ─────────────────────────────────────────────────────────

/// Heuristically match a single golden comment against a set of candidates.
///
/// **Algorithm (no LLM call):**
/// 1. Filter candidates by exact `file` and `line` match.
/// 2. Among filtered, check if any `finding.message` matches
///    `golden.message_regex` (regex match).
/// 3. Returns [`MatchResult::TruePositive`] if a match was found,
///    [`MatchResult::FalseNegative`] otherwise.
pub async fn judge_comment(
    golden: &GoldenComment,
    candidates: &[Finding],
) -> MatchResult {
    // Step 1: filter candidates by file + line
    let file_matches: Vec<&Finding> = candidates
        .iter()
        .filter(|f| f.file.as_deref() == Some(&golden.file) && f.line == Some(golden.line))
        .collect();

    if file_matches.is_empty() {
        return MatchResult::FalseNegative;
    }

    // Step 2: regex match on message
    let re = match Regex::new(&golden.message_regex) {
        Ok(re) => re,
        Err(e) => {
            tracing::warn!(
                "Invalid regex '{}' in golden comment: {}. Treating as no match.",
                golden.message_regex,
                e,
            );
            return MatchResult::FalseNegative;
        }
    };

    for finding in &file_matches {
        if re.is_match(&finding.message) {
            return MatchResult::TruePositive;
        }
    }

    MatchResult::FalseNegative
}

// ── Full pipeline ───────────────────────────────────────────────────────────

/// Run the full multi-agent consensus pipeline.
///
/// 1. Concurrently run all reviewer agents via [`run_reviewers`].
/// 2. For each golden comment, attempt heuristic matching ([`judge_comment`])
///    against all findings.
/// 3. Goldens that do not match heuristically fall back to the LLM judge.
/// 4. Remaining unmatched findings are classified as false positives.
/// 5. Compute precision / recall / F1 metrics.
///
/// If `cache` is provided, agent interactions and judge calls are saved.
pub async fn run_consensus(
    diff: &str,
    goldens: Vec<GoldenComment>,
    reviewer_configs: Vec<ReviewerConfig>,
    client: &openai::Client,
    judge: &Agent<ResponsesCompletionModel>,
    rules_preamble: Option<&str>,
    prompt_lib: Option<&PromptLibrary>,
    template_vars: Option<&HashMap<&str, &str>>,
    cache: Option<Arc<dyn CacheBackend>>,
) -> ConsensusReport {
    // Step 1: run all reviewers concurrently
    let agents = run_reviewers(reviewer_configs, diff, client, rules_preamble, prompt_lib, template_vars, cache.clone()).await;

    // Flatten all findings into a single mutable pool
    let mut unmatched: Vec<Finding> = agents
        .iter()
        .flat_map(|(_, findings)| findings.iter())
        .cloned()
        .collect();

    let mut true_positives: Vec<(GoldenComment, Finding)> = Vec::new();
    let mut false_negatives: Vec<GoldenComment> = Vec::new();

    // Step 2 & 3: match each golden, with LLM fallback
    for golden in &goldens {
        let heuristic_result = judge_comment(golden, &unmatched).await;

        match heuristic_result {
            MatchResult::TruePositive => {
                // Remove the matched finding from the pool
                if let Some(idx) = unmatched.iter().position(|f| {
                    f.file.as_deref() == Some(&golden.file)
                        && f.line == Some(golden.line)
                        && Regex::new(&golden.message_regex)
                            .ok()
                            .map_or(false, |re| re.is_match(&f.message))
                }) {
                    let matched = unmatched.remove(idx);
                    true_positives.push((golden.clone(), matched));
                }
            }
            MatchResult::FalseNegative => {
                // LLM judge fallback: try each unmatched finding
                let llm_matched = 'llm: {
                    for i in 0..unmatched.len() {
                        match run_judge(judge, &golden.message_regex, &unmatched[i].message).await
                        {
                            Ok(verdict) if verdict.match_ => {
                                // Cache the judge call if cache is active
                                if let Some(ref c) = cache {
                                    let verdict_json = serde_json::to_string(&verdict).unwrap_or_default();
                                    c.save_judge(&golden.message_regex, &unmatched[i].message, &verdict_json);
                                }
                                let matched = unmatched.remove(i);
                                true_positives.push((golden.clone(), matched));
                                break 'llm true;
                            }
                            _ => continue,
                        }
                    }
                    false
                };

                if !llm_matched {
                    false_negatives.push(golden.clone());
                }
            }
            MatchResult::FalsePositive => {
                // This variant isn't returned by judge_comment (it checks a golden
                // against candidates, so it only yields TP or FN).  Defensively
                // treat as FN.
                false_negatives.push(golden.clone());
            }
        }
    }

    // Step 4: whatever remains in unmatched are false positives
    let false_positives = unmatched;

    // Step 5: compute metrics
    let tp = true_positives.len();
    let fp = false_positives.len();
    let fn_count = false_negatives.len();

    let precision = if tp + fp > 0 {
        tp as f64 / (tp + fp) as f64
    } else if goldens.is_empty() {
        // No goldens and no findings → perfect by definition
        1.0
    } else {
        0.0
    };

    let recall = if tp + fn_count > 0 {
        tp as f64 / (tp + fn_count) as f64
    } else {
        1.0
    };

    let f1 = if (precision + recall) > 0.0 {
        2.0 * precision * recall / (precision + recall)
    } else {
        0.0
    };

    ConsensusReport {
        agents,
        true_positives,
        false_positives,
        false_negatives,
        precision,
        recall,
        f1,
    }
}

// ── Harness integration ────────────────────────────────────────────────────

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
pub async fn evaluate_pr_with_consensus(
    pr: &GoldenCommentEntry,
    diff: &str,
    client: &openai::Client,
    model: &str,
    judge: &Agent<ResponsesCompletionModel>,
    rules_preamble: Option<&str>,
    prompt_lib: Option<&PromptLibrary>,
    template_vars: Option<&HashMap<&str, &str>>,
    roles: &[&str],
    max_findings: usize,
    cache: Option<Arc<dyn CacheBackend>>,
) -> Result<PrResult> {
    // Build one reviewer config per selected role.
    let reviewer_configs: Vec<ReviewerConfig> = roles
        .iter()
        .map(|role_str| {
            let role = match *role_str {
                "SA" => Role::SA,
                "CL" => Role::CL,
                "AR" => Role::AR,
                "SEC" => Role::SEC,
                other => panic!("Unknown role string: {other}"),
            };
            ReviewerConfig {
                role,
                model: model.to_string(),
                max_findings,
            }
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

    // Run the pipeline with the PR diff.
    let report = run_consensus(
        diff,
        consensus_goldens,
        reviewer_configs,
        client,
        judge,
        rules_preamble,
        prompt_lib,
        template_vars,
        cache,
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

    Ok(PrResult {
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
    })
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_as_str() {
        assert_eq!(Role::SA.as_str(), "SA");
        assert_eq!(Role::CL.as_str(), "CL");
        assert_eq!(Role::AR.as_str(), "AR");
        assert_eq!(Role::SEC.as_str(), "SEC");
    }

    #[test]
    fn test_role_variants_are_distinct() {
        assert_ne!(Role::SA, Role::CL);
        assert_ne!(Role::CL, Role::AR);
        assert_ne!(Role::AR, Role::SEC);
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

    // ── judge_comment tests ─────────────────────────────────────────

    #[test]
    fn test_judge_comment_true_positive() {
        let golden = GoldenComment {
            file: "src/main.rs".into(),
            line: 42,
            message_regex: r"null.pointer".into(),
            severity: "error".into(),
            source: "SA".into(),
        };
        let candidates = vec![Finding {
            file: Some("src/main.rs".into()),
            line: Some(42),
            message: "Potential null pointer dereference".into(),
            severity: "error".into(),
            rule_code: Some("SA-001".into()),
            severity_audited: false,
            severity_audit_reason: None,
        }];
        let result =
            tokio::runtime::Runtime::new().unwrap().block_on(judge_comment(&golden, &candidates));
        assert_eq!(result, MatchResult::TruePositive);
    }

    #[test]
    fn test_judge_comment_false_negative_wrong_message() {
        let golden = GoldenComment {
            file: "src/main.rs".into(),
            line: 42,
            message_regex: r"buffer.overflow".into(),
            severity: "error".into(),
            source: "SA".into(),
        };
        let candidates = vec![Finding {
            file: Some("src/main.rs".into()),
            line: Some(42),
            message: "Potential null pointer".into(),
            severity: "error".into(),
            rule_code: Some("SA-001".into()),
            severity_audited: false,
            severity_audit_reason: None,
        }];
        let result =
            tokio::runtime::Runtime::new().unwrap().block_on(judge_comment(&golden, &candidates));
        assert_eq!(result, MatchResult::FalseNegative);
    }

    #[test]
    fn test_judge_comment_false_negative_wrong_file() {
        let golden = GoldenComment {
            file: "src/other.rs".into(),
            line: 42,
            message_regex: r"null".into(),
            severity: "error".into(),
            source: "SA".into(),
        };
        let candidates = vec![Finding {
            file: Some("src/main.rs".into()),
            line: Some(42),
            message: "Potential null pointer".into(),
            severity: "error".into(),
            rule_code: Some("SA-001".into()),
            severity_audited: false,
            severity_audit_reason: None,
        }];
        let result =
            tokio::runtime::Runtime::new().unwrap().block_on(judge_comment(&golden, &candidates));
        assert_eq!(result, MatchResult::FalseNegative);
    }

    #[test]
    fn test_judge_comment_no_candidates() {
        let golden = GoldenComment {
            file: "src/main.rs".into(),
            line: 42,
            message_regex: r".*".into(),
            severity: "error".into(),
            source: "SA".into(),
        };
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(judge_comment(&golden, &[]));
        assert_eq!(result, MatchResult::FalseNegative);
    }

    // ── ConsensusReport metrics ─────────────────────────────────────

    #[test]
    fn test_consensus_report_empty() {
        // No goldens, no findings → perfect metrics
        let report = ConsensusReport {
            agents: vec![],
            true_positives: vec![],
            false_positives: vec![],
            false_negatives: vec![],
            precision: 1.0,
            recall: 1.0,
            f1: 1.0,
        };
        assert!((report.precision - 1.0).abs() < 1e-6);
        assert!((report.recall - 1.0).abs() < 1e-6);
        assert!((report.f1 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_consensus_report_perfect() {
        // All findings match all goldens
        let report = ConsensusReport {
            agents: vec![],
            true_positives: vec![
                (
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
                        rule_code: None,
                        severity_audited: false,
                        severity_audit_reason: None,
                    },
                ),
            ],
            false_positives: vec![],
            false_negatives: vec![],
            precision: 1.0,
            recall: 1.0,
            f1: 1.0,
        };
        assert_eq!(report.true_positives.len(), 1);
        assert_eq!(report.false_positives.len(), 0);
        assert_eq!(report.false_negatives.len(), 0);
        assert!((report.precision - 1.0).abs() < 1e-6);
        assert!((report.recall - 1.0).abs() < 1e-6);
        assert!((report.f1 - 1.0).abs() < 1e-6);
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
                rule_code: None,
                severity_audited: false,
                severity_audit_reason: None,
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
        };
        assert_eq!(report.true_positives.len(), 0);
        assert_eq!(report.false_positives.len(), 1);
        assert_eq!(report.false_negatives.len(), 1);
        assert!((report.precision - 0.0).abs() < 1e-6);
        assert!((report.recall - 0.0).abs() < 1e-6);
        assert!((report.f1 - 0.0).abs() < 1e-6);
    }

    // ── ReviewerConfig serialization ────────────────────────────────

    #[test]
    fn test_reviewer_config_serialization() {
        let config = ReviewerConfig {
            role: Role::SEC,
            model: "gpt-4o".into(),
            max_findings: 15,
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("SEC"));
        assert!(json.contains("gpt-4o"));
        assert!(json.contains("15"));
        let deserialized: ReviewerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.role, Role::SEC);
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
}
