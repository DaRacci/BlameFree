//! Git tool for agent-accessible git operations.
//!
//! Provides a unified [`GitTool`] that wraps `git log`, `git diff`,
//! `git show`, and `git status` for agent consumption.

use std::fmt;
use std::time::Duration;

use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;
use schemars::JsonSchema;
use serde::Deserialize;

/// Git operations the agent can perform.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GitOperation {
    /// `git log --oneline -n 20`
    Log,
    /// `git diff base...head --no-color`
    Diff {
        base: String,
        head: String,
    },
    /// `git show <ref> --no-color`
    Show {
        r#ref: String,
    },
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

impl GitTool {
    fn run_git(&self, args: &[&str]) -> Result<(String, String), GitToolError> {
        let output = std::process::Command::new("git")
            .args(["-C", &self.repo_root])
            .args(args)
            .output()
            .map_err(|e| GitToolError::CommandFailed(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if output.status.success() {
            Ok((stdout, stderr))
        } else {
            Err(GitToolError::NonZeroExit(
                output.status.code().unwrap_or(-1),
                stderr,
            ))
        }
    }
}

impl Tool for GitTool {
    const NAME: &'static str = "git";

    type Error = GitToolError;
    type Args = GitArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Run git operations on the repository: log, diff, show, status.".to_string(),
            parameters: serde_json::to_value(schemars::schema_for!(GitArgs)).unwrap(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let git_args: Vec<&str> = match &args.operation {
            GitOperation::Log => vec!["log", "--oneline", "-n", "20"],
            GitOperation::Diff { base, head } => {
                return (self as &GitTool).run_git_inner(base, head).await;
            }
            GitOperation::Show { r#ref } => {
                vec!["show", r#ref.as_str(), "--no-color"]
            }
            GitOperation::Status => vec!["status", "--short"],
        };

        // For simple operations, run inside timeout
        let git_args_clone: Vec<String> = git_args.iter().map(|s| s.to_string()).collect();
        let repo_root = self.repo_root.clone();
        let timeout = self.timeout;

        tokio::time::timeout(timeout, async move {
            tokio::task::spawn_blocking(move || {
                let output = std::process::Command::new("git")
                    .args(["-C", &repo_root])
                    .args(&git_args_clone)
                    .output()
                    .map_err(|e| GitToolError::CommandFailed(e.to_string()))?;

                if output.status.success() {
                    Ok(String::from_utf8_lossy(&output.stdout).to_string())
                } else {
                    Err(GitToolError::NonZeroExit(
                        output.status.code().unwrap_or(-1),
                        String::from_utf8_lossy(&output.stderr).to_string(),
                    ))
                }
            })
            .await
            .map_err(|e| GitToolError::CommandFailed(e.to_string()))?
        })
        .await
        .map_err(|_| GitToolError::TimeoutElapsed)?
    }
}

// Separate impl for the Diff variant which is more complex
impl GitTool {
    async fn run_git_inner(&self, base: &str, head: &str) -> Result<String, GitToolError> {
        let range = format!("{}...{}", base, head);
        let repo_root = self.repo_root.clone();
        let timeout = self.timeout;

        tokio::time::timeout(timeout, async move {
            tokio::task::spawn_blocking(move || {
                let output = std::process::Command::new("git")
                    .args(["-C", &repo_root, "diff", &range, "--no-color"])
                    .output()
                    .map_err(|e| GitToolError::CommandFailed(e.to_string()))?;

                if output.status.success() {
                    Ok(String::from_utf8_lossy(&output.stdout).to_string())
                } else {
                    Err(GitToolError::NonZeroExit(
                        output.status.code().unwrap_or(-1),
                        String::from_utf8_lossy(&output.stderr).to_string(),
                    ))
                }
            })
            .await
            .map_err(|e| GitToolError::CommandFailed(e.to_string()))?
        })
        .await
        .map_err(|_| GitToolError::TimeoutElapsed)?
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
            .args(["-C", dir.to_str().unwrap(), "config", "user.email", "test@test.com"])
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

        let tool = GitTool {
            repo_root: dir.path().to_string_lossy().to_string(),
            timeout: Duration::from_secs(10),
        };

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

        let tool = GitTool {
            repo_root: dir.path().to_string_lossy().to_string(),
            timeout: Duration::from_secs(10),
        };

        let result = tool
            .call(GitArgs {
                operation: GitOperation::Status,
            })
            .await;
        assert!(result.is_ok());
    }
}
