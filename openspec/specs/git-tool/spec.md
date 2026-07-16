# git-tool Specification

## Purpose
Git operations (clean, diff, checkout) wrapped as tool implementations for agent-driven repository manipulation during review.
## Requirements
### Requirement: GitCleanTool
The system SHALL provide a git clean tool as a `rig::tool::Tool` implementation.

#### Scenario: Clean repository
- GIVEN a repository with untracked files and directories
- WHEN `GitCleanTool::call()` is invoked with valid `GitCleanArgs`
- THEN the system SHALL run `git -C {repo_path} clean -fdx`
- AND it SHALL remove all untracked files and directories
- AND it SHALL return the command stdout as a `String`

### Requirement: GitDiffTool
The system SHALL provide a git diff tool as a `rig::tool::Tool` implementation.

#### Scenario: Generate diff
- GIVEN a repository with a base ref and a head ref
- WHEN `GitDiffTool::call()` is invoked with valid `GitDiffArgs`
- THEN the system SHALL run `git -C {repo_path} diff {base}...{head} --no-color`
- AND it SHALL return the unified diff output as a `String`

### Requirement: Execution Strategy
The system SHALL execute git operations via `std::process::Command` wrapped in `tokio::task::spawn_blocking` with timeouts.

#### Scenario: Spawn blocking
- GIVEN any git tool invocation
- WHEN the tool runs
- THEN the git command SHALL execute on a blocking thread via `tokio::task::spawn_blocking`
- AND the async runtime SHALL NOT be blocked

#### Scenario: Timeout enforcement
- GIVEN a git operation that exceeds 60 seconds (120 seconds for clean)
- WHEN the timeout elapses
- THEN the system SHALL return `GitError::TimeoutElapsed`

### Requirement: GitError Type
The system SHALL define a `GitError` enum for typed git operation errors implementing `std::error::Error` and `std::fmt::Display`.

#### Scenario: Command failure
- GIVEN a git command that cannot be spawned
- WHEN the IO error occurs
- THEN the system SHALL return `GitError::CommandFailed(std::io::Error)`

#### Scenario: Non-zero exit
- GIVEN a git command that exits with a non-zero exit code
- WHEN the git tool captures the exit
- THEN the system SHALL return `GitError::NonZeroExit(i32, String)` containing the exit code and stderr

