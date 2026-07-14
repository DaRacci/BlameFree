//! [`rig_core::tool::Tool`] implementations

pub mod budget;
pub mod context;
pub mod error;
pub mod filesystem;
pub mod git;
pub mod linters;
pub mod macros;
pub mod mcp;
pub mod shell;

use crate::filesystem::{grep::GrepTool, list_dir::ListDirTool, read_file::ReadFileTool};
use crate::shell::ShellTool;
use rig_core::tool::server::ToolServer;
use schemars::JsonSchema;
use serde::Deserialize;
use std::process::Command;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct WorkingDirectoryArg {
    /// The root directory to run executables in.
    pub workdir: String,
}

pub fn build_tool_server(workdir: Option<&str>) -> ToolServer {
    let mut tool_server = ToolServer::new();
    if let Some(workdir) = workdir {
        tool_server = tool_server
            .tool(ReadFileTool {
                repo_root: workdir.to_string(),
                ..Default::default()
            })
            .tool(ShellTool {
                work_dir: workdir.to_string(),
                ..Default::default()
            })
            .tool(GrepTool {
                workdir: workdir.to_string(),
            })
            .tool(ListDirTool {
                workdir: workdir.to_string(),
            });
    }

    tool_server
}

/// Check if a binary is available on `$PATH`.
pub fn check_binary_exists(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
