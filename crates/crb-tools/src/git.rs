//! Git tool for agent-accessible git operations.

use std::io;
use std::time::Duration;

use schemars::JsonSchema;
use serde::Deserialize;
use tokio::process::Command;

use crate::error::GitError;
use crate::impl_tool;

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

impl_tool! {GitTool, GitArgs, GitError, String, "git",
    "Run git operations on the repository: log, diff, show, status.",
    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        match &args.operation {
            GitOperation::Log => run_git_command(&self.repo_root, &["log", "--oneline", "-n", "20"]).await,
            GitOperation::Diff { base, head } => {
                let range = format!("{}...{}", base, head);
                run_git_command(&self.repo_root, &["diff", &range, "--no-color"]).await
            }
            GitOperation::Show { r#ref } => run_git_command(&self.repo_root, &["show", r#ref.as_str(), "--no-color"]).await,
            GitOperation::Status => run_git_command(&self.repo_root, &["status", "--short"]).await,
        }
    }
}

/// Run a git subcommand in the given repository with a 60-second timeout.
///
/// Executes `git -C <repo_path> <args...>` via `spawn_blocking`, handles the
/// join error, the I/O error, the timeout, and extracts UTF-8 stdout on success
/// or returns a structured [`GitError`] on failure.
pub async fn run_git_command(repo_path: &str, args: &[&str]) -> Result<String, GitError> {
    let repo_path = repo_path.to_owned();
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();

    let result = tokio::time::timeout(Duration::from_secs(60), async move {
        tokio::task::spawn_blocking(move || {
            Command::new("git")
                .arg("-C")
                .arg(&repo_path)
                .args(&args)
                .output()
        })
        .await
        .map_err(|join_err| GitError::CommandFailed(io::Error::other(join_err.to_string())))?
        .await
        .map_err(GitError::CommandFailed)
    })
    .await
    .map_err(|_| GitError::TimeoutElapsed)??;

    if result.status.success() {
        Ok(String::from_utf8_lossy(&result.stdout).to_string())
    } else {
        Err(GitError::NonZeroExit(
            result.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&result.stderr).to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use rig_core::tool::Tool;

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
        let err = GitError::TimeoutElapsed;
        assert_eq!(err.to_string(), "git operation timed out");

        let err = GitError::NonZeroExit(128, "not a repo".into());
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
