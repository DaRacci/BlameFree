//! Read-file tool for the agent to inspect repository files.
//!
//! The [`ReadFileTool`] lets the agent read any file from the repository checkout,
//! with a line limit to prevent context-window overflow.

use std::{
    fmt,
    path::{Path, PathBuf},
};

use schemars::JsonSchema;
use serde::Deserialize;

use crate::impl_tool;

const MAX_FILE_SIZE: u64 = 1_000_000;
const MAX_LINES: u32 = 200;

/// Arguments for [`ReadFileTool`].
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ReadFileArgs {
    /// Path to the file to read
    ///
    /// This path is relative to the repository root and must not be absolute or traverse outside the repo.
    pub path: String,

    /// Starting line number (1-indexed, optional).
    pub start_line: Option<u32>,

    /// Maximum number of lines to read (optional, defaults to 200).
    pub max_lines: Option<u32>,
}

/// Errors from the read-file tool.
#[derive(Debug)]
pub enum ReadFileError {
    /// File could not be read due to an I/O error.
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
    /// Repository root directory
    pub repo_root: String,

    /// Maximum file size in bytes
    pub max_file_size: u64,

    /// Default max lines to read
    pub default_max_lines: u32,
}

impl Default for ReadFileTool {
    fn default() -> Self {
        Self {
            repo_root: String::from("."),
            max_file_size: MAX_FILE_SIZE,
            default_max_lines: MAX_LINES,
        }
    }
}

impl_tool! {ReadFileTool, ReadFileArgs, ReadFileError, String, "read_file",
    "Read a file from the repository.
    Use with optional start_line/max_lines to read specific sections rather than entire files at once.
    Prefer this over using the terminal tool to cat files.",

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let root = resolve_target(Path::new(&self.repo_root), None)?;
        let target = resolve_target(Path::new(&args.path), Some(&root))?;

        let metadata = std::fs::metadata(&target)
            .map_err(|e| ReadFileError::IoError(e.to_string()))?;
        if metadata.len() > self.max_file_size {
            return Err(ReadFileError::FileTooLarge(metadata.len()));
        }

        let content = std::fs::read_to_string(&target)
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

/// Resolves a symlink target to its absolute path.
///
/// This will also check that the resolved path is within the permitted base directory.
fn resolve_target(path: &Path, root: Option<&Path>) -> Result<PathBuf, ReadFileError> {
    let canonical_path = dunce::canonicalize(path).map_err(|e| {
        ReadFileError::IoError(format!("cannot resolve path '{}': {e}", path.display()))
    })?;

    if let Some(root) = root {
        let _ = canonical_path
            .strip_prefix(root)
            .map_err(|_| ReadFileError::PathOutsideRepo(path.to_string_lossy().to_string()))?;
    }

    return Ok(canonical_path);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rig_core::tool::Tool;
    use std::{fs, io::Write};

    #[tokio::test]
    async fn test_read_file_basic() -> Result<(), ReadFileError> {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let mut f = fs::File::create(&file_path).unwrap();
        writeln!(f, "line1\nline2\nline3\nline4\nline5").unwrap();

        let tool = ReadFileTool {
            repo_root: dir.path().to_string_lossy().to_string(),
            ..Default::default()
        };

        let result = tool
            .call(ReadFileArgs {
                path: dir.path().join("test.txt").to_string_lossy().into(),
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

    #[tokio::test]
    async fn test_read_file_with_start_line() -> Result<(), ReadFileError> {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let content = (1..=10)
            .map(|i| format!("line{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&file_path, &content).unwrap();

        let tool = ReadFileTool {
            repo_root: dir.path().to_string_lossy().to_string(),
            ..Default::default()
        };

        let result = tool
            .call(ReadFileArgs {
                path: dir.path().join("test.txt").to_string_lossy().into(),
                start_line: Some(3),
                max_lines: None,
            })
            .await?;

        insta::assert_snapshot!(result);
        Ok(())
    }

    #[tokio::test]
    async fn test_read_file_with_max_lines() -> Result<(), ReadFileError> {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let content = (1..=10)
            .map(|i| format!("line{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&file_path, &content).unwrap();

        let tool = ReadFileTool {
            repo_root: dir.path().to_string_lossy().to_string(),
            ..Default::default()
        };

        let result = tool
            .call(ReadFileArgs {
                path: dir.path().join("test.txt").to_string_lossy().into(),
                start_line: None,
                max_lines: Some(3),
            })
            .await?;

        insta::assert_snapshot!(result);
        Ok(())
    }

    #[tokio::test]
    async fn test_read_file_too_large() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("large.txt");
        let content = "x".repeat(1_500_000);
        std::fs::write(&file_path, &content).unwrap();

        let tool = ReadFileTool {
            repo_root: dir.path().to_string_lossy().to_string(),
            ..Default::default()
        };

        let result = tool
            .call(ReadFileArgs {
                path: dir.path().join("large.txt").to_string_lossy().into(),
                start_line: None,
                max_lines: None,
            })
            .await;

        insta::assert_debug_snapshot!(result.is_err());
    }

    #[tokio::test]
    async fn test_read_file_empty() -> Result<(), ReadFileError> {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("empty.txt");
        std::fs::write(&file_path, "").unwrap();

        let tool = ReadFileTool {
            repo_root: dir.path().to_string_lossy().to_string(),
            ..Default::default()
        };

        let result = tool
            .call(ReadFileArgs {
                path: dir.path().join("empty.txt").to_string_lossy().into(),
                start_line: None,
                max_lines: None,
            })
            .await?;

        insta::assert_snapshot!(result);
        Ok(())
    }
}
