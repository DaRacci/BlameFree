# error-types Specification

## Purpose
Typed error types for tool execution, defining a standard error enum implementing std::error::Error with Send + Sync bounds.
## Requirements
### Requirement: LinterError Type
The system SHALL define a typed `LinterError` enum implementing `std::error::Error` with `Send + Sync` bounds.

#### Scenario: Subprocess failure
- GIVEN a linter subprocess that cannot be spawned
- WHEN the linter tool attempts execution
- THEN the system SHALL return `LinterError::SubprocessFailed(std::io::Error)`

#### Scenario: Non-zero exit
- GIVEN a linter that exits with a non-zero exit code
- WHEN the linter tool captures the exit
- THEN the system SHALL return `LinterError::NonZeroExit(i32, String)` containing the exit code and stderr

#### Scenario: Timeout
- GIVEN a linter that exceeds the configured timeout duration
- WHEN the timeout elapses
- THEN the system SHALL return `LinterError::TimeoutElapsed`

#### Scenario: Parse failure
- GIVEN linter output that cannot be parsed into `Finding` structs
- WHEN the output parser fails
- THEN the system SHALL return `LinterError::ParseFailed(String)` with the reason

### Requirement: GitError Type
The system SHALL define a typed `GitError` enum implementing `std::error::Error` with `Send + Sync` bounds.

#### Scenario: Command failure
- GIVEN a git command that cannot be spawned or communicated with
- WHEN the git tool attempts execution
- THEN the system SHALL return `GitError::CommandFailed(std::io::Error)`

#### Scenario: Non-zero exit
- GIVEN a git command that exits with a non-zero exit code
- WHEN the git tool captures the exit
- THEN the system SHALL return `GitError::NonZeroExit(i32, String)` containing the exit code and stderr

#### Scenario: Timeout elapsed
- GIVEN a git operation that exceeds the timeout duration
- WHEN the timeout elapses
- THEN the system SHALL return `GitError::TimeoutElapsed`

### Requirement: ConfigError Type
The system SHALL define a typed `ConfigError` enum for TOML configuration errors.

#### Scenario: IO error
- GIVEN a config file that cannot be read due to an IO error
- WHEN `load_linter_config()` is called
- THEN the system SHALL return `ConfigError::IoError(std::io::Error)`

#### Scenario: Parse error
- GIVEN a config file with invalid TOML syntax
- WHEN `load_linter_config()` attempts to parse it
- THEN the system SHALL return `ConfigError::ParseError(String)`

#### Scenario: Validation error
- GIVEN a config file that passes TOML parsing but fails validation rules
- WHEN the system validates the parsed config
- THEN the system SHALL return `ConfigError::ValidationError(String)`

### Requirement: Error Recovery Strategy
The system SHALL apply standardized recovery strategies for each error type variant.

#### Scenario: Retry on subprocess failure
- GIVEN a `LinterError::SubprocessFailed` for a non-optional linter
- WHEN the error is raised
- THEN the system SHALL retry once after a 500ms backoff

#### Scenario: Parse on linter exit code 1
- GIVEN a linter that exits with code 1 (lint found issues)
- WHEN the linter tool receives exit code 1
- THEN the system SHALL still attempt to parse stdout for findings

#### Scenario: Timeout recovery
- GIVEN a `LinterError::TimeoutElapsed`
- WHEN the error is raised
- THEN the system SHALL log a warning and suggest increasing the timeout configuration

