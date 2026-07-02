use rig_core::agent::Agent;
use rig_core::client::CompletionClient;
use rig_core::providers::openai;
use rig_core::providers::openai::responses_api::ResponsesCompletionModel;
use std::collections::HashMap;

#[cfg(feature = "exp14_submit_finding")]
use std::sync::{Arc, Mutex};

pub mod manifest;
pub mod prompts;
pub mod templates;

#[cfg(feature = "exp14_submit_finding")]
pub mod submit_finding;

use crate::templates::{build_agent_context, TemplateEngine};
pub use crate::manifest::AgentManifest;

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
use std::path::Path;

/// Build a rig agent for the given role with optional prompt library,
/// template variables, agent manifest, extra preamble text, and filesystem tools.
///
/// If `template_engine` and `agent_manifest` are both `Some`, the agent's
/// preamble is rendered from `agent.hbs` using the manifest entry's data
/// and section files from `prompts/sections/`.
///
/// If `prompt_lib` is `Some`, the role preamble is resolved through the
/// library (custom prompts from files, falling back to built-in defaults).
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
    prompt_lib: &PromptLibrary,
    template_engine: Option<&TemplateEngine>,
    agent_manifest: Option<&AgentManifest>,
    template_vars: Option<&HashMap<String, serde_json::Value>>,
    extra_preamble: Option<&str>,
    workdir: Option<&str>,
    additional_params: Option<serde_json::Value>,
    #[cfg(feature = "exp14_submit_finding")]
    collector: Option<Arc<Mutex<submit_finding::SubmitFindingCollector>>>,
) -> Agent<ResponsesCompletionModel> {
    let role_preamble = match (template_engine, agent_manifest) {
        // Primary path: template engine + manifest → render agent.hbs
        (Some(engine), Some(manifest)) => {
            if let Some(entry) = manifest.get(role) {
                let sections_dir = std::path::Path::new("prompts/sections");
                // Extract max_findings from template_vars if present
                let max_findings = template_vars
                    .and_then(|tv| tv.get("max_findings"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(20) as usize;
                let extra = template_vars.map(|tv| {
                    let mut map = tv.clone();
                    map.remove("max_findings");
                    map
                });
                match build_agent_context(engine, entry, sections_dir, max_findings, extra) {
                    Ok(ctx) => match engine.render("agent", &ctx) {
                        Ok(rendered) => rendered,
                        Err(e) => {
                            tracing::warn!("Failed to render agent template for '{}': {}. Using empty preamble.", role, e);
                            String::new()
                        }
                    },
                    Err(e) => {
                        tracing::warn!("Failed to build agent context for '{}': {}. Using empty preamble.", role, e);
                        String::new()
                    }
                }
            } else {
                tracing::warn!(
                    "Unknown role '{}' not found in manifest. Using empty preamble.",
                    role
                );
                String::new()
            }
        }
        // Legacy template engine path (no manifest)
        (Some(engine), None) => {
            let role_lower = role.to_lowercase();
            let vars: serde_json::Value = template_vars
                .map(|m| {
                    serde_json::Value::Object(
                        m.iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect(),
                    )
                })
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
            if engine.has_template(&role_lower) {
                engine.render(&role_lower, &vars).unwrap_or_default()
            } else if engine.has_template("default") {
                engine.render("default", &vars).unwrap_or_default()
            } else {
                String::new()
            }
        }
        // Embedded prompt library path (new default, no engine/manifest)
        (None, _) => {
            let empty_map = HashMap::new();
            let vars: HashMap<String, serde_json::Value> = template_vars
                .map(|v| v.clone())
                .unwrap_or(empty_map);
            prompt_lib.render(role, &vars)
        }
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

    // Build agent with tools.
    // Note: AgentBuilder's ToolState generic changes when .tool() is called
    // (NoToolConfig → WithBuilderTools), so each tool combination needs its
    // own builder chain.
    #[cfg(feature = "exp14_submit_finding")]
    {
        let has_wd = workdir.is_some();
        let has_collector = collector.is_some();

        match (has_wd, has_collector) {
            (true, true) => {
                let wd = workdir.unwrap().to_string();
                let submit_tool =
                    submit_finding::SubmitFindingTool::new(collector.unwrap());
                let mut builder = client
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
                    .tool(submit_tool)
                    .default_max_turns(6)
                    .temperature(0.3);
                if let Some(ref params) = additional_params {
                    builder = builder.additional_params(params.clone());
                }
                builder.build()
            }
            (true, false) => {
                let wd = workdir.unwrap().to_string();
                let mut builder = client
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
                    .temperature(0.3);
                if let Some(ref params) = additional_params {
                    builder = builder.additional_params(params.clone());
                }
                builder.build()
            }
            (false, true) => {
                let submit_tool =
                    submit_finding::SubmitFindingTool::new(collector.unwrap());
                let mut builder = client
                    .agent(model)
                    .preamble(&full_preamble)
                    .tool(submit_tool)
                    .temperature(0.3);
                if let Some(ref params) = additional_params {
                    builder = builder.additional_params(params.clone());
                }
                builder.build()
            }
            (false, false) => {
                let mut builder = client
                    .agent(model)
                    .preamble(&full_preamble)
                    .temperature(0.3);
                if let Some(ref params) = additional_params {
                    builder = builder.additional_params(params.clone());
                }
                builder.build()
            }
        }
    }

    #[cfg(not(feature = "exp14_submit_finding"))]
    {
        if let Some(wd) = workdir {
            let wd = wd.to_string();
            let mut builder = client
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
                .temperature(0.3);
            if let Some(ref params) = additional_params {
                builder = builder.additional_params(params.clone());
            }
            builder.build()
        } else {
            let mut builder = client
                .agent(model)
                .preamble(&full_preamble)
                .temperature(0.3);
            if let Some(ref params) = additional_params {
                builder = builder.additional_params(params.clone());
            }
            builder.build()
        }
    }
}


