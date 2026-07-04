//! Grep tool for searching file contents with regex.
//!
//! The [`GrepTool`] lets the agent search file contents using `grep -rn`,
//! returning matching lines with file paths in `path:line:content` format.

use std::fmt;
use std::time::Duration;

use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;
use schemars::JsonSchema;
use serde::Deserialize;

/// Arguments for [`GrepTool`].
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct GrepArgs {
    /// Regex pattern to search for in file contents.
    pub pattern: String,

    /// Directory path to search in (relative to repo root, optional; defaults to repo root).
    pub path: Option<String>,

    /// Optional file glob pattern to filter which files to search (e.g. "*.rs").
    pub file_glob: Option<String>,
}

/// Errors from the grep tool.
#[derive(Debug)]
pub enum GrepError {
    /// The grep subprocess could not be spawned.
    CommandFailed(String),
    /// Grep exited with a non-zero exit status other than 1 (no matches).
    NonZeroExit(i32, String),
    /// The command exceeded its time limit.
    TimeoutElapsed,
}

impl fmt::Display for GrepError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CommandFailed(e) => write!(f, "grep command failed: {e}"),
            Self::NonZeroExit(code, stderr) => {
                write!(f, "grep exited with code {code}: {stderr}")
            }
            Self::TimeoutElapsed => write!(f, "grep command timed out"),
        }
    }
}

impl std::error::Error for GrepError {}

/// Tool that searches file contents using `grep -rn --no-messages`.
///
/// Output is capped at 50 KB. Exit code 1 (no matches) returns an empty
/// string rather than an error. Commands time out after 30 seconds.
pub struct GrepTool {
    /// Working directory (repo root) for the grep command.
    pub workdir: String,
}

impl GrepTool {
    const MAX_OUTPUT: usize = 50_000;
    const TIMEOUT: Duration = Duration::from_secs(8);
}

impl Tool for GrepTool {
    const NAME: &'static str = "grep";

    type Error = GrepError;
    type Args = GrepArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description:
                "Search file contents using regex. Returns matching lines with file paths in format \
                 path:line:content. Use before reading files to find relevant locations. Do NOT use \
                 the terminal tool for searches."
                    .to_string(),
            parameters: serde_json::to_value(schemars::schema_for!(GrepArgs)).unwrap_or_default(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let workdir = self.workdir.clone();
        let pattern = args.pattern.clone();
        // Reject absolute paths
        if let Some(ref p) = args.path {
            if p.starts_with('/') {
                return Err(GrepError::CommandFailed(format!(
                    "absolute paths not allowed: {p}"
                )));
            }
        }

        let search_path = args.path.clone().unwrap_or_default();
        let file_glob = args.file_glob.clone();

        let result = tokio::time::timeout(Self::TIMEOUT, {
            async move {
                tokio::task::spawn_blocking(move || {
                    let mut cmd = std::process::Command::new("grep");
                    cmd.arg("-rn")
                        .arg("--no-messages")
                        .arg(&pattern)
                        .current_dir(&workdir);

                    if !search_path.is_empty() {
                        cmd.arg(&search_path);
                    }
                    if let Some(glob) = &file_glob {
                        cmd.arg("--include").arg(glob);
                    }

                    let output = cmd
                        .output()
                        .map_err(|e| GrepError::CommandFailed(e.to_string()))?;

                    if !output.status.success() {
                        let code = output.status.code().unwrap_or(-1);
                        // grep exits 1 when no matches — treat as empty result, not an error
                        if code != 1 {
                            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                            return Err(GrepError::NonZeroExit(code, stderr));
                        }
                    }

                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    if stdout.len() > Self::MAX_OUTPUT {
                        Ok(format!(
                            "{}\n... (output truncated at {} bytes)",
                            &stdout[..Self::MAX_OUTPUT],
                            Self::MAX_OUTPUT
                        ))
                    } else {
                        Ok(stdout)
                    }
                })
                .await
                .map_err(|e| GrepError::CommandFailed(e.to_string()))?
            }
        })
        .await
        .map_err(|_| GrepError::TimeoutElapsed)??;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_grep_error_display() {
        let err = GrepError::TimeoutElapsed;
        assert_eq!(err.to_string(), "grep command timed out");

        let err = GrepError::CommandFailed("permission denied".into());
        assert_eq!(err.to_string(), "grep command failed: permission denied");

        let err = GrepError::NonZeroExit(2, "error".into());
        assert_eq!(err.to_string(), "grep exited with code 2: error");
    }

    #[tokio::test]
    async fn test_grep_finds_pattern() -> Result<(), GrepError> {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "hello world\nfoo bar\nbaz qux").unwrap();

        let tool = GrepTool {
            workdir: dir.path().to_string_lossy().to_string(),
        };

        let result = tool
            .call(GrepArgs {
                pattern: "hello".into(),
                path: None,
                file_glob: None,
            })
            .await?;

        assert!(result.contains("hello"));
        Ok(())
    }

    #[tokio::test]
    async fn test_grep_no_matches() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "hello world").unwrap();

        let tool = GrepTool {
            workdir: dir.path().to_string_lossy().to_string(),
        };

        let result = tool
            .call(GrepArgs {
                pattern: "nonexistent".into(),
                path: None,
                file_glob: None,
            })
            .await;

        // Exit code 1 (no matches) should produce empty string, not an error
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "");
    }
}
