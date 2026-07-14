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
