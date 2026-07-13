use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::severity::Severity;

/// A finding that has been reported by an agent.
///
/// A finding is not a definitive report but an observation an LLM has produced.
///
/// We support an array of aliases for each field so to give the LLM output
/// a better chance of matching the expected severity level.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct Finding {
    /// Source file path where the issue was found, if available.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "component",
        alias = "source_file",
        alias = "source_path",
        alias = "source_name",
        alias = "source",
        alias = "file_source",
        alias = "file_path",
        alias = "file_name",
        alias = "path"
    )]
    pub file: Option<String>,

    /// Line number in the source file, if available.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "line_number"
    )]
    pub line: Option<u32>,

    /// Human-readable description of the finding.
    #[serde(
        default,
        alias = "description",
        alias = "details",
        alias = "text",
        alias = "body"
    )]
    pub message: String,

    /// The agents claimed severity level of the finding.
    #[serde(default, alias = "severity_level", alias = "level", alias = "priority")]
    pub severity: Severity,

    // TODO: Well typed enum or struct
    /// Optional rule or check identifier (e.g. "S001", "R101").
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "rule_id",
        alias = "check_id",
        alias = "category"
    )]
    pub rule_code: Option<String>,

    /// Whether the severity has been audited/downgraded by the severity auditor.
    #[serde(default)]
    pub severity_audited: bool,

    /// Reason for the severity audit result (e.g., downgrade category, protection reason).
    #[serde(default)]
    pub severity_audit_reason: Option<String>,

    /// Evidence supporting the finding (command output, code snippet, etc.).
    #[serde(default)]
    pub evidence: Option<String>,

    /// Path trace / call chain showing how the issue was reached.
    #[serde(default)]
    pub path_trace: Option<String>,

    /// Self reported Confidence level from the agent.
    #[serde(default)]
    pub confidence: Option<ConfidenceLevel>,

    /// Agent tag that found this issue.
    #[serde(default)]
    pub found_by: Option<String>,

    /// Number of agents that flagged this finding.
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "num_agents")]
    pub agent_count: Option<u64>,

    /// Whether this finding was cross-validated by multiple agents/occurrences.
    #[serde(default)]
    pub cross_validated: bool,

    /// How many agents/occurrences cross-validated this finding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cross_validated_by: Option<u64>,

    /// How many original findings were merged to produce this one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged_from: Option<u64>,
}

/// A confidence level for a finding.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum ConfidenceLevel {
    /// The finding is confirmed with high certainty.
    Confirmed,

    /// The finding is likely but not certain.
    Likely,

    /// The finding is uncertain or speculative.
    Uncertain,
}
