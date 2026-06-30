//! submit-finding Tool implementation for EXP-014.
//!
//! Implements a rig::Tool that agents call during their reasoning loop to
//! submit structured findings.  The tool validates required fields, normalizes
//! severity, and accumulates valid findings in an in-memory collector.
//!
//! Feature flag: `exp14_submit_finding` (default OFF)

use std::fmt;
use std::sync::Arc;
use std::sync::Mutex;

use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Finding;

// ── SubmitFindingCollector ────────────────────────────────────────────

/// Thread-safe in-memory collector for findings submitted via the Tool.
///
/// Multiple [`SubmitFindingTool`] instances can share the same collector
/// via [`SubmitFindingTool::with_collector`].
#[derive(Clone, Default)]
pub struct SubmitFindingCollector {
    findings: Arc<Mutex<Vec<Finding>>>,
}

impl SubmitFindingCollector {
    /// Create a new empty collector.
    pub fn new() -> Self {
        Self::default()
    }

    /// Submit a new finding to the collector.
    pub fn submit(&self, finding: Finding) {
        if let Ok(mut lock) = self.findings.lock() {
            lock.push(finding);
        }
    }

    /// Drain all collected findings, leaving the collector empty.
    pub fn drain(&self) -> Vec<Finding> {
        if let Ok(mut lock) = self.findings.lock() {
            std::mem::take(&mut *lock)
        } else {
            Vec::new()
        }
    }

    /// Return the number of collected findings without draining.
    pub fn len(&self) -> usize {
        self.findings.lock().map(|l| l.len()).unwrap_or(0)
    }

    /// Check if the collector is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// ── SubmitFindingArgs ─────────────────────────────────────────────────

/// Arguments for the `submit_finding` Tool.
///
/// Agents populate these fields during their reasoning loop to submit
/// a structured finding for code review.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SubmitFindingArgs {
    /// Source file path (relative to repo root).
    pub file: Option<String>,
    /// Line number in the source file.
    pub line: Option<u32>,
    /// Finding description / message.
    pub message: String,
    /// Severity level: Critical, High, Medium, Low, or Info.
    pub severity: String,
    /// Rule code or identifier for the finding (e.g. "SEC-001").
    pub rule_code: Option<String>,
    /// Evidence supporting the finding (command output, code snippet, etc.).
    pub evidence: Option<String>,
    /// Path trace / call chain showing how the issue was reached.
    pub path_trace: Option<String>,
    /// Confidence level: CONFIRMED, LIKELY, or UNCERTAIN.
    pub confidence: Option<String>,
    /// Agent tag that found this issue (SA, CL, AR, SEC).
    pub found_by: Option<String>,
}

// ── SubmitFindingResponse ─────────────────────────────────────────────

/// Output returned by the `submit_finding` Tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SubmitFindingResponse {
    /// Whether the finding passed validation and was accepted.
    pub accepted: bool,
    /// The validated Finding (present only if accepted).
    pub finding: Option<Finding>,
    /// Validation errors that prevented acceptance.
    pub errors: Vec<String>,
    /// Non-blocking quality warnings.
    pub warnings: Vec<String>,
}

// ── SubmitFindingError ────────────────────────────────────────────────

/// Errors from the submit_finding tool.
#[derive(Debug)]
pub enum SubmitFindingError {
    /// Validation failed — the response contains error details.
    ValidationFailed(SubmitFindingResponse),
}

impl fmt::Display for SubmitFindingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ValidationFailed(resp) => {
                write!(f, "validation failed: {}", resp.errors.join("; "))
            }
        }
    }
}

impl std::error::Error for SubmitFindingError {}

// ── Validation helpers ────────────────────────────────────────────────

/// Normalize a severity string to its canonical capitalized form.
fn normalize_severity(severity: &str) -> Result<String, String> {
    let lower = severity.to_lowercase();
    match lower.as_str() {
        "critical" => Ok("Critical".to_string()),
        "high" => Ok("High".to_string()),
        "medium" => Ok("Medium".to_string()),
        "low" => Ok("Low".to_string()),
        "info" | "informational" => Ok("Info".to_string()),
        _ => Err(format!(
            "Invalid severity '{}'. Must be one of: Critical, High, Medium, Low, Info",
            severity
        )),
    }
}

/// Validate an agent tag (found_by).
fn validate_found_by(found_by: &str) -> Result<(), String> {
    let upper = found_by.to_uppercase();
    match upper.as_str() {
        "SA" | "CL" | "AR" | "SEC" => Ok(()),
        _ => Err(format!(
            "Invalid found_by '{}'. Must be one of: SA, CL, AR, SEC",
            found_by
        )),
    }
}

/// Validate a confidence level.
fn validate_confidence(confidence: &str) -> Result<(), String> {
    let upper = confidence.to_uppercase();
    match upper.as_str() {
        "CONFIRMED" | "LIKELY" | "UNCERTAIN" => Ok(()),
        _ => Err(format!(
            "Invalid confidence '{}'. Must be one of: CONFIRMED, LIKELY, UNCERTAIN",
            confidence
        )),
    }
}

/// Validate submission arguments, returning (errors, warnings).
fn validate_args(args: &SubmitFindingArgs) -> (Vec<String>, Vec<String>) {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // message is required
    if args.message.trim().is_empty() {
        errors.push("message is required and must not be empty".to_string());
    }

    // severity is required
    if args.severity.trim().is_empty() {
        errors.push("severity is required".to_string());
    }

    // file provided without line — warning
    if args.file.is_some() && args.line.is_none() {
        warnings.push(
            "file provided without line — finding won't be precisely located".to_string(),
        );
    }

    // Evidence quality check
    if let Some(ref evidence) = args.evidence {
        if evidence.trim().len() < 10 {
            warnings.push(
                "evidence is very short — consider providing more detail".to_string(),
            );
        }
    }

    // Validate found_by if present
    if let Some(ref found_by) = args.found_by {
        if let Err(e) = validate_found_by(found_by) {
            errors.push(e);
        }
    }

    // Validate confidence if present
    if let Some(ref confidence) = args.confidence {
        if let Err(e) = validate_confidence(confidence) {
            errors.push(e);
        }
    }

    (errors, warnings)
}

// ── SubmitFindingTool ─────────────────────────────────────────────────

/// A rig [`Tool`] that validates and collects findings submitted by an agent.
///
/// Agents call this tool during their reasoning loop to submit individual
/// findings.  The tool validates required fields (message, severity),
/// normalizes severity to the canonical form, and returns structured
/// feedback via [`SubmitFindingResponse`].  Valid findings are accumulated
/// in a shared [`SubmitFindingCollector`].
///
/// # Collector sharing
///
/// Multiple [`SubmitFindingTool`] instances can share the same collector
/// via [`SubmitFindingTool::with_collector`], allowing findings from
/// different agents or roles to be aggregated in a single collection.
pub struct SubmitFindingTool {
    /// The shared collector for accumulating findings.
    pub collector: Arc<Mutex<SubmitFindingCollector>>,
}

impl SubmitFindingTool {
    /// Create a new tool backed by a fresh collector.
    pub fn new() -> Self {
        Self {
            collector: Arc::new(Mutex::new(SubmitFindingCollector::new())),
        }
    }

    /// Create a tool that shares a collector with other tools / agents.
    pub fn with_collector(collector: Arc<Mutex<SubmitFindingCollector>>) -> Self {
        Self { collector }
    }

    /// Drain all collected findings from the shared collector.
    pub fn drain_findings(&self) -> Vec<Finding> {
        if let Ok(lock) = self.collector.lock() {
            lock.drain()
        } else {
            Vec::new()
        }
    }
}

impl Default for SubmitFindingTool {
    fn default() -> Self {
        Self::new()
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
            description: "Submit a structured code review finding. Use this tool when you identify \
                a bug, security issue, logic error, or architectural concern. The tool validates \
                required fields, normalizes severity, and returns structured feedback. \
                Provide as much detail as possible, especially in the message and evidence fields."
                .to_string(),
            parameters: serde_json::to_value(schemars::schema_for!(SubmitFindingArgs)).unwrap(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        // 1. Run validation
        let (errors, warnings) = validate_args(&args);

        if !errors.is_empty() {
            return Ok(SubmitFindingResponse {
                accepted: false,
                finding: None,
                errors,
                warnings,
            });
        }

        // 2. Normalize severity (re-validate since severity is required but
        //    may have a value that doesn't match any accepted level).
        let severity = match normalize_severity(&args.severity) {
            Ok(s) => s,
            Err(e) => {
                return Ok(SubmitFindingResponse {
                    accepted: false,
                    finding: None,
                    errors: vec![e],
                    warnings: Vec::new(),
                });
            }
        };

        // 3. Build the Finding
        let finding = Finding {
            file: args.file.clone(),
            line: args.line,
            message: args.message.clone(),
            severity,
            rule_code: args.rule_code.clone(),
            severity_audited: false,
            severity_audit_reason: None,
            evidence: args.evidence.clone(),
            path_trace: args.path_trace.clone(),
            confidence: args.confidence.clone(),
            found_by: args.found_by.clone(),
        };

        // 4. Store in the in-memory collector
        if let Ok(lock) = self.collector.lock() {
            lock.submit(finding.clone());
        }

        Ok(SubmitFindingResponse {
            accepted: true,
            finding: Some(finding),
            errors: Vec::new(),
            warnings,
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collector_empty_on_creation() {
        let collector = SubmitFindingCollector::new();
        assert!(collector.is_empty());
        assert_eq!(collector.len(), 0);
    }

    #[test]
    fn test_collector_submit_and_drain() {
        let collector = SubmitFindingCollector::new();

        let finding = Finding {
            file: Some("test.rs".to_string()),
            line: Some(42),
            message: "Test finding".to_string(),
            severity: "High".to_string(),
            rule_code: Some("TEST-001".to_string()),
            severity_audited: false,
            severity_audit_reason: None,
            evidence: None,
            path_trace: None,
            confidence: None,
            found_by: None,
        };

        collector.submit(finding);
        assert_eq!(collector.len(), 1);

        let drained = collector.drain();
        assert_eq!(drained.len(), 1);
        assert!(collector.is_empty());
    }

    #[test]
    fn test_collector_multi_submit() {
        let collector = SubmitFindingCollector::new();
        for i in 0..5 {
            collector.submit(Finding {
                file: None,
                line: None,
                message: format!("Finding {i}"),
                severity: "Low".to_string(),
                rule_code: None,
                severity_audited: false,
                severity_audit_reason: None,
                evidence: None,
                path_trace: None,
                confidence: None,
                found_by: None,
            });
        }
        assert_eq!(collector.len(), 5);
        assert_eq!(collector.drain().len(), 5);
        assert!(collector.is_empty());
    }

    #[test]
    fn test_validate_empty_message() {
        let args = SubmitFindingArgs {
            file: Some("test.rs".to_string()),
            line: Some(42),
            message: "  ".to_string(),
            severity: "High".to_string(),
            rule_code: None,
            evidence: None,
            path_trace: None,
            confidence: None,
            found_by: None,
        };
        let (errors, _) = validate_args(&args);
        assert!(!errors.is_empty());
        assert!(errors[0].contains("message"));
    }

    #[test]
    fn test_validate_empty_severity() {
        let args = SubmitFindingArgs {
            file: None,
            line: None,
            message: "Test".to_string(),
            severity: "  ".to_string(),
            rule_code: None,
            evidence: None,
            path_trace: None,
            confidence: None,
            found_by: None,
        };
        let (errors, _) = validate_args(&args);
        assert!(!errors.is_empty());
        assert!(errors[0].contains("severity"));
    }

    #[test]
    fn test_normalize_severity() {
        assert_eq!(normalize_severity("critical").unwrap(), "Critical");
        assert_eq!(normalize_severity("HIGH").unwrap(), "High");
        assert_eq!(normalize_severity("Medium").unwrap(), "Medium");
        assert_eq!(normalize_severity("low").unwrap(), "Low");
        assert_eq!(normalize_severity("INFO").unwrap(), "Info");
        assert_eq!(normalize_severity("informational").unwrap(), "Info");
        assert!(normalize_severity("invalid").is_err());
        assert!(normalize_severity("CRIT").is_err());
    }

    #[test]
    fn test_validate_found_by() {
        assert!(validate_found_by("SA").is_ok());
        assert!(validate_found_by("cl").is_ok());
        assert!(validate_found_by("AR").is_ok());
        assert!(validate_found_by("sec").is_ok());
        assert!(validate_found_by("INVALID").is_err());
        assert!(validate_found_by("").is_err());
    }

    #[test]
    fn test_validate_confidence() {
        assert!(validate_confidence("CONFIRMED").is_ok());
        assert!(validate_confidence("likely").is_ok());
        assert!(validate_confidence("Uncertain").is_ok());
        assert!(validate_confidence("INVALID").is_err());
        assert!(validate_confidence("").is_err());
    }

    #[test]
    fn test_file_without_line_warning() {
        let args = SubmitFindingArgs {
            file: Some("src/main.rs".to_string()),
            line: None,
            message: "Test".to_string(),
            severity: "Medium".to_string(),
            rule_code: None,
            evidence: None,
            path_trace: None,
            confidence: None,
            found_by: None,
        };
        let (errors, warnings) = validate_args(&args);
        assert!(errors.is_empty());
        assert!(!warnings.is_empty());
        assert!(warnings[0].contains("file provided without line"));
    }

    #[test]
    fn test_short_evidence_warning() {
        let args = SubmitFindingArgs {
            file: None,
            line: None,
            message: "Test".to_string(),
            severity: "High".to_string(),
            rule_code: None,
            evidence: Some("short".to_string()),
            path_trace: None,
            confidence: None,
            found_by: None,
        };
        let (_, warnings) = validate_args(&args);
        assert!(!warnings.is_empty());
        assert!(warnings[0].contains("evidence is very short"));
    }

    #[tokio::test]
    async fn test_tool_valid_submission() {
        let tool = SubmitFindingTool::new();
        let args = SubmitFindingArgs {
            file: Some("src/main.rs".to_string()),
            line: Some(100),
            message: "Potential null pointer dereference".to_string(),
            severity: "High".to_string(),
            rule_code: Some("SEC-001".to_string()),
            evidence: Some("grep -n 'unwrap()' src/main.rs -> line 100".to_string()),
            path_trace: None,
            confidence: Some("LIKELY".to_string()),
            found_by: Some("SA".to_string()),
        };

        let result = tool.call(args).await.unwrap();
        assert!(result.accepted);
        assert!(result.finding.is_some());
        assert!(result.errors.is_empty());
        let finding = result.finding.unwrap();
        assert_eq!(finding.severity, "High");
        assert_eq!(finding.confidence.as_deref(), Some("LIKELY"));
        assert_eq!(finding.found_by.as_deref(), Some("SA"));
        assert_eq!(finding.evidence.as_deref(), Some("grep -n 'unwrap()' src/main.rs -> line 100"));

        // Collector should have the finding
        let findings = tool.drain_findings();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].message, "Potential null pointer dereference");
    }

    #[tokio::test]
    async fn test_tool_invalid_severity() {
        let tool = SubmitFindingTool::new();
        let args = SubmitFindingArgs {
            file: None,
            line: None,
            message: "Test".to_string(),
            severity: "INVALID".to_string(),
            rule_code: None,
            evidence: None,
            path_trace: None,
            confidence: None,
            found_by: None,
        };

        let result = tool.call(args).await.unwrap();
        assert!(!result.accepted);
        assert!(result.finding.is_none());
        assert!(!result.errors.is_empty());
        assert!(result.errors[0].contains("severity"));
    }

    #[tokio::test]
    async fn test_tool_invalid_found_by() {
        let tool = SubmitFindingTool::new();
        let args = SubmitFindingArgs {
            file: None,
            line: None,
            message: "Test finding".to_string(),
            severity: "Medium".to_string(),
            rule_code: None,
            evidence: None,
            path_trace: None,
            confidence: None,
            found_by: Some("INVALID".to_string()),
        };

        let result = tool.call(args).await.unwrap();
        assert!(!result.accepted);
        assert!(result.finding.is_none());
        assert!(result.errors.iter().any(|e| e.contains("found_by")));
    }

    #[tokio::test]
    async fn test_tool_invalid_confidence() {
        let tool = SubmitFindingTool::new();
        let args = SubmitFindingArgs {
            file: None,
            line: None,
            message: "Test finding".to_string(),
            severity: "Low".to_string(),
            rule_code: None,
            evidence: None,
            path_trace: None,
            confidence: Some("MAYBE".to_string()),
            found_by: None,
        };

        let result = tool.call(args).await.unwrap();
        assert!(!result.accepted);
        assert!(result.finding.is_none());
        assert!(result.errors.iter().any(|e| e.contains("confidence")));
    }

    #[tokio::test]
    async fn test_tool_minimal_valid_submission() {
        let tool = SubmitFindingTool::new();
        let args = SubmitFindingArgs {
            file: None,
            line: None,
            message: "A simple finding".to_string(),
            severity: "info".to_string(),
            rule_code: None,
            evidence: None,
            path_trace: None,
            confidence: None,
            found_by: None,
        };

        let result = tool.call(args).await.unwrap();
        assert!(result.accepted);
        assert!(result.finding.is_some());
        let finding = result.finding.unwrap();
        assert_eq!(finding.severity, "Info");
        assert!(finding.evidence.is_none());
        assert!(finding.found_by.is_none());
    }

    #[tokio::test]
    async fn test_tool_empty_message_rejected() {
        let tool = SubmitFindingTool::new();
        let args = SubmitFindingArgs {
            file: None,
            line: None,
            message: "".to_string(),
            severity: "High".to_string(),
            rule_code: None,
            evidence: None,
            path_trace: None,
            confidence: None,
            found_by: None,
        };

        let result = tool.call(args).await.unwrap();
        assert!(!result.accepted);
        assert!(result.finding.is_none());
    }

    #[tokio::test]
    async fn test_tool_severity_normalization_case_insensitive() {
        let tool = SubmitFindingTool::new();
        let args = SubmitFindingArgs {
            file: None,
            line: None,
            message: "Test".to_string(),
            severity: "CRITICAL".to_string(),
            rule_code: None,
            evidence: None,
            path_trace: None,
            confidence: None,
            found_by: None,
        };

        let result = tool.call(args).await.unwrap();
        assert!(result.accepted);
        assert_eq!(result.finding.unwrap().severity, "Critical");
    }

    #[tokio::test]
    async fn test_tool_with_collector_sharing() {
        let shared = Arc::new(Mutex::new(SubmitFindingCollector::new()));
        let tool1 = SubmitFindingTool::with_collector(shared.clone());
        let tool2 = SubmitFindingTool::with_collector(shared);

        let args = SubmitFindingArgs {
            file: None,
            line: None,
            message: "Finding from tool1".to_string(),
            severity: "Low".to_string(),
            rule_code: None,
            evidence: None,
            path_trace: None,
            confidence: None,
            found_by: None,
        };
        tool1.call(args).await.unwrap();

        let args2 = SubmitFindingArgs {
            file: None,
            line: None,
            message: "Finding from tool2".to_string(),
            severity: "Info".to_string(),
            rule_code: None,
            evidence: None,
            path_trace: None,
            confidence: None,
            found_by: None,
        };
        tool2.call(args2).await.unwrap();

        let findings = tool1.drain_findings();
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].message, "Finding from tool1");
        assert_eq!(findings[1].message, "Finding from tool2");
    }
}
