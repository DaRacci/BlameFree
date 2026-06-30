use rig_core::agent::Agent;
use rig_core::client::CompletionClient;
use rig_core::providers::openai;
use rig_core::providers::openai::responses_api::ResponsesCompletionModel;
use std::collections::HashMap;

pub mod prompts;

pub use crb_tools::Finding;

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

/// Build a rig agent for the given role with optional prompt library,
/// template variables, extra preamble text, and filesystem tools.
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
/// If `extra_preamble` is `Some`, it is appended after the role preamble.
/// This is used for tool-calling instructions and other supplementary content.
///
/// `template_vars` provides variable substitutions for the prompt template
/// (e.g. `{diff}`, `{role}`, `{file_list}`, `{language}`).
///
/// If `workdir` is `Some`, four filesystem tools are registered on the agent:
/// `read_file`, `grep`, `terminal`, and `list_dir` - all scoped to the given
/// working directory (typically the PR worktree checkout).  If `workdir` is
/// `None`, no tools are registered and the agent operates on the diff text alone.
pub fn build_agent(
    client: &openai::Client,
    model: &str,
    role: &str,
    rules_preamble: Option<&str>,
    prompt_lib: Option<&PromptLibrary>,
    template_vars: Option<&HashMap<&str, &str>>,
    extra_preamble: Option<&str>,
    workdir: Option<&str>,
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
    let mut full_preamble = match rules_preamble {
        Some(rp) if !rp.is_empty() => format!("{rp}\n\n{role_preamble}"),
        _ => role_preamble.to_string(),
    };

    // Append extra preamble (tool instructions, etc.)
    if let Some(extra) = extra_preamble {
        if !extra.is_empty() {
            full_preamble = format!("{full_preamble}\n\n{extra}");
        }
    }

    // Register filesystem tools if a workdir is provided.
    // Note: AgentBuilder's ToolState generic changes when .tool() is called
    // (NoToolConfig → WithBuilderTools), so we must use two separate build paths.
    if let Some(wd) = workdir {
        let wd = wd.to_string();
        client
            .agent(model)
            .preamble(&full_preamble)
            .tool(crb_tools::read_file::ReadFileTool {
                repo_root: wd.clone(),
                ..Default::default()
            })
            .tool(crb_tools::shell::ShellTool {
                work_dir: wd.clone(),
                ..Default::default()
            })
            .tool(crb_tools::grep::GrepTool { workdir: wd.clone() })
            .tool(crb_tools::list_dir::ListDirTool { workdir: wd })
            .default_max_turns(6)
            .temperature(0.3)
            .build()
    } else {
        client
            .agent(model)
            .preamble(&full_preamble)
            .temperature(0.3)
            .build()
    }
}

/// All supported agent role identifiers.
pub const AGENT_ROLES: &[&str] = &["SA", "CL", "AR", "SEC"];
