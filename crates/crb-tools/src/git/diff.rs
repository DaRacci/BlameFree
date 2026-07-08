use rig_core::{completion::ToolDefinition, tool::Tool};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::error::GitError;

use super::runner::run_git_command;

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
        let range = format!("{}...{}", args.base, args.head);
        run_git_command(&args.repo_path, &["diff", &range, "--no-color"]).await
    }
}
