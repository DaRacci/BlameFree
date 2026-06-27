//! Shell tool for running arbitrary shell commands.
//!
//! The [`ShellTool`] lets the agent execute shell commands on the repository
//! checkout.  Commands are run inside a timeout with output capped to avoid
//! blowing the context window.

use std::fmt;
use std::time::Duration;

use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;
use schemars::JsonSchema;
use serde::Deserialize;

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
    /// The command exited with non-zero status.
    NonZeroExit(i32, String),
    /// The command exceeded its time limit.
    TimeoutElapsed,
    /// Output exceeded max size.
    OutputTooLarge(usize),
}

impl fmt::Display for ShellError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SpawnFailed(e) => write!(f, "shell spawn failed: {e}"),
            Self::NonZeroExit(code, stderr) => {
                write!(f, "shell exited with code {code}: {stderr}")
            }
            Self::TimeoutElapsed => write!(f, "shell command timed out"),
            Self::OutputTooLarge(n) => {
                write!(f, "shell output too large: {n} bytes (max 100KB)")
            }
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

impl Tool for ShellTool {
    const NAME: &'static str = "shell";

    type Error = ShellError;
    type Args = ShellArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Run a shell command in the repository working directory. Use for building, testing, grepping, or any CLI operation.".to_string(),
            parameters: serde_json::to_value(schemars::schema_for!(ShellArgs)).unwrap(),
        }
    }

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
            .map_err(|join_err| {
                ShellError::SpawnFailed(join_err.to_string())
            })?
            .map_err(|io_err| ShellError::SpawnFailed(io_err.to_string()))
        })
        .await
        .map_err(|_| ShellError::TimeoutElapsed)??;

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr).to_string();
            return Err(ShellError::NonZeroExit(
                result.status.code().unwrap_or(-1),
                stderr,
            ));
        }

        let stdout = String::from_utf8_lossy(&result.stdout).to_string();
        if stdout.len() > max_output {
            return Err(ShellError::OutputTooLarge(stdout.len()));
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

        let err = ShellError::NonZeroExit(127, "not found".into());
        assert_eq!(err.to_string(), "shell exited with code 127: not found");

        let err = ShellError::OutputTooLarge(200_000);
        assert!(err.to_string().contains("output too large"));
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
        assert!(result.is_err());
        match result.unwrap_err() {
            ShellError::NonZeroExit(code, _) => assert_eq!(code, 42),
            other => panic!("expected NonZeroExit, got {other:?}"),
        }
    }
}
