//! `SubmitFindingTool` — a [`rig::Tool`] that agents call to submit findings.
//!
//! Provides structured finding submission with instant validation, severity
//! normalization, agent-tag enforcement, and in-memory collection via
//! [`SubmitFindingCollector`].
//!
//! # Usage
//!
//! ```ignore
//! use std::sync::{Arc, Mutex};
//! use crb_agents::submit_finding::{SubmitFindingTool, SubmitFindingCollector};
//!
//! let collector = Arc::new(Mutex::new(SubmitFindingCollector::new()));
//! let tool = SubmitFindingTool::new(collector);
//! ```

use std::sync::{Arc, Mutex};

use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Finding;

// ── Collector ──────────────────────────────────────────────────────────────

/// Thread-safe in-memory collector that accumulates submitted findings.
///
/// Designed to be shared across multiple agent tool instances via `Arc<Mutex<..>>`.
/// After all agents have run, call [`drain`](SubmitFindingCollector::drain) to
/// retrieve all collected findings atomically.
#[derive(Debug, Default)]
pub struct SubmitFindingCollector {
    findings: Vec<Finding>,
}

impl SubmitFindingCollector {
    /// Create a new empty collector.
    pub fn new() -> Self {
        Self { findings: Vec::new() }
    }

    /// Submit a single finding (validated).
    pub fn submit(&mut self, finding: Finding) {
        self.findings.push(finding);
    }

    /// Take all collected findings, leaving the collector empty.
    pub fn drain(&mut self) -> Vec<Finding> {
        std::mem::take(&mut self.findings)
    }

    /// Number of findings collected so far.
    pub fn len(&self) -> usize {
        self.findings.len()
    }

    /// Whether the collector is empty.
    pub fn is_empty(&self) -> bool {
        self.findings.is_empty()
    }
}

// ── Tool Input ────────────────────────────────────────────────────────────

/// Arguments accepted by [`SubmitFindingTool`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SubmitFindingArgs {
    /// File path where the issue was found (optional).
    pub file: Option<String>,
    /// Line number where the issue was found (optional).
    pub line: Option<u32>,
    /// Human-readable description of the finding.
    pub message: String,
    /// Severity level: "critical", "high", "medium", "low", or "info" (case-insensitive).
    pub severity: String,
    /// Optional rule code (e.g. "SA-001", "GEN-CL-002").
    pub rule_code: Option<String>,
    /// Optional evidence string supporting the finding.
    #[serde(default)]
    pub evidence: Option<String>,
    /// Optional path trace showing how the finding was reached.
    #[serde(default)]
    pub path_trace: Option<String>,
    /// Confidence level: "confirmed", "likely", "uncertain" (case-insensitive).
    #[serde(default)]
    pub confidence: Option<String>,
    /// Which agent role found this: "SA", "CL", "AR", "SEC", "GEN" (case-insensitive).
    #[serde(default)]
    pub found_by: Option<String>,
}

// ── Tool Response ──────────────────────────────────────────────────────────

/// Structured response returned by [`SubmitFindingTool`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitFindingResponse {
    /// Whether the finding was accepted.
    pub accepted: bool,
    /// The validated finding (only present when `accepted` is true).
    pub finding: Option<Finding>,
    /// Validation error messages (present when `accepted` is false).
    pub errors: Vec<String>,
    /// Non-blocking quality warnings.
    pub warnings: Vec<String>,
}

// ── Severity normalization ────────────────────────────────────────────────

/// Normalize a severity string to capitalized form.
fn normalize_severity(raw: &str) -> Result<String, String> {
    match raw.to_lowercase().as_str() {
        "critical" => Ok("Critical".to_string()),
        "high" => Ok("High".to_string()),
        "medium" => Ok("Medium".to_string()),
        "low" => Ok("Low".to_string()),
        "info" | "informational" => Ok("Info".to_string()),
        other => Err(format!(
            "Invalid severity '{}'. Must be one of: critical, high, medium, low, info",
            other
        )),
    }
}

/// Normalize a confidence string.
fn normalize_confidence(raw: &str) -> Result<String, String> {
    match raw.to_lowercase().as_str() {
        "confirmed" => Ok("CONFIRMED".to_string()),
        "likely" => Ok("LIKELY".to_string()),
        "uncertain" => Ok("UNCERTAIN".to_string()),
        other => Err(format!(
            "Invalid confidence '{}'. Must be one of: confirmed, likely, uncertain",
            other
        )),
    }
}

/// Validate and normalize a found_by role string.
fn normalize_found_by(raw: &str) -> Result<String, String> {
    match raw.to_uppercase().as_str() {
        "SA" | "CL" | "AR" | "SEC" | "GEN" => Ok(raw.to_uppercase()),
        other => Err(format!(
            "Invalid found_by '{}'. Must be one of: SA, CL, AR, SEC, GEN",
            other
        )),
    }
}

// ── Error Type ─────────────────────────────────────────────────────────────

/// Errors from the submit-finding tool.
#[derive(Debug)]
pub enum SubmitFindingError {
    /// Internal tool error.
    Internal(String),
}

impl std::fmt::Display for SubmitFindingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Internal(e) => write!(f, "submit_finding error: {e}"),
        }
    }
}

impl std::error::Error for SubmitFindingError {}

// ── Tool Implementation ───────────────────────────────────────────────────

/// A [`rig::Tool`] that allows agents to submit findings with structured validation.
///
/// The tool validates inputs, normalizes severities, and collects the finding
/// into the shared [`SubmitFindingCollector`]. Returns structured feedback
/// including any validation errors or quality warnings.
#[derive(Debug, Clone)]
pub struct SubmitFindingTool {
    /// Shared collector for accumulated findings.
    collector: Arc<Mutex<SubmitFindingCollector>>,
}

impl SubmitFindingTool {
    /// Create a new `SubmitFindingTool` with the given shared collector.
    pub fn new(collector: Arc<Mutex<SubmitFindingCollector>>) -> Self {
        Self { collector }
    }

    /// Validate and normalize the args, returning (normalized_Finding, errors, warnings).
    fn validate(args: SubmitFindingArgs) -> (Option<Finding>, Vec<String>, Vec<String>) {
        let mut errors: Vec<String> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();

        // ── Required fields ─────────────────────────────────────────────
        if args.message.trim().is_empty() {
            errors.push("'message' is required and must not be empty".to_string());
        }

        // ── Severity normalization ─────────────────────────────────────
        let severity = match normalize_severity(&args.severity) {
            Ok(s) => s,
            Err(e) => {
                errors.push(e);
                "Medium".to_string() // fallback
            }
        };

        // ── Confidence normalization ───────────────────────────────────
        let _confidence = match &args.confidence {
            Some(raw) => match normalize_confidence(raw) {
                Ok(c) => Some(c),
                Err(e) => {
                    errors.push(e);
                    None
                }
            },
            None => None,
        };

        // ── Found_by normalization ─────────────────────────────────────
        let _found_by = match &args.found_by {
            Some(raw) => match normalize_found_by(raw) {
                Ok(f) => Some(f),
                Err(e) => {
                    errors.push(e);
                    None
                }
            },
            None => None,
        };

        // ── Quality warnings ───────────────────────────────────────────
        if args.line.is_none() {
            warnings.push("Finding has no line number — consider providing a specific line".to_string());
        }
        if let Some(ref evidence) = args.evidence {
            if evidence.trim().len() < 10 {
                warnings.push("Evidence is very short (< 10 chars) — consider adding more detail".to_string());
            }
        } else {
            warnings.push("Finding has no evidence — consider including code excerpts or reasoning".to_string());
        }

        // Return early if there are hard errors
        if !errors.is_empty() {
            return (None, errors, warnings);
        }

        // ── Build Finding ──────────────────────────────────────────────
        let finding = Finding {
            file: args.file,
            line: args.line,
            message: args.message,
            severity,
            rule_code: args.rule_code,
            severity_audited: false,
            severity_audit_reason: None,
            evidence: None,
            path_trace: None,
            confidence: None,
            found_by: None,
        };

        (Some(finding), errors, warnings)
    }
}

impl Tool for SubmitFindingTool {
    const NAME: &'static str = "submit_finding";

    type Error = SubmitFindingError;
    type Args = SubmitFindingArgs;
    type Output = SubmitFindingResponse;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Submit a code review finding with structured validation. \
                         Call this tool for each finding you identify. \
                         Parameters include file, line, message, severity, \
                         rule_code, evidence, confidence, and found_by."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "file": {
                        "type": "string",
                        "description": "File path where the issue was found (optional)"
                    },
                    "line": {
                        "type": "integer",
                        "description": "Line number where the issue occurs (optional)"
                    },
                    "message": {
                        "type": "string",
                        "description": "Human-readable description of the finding (required)"
                    },
                    "severity": {
                        "type": "string",
                        "description": "Severity: critical, high, medium, low, or info (case-insensitive)",
                        "enum": ["critical", "high", "medium", "low", "info"]
                    },
                    "rule_code": {
                        "type": "string",
                        "description": "Optional rule code (e.g. SA-001, GEN-CL-002)"
                    },
                    "evidence": {
                        "type": "string",
                        "description": "Evidence supporting the finding (optional)"
                    },
                    "path_trace": {
                        "type": "string",
                        "description": "Path trace showing how the finding was reached (optional)"
                    },
                    "confidence": {
                        "type": "string",
                        "description": "Confidence level: confirmed, likely, uncertain (case-insensitive)",
                        "enum": ["confirmed", "likely", "uncertain"]
                    },
                    "found_by": {
                        "type": "string",
                        "description": "Which agent role found this: SA, CL, AR, SEC, or GEN",
                        "enum": ["SA", "CL", "AR", "SEC", "GEN"]
                    }
                },
                "required": ["message", "severity"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let (finding_opt, errors, warnings) = Self::validate(args);

        match finding_opt {
            Some(finding) => {
                // Store in collector
                if let Ok(mut collector) = self.collector.lock() {
                    collector.submit(finding.clone());
                }
                Ok(SubmitFindingResponse {
                    accepted: true,
                    finding: Some(finding),
                    errors: Vec::new(),
                    warnings,
                })
            }
            None => Ok(SubmitFindingResponse {
                accepted: false,
                finding: None,
                errors,
                warnings,
            }),
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tool() -> SubmitFindingTool {
        SubmitFindingTool::new(Arc::new(Mutex::new(SubmitFindingCollector::new())))
    }

    #[tokio::test]
    async fn test_submit_valid_finding() {
        let tool = make_tool();
        let args = SubmitFindingArgs {
            file: Some("src/main.rs".to_string()),
            line: Some(42),
            message: "Potential null pointer dereference".to_string(),
            severity: "high".to_string(),
            rule_code: Some("SA-001".to_string()),
            evidence: Some("Code at line 42 calls unwrap() on Option".to_string()),
            path_trace: None,
            confidence: Some("confirmed".to_string()),
            found_by: Some("SA".to_string()),
        };
        let resp = tool.call(args).await.unwrap();
        assert!(resp.accepted);
        assert!(resp.errors.is_empty());
        assert!(resp.finding.is_some());
        let finding = resp.finding.unwrap();
        assert_eq!(finding.severity, "High"); // normalized
        assert_eq!(finding.message, "Potential null pointer dereference");
        assert_eq!(finding.file.as_deref(), Some("src/main.rs"));
        assert_eq!(finding.line, Some(42));
    }

    #[tokio::test]
    async fn test_submit_missing_message() {
        let tool = make_tool();
        let args = SubmitFindingArgs {
            file: None,
            line: None,
            message: "".to_string(),
            severity: "high".to_string(),
            rule_code: None,
            evidence: None,
            path_trace: None,
            confidence: None,
            found_by: None,
        };
        let resp = tool.call(args).await.unwrap();
        assert!(!resp.accepted);
        assert!(!resp.errors.is_empty());
        assert!(resp.errors.iter().any(|e| e.contains("message")));
    }

    #[tokio::test]
    async fn test_invalid_severity() {
        let tool = make_tool();
        let args = SubmitFindingArgs {
            file: None,
            line: Some(1),
            message: "A bug".to_string(),
            severity: "super-critical".to_string(),
            rule_code: None,
            evidence: Some("evidence here".to_string()),
            path_trace: None,
            confidence: None,
            found_by: None,
        };
        let resp = tool.call(args).await.unwrap();
        assert!(!resp.accepted);
        assert!(resp.errors.iter().any(|e| e.contains("severity")));
    }

    #[tokio::test]
    async fn test_severity_case_insensitive() {
        let tool = make_tool();
        for (input, expected) in &[
            ("CRITICAL", "Critical"),
            ("Critical", "Critical"),
            ("critical", "Critical"),
            ("HIGH", "High"),
            ("Medium", "Medium"),
            ("low", "Low"),
            ("INFO", "Info"),
        ] {
            let args = SubmitFindingArgs {
                file: None,
                line: Some(1),
                message: "test finding".to_string(),
                severity: input.to_string(),
                rule_code: None,
                evidence: Some("detailed evidence text here".to_string()),
                path_trace: None,
                confidence: None,
                found_by: None,
            };
            let resp = tool.call(args).await.unwrap();
            assert!(resp.accepted, "Failed for severity '{}'", input);
            assert_eq!(resp.finding.unwrap().severity, *expected);
        }
    }

    #[tokio::test]
    async fn test_confidence_normalization() {
        let tool = make_tool();
        for (input, _expected) in &[
            ("confirmed", "CONFIRMED"),
            ("LIKELY", "LIKELY"),
            ("Uncertain", "UNCERTAIN"),
        ] {
            let args = SubmitFindingArgs {
                file: None,
                line: Some(1),
                message: "test".to_string(),
                severity: "medium".to_string(),
                rule_code: None,
                evidence: Some("detailed evidence text here".to_string()),
                path_trace: None,
                confidence: Some(input.to_string()),
                found_by: None,
            };
            let resp = tool.call(args).await.unwrap();
            assert!(resp.accepted, "Failed for confidence '{}'", input);
        }
    }

    #[tokio::test]
    async fn test_invalid_confidence() {
        let tool = make_tool();
        let args = SubmitFindingArgs {
            file: None,
            line: Some(1),
            message: "test".to_string(),
            severity: "medium".to_string(),
            rule_code: None,
            evidence: Some("detailed evidence text here".to_string()),
            path_trace: None,
            confidence: Some("maybe".to_string()),
            found_by: None,
        };
        let resp = tool.call(args).await.unwrap();
        assert!(!resp.accepted);
        assert!(resp.errors.iter().any(|e| e.contains("confidence")));
    }

    #[tokio::test]
    async fn test_found_by_normalization() {
        let tool = make_tool();
        for role in &["SA", "CL", "AR", "SEC", "GEN"] {
            let args = SubmitFindingArgs {
                file: None,
                line: Some(1),
                message: "test".to_string(),
                severity: "medium".to_string(),
                rule_code: None,
                evidence: Some("detailed evidence text here".to_string()),
                path_trace: None,
                confidence: None,
                found_by: Some(role.to_string()),
            };
            let resp = tool.call(args).await.unwrap();
            assert!(resp.accepted, "Failed for found_by '{}'", role);
        }
    }

    #[tokio::test]
    async fn test_invalid_found_by() {
        let tool = make_tool();
        let args = SubmitFindingArgs {
            file: None,
            line: Some(1),
            message: "test".to_string(),
            severity: "medium".to_string(),
            rule_code: None,
            evidence: Some("detailed evidence text here".to_string()),
            path_trace: None,
            confidence: None,
            found_by: Some("QA".to_string()),
        };
        let resp = tool.call(args).await.unwrap();
        assert!(!resp.accepted);
        assert!(resp.errors.iter().any(|e| e.contains("found_by")));
    }

    #[tokio::test]
    async fn test_quality_warnings_no_line() {
        let tool = make_tool();
        let args = SubmitFindingArgs {
            file: Some("src/main.rs".to_string()),
            line: None,
            message: "A bug".to_string(),
            severity: "high".to_string(),
            rule_code: None,
            evidence: Some("detailed evidence".to_string()),
            path_trace: None,
            confidence: None,
            found_by: None,
        };
        let resp = tool.call(args).await.unwrap();
        assert!(resp.accepted);
        assert!(!resp.warnings.is_empty());
        assert!(resp.warnings.iter().any(|w| w.contains("line number")));
    }

    #[tokio::test]
    async fn test_quality_warnings_short_evidence() {
        let tool = make_tool();
        let args = SubmitFindingArgs {
            file: None,
            line: Some(1),
            message: "A bug".to_string(),
            severity: "high".to_string(),
            rule_code: None,
            evidence: Some("short".to_string()),
            path_trace: None,
            confidence: None,
            found_by: None,
        };
        let resp = tool.call(args).await.unwrap();
        assert!(resp.accepted);
        assert!(!resp.warnings.is_empty());
        assert!(resp.warnings.iter().any(|w| w.contains("short")));
    }

    #[tokio::test]
    async fn test_collector_drain() {
        let collector = Arc::new(Mutex::new(SubmitFindingCollector::new()));
        let tool = SubmitFindingTool::new(collector.clone());

        // Submit 3 findings
        for i in 0..3 {
            let args = SubmitFindingArgs {
                file: Some(format!("file_{}.rs", i)),
                line: Some(i),
                message: format!("Finding {}", i),
                severity: "low".to_string(),
                rule_code: None,
                evidence: Some("detailed evidence text".to_string()),
                path_trace: None,
                confidence: None,
                found_by: None,
            };
            tool.call(args).await.unwrap();
        }

        // Drain collector
        let findings = collector.lock().unwrap().drain();
        assert_eq!(findings.len(), 3);
        assert!(collector.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_collector_shared_across_tools() {
        let collector = Arc::new(Mutex::new(SubmitFindingCollector::new()));
        let tool1 = SubmitFindingTool::new(collector.clone());
        let tool2 = SubmitFindingTool::new(collector.clone());

        let args1 = SubmitFindingArgs {
            file: Some("a.rs".to_string()),
            line: Some(1),
            message: "Finding from tool1".to_string(),
            severity: "medium".to_string(),
            rule_code: None,
            evidence: Some("detailed evidence text".to_string()),
            path_trace: None,
            confidence: None,
            found_by: None,
        };
        tool1.call(args1).await.unwrap();

        let args2 = SubmitFindingArgs {
            file: Some("b.rs".to_string()),
            line: Some(2),
            message: "Finding from tool2".to_string(),
            severity: "high".to_string(),
            rule_code: None,
            evidence: Some("detailed evidence text".to_string()),
            path_trace: None,
            confidence: None,
            found_by: None,
        };
        tool2.call(args2).await.unwrap();

        let findings = collector.lock().unwrap().drain();
        assert_eq!(findings.len(), 2);
    }

    #[tokio::test]
    async fn test_tool_definition() {
        let tool = make_tool();
        let def = tool.definition("test".to_string()).await;
        assert_eq!(def.name, "submit_finding");
        assert!(def.description.contains("finding"));
    }
}
