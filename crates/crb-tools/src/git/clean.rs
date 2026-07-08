use rig_core::{completion::ToolDefinition, tool::Tool};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::error::GitError;

use super::runner::run_git_command;

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
        run_git_command(&args.repo_path, &["clean", "-fdx"]).await
    }
}
