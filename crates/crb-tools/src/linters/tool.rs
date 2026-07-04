use std::{io, time::Duration};

use crb_shared::finding::Finding;
use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio::process::Command;

use crate::error::LinterError;

/// Arguments accepted by every linter tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct LinterArgs {
    /// Absolute or relative filesystem path to the repository root.
    pub repo_path: String,
}

/// A generic linter tool wrapping any CLI linter.
///
/// Parameterized by the command to run, the output parser function, and a
/// per-invocation timeout.
pub struct LinterTool {
    /// Display name for the linter.
    pub name: String,

    /// Command and initial arguments (never a shell string).
    pub cmd: Vec<String>,

    /// Function that parses linter stdout into [`Finding`] values.
    pub parser: fn(&str) -> Result<Vec<Finding>, LinterError>,

    /// Per-invocation timeout.
    pub timeout: Duration,
}

impl LinterTool {
    /// Convert a `JoinError` from a panicked spawned task into an `io::Error`.
    fn join_error_to_io(e: tokio::task::JoinError) -> io::Error {
        io::Error::other(e.to_string())
    }
}

impl Tool for LinterTool {
    const NAME: &'static str = "linter";

    type Error = LinterError;
    type Args = LinterArgs;
    type Output = Vec<Finding>;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name.clone(),
            description: format!("Run `{}` linter on a repository", self.name),
            parameters: serde_json::to_value(schemars::schema_for!(LinterArgs)).unwrap_or_default(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let cmd = self.cmd.clone();
        let repo_path = args.repo_path;
        let timeout = self.timeout;

        let result = tokio::time::timeout(timeout, async {
            tokio::task::spawn_blocking(move || {
                Command::new(&cmd[0])
                    .args(&cmd[1..])
                    .arg(&repo_path)
                    .output()
            })
            .await
            .map_err(|join_err| LinterError::SubprocessFailed(Self::join_error_to_io(join_err)))?
            .await
            .map_err(LinterError::SubprocessFailed)
        })
        .await
        .map_err(|_| LinterError::TimeoutElapsed)??;

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr).to_string();
            return Err(LinterError::NonZeroExit(
                result.status.code().unwrap_or(-1),
                stderr,
            ));
        }

        let stdout = String::from_utf8_lossy(&result.stdout).to_string();
        (self.parser)(&stdout)
    }
}
