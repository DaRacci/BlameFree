//! List-directory tool for exploring repository structure.
//!
//! The [`ListDirTool`] lets the agent list the contents of a directory,
//! returning filenames with indicators: directories show '/', files show
//! ' (file)'.

use std::fmt;

use rig_core::tool::Tool;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::impl_tool;

/// Arguments for [`ListDirTool`].
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ListDirArgs {
    /// Path to the directory to list (relative to the repo root).
    pub path: String,

    /// Maximum number of items to return (optional).
    pub max_items: Option<usize>,
}

/// Errors from the list-dir tool.
#[derive(Debug)]
pub enum ListDirError {
    /// Directory could not be read.
    IoError(String),
}

impl fmt::Display for ListDirError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IoError(e) => write!(f, "list directory error: {e}"),
        }
    }
}

impl std::error::Error for ListDirError {}

/// Tool that lists the contents of a directory (non-recursive).
///
/// Returns filenames with '/' appended to directory names, sorted by name.
pub struct ListDirTool {
    /// Working directory (repo root).
    pub workdir: String,
}

impl_tool! {ListDirTool, ListDirArgs, ListDirError, String, "list_dir",
    "List the contents of a directory. Returns filenames with indicators: directories show '/', files show ' (file)'.",
    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        // TODO:  check to ensure that the path does not contain '..' to prevent directory traversal attacks.
        if args.path.starts_with('/') {
            return Err(ListDirError::IoError(format!(
                "absolute paths not allowed: {}",
                args.path
            )));
        }

        let workdir = self.workdir.clone();
        let path = args.path.clone();
        let max_items = args.max_items;

        let entries = tokio::task::spawn_blocking(move || {
            let dir_path = format!("{}/{}", workdir.trim_end_matches('/'), path);
            let read_dir =
                std::fs::read_dir(&dir_path).map_err(|e| ListDirError::IoError(e.to_string()))?;

            let mut names: Vec<String> = read_dir
                .filter_map(|entry| {
                    let entry = entry.ok()?;
                    let name = entry.file_name().into_string().ok()?;
                    let path = entry.path();
                    if path.is_dir() {
                        Some(format!("{name}/"))
                    } else {
                        Some(format!("{name} (file)"))
                    }
                })
                .collect();

            names.sort();

            if let Some(max) = max_items {
                names.truncate(max);
            }

            Ok(names.join("\n"))
        })
        .await
        .map_err(|e| ListDirError::IoError(e.to_string()))??;

        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_list_dir_error_display() {
        let err = ListDirError::IoError("not found".into());
        assert_eq!(err.to_string(), "list directory error: not found");
    }

    #[tokio::test]
    async fn test_list_dir_basic() -> Result<(), ListDirError> {
        let dir = tempfile::tempdir().unwrap();

        // Create some files and directories
        std::fs::create_dir(dir.path().join("subdir")).unwrap();
        let mut f = std::fs::File::create(dir.path().join("file1.txt")).unwrap();
        writeln!(f, "content").unwrap();
        std::fs::File::create(dir.path().join("file2.txt")).unwrap();

        let tool = ListDirTool {
            workdir: dir.path().to_string_lossy().to_string(),
        };

        let result = tool
            .call(ListDirArgs {
                path: ".".into(),
                max_items: None,
            })
            .await?;

        assert!(result.contains("file1.txt (file)"));
        assert!(result.contains("file2.txt (file)"));
        assert!(result.contains("subdir/"));
        Ok(())
    }

    #[tokio::test]
    async fn test_list_dir_with_max_items() -> Result<(), ListDirError> {
        let dir = tempfile::tempdir().unwrap();

        std::fs::File::create(dir.path().join("a.txt")).unwrap();
        std::fs::File::create(dir.path().join("b.txt")).unwrap();
        std::fs::File::create(dir.path().join("c.txt")).unwrap();

        let tool = ListDirTool {
            workdir: dir.path().to_string_lossy().to_string(),
        };

        let result = tool
            .call(ListDirArgs {
                path: ".".into(),
                max_items: Some(2),
            })
            .await?;

        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn test_list_dir_not_found() {
        let tool = ListDirTool {
            workdir: "/nonexistent".to_string(),
        };

        let result = tool
            .call(ListDirArgs {
                path: ".".into(),
                max_items: None,
            })
            .await;

        assert!(result.is_err());
    }
}
