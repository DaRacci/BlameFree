# Design: Tool Calling Infrastructure

## 1. Overview

The review harness wraps external CLI tools (linters, git commands) as Rig
`Tool` trait implementations. This enables:

- Typed input/output schemas (auto-generated JSON Schema for LLM consumption).
- Concurrent execution via tokio.
- Uniform error handling via custom error types implementing `std::error::Error`.
- Configurable tool definitions via TOML files (no code changes to add a new linter).

## 2. Core Patterns

### 2.1 LinterTool Pattern

All linters share a common struct. The linter command, name, and output parser
are injected at construction time.

```rust
use rig::tool::Tool;
use schemars::JsonSchema;
use serde::Deserialize;
use async_trait::async_trait;
use std::time::Duration;

/// Arguments accepted by every linter tool.
#[derive(Deserialize, JsonSchema)]
struct LinterArgs {
    /// Absolute or relative path to the repository to lint.
    repo_path: String,
}

/// A single finding from a linter run.
#[derive(Serialize, Debug)]
struct Finding {
    severity: String,   // "error" | "warning" | "info"
    path: String,       // file path relative to repo root
    line: u32,
    column: u32,
    message: String,
    code: Option<String>, // linter-specific rule code
}

/// Error type for linter operations.
#[derive(Debug)]
enum LinterError {
    SubprocessFailed(std::io::Error),
    NonZeroExit(i32, String),   // exit code + stderr
    TimeoutElapsed,
    ParseFailed(String),        // reason
}

impl std::error::Error for LinterError {}
impl std::fmt::Display for LinterError { /* ... */ }
impl From<std::io::Error> for LinterError { /* ... */ }

/// Generic linter tool wrapping any CLI linter.
struct LinterTool {
    name: &'static str,
    cmd: Vec<String>,
    parser: fn(&str) -> Result<Vec<Finding>, LinterError>,
    timeout: Duration,
}

#[async_trait]
impl Tool for LinterTool {
    const NAME: &'static str = "ruff"; // overridden per-instance via constructor

    type Error = LinterError;
    type Args = LinterArgs;
    type Output = Vec<Finding>;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name.into(),
            description: format!("Run `{}` linter on a repository", self.name),
            parameters: serde_json::to_value(self.args_schema()).unwrap(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let result = tokio::time::timeout(self.timeout, async {
            tokio::process::Command::new(&self.cmd[0])
                .args(&self.cmd[1..])
                .arg(&args.repo_path)
                .output()
                .await
                .map_err(LinterError::SubprocessFailed)
        })
        .await
        .map_err(|_| LinterError::TimeoutElapsed)??;

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr).to_string();
            return Err(LinterError::NonZeroExit(
                result.status.code().unwrap_or(-1),
                stderr,
            ));
        }

        (self.parser)(&String::from_utf8_lossy(&result.stdout))
    }
}
```

### 2.2 Concrete Linter Instances

```rust
/// Factory: create a RuffTool from config.
fn create_ruff_tool(config: &LinterConfig) -> LinterTool {
    LinterTool {
        name: config.name,       // "ruff"
        cmd: config.cmd.clone(), // ["ruff", "check"]
        parser: parse_ruff_output,
        timeout: Duration::from_secs(config.timeout_secs.unwrap_or(60)),
    }
}

fn parse_ruff_output(stdout: &str) -> Result<Vec<Finding>, LinterError> {
    // Ruff outputs JSON lines: {"code": "F841", "location": {"row": 10, "col": 5}, ...
    // or traditional format: path:line:col: code message
    serde_json::from_str::<Vec<RuffJsonFinding>>(stdout)
        .map(|items| {
            items
                .into_iter()
                .map(|f| Finding {
                    severity: "error".into(),
                    path: f.filename,
                    line: f.location.row,
                    column: f.location.column,
                    message: f.message,
                    code: Some(f.code),
                })
                .collect()
        })
        .map_err(|e| LinterError::ParseFailed(e.to_string()))
}

fn parse_eslint_output(stdout: &str) -> Result<Vec<Finding>, LinterError> {
    // ESLint JSON output: [{"filePath": "...", "messages": [{"ruleId": "...", ...}]}]
    // Similar conversion to Finding struct.
    todo!()
}

fn parse_govet_output(stdout: &str) -> Result<Vec<Finding>, LinterError> {
    // go vet outputs: ./path/file.go:line:col: message
    // Parse each line with regex.
    todo!()
}
```

### 2.3 GitTool Pattern

Git operations are simpler: no output parser needed, just command execution.
We use `std::process::Command` (not `tokio::process::Command`) for git because
git operations are fast and `tokio::process` carries overhead. For long-running
git operations (large clone), wrap in `tokio::task::spawn_blocking`.

```rust
#[derive(Deserialize, JsonSchema)]
struct GitCleanArgs {
    repo_path: String,
}

#[derive(Deserialize, JsonSchema)]
struct GitDiffArgs {
    repo_path: String,
    base: String,   // base ref (e.g. "main")
    head: String,   // head ref (e.g. "feature-branch")
}

#[derive(Debug)]
enum GitError {
    CommandFailed(std::io::Error),
    NonZeroExit(i32, String), // exit code + stderr
    TimeoutElapsed,
}

impl std::error::Error for GitError {}
impl std::fmt::Display for GitError { /* ... */ }

struct GitCleanTool;

#[async_trait]
impl Tool for GitCleanTool {
    const NAME: &'static str = "git_clean";
    type Error = GitError;
    type Args = GitCleanArgs;
    type Output = String; // stdout of the command

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        tokio::time::timeout(Duration::from_secs(120), async {
            tokio::task::spawn_blocking(move || {
                std::process::Command::new("git")
                    .args(["-C", &args.repo_path, "clean", "-fdx"])
                    .output()
                    .map_err(GitError::CommandFailed)
            })
            .await
            .map_err(|e| GitError::CommandFailed(std::io::Error::new(std::io::ErrorKind::Other, e)))?
        })
        .await
        .map_err(|_| GitError::TimeoutElapsed)?
        .and_then(|output| {
            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).to_string())
            } else {
                Err(GitError::NonZeroExit(
                    output.status.code().unwrap_or(-1),
                    String::from_utf8_lossy(&output.stderr).to_string(),
                ))
            }
        })
    }
}

struct GitDiffTool;

#[async_trait]
impl Tool for GitDiffTool {
    const NAME: &'static str = "git_diff";
    type Error = GitError;
    type Args = GitDiffArgs;
    type Output = String; // unified diff

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        tokio::time::timeout(Duration::from_secs(60), async {
            tokio::task::spawn_blocking(move || {
                std::process::Command::new("git")
                    .args([
                        "-C", &args.repo_path,
                        "diff",
                        &format!("{}...{}", args.base, args.head),
                        "--no-color",
                    ])
                    .output()
                    .map_err(GitError::CommandFailed)
            })
            .await
            .map_err(|e| GitError::CommandFailed(std::io::Error::new(std::io::ErrorKind::Other, e)))?
        })
        .await
        .map_err(|_| GitError::TimeoutElapsed)?
        .and_then(|output| {
            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).to_string())
            } else {
                Err(GitError::NonZeroExit(
                    output.status.code().unwrap_or(-1),
                    String::from_utf8_lossy(&output.stderr).to_string(),
                ))
            }
        })
    }
}
```

## 3. TOML Linter Configuration

Linters are defined in a TOML file (e.g., `linters.toml`) alongside the harness
config. No code changes needed to add a new linter.

```toml
[linters.ruff]
name = "ruff"
cmd = ["ruff", "check"]
timeout_secs = 60
output_format = "json"

[linters.eslint]
name = "eslint"
cmd = ["npx", "eslint", "--format", "json"]
timeout_secs = 90
output_format = "json"

[linters.govet]
name = "go vet"
cmd = ["go", "vet", "./..."]
timeout_secs = 120
output_format = "text"
```

## 4. Module Structure

```
crb-harness/src/tools/
├── mod.rs
│   // Re-exports public types: LinterTool, LinterArgs, LinterError, Finding
│   // Re-exports: GitCleanTool, GitDiffTool, GitCleanArgs, GitDiffArgs, GitError
│   // Re-exports: load_linter_config()
│
├── linter.rs
│   // LinterArgs, Finding, LinterError
│   // LinterTool struct + Tool impl
│   // LinterConfig struct + load_linter_config()
│   // parse_ruff_output(), parse_eslint_output(), parse_govet_output()
│   // create_ruff_tool(), create_eslint_tool(), create_govet_tool()
│   // Tests: each parser with known-good and known-bad input
│
└── git.rs
    // GitCleanArgs, GitDiffArgs, GitError
    // GitCleanTool, GitDiffTool structs + Tool impls
    // Tests: git operations against a temp repo
```

## 5. Error Handling Strategy

| Error Variant | When Raised | Recovery |
|---|---|---|
| `LinterError::SubprocessFailed` | IO error spawning/communicating | Retry after brief backoff |
| `LinterError::NonZeroExit` | Linter exited non-zero (with stderr) | Inspect stderr; may indicate broken repo |
| `LinterError::TimeoutElapsed` | Subprocess exceeded timeout | Increase timeout or split input |
| `LinterError::ParseFailed` | Parser could not interpret stdout | Fall back to raw stdout string |
| `GitError::CommandFailed` | IO error running git | Check git installation |
| `GitError::NonZeroExit` | Git command failed | Check repo state |
| `GitError::TimeoutElapsed` | Git operation timed out | Likely large repo; increase timeout |

## 6. Concurrent Execution

Multiple linter tools run concurrently via `tokio::join!` or `tokio::try_join!`:

```rust
async fn run_all_linters(repo_path: &str) -> Vec<(String, Result<Vec<Finding>, LinterError>)> {
    let config = load_linter_config("linters.toml").unwrap();
    let mut handles = Vec::new();

    for linter in config.linters.values() {
        let tool = create_linter_tool(linter);
        let args = LinterArgs { repo_path: repo_path.into() };
        handles.push(tokio::spawn(async move {
            (tool.name, tool.call(args).await)
        }));
    }

    let mut results = Vec::new();
    for handle in handles {
        results.push(handle.await.unwrap());
    }
    results
}
```

## 7. Non-Tool Functions

Not every function needs to be a `Tool`. Internal orchestration functions remain
plain Rust:

- `load_linter_config(path: &str) -> Result<LinterConfig, ConfigError>`
- `run_all_linters(repo_path: &str) -> Vec<(String, Result<Vec<Finding>>)>`
- `aggregate_results(results: Vec<(String, Vec<Finding>)>) -> LintReport`

These benefit from separate module organization in `crb-harness/src/`.
