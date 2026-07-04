use std::{
    error::Error,
    fmt::{self, Display},
    io,
};

/// Errors that can occur when running a linter subprocess.
#[derive(Debug)]
pub enum LinterError {
    /// The subprocess could not be spawned or communicated with.
    SubprocessFailed(io::Error),
    /// The linter exited with a non-zero exit code.
    NonZeroExit(i32, String),
    /// The linter did not complete within the configured timeout.
    TimeoutElapsed,
    /// The linter output could not be parsed into [`Finding`] values.
    ParseFailed(String),
}

impl Display for LinterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SubprocessFailed(e) => write!(f, "linter subprocess failed: {e}"),
            Self::NonZeroExit(code, stderr) => {
                write!(f, "linter exited with code {code}: {stderr}")
            }
            Self::TimeoutElapsed => write!(f, "linter timed out"),
            Self::ParseFailed(reason) => {
                write!(f, "failed to parse linter output: {reason}")
            }
        }
    }
}

impl Error for LinterError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::SubprocessFailed(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for LinterError {
    fn from(e: io::Error) -> Self {
        Self::SubprocessFailed(e)
    }
}

/// Errors that can occur when running a git subprocess.
#[derive(Debug)]
pub enum GitError {
    /// The git command could not be spawned or communicated with.
    CommandFailed(io::Error),
    /// Git exited with a non-zero exit code.
    NonZeroExit(i32, String),
    /// The git operation did not complete within the configured timeout.
    TimeoutElapsed,
}

impl Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CommandFailed(e) => write!(f, "git command failed: {e}"),
            Self::NonZeroExit(code, stderr) => {
                write!(f, "git exited with code {code}: {stderr}")
            }
            Self::TimeoutElapsed => write!(f, "git operation timed out"),
        }
    }
}

impl Error for GitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::CommandFailed(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for GitError {
    fn from(e: io::Error) -> Self {
        Self::CommandFailed(e)
    }
}

/// Errors that can occur when loading linter configuration.
#[derive(Debug)]
pub enum ConfigError {
    /// The configuration file could not be read.
    IoError(io::Error),
    /// The configuration file could not be parsed as TOML.
    ParseError(String),
    /// The configuration failed validation.
    ValidationError(String),
}

impl Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IoError(e) => write!(f, "config I/O error: {e}"),
            Self::ParseError(reason) => write!(f, "config parse error: {reason}"),
            Self::ValidationError(reason) => write!(f, "config validation error: {reason}"),
        }
    }
}

impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::IoError(e) => Some(e),
            _ => None,
        }
    }
}

/// Errors from MCP tool operations.
#[derive(Debug)]
pub enum McpError {
    /// Server connection or transport error.
    TransportError(String),
    /// The MCP tool call returned an error.
    ToolError(String),
    /// Configuration error.
    ConfigError(String),
    /// Request timed out.
    TimeoutElapsed,
}

impl fmt::Display for McpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TransportError(e) => write!(f, "MCP transport error: {e}"),
            Self::ToolError(e) => write!(f, "MCP tool error: {e}"),
            Self::ConfigError(e) => write!(f, "MCP config error: {e}"),
            Self::TimeoutElapsed => write!(f, "MCP request timed out"),
        }
    }
}

impl std::error::Error for McpError {}
