use rig_core::agent::Agent;
use rig_core::client::CompletionClient;
use rig_core::providers::openai;
use rig_core::providers::openai::responses_api::ResponsesCompletionModel;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A structured finding returned by an agent.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Finding {
    pub file: Option<String>,
    pub line: Option<u32>,
    pub message: String,
    pub severity: String,
    pub rule_code: Option<String>,
    /// Whether the severity has been audited/downgraded by the severity auditor.
    #[serde(default)]
    pub severity_audited: bool,
    /// Reason for the severity audit result (e.g., downgrade category, protection reason).
    #[serde(default)]
    pub severity_audit_reason: Option<String>,
}

/// Convert Finding to serde_json::Map for backward compatibility
pub fn finding_to_map(f: &Finding) -> serde_json::Map<String, serde_json::Value> {
    serde_json::to_value(f)
        .unwrap_or_default()
        .as_object()
        .cloned()
        .unwrap_or_default()
}

/// Convert serde_json::Map to Finding
pub fn map_to_finding(m: &serde_json::Map<String, serde_json::Value>) -> Option<Finding> {
    serde_json::from_value(serde_json::Value::Object(m.clone())).ok()
}

// ── Role-specific preamble prompts ─────────────────────────────────────────

const SA_PREAMBLE: &str = "\
You are a static analysis specialist. Analyze the provided code diff for \
potential bugs, code smells, and violations of best practices. Focus on \
correctness, error handling, and code quality issues. Respond with a JSON \
array of findings.";

const CL_PREAMBLE: &str = "\
You are a code logic expert. Examine the diff for logical errors, incorrect \
assumptions, off-by-one errors, race conditions, and concurrency issues. \
Focus on whether the code correctly implements its intended logic. Respond \
with a JSON array of findings.";

const AR_PREAMBLE: &str = "\
You are an architecture reviewer. Evaluate the diff for architectural concerns: \
coupling, cohesion, separation of concerns, design pattern violations, and \
maintainability issues. Focus on the high-level structure and design decisions. \
Respond with a JSON array of findings.";

const SEC_PREAMBLE: &str = "\
You are a security specialist. Review the diff for security vulnerabilities: \
injection flaws, authentication/authorization issues, data exposure, input \
validation problems, and other security weaknesses. Focus on OWASP Top 10 \
categories. Respond with a JSON array of findings.";

const DEFAULT_PREAMBLE: &str = "\
You are a code reviewer. Analyze the provided code diff and identify any \
issues. Respond with a JSON array of findings.";

/// Build a rig agent for the given role with optional rules preamble.
///
/// If `rules_preamble` is `Some` and non-empty, it is prepended before the
/// role-specific preamble, separated by a blank line.  This allows project-
/// level rules to be injected into the agent's system prompt before its
/// role-specific instructions.
pub fn build_agent(
    client: &openai::Client,
    model: &str,
    role: &str,
    rules_preamble: Option<&str>,
) -> Agent<ResponsesCompletionModel> {
    let role_preamble = match role {
        "SA" => SA_PREAMBLE,
        "CL" => CL_PREAMBLE,
        "AR" => AR_PREAMBLE,
        "SEC" => SEC_PREAMBLE,
        _ => DEFAULT_PREAMBLE,
    };
    let full_preamble = match rules_preamble {
        Some(rp) if !rp.is_empty() => format!("{}\n\n{}", rp, role_preamble),
        _ => role_preamble.to_string(),
    };
    client.agent(model).preamble(&full_preamble).build()
}

/// All supported agent role identifiers.
pub const AGENT_ROLES: &[&str] = &["SA", "CL", "AR", "SEC"];
