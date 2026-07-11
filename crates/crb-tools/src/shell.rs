//! Shell tool for running arbitrary shell commands.
//!
//! The [`ShellTool`] lets the agent execute shell commands on the repository
//! checkout.  Commands are run inside a timeout with output capped to avoid
//! blowing the context window.

use std::fmt;
use std::time::Duration;

/// Maximum output size in bytes (100 KB). Output beyond this is truncated.
const MAX_OUTPUT: usize = 100_000;

use rig_core::tool::Tool;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::impl_tool;

/// Arguments for [`ShellTool`].
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ShellArgs {
    /// Shell command to run (e.g. "cat Cargo.toml", "make build").
    pub command: String,
}

/// Errors from shell tool execution.
#[derive(Debug)]
pub enum ShellError {
    /// The subprocess could not be spawned.
    SpawnFailed(String),
    /// The command exceeded its time limit.
    TimeoutElapsed,
}

impl fmt::Display for ShellError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SpawnFailed(e) => write!(f, "shell spawn failed: {e}"),
            Self::TimeoutElapsed => write!(f, "shell command timed out"),
        }
    }
}

impl std::error::Error for ShellError {}

/// Tool that runs a shell command via `sh -c`.
///
/// Commands timeout after 30 seconds.  Output is capped at 100 KB.
pub struct ShellTool {
    /// Working directory for the command.
    pub work_dir: String,
    /// Per-invocation timeout.
    pub timeout: Duration,
    /// Max output bytes (capped to avoid context overflow).
    pub max_output: usize,
}

impl Default for ShellTool {
    fn default() -> Self {
        Self {
            work_dir: String::from("."),
            timeout: Duration::from_secs(30),
            max_output: 100_000,
        }
    }
}

impl_tool! {ShellTool, ShellArgs, ShellError, String, "shell",
    "Execute a shell command in the repository working directory. Use for running tests, builds, linters, or any CLI operation. Output is capped at 100KB; very long output will be truncated with a note. IMPORTANT: Do NOT use this for reading files (use read_file), searching code (use grep), or listing directories (use list_dir).",
    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let cmd = args.command;
        let work_dir = self.work_dir.clone();
        let max_output = self.max_output;

        let result = tokio::time::timeout(self.timeout, async {
            tokio::task::spawn_blocking(move || {
                std::process::Command::new("sh")
                    .args(["-c", &cmd])
                    .current_dir(&work_dir)
                    .output()
            })
            .await
            .map_err(|join_err| ShellError::SpawnFailed(join_err.to_string()))?
            .map_err(|io_err| ShellError::SpawnFailed(io_err.to_string()))
        })
        .await
        .map_err(|_| ShellError::TimeoutElapsed)??;

        if !result.status.success() {
            let code = result.status.code().unwrap_or(-1);
            let mut output = String::new();
            if !result.stdout.is_empty() {
                output = String::from_utf8_lossy(&result.stdout).to_string();
            }
            let stderr = String::from_utf8_lossy(&result.stderr).to_string();
            if !output.is_empty() || !stderr.is_empty() {
                let sep = if !output.is_empty() && !stderr.is_empty() {
                    "\n"
                } else {
                    ""
                };
                output = format!("{}{}{}", output, sep, stderr);
            }
            output = format!("{}\n[exit code: {code}]", output);
            if output.len() > max_output {
                output = format!(
                    "{}\n... (output truncated at {} bytes)",
                    &output[..MAX_OUTPUT],
                    MAX_OUTPUT
                );
            }
            return Ok(output);
        }

        let mut stdout = String::from_utf8_lossy(&result.stdout).to_string();
        if stdout.len() > max_output {
            stdout = format!(
                "{}\n... (output truncated at {} bytes)",
                &stdout[..MAX_OUTPUT],
                MAX_OUTPUT
            );
        }

        Ok(stdout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_error_display() {
        let err = ShellError::TimeoutElapsed;
        assert_eq!(err.to_string(), "shell command timed out");
    }

    #[tokio::test]
    async fn test_shell_echo() {
        let tool = ShellTool {
            work_dir: String::from("."),
            ..Default::default()
        };
        let result = tool
            .call(ShellArgs {
                command: "echo hello".into(),
            })
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().trim(), "hello");
    }

    #[tokio::test]
    async fn test_shell_failure() {
        let tool = ShellTool::default();
        let result = tool
            .call(ShellArgs {
                command: "exit 42".into(),
            })
            .await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("[exit code: 42]"));
    }
}
