use std::{io, process::Command, time::Duration};

use rig_core::{completion::ToolDefinition, tool::Tool};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::error::GitError;

/// Arguments for [`GitCleanTool`].
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct GitCleanArgs {
    /// Absolute or relative path to the repository.
    pub repo_path: String,
}

/// Tool that runs `git clean -fdx` to remove untracked files.
pub struct GitCleanTool;

impl Tool for GitCleanTool {
    const NAME: &'static str = "git_clean";

    type Error = GitError;
    type Args = GitCleanArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Remove untracked files from a git repository (git clean -fdx)"
                .to_string(),
            parameters: serde_json::to_value(schemars::schema_for!(GitCleanArgs))
                .unwrap_or_default(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let repo_path = args.repo_path;

        let result = tokio::time::timeout(Duration::from_secs(60), async {
            tokio::task::spawn_blocking(move || {
                Command::new("git")
                    .args(["-C", &repo_path, "clean", "-fdx"])
                    .output()
            })
            .await
            .map_err(|join_err| GitError::CommandFailed(io::Error::other(join_err.to_string())))?
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
