use std::{io, time::Duration};

use rig_core::{completion::ToolDefinition, tool::Tool};
use schemars::JsonSchema;
use serde::Deserialize;
use tokio::process::Command;

use crate::error::GitError;

/// Arguments for [`GitDiffTool`].
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct GitDiffArgs {
    /// Absolute or relative path to the repository.
    pub repo_path: String,

    /// Base git ref (e.g. "main", "origin/main", "HEAD~1").
    pub base: String,

    /// Head git ref (e.g. "feature-branch", "HEAD").
    pub head: String,
}

/// Tool that runs `git diff base...head --no-color`.
pub struct GitDiffTool;

impl Tool for GitDiffTool {
    const NAME: &'static str = "git_diff";

    type Error = GitError;
    type Args = GitDiffArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Get the diff between two git refs (git diff base...head)".to_string(),
            parameters: serde_json::to_value(schemars::schema_for!(GitDiffArgs))
                .unwrap_or_default(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let repo_path = args.repo_path;
        let range = format!("{}...{}", args.base, args.head);

        let result = tokio::time::timeout(Duration::from_secs(60), async {
            tokio::task::spawn_blocking(move || {
                Command::new("git")
                    .args(["-C", &repo_path, "diff", &range, "--no-color"])
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
}
