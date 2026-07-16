use std::io;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum LinterError {
    #[error("subprocess could not be spawned or communicated with: {0}")]
    SubprocessFailed(io::Error),

    #[error("linter exited with code {0}: {1}")]
    NonZeroExit(i32, String),

    #[error("linter operation did not complete within the configured timeout")]
    TimeoutElapsed,

    #[error("failed to parse linter output: {0}")]
    ParseFailed(String),
}

#[derive(Error, Debug)]
pub enum GitError {
    #[error("subprocess could not be spawned or communicated with: {0}")]
    CommandFailed(io::Error),

    #[error("git exited with code {0}: {1}")]
    NonZeroExit(i32, String),

    #[error("git operation did not complete within the configured timeout")]
    TimeoutElapsed,
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("configuration file could not be read: {0}")]
    IoError(io::Error),

    #[error("configuration file could not be parsed: {0}")]
    ParseError(String),

    #[error("configuration file validation error: {0}")]
    ValidationError(String),
}

#[derive(Error, Debug)]
pub enum McpError {
    #[error("MCP transport error: {0}")]
    TransportError(String),

    #[error("MCP tool error: {0}")]
    ToolError(String),

    #[error("MCP configuration error: {0}")]
    ConfigError(String),

    #[error("MCP request timed out")]
    TimeoutElapsed,
}

#[derive(Error, Debug)]
pub enum GrepError {
    #[error("subprocess could not be spawned or communicated with: {0}")]
    CommandFailed(String),

    #[error("grep exited with code {0}: {1}")]
    NonZeroExit(i32, String),

    #[error("grep operation did not complete within the configured timeout")]
    TimeoutElapsed,
}

#[derive(Error, Debug)]
pub enum ListDirError {
    #[error("directory could not be read: {0}")]
    IoError(String),
}

#[derive(Error, Debug)]
pub enum ShellError {
    #[error("shell command could not be spawned: {0}")]
    SpawnFailed(String),

    #[error("shell command timed out")]
    TimeoutElapsed,
}

impl From<io::Error> for LinterError {
    fn from(e: io::Error) -> Self {
        Self::SubprocessFailed(e)
    }
}

impl From<io::Error> for GitError {
    fn from(e: io::Error) -> Self {
        Self::CommandFailed(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linter_error_displays() {
        let err1 = LinterError::SubprocessFailed(io::Error::new(io::ErrorKind::PermissionDenied, "denied"));
        insta::assert_snapshot!(err1.to_string());

        let err2 = LinterError::NonZeroExit(1, "syntax error".into());
        insta::assert_snapshot!(err2.to_string());

        let err3 = LinterError::TimeoutElapsed;
        insta::assert_snapshot!(err3.to_string());

        let err4 = LinterError::ParseFailed("unexpected token".into());
        insta::assert_snapshot!(err4.to_string());
    }

    #[test]
    fn test_git_error_displays() {
        let err1 = GitError::CommandFailed(io::Error::new(io::ErrorKind::NotFound, "git not found"));
        insta::assert_snapshot!(err1.to_string());

        let err2 = GitError::NonZeroExit(128, "fatal: not a git repository".into());
        insta::assert_snapshot!(err2.to_string());

        let err3 = GitError::TimeoutElapsed;
        insta::assert_snapshot!(err3.to_string());
    }

    #[test]
    fn test_config_error_displays() {
        let err1 = ConfigError::IoError(io::Error::new(io::ErrorKind::NotFound, "file not found"));
        insta::assert_snapshot!(err1.to_string());

        let err2 = ConfigError::ParseError("expected table at line 3".into());
        insta::assert_snapshot!(err2.to_string());

        let err3 = ConfigError::ValidationError("empty name field".into());
        insta::assert_snapshot!(err3.to_string());
    }

    #[test]
    fn test_mcp_error_displays() {
        let err1 = McpError::TransportError("connection refused".into());
        insta::assert_snapshot!(err1.to_string());

        let err2 = McpError::ToolError("tool not found: search".into());
        insta::assert_snapshot!(err2.to_string());

        let err3 = McpError::ConfigError("missing server URL".into());
        insta::assert_snapshot!(err3.to_string());

        let err4 = McpError::TimeoutElapsed;
        insta::assert_snapshot!(err4.to_string());
    }

    #[test]
    fn test_grep_error_displays() {
        let err1 = GrepError::CommandFailed("rg not found on PATH".into());
        insta::assert_snapshot!(err1.to_string());

        let err2 = GrepError::NonZeroExit(2, "error reading file".into());
        insta::assert_snapshot!(err2.to_string());

        let err3 = GrepError::TimeoutElapsed;
        insta::assert_snapshot!(err3.to_string());
    }

    #[test]
    fn test_list_dir_error_displays() {
        let err = ListDirError::IoError("permission denied".into());
        insta::assert_snapshot!(err.to_string());
    }

    #[test]
    fn test_shell_error_displays() {
        let err1 = ShellError::SpawnFailed("binary not found".into());
        insta::assert_snapshot!(err1.to_string());

        let err2 = ShellError::TimeoutElapsed;
        insta::assert_snapshot!(err2.to_string());
    }

    #[test]
    fn test_linter_error_from_io_error() {
        let io_err = io::Error::new(io::ErrorKind::ConnectionRefused, "connection reset");
        let linter_err: LinterError = io_err.into();
        insta::assert_snapshot!(linter_err.to_string());
    }

    #[test]
    fn test_git_error_from_io_error() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "no such file or directory");
        let git_err: GitError = io_err.into();
        insta::assert_snapshot!(git_err.to_string());
    }

    #[test]
    fn test_error_non_zero_exit_negative_code() {
        let err1 = LinterError::NonZeroExit(-1, "killed by signal".into());
        insta::assert_snapshot!(err1.to_string());

        let err2 = GitError::NonZeroExit(-2, "segfault".into());
        insta::assert_snapshot!(err2.to_string());

        let err3 = GrepError::NonZeroExit(-9, "SIGKILL".into());
        insta::assert_snapshot!(err3.to_string());
    }
}
