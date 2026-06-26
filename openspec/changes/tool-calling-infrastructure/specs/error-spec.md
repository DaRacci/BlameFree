# Error Type Specification

## Type: Interface Spec

## 1. Design Principles

- Every tool has a dedicated error enum implementing `std::error::Error`.
- Errors are `Send + Sync` (required by `rig::tool::Tool` bound).
- The `Display` impl produces actionable, human-readable messages.
- The `Error::source()` chain preserves the original error when applicable.

## 2. LinterError

```rust
#[derive(Debug)]
pub enum LinterError {
    /// The subprocess could not be spawned or communicated with.
    SubprocessFailed(std::io::Error),

    /// The linter exited with a non-zero exit code.
    /// Contains (exit_code, stderr_string).
    NonZeroExit(i32, String),

    /// The linter did not complete within the configured timeout.
    TimeoutElapsed,

    /// The linter output could not be parsed into Findings.
    ParseFailed(String),
}

impl std::fmt::Display for LinterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SubprocessFailed(e) => {
                write!(f, "linter subprocess failed: {e}")
            }
            Self::NonZeroExit(code, stderr) => {
                write!(f, "linter exited with code {code}: {stderr}")
            }
            Self::TimeoutElapsed => {
                write!(f, "linter timed out")
            }
            Self::ParseFailed(reason) => {
                write!(f, "failed to parse linter output: {reason}")
            }
        }
    }
}

impl std::error::Error for LinterError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::SubprocessFailed(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for LinterError {
    fn from(e: std::io::Error) -> Self {
        Self::SubprocessFailed(e)
    }
}
```

## 3. GitError

```rust
#[derive(Debug)]
pub enum GitError {
    /// The git command could not be spawned or communicated with.
    CommandFailed(std::io::Error),

    /// Git exited with a non-zero exit code.
    /// Contains (exit_code, stderr_string).
    NonZeroExit(i32, String),

    /// The git operation did not complete within the configured timeout.
    TimeoutElapsed,
}

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CommandFailed(e) => {
                write!(f, "git command failed: {e}")
            }
            Self::NonZeroExit(code, stderr) => {
                write!(f, "git exited with code {code}: {stderr}")
            }
            Self::TimeoutElapsed => {
                write!(f, "git operation timed out")
            }
        }
    }
}

impl std::error::Error for GitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CommandFailed(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for GitError {
    fn from(e: std::io::Error) -> Self {
        Self::CommandFailed(e)
    }
}
```

## 4. ConfigError

```rust
#[derive(Debug)]
pub enum ConfigError {
    /// File could not be read.
    IoError(std::io::Error),
    /// TOML could not be parsed.
    ParseError(String),
    /// Validation failed.
    ValidationError(String),
}

impl std::fmt::Display for ConfigError { /* ... */ }
impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::IoError(e) => Some(e),
            _ => None,
        }
    }
}
```

## 5. Error Recovery Strategy

| Error | Recovery |
|---|---|
| `LinterError::SubprocessFailed` | Retry once after 500ms. If still fails and `optional=true`, skip. |
| `LinterError::NonZeroExit` | Log stderr. If exit code is 1 (lint found issues), parse stdout anyway. |
| `LinterError::TimeoutElapsed` | Increase timeout config and retry. Log warning. |
| `LinterError::ParseFailed` | Return raw stdout as a single "unparseable output" finding. |
| `GitError::CommandFailed` | Check git is installed. Surface to user. |
| `GitError::NonZeroExit` | Surface stderr to user. |
| `GitError::TimeoutElapsed` | Suggest splitting diff or running locally. |
