//! Git tool for agent-accessible git operations.
//!
//! Provides a unified [`GitTool`] that wraps `git log`, `git diff`,
//! `git show`, and `git status` for agent consumption.

pub mod clean;
pub mod diff;

use std::fmt;
use std::time::Duration;

use rig_core::tool::Tool;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::impl_tool;
use super::runner::run_git_command;
use crate::error::GitError;

/// Git operations the agent can perform.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GitOperation {
    /// `git log --oneline -n 20`
    Log,

    /// `git diff base...head --no-color`
    Diff { base: String, head: String },

    /// `git show <ref> --no-color`
    Show { r#ref: String },

    /// `git status --short`
    Status,
}

/// Arguments for [`GitTool`].
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct GitArgs {
    /// The git operation to perform.
    pub operation: GitOperation,
}

/// Errors from git tool execution.
#[derive(Debug)]
pub enum GitToolError {
    /// Git command could not be spawned.
    CommandFailed(String),

    /// Git exited with non-zero exit code.
    NonZeroExit(i32, String),

    /// Operation exceeded timeout.
    TimeoutElapsed,
}

impl fmt::Display for GitToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CommandFailed(e) => write!(f, "git command failed: {e}"),
            Self::NonZeroExit(code, stderr) => write!(f, "git exited with code {code}: {stderr}"),
            Self::TimeoutElapsed => write!(f, "git operation timed out"),
        }
    }
}

impl std::error::Error for GitToolError {}

impl From<GitError> for GitToolError {
    fn from(e: GitError) -> Self {
        match e {
            GitError::CommandFailed(io_err) => Self::CommandFailed(io_err.to_string()),
            GitError::NonZeroExit(code, msg) => Self::NonZeroExit(code, msg),
            GitError::TimeoutElapsed => Self::TimeoutElapsed,
        }
    }
}

/// Tool that provides git operations for the agent.
///
/// Supports log, diff, show, and status operations with path-safety
/// (rejects commands outside the repo root).
pub struct GitTool {
    /// Repository root directory.
    pub repo_root: String,

    /// Per-invocation timeout.
    pub timeout: Duration,
}

impl Default for GitTool {
    fn default() -> Self {
        Self {
            repo_root: String::from("."),
            timeout: Duration::from_secs(30),
        }
    }
}

impl_tool! {GitTool, GitArgs, GitToolError, String, "git",
    "Run git operations on the repository: log, diff, show, status.",
    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        match &args.operation {
            GitOperation::Log => {
                run_git_command(&self.repo_root, &["log", "--oneline", "-n", "20"])
                    .await
                    .map_err(GitToolError::from)
            }
            GitOperation::Diff { base, head } => {
                let range = format!("{}...{}", base, head);
                run_git_command(&self.repo_root, &["diff", &range, "--no-color"])
                    .await
                    .map_err(GitToolError::from)
            }
            GitOperation::Show { r#ref } => {
                run_git_command(&self.repo_root, &["show", r#ref.as_str(), "--no-color"])
                    .await
                    .map_err(GitToolError::from)
            }
            GitOperation::Status => {
                run_git_command(&self.repo_root, &["status", "--short"])
                    .await
                    .map_err(GitToolError::from)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn init_test_repo(dir: &std::path::Path) {
        Command::new("git")
            .args(["init", "--initial-branch=main"])
            .arg(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args([
                "-C",
                dir.to_str().unwrap(),
                "config",
                "user.email",
                "test@test.com",
            ])
            .output()
            .unwrap();
        Command::new("git")
            .args(["-C", dir.to_str().unwrap(), "config", "user.name", "Test"])
            .output()
            .unwrap();
        std::fs::write(dir.join("file.txt"), b"hello").unwrap();
        Command::new("git")
            .args(["-C", dir.to_str().unwrap(), "add", "."])
            .output()
            .unwrap();
        Command::new("git")
            .args(["-C", dir.to_str().unwrap(), "commit", "-m", "initial"])
            .output()
            .unwrap();
    }

    fn make_git_tool(repo_root: &std::path::Path) -> GitTool {
        GitTool {
            repo_root: repo_root.to_string_lossy().to_string(),
            timeout: Duration::from_secs(10),
        }
    }

    #[test]
    fn test_git_error_display() {
        let err = GitToolError::TimeoutElapsed;
        assert_eq!(err.to_string(), "git operation timed out");

        let err = GitToolError::NonZeroExit(128, "not a repo".into());
        assert_eq!(err.to_string(), "git exited with code 128: not a repo");
    }

    #[tokio::test]
    async fn test_git_log() {
        let dir = tempfile::tempdir().unwrap();
        init_test_repo(dir.path());

        let tool = make_git_tool(dir.path());

        let result = tool
            .call(GitArgs {
                operation: GitOperation::Log,
            })
            .await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("initial"));
    }

    #[tokio::test]
    async fn test_git_status() {
        let dir = tempfile::tempdir().unwrap();
        init_test_repo(dir.path());

        let tool = make_git_tool(dir.path());

        let result = tool
            .call(GitArgs {
                operation: GitOperation::Status,
            })
            .await;
        assert!(result.is_ok());
    }
}
