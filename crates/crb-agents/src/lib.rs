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

pub use crate::manifest::AgentManifest;

use crate::prompts::PromptLibrary;

/// Build a rig agent for the given role using the embedded prompt library.
///
/// The role's preamble is resolved through the [`PromptLibrary`], which loads
/// prompts from the embedded `include_dir!` prompts directory at compile time.
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
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn build_agent(
    client: &openai::Client,
    model: &str,
    role: &str,
    rules_preamble: Option<&str>,
    prompt_lib: &PromptLibrary,
    template_vars: Option<&HashMap<String, serde_json::Value>>,
    extra_preamble: Option<&str>,
    workdir: Option<&str>,
    additional_params: Option<serde_json::Value>,
    #[cfg(feature = "exp14_submit_finding")] collector: Option<
        Arc<Mutex<submit_finding::SubmitFindingCollector>>,
    >,
) -> Agent<ResponsesCompletionModel> {
    let empty_map = HashMap::new();
    let vars: HashMap<String, serde_json::Value> = template_vars.cloned().unwrap_or(empty_map);
    let role_preamble = prompt_lib.render(role, vars);
    let mut full_preamble = match rules_preamble {
        Some(rp) if !rp.is_empty() => format!("{rp}\n\n{role_preamble}"),
        _ => role_preamble.to_string(),
    };

    if let Some(extra) = extra_preamble {
        if !extra.is_empty() {
            full_preamble = format!("{full_preamble}\n\n{extra}");
        }
    }

    // Build agent with tools.
    // Note: AgentBuilder's ToolState generic changes when .tool() is called
    // (NoToolConfig -> WithBuilderTools), so each tool combination needs its
    // own builder chain.
    #[cfg(feature = "exp14_submit_finding")]
    {
        let has_wd = workdir.is_some();
        let has_collector = collector.is_some();

        match (has_wd, has_collector) {
            (true, true) => {
                let wd = workdir.unwrap().to_string();
                let submit_tool = submit_finding::SubmitFindingTool::new(collector.unwrap());
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
                    .tool(crb_tools::grep::GrepTool {
                        workdir: wd.clone(),
                    })
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
                    .tool(crb_tools::grep::GrepTool {
                        workdir: wd.clone(),
                    })
                    .tool(crb_tools::list_dir::ListDirTool { workdir: wd })
                    .default_max_turns(6)
                    .temperature(0.3);
                if let Some(ref params) = additional_params {
                    builder = builder.additional_params(params.clone());
                }
                builder.build()
            }
            (false, true) => {
                let submit_tool = submit_finding::SubmitFindingTool::new(collector.unwrap());
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
                .tool(crb_tools::grep::GrepTool {
                    workdir: wd.clone(),
                })
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
