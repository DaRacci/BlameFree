//! Tool implementations for the code review benchmark harness.
//!
//! This crate provides Rig `Tool` trait implementations for:
//!
//! - **Agent tools** — [`ShellTool`], [`ReadFileTool`], [`GitTool`], [`MCPTool`]
//!   for LLM-agent-in-the-loop tool calling.
//! - **Linter tools** — [`LinterTool`] (generic), with parsers for ruff (JSON),
//!   ESLint (JSON), `go vet` (text).
//! - **Git tools** — [`GitCleanTool`], [`GitDiffTool`] for pre-review git operations.
//! - **Budgets** — [`ToolCallBudget`] / [`ToolCallTracker`] for limiting tool usage.
//!
//! # Per-role tool assignment
//!
//! [`tools_for_role()`] returns the set of [`Tool`] instances appropriate for a
//! given reviewer role (SA, CL, AR, SEC).  [`tool_prompt_section()`] renders the
//! tool-calling preamble for inclusion in the agent's system prompt.

pub mod budget;
pub mod error;
pub mod git;
pub mod grep;
#[cfg(feature = "exp14_template_vars")]
pub mod language_detector;
pub mod linters;
pub mod list_dir;
pub mod mcp;
pub mod read_file;
pub mod shell;
#[cfg(feature = "exp14_submit_finding")]
pub mod submit_finding;

use crate::linters::config::{LinterConfig, OutputFormat};
use crate::linters::eslint::parse_eslint_output;
use crate::linters::govet::parse_govet_output;
use crate::linters::ruff::parse_ruff_output;
use crate::linters::tool::LinterTool;
use std::process::Command;
use std::time::Duration;

use crb_shared::finding::Finding;
pub use grep::GrepTool;
pub use linters::config::load_linter_config;
pub use list_dir::ListDirTool;

use crate::budget::ToolCallBudget;
use crate::error::LinterError;

/// Internal helper to create a [`LinterTool`] from a [`LinterConfig`] and a parser function.
fn create_linter_tool_inner(
    config: &LinterConfig,
    parser: fn(&str) -> Result<Vec<Finding>, LinterError>,
) -> LinterTool {
    LinterTool {
        name: config.name.clone(),
        cmd: config.cmd.clone(),
        parser,
        timeout: Duration::from_secs(config.timeout_secs.unwrap_or(60)),
    }
}

/// Create a [`LinterTool`] for ruff from its configuration.
pub fn create_ruff_tool(config: &LinterConfig) -> LinterTool {
    create_linter_tool_inner(config, parse_ruff_output)
}

/// Create a [`LinterTool`] for ESLint from its configuration.
pub fn create_eslint_tool(config: &LinterConfig) -> LinterTool {
    create_linter_tool_inner(config, parse_eslint_output)
}

/// Create a [`LinterTool`] for `go vet` from its configuration.
pub fn create_govet_tool(config: &LinterConfig) -> LinterTool {
    create_linter_tool_inner(config, parse_govet_output)
}

/// Create a [`LinterTool`] from configuration, selecting the parser based on
/// the `output_format` field.
pub fn create_linter_tool(config: &LinterConfig) -> LinterTool {
    let parser: fn(&str) -> Result<Vec<Finding>, LinterError> = match config.output_format {
        OutputFormat::Json => parse_ruff_output,
        OutputFormat::Text => parse_govet_output,
    };
    create_linter_tool_inner(config, parser)
}

/// Check if a binary is available on `$PATH`.
pub fn check_binary_exists(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Returns the tool names appropriate for a given reviewer role.
///
/// Each role gets a different set of tools based on what it needs to do:
/// - **SA** (Static Analysis): shell, read_file
/// - **CL** (Code Logic): shell, read_file, git
/// - **AR** / **ARCH** (Architecture): shell, read_file, git
/// - **SEC** (Security): shell, read_file, git
pub fn tools_for_role(role: &str) -> Vec<&'static str> {
    match role {
        "CL" | "AR" | "ARCH" | "SEC" => vec!["shell", "read_file", "git"],
        _ => vec!["shell", "read_file"],
    }
}

/// Renders the tool-calling preamble section for inclusion in an LLM
/// agent system prompt.
///
/// This tells the agent what tools are available, how to use them, and
/// what budget constraints apply.
///
/// If `mcp_tool_names` is non-empty, those MCP tool names are appended
/// to the available-tools list so the agent knows about them.
pub fn tool_prompt_section(
    role: &str,
    budget: &ToolCallBudget,
    mcp_tool_names: &[String],
) -> String {
    let tool_names = tools_for_role(role);

    let tools_description = tool_names.join(", ");
    let call_limit = budget.max_per_tool;

    let mcp_section = if mcp_tool_names.is_empty() {
        String::new()
    } else {
        format!(
            "\nMCP tools available:\n{}\n",
            mcp_tool_names
                .iter()
                .map(|n| format!("- **{n}**: An MCP (Model Context Protocol) tool. Use it by calling it with JSON arguments as specified by its tool definition."))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };

    format!(
        "You have access to the following tools during this review: {tools_description}.\
{mcp_section}

Tool usage rules:
- Each tool invocation returns text output. Use tools to inspect files, run commands, or check git history.
- You may call tools multiple times but are limited to a total of {call_limit} calls per tool and {} overall.
- If a tool fails, try again with different arguments or skip that check.
- Use `read_file` to examine specific files, `shell` to run commands like grep/build/tests, and `git` to inspect commit history or diffs.
- Keep your tool usage targeted and efficient — prefer `read_file` over `shell cat`.

Available tools:
- **read_file**: Read a file from the repository. Specify path (relative to repo root), optional start_line (1-indexed), and optional max_lines.
- **shell**: Run a shell command in the repository working directory. Use for building, testing, grepping, or any CLI operation.
- **git**: Run git operations on the repository: log, diff, show, status.

Use tools by calling them with JSON arguments as specified by each tool's definition.",
        budget.max_total_calls,
    )
}

#[cfg(test)]
mod tests {
    use crate::error::GitError;

    use super::*;

    #[test]
    fn test_create_ruff_tool() {
        let config = LinterConfig {
            name: "ruff".to_string(),
            cmd: vec!["ruff".to_string(), "check".to_string()],
            timeout_secs: Some(60),
            output_format: OutputFormat::Json,
            environment: None,
            optional: Some(false),
        };
        let tool = create_ruff_tool(&config);
        assert_eq!(tool.name, "ruff");
        assert_eq!(tool.cmd, vec!["ruff", "check"]);
        assert_eq!(tool.timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_create_eslint_tool() {
        let config = LinterConfig {
            name: "eslint".to_string(),
            cmd: vec!["npx".to_string(), "eslint".to_string()],
            timeout_secs: Some(90),
            output_format: OutputFormat::Json,
            environment: None,
            optional: Some(true),
        };
        let tool = create_eslint_tool(&config);
        assert_eq!(tool.name, "eslint");
        assert_eq!(tool.timeout, Duration::from_secs(90));
    }

    #[test]
    fn test_create_govet_tool() {
        let config = LinterConfig {
            name: "go vet".to_string(),
            cmd: vec!["go".to_string(), "vet".to_string(), "./...".to_string()],
            timeout_secs: None,
            output_format: OutputFormat::Text,
            environment: None,
            optional: Some(true),
        };
        let tool = create_govet_tool(&config);
        assert_eq!(tool.name, "go vet");
        // Default timeout when None
        assert_eq!(tool.timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_linter_error_from_io() {
        let io_err = std::io::Error::other("oops");
        let linter_err: LinterError = io_err.into();
        match linter_err {
            LinterError::SubprocessFailed(_) => {}
            other => panic!("expected SubprocessFailed, got {other:?}"),
        }
    }

    #[test]
    fn test_git_error_from_io() {
        let io_err = std::io::Error::other("oops");
        let git_err: GitError = io_err.into();
        match git_err {
            GitError::CommandFailed(_) => {}
            other => panic!("expected CommandFailed, got {other:?}"),
        }
    }
}
