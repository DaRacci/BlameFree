use rig_core::agent::Agent;
use rig_core::client::CompletionClient;
use rig_core::providers::openai;
use rig_core::providers::openai::responses_api::ResponsesCompletionModel;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod prompts;

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

use crate::prompts::PromptLibrary;

// ── Role-specific preamble prompts ─────────────────────────────────────────

const SA_PREAMBLE: &str = "\
IMPORTANT: Your ENTIRE response must be a valid JSON array. No markdown, no explanation, no code fences. Start with [ and end with ].

You are a static analysis specialist. Analyze the provided code diff for \
potential bugs, code smells, and violations of best practices. Focus on \
correctness, error handling, and code quality issues. Respond with a JSON \
array of findings.";

const CL_PREAMBLE: &str = "\
IMPORTANT: Your ENTIRE response must be a valid JSON array. No markdown, no explanation, no code fences. Start with [ and end with ].

You are a code logic expert. Examine the diff for logical errors, incorrect \
assumptions, off-by-one errors, race conditions, and concurrency issues. \
Focus on whether the code correctly implements its intended logic. Respond \
with a JSON array of findings.";

const AR_PREAMBLE: &str = "\
IMPORTANT: Your ENTIRE response must be a valid JSON array. No markdown, no explanation, no code fences. Start with [ and end with ].

You are an architecture reviewer. Evaluate the diff for architectural concerns: \
coupling, cohesion, separation of concerns, design pattern violations, and \
maintainability issues. Focus on the high-level structure and design decisions. \
Respond with a JSON array of findings.";

const SEC_PREAMBLE: &str = "\
IMPORTANT: Your ENTIRE response must be a valid JSON array. No markdown, no explanation, no code fences. Start with [ and end with ].

You are a security specialist. Review the diff for security vulnerabilities: \
injection flaws, authentication/authorization issues, data exposure, input \
validation problems, and other security weaknesses. Focus on OWASP Top 10 \
categories. Respond with a JSON array of findings.";

const DEFAULT_PREAMBLE: &str = "\
IMPORTANT: Your ENTIRE response must be a valid JSON array. No markdown, no explanation, no code fences. Start with [ and end with ].

You are a code reviewer. Analyze the provided code diff and identify any \
issues. Respond with a JSON array of findings.";

/// Build a rig agent for the given role with optional prompt library and
/// template variables.
///
/// If `prompt_lib` is `Some`, the role preamble is resolved through the
/// library (custom prompts from files, falling back to built-in defaults).
/// If `prompt_lib` is `None`, the original hardcoded const strings are used.
///
/// If `rules_preamble` is `Some` and non-empty, it is prepended before the
/// role-specific preamble, separated by a blank line.  This allows project-
/// level rules to be injected into the agent's system prompt before its
/// role-specific instructions.
///
/// `template_vars` provides variable substitutions for the prompt template
/// (e.g. `{diff}`, `{role}`, `{file_list}`, `{language}`).
pub fn build_agent(
    client: &openai::Client,
    model: &str,
    role: &str,
    rules_preamble: Option<&str>,
    prompt_lib: Option<&PromptLibrary>,
    template_vars: Option<&HashMap<&str, &str>>,
) -> Agent<ResponsesCompletionModel> {
    let role_preamble = match prompt_lib {
        Some(lib) => {
            let empty_map = HashMap::new();
            let vars = template_vars.unwrap_or(&empty_map);
            lib.render(role, vars)
        }
        None => match role {
            "SA" => SA_PREAMBLE.to_string(),
            "CL" => CL_PREAMBLE.to_string(),
            "AR" => AR_PREAMBLE.to_string(),
            "SEC" => SEC_PREAMBLE.to_string(),
            _ => DEFAULT_PREAMBLE.to_string(),
        },
    };
    let full_preamble = match rules_preamble {
        Some(rp) if !rp.is_empty() => format!("{}\n\n{}", rp, role_preamble),
        _ => role_preamble.to_string(),
    };
    client.agent(model).preamble(&full_preamble).build()
}

/// All supported agent role identifiers.
pub const AGENT_ROLES: &[&str] = &["SA", "CL", "AR", "SEC"];
