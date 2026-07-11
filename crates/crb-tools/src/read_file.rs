//! Read-file tool for the agent to inspect repository files.
//!
//! The [`ReadFileTool`] lets the agent read any file from the repository
//! checkout, with a line limit to prevent context-window overflow.

use std::fmt;

use rig_core::tool::Tool;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::impl_tool;

/// Arguments for [`ReadFileTool`].
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ReadFileArgs {
    /// Path to the file to read (relative to repo root).
    pub path: String,

    /// Starting line number (1-indexed, optional).
    pub start_line: Option<u32>,

    /// Maximum number of lines to read (optional, defaults to 200).
    pub max_lines: Option<u32>,
}

/// Errors from the read-file tool.
#[derive(Debug)]
pub enum ReadFileError {
    /// File could not be read (not found, permissions, etc.).
    IoError(String),
    /// Path is outside the allowed repository root.
    PathOutsideRepo(String),
    /// File is too large.
    FileTooLarge(u64),
}

impl fmt::Display for ReadFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IoError(e) => write!(f, "read file error: {e}"),
            Self::PathOutsideRepo(p) => write!(f, "path is outside the repo root: {p}"),
            Self::FileTooLarge(s) => write!(f, "file too large: {s} bytes (max 1MB)"),
        }
    }
}

impl std::error::Error for ReadFileError {}

/// Tool that reads a file from the repository, with path-safety checks.
///
/// The tool:
/// - Canonicalises the requested path and checks it's under `repo_root`.
/// - Rejects files larger than 1 MB.
/// - Supports line-range reading via `start_line` and `max_lines`.
pub struct ReadFileTool {
    /// Repository root directory (all paths must be under this).
    pub repo_root: String,
    /// Maximum file size in bytes (default: 1 MB).
    pub max_file_size: u64,
    /// Default max lines to read (default: 200).
    pub default_max_lines: u32,
}

impl Default for ReadFileTool {
    fn default() -> Self {
        Self {
            repo_root: String::from("."),
            max_file_size: 1_000_000,
            default_max_lines: 200,
        }
    }
}

impl_tool! {ReadFileTool, ReadFileArgs, ReadFileError, String, "read_file",
    "Read a file from the repository. Use with optional start_line/max_lines to read specific sections rather than entire files at once. Prefer this over using the terminal tool to cat files.",
    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        // Reject absolute paths
        // TODO: check against path traversal (e.g., `..`) in addition to canonicalization
        if args.path.starts_with('/') {
            return Err(ReadFileError::IoError(format!(
                "absolute paths not allowed: {}",
                args.path
            )));
        }

        let repo_root = dunce::canonicalize(&self.repo_root)
            .map_err(|e| ReadFileError::IoError(e.to_string()))?;
        let target = repo_root.join(&args.path);

        // ensure path is within repo root
        let target_canonical = dunce::canonicalize(&target).map_err(|e| {
            ReadFileError::IoError(format!("cannot resolve path '{}': {e}", args.path))
        })?;

        if !target_canonical.starts_with(&repo_root) {
            return Err(ReadFileError::PathOutsideRepo(args.path.clone()));
        }

        let metadata = std::fs::metadata(&target_canonical)
            .map_err(|e| ReadFileError::IoError(e.to_string()))?;
        if metadata.len() > self.max_file_size {
            return Err(ReadFileError::FileTooLarge(metadata.len()));
        }

        let content = std::fs::read_to_string(&target_canonical)
            .map_err(|e| ReadFileError::IoError(e.to_string()))?;

        let start = args.start_line.unwrap_or(1).max(1) as usize - 1;
        let max_lines = args.max_lines.unwrap_or(self.default_max_lines).max(50) as usize;

        let lines: Vec<&str> = content.lines().skip(start).take(max_lines).collect();
        let result = lines.join("\n");

        if lines.len() < content.lines().count() - start {
            Ok(format!(
                "{result}\n... (showing {} of {} lines)",
                lines.len(),
                content.lines().count()
            ))
        } else {
            Ok(result)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_read_file_error_display() {
        let err = ReadFileError::IoError("not found".into());
        assert!(err.to_string().contains("not found"));

        let err = ReadFileError::PathOutsideRepo("../../etc/passwd".into());
        assert!(err.to_string().contains("outside the repo root"));

        let err = ReadFileError::FileTooLarge(2_000_000);
        assert!(err.to_string().contains("too large"));
    }

    #[tokio::test]
    async fn test_read_file_basic() -> Result<(), ReadFileError> {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "line1\nline2\nline3\nline4\nline5").unwrap();

        let tool = ReadFileTool {
            repo_root: dir.path().to_string_lossy().to_string(),
            ..Default::default()
        };

        let result = tool
            .call(ReadFileArgs {
                path: "test.txt".into(),
                start_line: None,
                max_lines: None,
            })
            .await?;

        assert!(result.contains("line1"));
        assert!(result.contains("line5"));
        Ok(())
    }

    #[tokio::test]
    async fn test_read_file_outside_repo() {
        let tool = ReadFileTool {
            repo_root: String::from("/tmp"),
            ..Default::default()
        };

        let result = tool
            .call(ReadFileArgs {
                path: "../etc/passwd".into(),
                start_line: None,
                max_lines: None,
            })
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            ReadFileError::IoError(_) | ReadFileError::PathOutsideRepo(_) => {} // acceptable
            other => panic!("expected IoError or PathOutsideRepo, got {other:?}"),
        }
    }
}
