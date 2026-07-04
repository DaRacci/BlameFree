//! Tool implementations for the code review benchmark harness.
//!
//! This crate provides Rig `Tool` trait implementations for:
//!
//! - **Agent tools** — [`ShellTool`], [`ReadFileTool`], [`GitTool`], [`MCPTool`]
//!   for LLM-agent-in-the-loop tool calling.
//! - **Linter tools** — [`LinterTool`] (generic), with parsers for ruff (JSON),
//!   ESLint (JSON), `go vet` (text).
//! - **Git tools** — [`GitCleanTool`], [`GitDiffTool`] for pre-review git operations.
//! - **Budgets** — [`ToolCallBudget`] / [`ToolCallTracker`] for limiting tool usage.
//!
//! # Per-role tool assignment
//!
//! [`tools_for_role()`] returns the set of [`Tool`] instances appropriate for a
//! given reviewer role (SA, CL, AR, SEC).  [`tool_prompt_section()`] renders the
//! tool-calling preamble for inclusion in the agent's system prompt.

pub mod budget;
#[cfg(feature = "exp14_template_vars")]
pub mod language_detector;
pub mod git;
pub mod grep;
pub mod list_dir;
pub mod mcp;
pub mod mcp_config;
pub mod read_file;
pub mod shell;

use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;
use std::time::Duration;

pub use grep::GrepTool;
pub use list_dir::ListDirTool;
use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ── Finding type ────────────────────────────────────────────────────────────

/// A structured finding returned by an agent.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Finding {
    pub file: Option<String>,
    pub line: Option<u32>,
    pub message: String,
    pub severity: String,
    pub rule_code: Option<String>,
    /// Whether the severity has been audited/downgraded by the severity auditor.
    #[serde(default)]
    pub severity_audited: bool,
    /// Reason for the severity audit result (e.g., downgrade category, protection reason).
    #[serde(default)]
    pub severity_audit_reason: Option<String>,
    /// Evidence supporting the finding (command output, code snippet, etc.).
    #[serde(default)]
    pub evidence: Option<String>,
    /// Path trace / call chain showing how the issue was reached.
    #[serde(default)]
    pub path_trace: Option<String>,
    /// Confidence level: CONFIRMED, LIKELY, or UNCERTAIN.
    #[serde(default)]
    pub confidence: Option<String>,
    /// Agent tag that found this issue (SA, CL, AR, SEC, or custom).
    #[serde(default)]
    pub found_by: Option<String>,
}

// ── Deduplication ────────────────────────────────────────────────────────────

/// Deduplicate a list of findings by (file, line) pairs.
///
/// When two findings share the same file path and line number, only the first
/// occurrence is kept. This avoids double-counting findings that multiple
/// agents or chunks produced for the same location.
///
/// # Ordering
///
/// The deduplication is stable: the first occurrence of each (file, line) pair
/// is retained, and subsequent duplicates are dropped.
pub fn deduplicate_findings(findings: Vec<Finding>) -> Vec<Finding> {
    let mut seen: HashSet<(String, u32)> = HashSet::new();
    let mut result = Vec::with_capacity(findings.len());

    for f in findings {
        let key = (f.file.clone().unwrap_or_default(), f.line.unwrap_or(0));
        if seen.insert(key) {
            result.push(f);
        }
    }

    result
}

// ── Error Types ─────────────────────────────────────────────────────────────

/// Errors that can occur when running a linter subprocess.
#[derive(Debug)]
pub enum LinterError {
    /// The subprocess could not be spawned or communicated with.
    SubprocessFailed(std::io::Error),
    /// The linter exited with a non-zero exit code.
    NonZeroExit(i32, String),
    /// The linter did not complete within the configured timeout.
    TimeoutElapsed,
    /// The linter output could not be parsed into [`Finding`] values.
    ParseFailed(String),
}

impl fmt::Display for LinterError {
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

impl From<std::io::Error> for LinterError {
    fn from(e: std::io::Error) -> Self {
        Self::SubprocessFailed(e)
    }
}

/// Errors that can occur when running a git subprocess.
#[derive(Debug)]
pub enum GitError {
    /// The git command could not be spawned or communicated with.
    CommandFailed(std::io::Error),
    /// Git exited with a non-zero exit code.
    NonZeroExit(i32, String),
    /// The git operation did not complete within the configured timeout.
    TimeoutElapsed,
}

impl fmt::Display for GitError {
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

impl From<std::io::Error> for GitError {
    fn from(e: std::io::Error) -> Self {
        Self::CommandFailed(e)
    }
}

/// Errors that can occur when loading linter configuration.
#[derive(Debug)]
pub enum ConfigError {
    /// The configuration file could not be read.
    IoError(std::io::Error),
    /// The configuration file could not be parsed as TOML.
    ParseError(String),
    /// The configuration failed validation.
    ValidationError(String),
}

impl fmt::Display for ConfigError {
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

// ── Linter Tool ─────────────────────────────────────────────────────────────

/// Arguments accepted by every linter tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct LinterArgs {
    /// Absolute or relative filesystem path to the repository root.
    pub repo_path: String,
}

/// A generic linter tool wrapping any CLI linter.
///
/// Parameterized by the command to run, the output parser function, and a
/// per-invocation timeout.
pub struct LinterTool {
    /// Display name for the linter (e.g. "ruff", "eslint").
    pub name: String,
    /// Command and initial arguments (never a shell string).
    pub cmd: Vec<String>,
    /// Function that parses linter stdout into [`Finding`] values.
    pub parser: fn(&str) -> Result<Vec<Finding>, LinterError>,
    /// Per-invocation timeout.
    pub timeout: Duration,
}

impl LinterTool {
    /// Convert a `JoinError` from a panicked spawned task into an `io::Error`.
    fn join_error_to_io(e: tokio::task::JoinError) -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
    }
}

impl Tool for LinterTool {
    const NAME: &'static str = "linter";

    type Error = LinterError;
    type Args = LinterArgs;
    type Output = Vec<Finding>;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name.clone(),
            description: format!("Run `{}` linter on a repository", self.name),
            parameters: serde_json::to_value(schemars::schema_for!(LinterArgs)).unwrap(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let cmd = self.cmd.clone();
        let repo_path = args.repo_path;
        let timeout = self.timeout;

        // spawn_blocking returns JoinHandle<Result<Output, io::Error>>.
        // After .await we have Result<Result<Output, io::Error>, JoinError>.
        // Chain: timeout(blocking -> io::Result -> unwrap -> parse)
        let result = tokio::time::timeout(timeout, async {
            tokio::task::spawn_blocking(move || {
                std::process::Command::new(&cmd[0])
                    .args(&cmd[1..])
                    .arg(&repo_path)
                    .output()
            })
            .await
            .map_err(|join_err| {
                LinterError::SubprocessFailed(Self::join_error_to_io(join_err))
            })?
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

        let stdout = String::from_utf8_lossy(&result.stdout).to_string();
        (self.parser)(&stdout)
    }
}

// ── Parser Functions ────────────────────────────────────────────────────────

/// Internal JSON structure for ruff output.
#[derive(Debug, Deserialize)]
struct RuffJsonFinding {
    code: String,
    filename: String,
    location: RuffLocation,
    message: String,
}

#[derive(Debug, Deserialize)]
struct RuffLocation {
    #[allow(dead_code)]
    column: u32,
    row: u32,
}

/// Parse ruff JSON output into [`Finding`] values.
///
/// Ruff outputs JSON in the format:
/// ```json
/// [{"code": "F841", "filename": "src/main.py", "location": {"row": 10, "column": 5}, "message": "..."}]
/// ```
pub fn parse_ruff_output(stdout: &str) -> Result<Vec<Finding>, LinterError> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let items: Vec<RuffJsonFinding> =
        serde_json::from_str(trimmed).map_err(|e| LinterError::ParseFailed(e.to_string()))?;

    Ok(items
        .into_iter()
        .map(|f| Finding {
            file: Some(f.filename),
            line: Some(f.location.row),
            message: f.message,
            severity: "error".to_string(),
            rule_code: Some(f.code),
            severity_audited: false,
            severity_audit_reason: None,
            evidence: None,
            path_trace: None,
            confidence: None,
            found_by: None,
        })
        .collect())
}

/// Internal JSON structure for ESLint output.
#[derive(Debug, Deserialize)]
struct EslintFileResult {
    #[serde(alias = "filePath")]
    file_path: String,
    messages: Vec<EslintMessage>,
}

#[derive(Debug, Deserialize)]
struct EslintMessage {
    #[serde(alias = "ruleId")]
    rule_id: Option<String>,
    severity: i32,
    line: u32,
    #[allow(dead_code)]
    column: u32,
    message: String,
}

/// Parse ESLint JSON output into [`Finding`] values.
///
/// ESLint outputs JSON in the format:
/// ```json
/// [{"filePath": "...", "messages": [{"ruleId": "...", "severity": 2, "line": 15, "column": 3, "message": "..."}]}]
/// ```
pub fn parse_eslint_output(stdout: &str) -> Result<Vec<Finding>, LinterError> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let items: Vec<EslintFileResult> =
        serde_json::from_str(trimmed).map_err(|e| LinterError::ParseFailed(e.to_string()))?;

    let mut findings = Vec::new();
    for file_result in items {
        for msg in file_result.messages {
            let severity = match msg.severity {
                2 => "error".to_string(),
                1 => "warning".to_string(),
                _ => "info".to_string(),
            };
            findings.push(Finding {
                file: Some(file_result.file_path.clone()),
                line: Some(msg.line),
                message: msg.message,
                severity,
                rule_code: msg.rule_id,
                severity_audited: false,
                severity_audit_reason: None,
                evidence: None,
                path_trace: None,
                confidence: None,
                found_by: None,
            });
        }
    }
    Ok(findings)
}

/// Parse `go vet` text output into [`Finding`] values.
///
/// `go vet` outputs lines in the format:
/// ```text
/// ./src/main.go:25:2: unreachable code
/// ```
pub fn parse_govet_output(stdout: &str) -> Result<Vec<Finding>, LinterError> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let mut findings = Vec::new();
    for line in trimmed.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Parse: ./path/file.go:line:col: message
        // Or: ./path/file.go:line: message
        let parts: Vec<&str> = line.splitn(4, ':').collect();
        if parts.len() < 3 {
            // Not a recognized go vet format; skip.
            continue;
        }

        let file = parts[0].to_string();
        let line_num: Option<u32> = parts[1].parse().ok();

        // Message is everything after the last colon-separated segment.
        let message = if parts.len() >= 4 {
            // Format: path:line:col: message
            parts[3..].join(":").trim().to_string()
        } else {
            // Format: path:line: message
            parts[2..].join(":").trim().to_string()
        };

        findings.push(Finding {
            file: Some(file),
            line: line_num,
            message,
            severity: "warning".to_string(),
            rule_code: None,
            severity_audited: false,
            severity_audit_reason: None,
            evidence: None,
            path_trace: None,
            confidence: None,
            found_by: None,
        });
    }

    Ok(findings)
}

/// Parse staticcheck JSON output into Finding values.
///
/// TODO: Implement staticcheck JSON output parsing.
/// staticcheck outputs JSON lines with fields: file, line, column, message, severity.
pub fn parse_staticcheck_output(_stdout: &str) -> Result<Vec<Finding>, LinterError> {
    // TODO: Implement staticcheck JSON parser
    Ok(Vec::new())
}

/// Parse rubocop JSON output into Finding values.
///
/// TODO: Implement rubocop JSON output parsing.
/// rubocop outputs JSON with: files[].offenses[].{severity, message, cop_name, location.{line, column}}
pub fn parse_rubocop_output(_stdout: &str) -> Result<Vec<Finding>, LinterError> {
    // TODO: Implement rubocop JSON parser
    Ok(Vec::new())
}

/// Parse checkstyle XML output into Finding values.
///
/// TODO: Implement checkstyle XML output parsing.
/// checkstyle outputs XML with: <checkstyle><file><error line="..." column="..." severity="..." message="..." source="..."/></file></checkstyle>
pub fn parse_checkstyle_output(_stdout: &str) -> Result<Vec<Finding>, LinterError> {
    // TODO: Implement checkstyle XML parser
    Ok(Vec::new())
}

// ── Linter Configuration ────────────────────────────────────────────────────

/// A single linter definition from the TOML config file.
#[derive(Debug, Clone, Deserialize)]
pub struct LinterConfig {
    /// Display name for the linter.
    pub name: String,
    /// Command and arguments (never a shell string).
    pub cmd: Vec<String>,
    /// Per-invocation timeout in seconds.
    pub timeout_secs: Option<u64>,
    /// Output format: "json" or "text".
    pub output_format: String,
    /// If true, a missing binary is non-fatal.
    pub optional: Option<bool>,
}

/// Top-level config file structure.
#[derive(Debug, Deserialize)]
pub struct LinterConfigFile {
    pub linters: HashMap<String, LinterConfig>,
}

/// Load and validate linter configuration from a TOML file.
///
/// Returns a `HashMap` keyed by linter identifier (e.g. "ruff", "eslint").
pub fn load_linter_config(path: &str) -> Result<HashMap<String, LinterConfig>, ConfigError> {
    let content =
        std::fs::read_to_string(path).map_err(|e| ConfigError::IoError(e))?;

    let config: LinterConfigFile =
        toml::from_str(&content).map_err(|e| ConfigError::ParseError(e.to_string()))?;

    // Validate
    for (key, lc) in &config.linters {
        if lc.cmd.is_empty() {
            return Err(ConfigError::ValidationError(format!(
                "linter '{key}' has empty cmd"
            )));
        }
        if lc.output_format != "json" && lc.output_format != "text" {
            return Err(ConfigError::ValidationError(format!(
                "linter '{key}' has invalid output_format '{}' (must be 'json' or 'text')",
                lc.output_format
            )));
        }
        if lc.name.is_empty() {
            return Err(ConfigError::ValidationError(format!(
                "linter '{key}' has empty name"
            )));
        }
    }

    Ok(config.linters)
}

// ── Factory Functions ───────────────────────────────────────────────────────

/// Create a [`LinterTool`] for ruff from its configuration.
pub fn create_ruff_tool(config: &LinterConfig) -> LinterTool {
    LinterTool {
        name: config.name.clone(),
        cmd: config.cmd.clone(),
        parser: parse_ruff_output,
        timeout: Duration::from_secs(config.timeout_secs.unwrap_or(60)),
    }
}

/// Create a [`LinterTool`] for ESLint from its configuration.
pub fn create_eslint_tool(config: &LinterConfig) -> LinterTool {
    LinterTool {
        name: config.name.clone(),
        cmd: config.cmd.clone(),
        parser: parse_eslint_output,
        timeout: Duration::from_secs(config.timeout_secs.unwrap_or(60)),
    }
}

/// Create a [`LinterTool`] for `go vet` from its configuration.
pub fn create_govet_tool(config: &LinterConfig) -> LinterTool {
    LinterTool {
        name: config.name.clone(),
        cmd: config.cmd.clone(),
        parser: parse_govet_output,
        timeout: Duration::from_secs(config.timeout_secs.unwrap_or(60)),
    }
}

/// Create a LinterTool from staticcheck configuration.
pub fn create_staticcheck_tool(config: &LinterConfig) -> LinterTool {
    LinterTool {
        name: config.name.clone(),
        cmd: config.cmd.clone(),
        parser: parse_staticcheck_output,
        timeout: Duration::from_secs(config.timeout_secs.unwrap_or(120)),
    }
}

/// Create a LinterTool from rubocop configuration.
pub fn create_rubocop_tool(config: &LinterConfig) -> LinterTool {
    LinterTool {
        name: config.name.clone(),
        cmd: config.cmd.clone(),
        parser: parse_rubocop_output,
        timeout: Duration::from_secs(config.timeout_secs.unwrap_or(120)),
    }
}

/// Create a LinterTool from checkstyle configuration.
pub fn create_checkstyle_tool(config: &LinterConfig) -> LinterTool {
    LinterTool {
        name: config.name.clone(),
        cmd: config.cmd.clone(),
        parser: parse_checkstyle_output,
        timeout: Duration::from_secs(config.timeout_secs.unwrap_or(120)),
    }
}

/// Parser that always returns an error (used for unknown output formats).
fn unknown_format_parser(_stdout: &str) -> Result<Vec<Finding>, LinterError> {
    Err(LinterError::ParseFailed("unknown output format (expected 'json' or 'text')".into()))
}

/// Create a [`LinterTool`] from configuration, selecting the parser based on
/// the `output_format` field.
pub fn create_linter_tool(config: &LinterConfig) -> LinterTool {
    let parser: fn(&str) -> Result<Vec<Finding>, LinterError> = match config.output_format.as_str() {
        "json" => parse_ruff_output, // used as default json parser
        "text" => parse_govet_output,
        _ => unknown_format_parser,
    };
    LinterTool {
        name: config.name.clone(),
        cmd: config.cmd.clone(),
        parser,
        timeout: Duration::from_secs(config.timeout_secs.unwrap_or(60)),
    }
}

// ── Git Tools ───────────────────────────────────────────────────────────────

/// Arguments for [`GitCleanTool`].
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct GitCleanArgs {
    /// Absolute or relative path to the repository.
    pub repo_path: String,
}

/// Tool that runs `git clean -fdx` to remove untracked files.
pub struct GitCleanTool;

impl Tool for GitCleanTool {
    const NAME: &'static str = "git_clean";

    type Error = GitError;
    type Args = GitCleanArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Remove untracked files from a git repository (git clean -fdx)".to_string(),
            parameters: serde_json::to_value(schemars::schema_for!(GitCleanArgs)).unwrap(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let repo_path = args.repo_path;

        let result = tokio::time::timeout(Duration::from_secs(60), async {
            tokio::task::spawn_blocking(move || {
                std::process::Command::new("git")
                    .args(["-C", &repo_path, "clean", "-fdx"])
                    .output()
            })
            .await
            .map_err(|join_err| {
                GitError::CommandFailed(
                    std::io::Error::new(std::io::ErrorKind::Other, join_err.to_string()),
                )
            })?
            .map_err(GitError::CommandFailed)
        })
        .await
        .map_err(|_| GitError::TimeoutElapsed)??;

        if result.status.success() {
            Ok(String::from_utf8_lossy(&result.stdout).to_string())
        } else {
            Err(GitError::NonZeroExit(
                result.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&result.stderr).to_string(),
            ))
        }
    }
}

/// Arguments for [`GitDiffTool`].
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct GitDiffArgs {
    /// Absolute or relative path to the repository.
    pub repo_path: String,
    /// Base git ref (e.g. "main", "origin/main", "HEAD~1").
    pub base: String,
    /// Head git ref (e.g. "feature-branch", "HEAD").
    pub head: String,
}

/// Tool that runs `git diff base...head --no-color`.
pub struct GitDiffTool;

impl Tool for GitDiffTool {
    const NAME: &'static str = "git_diff";

    type Error = GitError;
    type Args = GitDiffArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Get the diff between two git refs (git diff base...head)".to_string(),
            parameters: serde_json::to_value(schemars::schema_for!(GitDiffArgs)).unwrap(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let repo_path = args.repo_path;
        let range = format!("{}...{}", args.base, args.head);

        let result = tokio::time::timeout(Duration::from_secs(60), async {
            tokio::task::spawn_blocking(move || {
                std::process::Command::new("git")
                    .args(["-C", &repo_path, "diff", &range, "--no-color"])
                    .output()
            })
            .await
            .map_err(|join_err| {
                GitError::CommandFailed(
                    std::io::Error::new(std::io::ErrorKind::Other, join_err.to_string()),
                )
            })?
            .map_err(GitError::CommandFailed)
        })
        .await
        .map_err(|_| GitError::TimeoutElapsed)??;

        if result.status.success() {
            Ok(String::from_utf8_lossy(&result.stdout).to_string())
        } else {
            Err(GitError::NonZeroExit(
                result.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&result.stderr).to_string(),
            ))
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Check whether a binary is available on `$PATH`.
pub fn check_binary_exists(name: &str) -> bool {
    std::process::Command::new("which")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ── Per-role Tool Assignment ──────────────────────────────────────────────

use crate::budget::ToolCallBudget;

/// Returns the tool names appropriate for a given reviewer role.
///
/// Each role gets a different set of tools based on what it needs to do:
/// - **SA** (Static Analysis): shell, read_file
/// - **CL** (Code Logic): shell, read_file, git
/// - **AR** / **ARCH** (Architecture): shell, read_file, git
/// - **SEC** (Security): shell, read_file, git
pub fn tools_for_role(role: &str) -> Vec<&'static str> {
    match role {
        "CL" | "AR" | "ARCH" | "SEC" => vec!["shell", "read_file", "git"],
        _ => vec!["shell", "read_file"],
    }
}

/// Renders the tool-calling preamble section for inclusion in an LLM
/// agent system prompt.
///
/// This tells the agent what tools are available, how to use them, and
/// what budget constraints apply.
///
/// If `mcp_tool_names` is non-empty, those MCP tool names are appended
/// to the available-tools list so the agent knows about them.
pub fn tool_prompt_section(
    role: &str,
    budget: &ToolCallBudget,
    mcp_tool_names: &[String],
) -> String {
    let tool_names = tools_for_role(role);

    let tools_description = tool_names.join(", ");
    let call_limit = budget.max_per_tool;

    let mcp_section = if mcp_tool_names.is_empty() {
        String::new()
    } else {
        format!(
            "\nMCP tools available:\n{}\n",
            mcp_tool_names
                .iter()
                .map(|n| format!("- **{n}**: An MCP (Model Context Protocol) tool. Use it by calling it with JSON arguments as specified by its tool definition."))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };

    format!(
        "You have access to the following tools during this review: {tools_description}.\
{mcp_section}

Tool usage rules:
- Each tool invocation returns text output. Use tools to inspect files, run commands, or check git history.
- You may call tools multiple times but are limited to a total of {call_limit} calls per tool and {} overall.
- If a tool fails, try again with different arguments or skip that check.
- Use `read_file` to examine specific files, `shell` to run commands like grep/build/tests, and `git` to inspect commit history or diffs.
- Keep your tool usage targeted and efficient — prefer `read_file` over `shell cat`.

Available tools:
- **read_file**: Read a file from the repository. Specify path (relative to repo root), optional start_line (1-indexed), and optional max_lines.
- **shell**: Run a shell command in the repository working directory. Use for building, testing, grepping, or any CLI operation.
- **git**: Run git operations on the repository: log, diff, show, status.

Use tools by calling them with JSON arguments as specified by each tool's definition.",
        budget.max_total_calls,
    )
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Parser Tests: Ruff ─────────────────────────────────────────────────

    #[test]
    fn test_parse_ruff_output_empty() {
        let result = parse_ruff_output("");
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_parse_ruff_output_whitespace() {
        let result = parse_ruff_output("  \n  ");
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_parse_ruff_output_valid() {
        let json = r#"[
            {"code": "F841", "filename": "src/main.py", "location": {"row": 10, "column": 5}, "message": "Local variable `x` is assigned but never used"},
            {"code": "E501", "filename": "src/utils.py", "location": {"row": 42, "column": 80}, "message": "Line too long"}
        ]"#;
        let result = parse_ruff_output(json);
        assert!(result.is_ok());
        let findings = result.unwrap();
        assert_eq!(findings.len(), 2);

        assert_eq!(findings[0].file.as_deref(), Some("src/main.py"));
        assert_eq!(findings[0].line, Some(10));
        assert_eq!(
            findings[0].message,
            "Local variable `x` is assigned but never used"
        );
        assert_eq!(findings[0].severity, "error");
        assert_eq!(findings[0].rule_code.as_deref(), Some("F841"));

        assert_eq!(findings[1].file.as_deref(), Some("src/utils.py"));
        assert_eq!(findings[1].line, Some(42));
        assert_eq!(findings[1].rule_code.as_deref(), Some("E501"));
    }

    #[test]
    fn test_parse_ruff_output_malformed() {
        let result = parse_ruff_output("not valid json");
        assert!(result.is_err());
        match result.unwrap_err() {
            LinterError::ParseFailed(_) => {} // expected
            other => panic!("expected ParseFailed, got {other:?}"),
        }
    }

    // ── Parser Tests: ESLint ───────────────────────────────────────────────

    #[test]
    fn test_parse_eslint_output_empty() {
        let result = parse_eslint_output("");
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_parse_eslint_output_valid() {
        let json = r#"[
            {
                "filePath": "/repo/src/app.js",
                "messages": [
                    {
                        "ruleId": "no-unused-vars",
                        "severity": 2,
                        "line": 15,
                        "column": 3,
                        "message": "'x' is assigned but never used"
                    },
                    {
                        "ruleId": "no-console",
                        "severity": 1,
                        "line": 20,
                        "column": 1,
                        "message": "Unexpected console statement"
                    }
                ]
            }
        ]"#;
        let result = parse_eslint_output(json);
        assert!(result.is_ok());
        let findings = result.unwrap();
        assert_eq!(findings.len(), 2);

        assert_eq!(findings[0].file.as_deref(), Some("/repo/src/app.js"));
        assert_eq!(findings[0].line, Some(15));
        assert_eq!(findings[0].message, "'x' is assigned but never used");
        assert_eq!(findings[0].severity, "error");
        assert_eq!(findings[0].rule_code.as_deref(), Some("no-unused-vars"));

        assert_eq!(findings[1].severity, "warning");
        assert_eq!(findings[1].rule_code.as_deref(), Some("no-console"));
    }

    #[test]
    fn test_parse_eslint_output_malformed() {
        let result = parse_eslint_output("{bad json");
        assert!(result.is_err());
        match result.unwrap_err() {
            LinterError::ParseFailed(_) => {}
            other => panic!("expected ParseFailed, got {other:?}"),
        }
    }

    // ── Parser Tests: go vet ───────────────────────────────────────────────

    #[test]
    fn test_parse_govet_output_empty() {
        let result = parse_govet_output("");
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_parse_govet_output_valid() {
        let text = "./src/main.go:25:2: unreachable code\n./src/util.go:42:6: X is unused\n";
        let result = parse_govet_output(text);
        assert!(result.is_ok());
        let findings = result.unwrap();
        assert_eq!(findings.len(), 2);

        assert_eq!(findings[0].file.as_deref(), Some("./src/main.go"));
        assert_eq!(findings[0].line, Some(25));
        assert_eq!(findings[0].message, "unreachable code");
        assert_eq!(findings[0].severity, "warning");
        assert!(findings[0].rule_code.is_none());

        assert_eq!(findings[1].file.as_deref(), Some("./src/util.go"));
        assert_eq!(findings[1].line, Some(42));
        assert_eq!(findings[1].message, "X is unused");
    }

    #[test]
    fn test_parse_govet_output_no_colon_format() {
        // Some go vet output may not have colons in the expected format
        let text = "./src/main.go:25: unreachable code";
        let result = parse_govet_output(text);
        assert!(result.is_ok());
        let findings = result.unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file.as_deref(), Some("./src/main.go"));
        assert_eq!(findings[0].line, Some(25));
        assert_eq!(findings[0].message, "unreachable code");
    }

    // ── Config Tests ───────────────────────────────────────────────────────

    #[test]
    fn test_load_linter_config_file_not_found() {
        let result = load_linter_config("/nonexistent/path/linters.toml");
        assert!(result.is_err());
        match result.unwrap_err() {
            ConfigError::IoError(_) => {}
            other => panic!("expected IoError, got {other:?}"),
        }
    }

    #[test]
    fn test_load_linter_config_invalid_toml() {
        // Write a temp file with invalid TOML and load it
        let dir = std::env::temp_dir();
        let path = dir.join("test_invalid_linters.toml");
        std::fs::write(&path, "not toml = [[[")
            .expect("failed to write temp config");
        let result = load_linter_config(path.to_str().unwrap());
        std::fs::remove_file(&path).ok();
        assert!(result.is_err());
        match result.unwrap_err() {
            ConfigError::ParseError(_) => {}
            other => panic!("expected ParseError, got {other:?}"),
        }
    }

    #[test]
    fn test_load_linter_config_validation_empty_cmd() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_empty_cmd_linters.toml");
        let toml_content = r#"
[linters.test]
name = "test"
cmd = []
timeout_secs = 60
output_format = "json"
optional = false
"#;
        std::fs::write(&path, toml_content).expect("failed to write temp config");
        let result = load_linter_config(path.to_str().unwrap());
        std::fs::remove_file(&path).ok();
        assert!(result.is_err());
        match result.unwrap_err() {
            ConfigError::ValidationError(msg) => {
                assert!(msg.contains("empty cmd"));
            }
            other => panic!("expected ValidationError, got {other:?}"),
        }
    }

    #[test]
    fn test_load_linter_config_validation_bad_format() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_bad_format_linters.toml");
        let toml_content = r#"
[linters.test]
name = "test"
cmd = ["test"]
timeout_secs = 60
output_format = "yaml"
optional = false
"#;
        std::fs::write(&path, toml_content).expect("failed to write temp config");
        let result = load_linter_config(path.to_str().unwrap());
        std::fs::remove_file(&path).ok();
        assert!(result.is_err());
        match result.unwrap_err() {
            ConfigError::ValidationError(msg) => {
                assert!(msg.contains("invalid output_format"));
            }
            other => panic!("expected ValidationError, got {other:?}"),
        }
    }

    #[test]
    fn test_load_linter_config_valid() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_valid_linters.toml");
        let toml_content = r#"
[linters.ruff]
name = "ruff"
cmd = ["ruff", "check"]
timeout_secs = 60
output_format = "json"
optional = false

[linters.eslint]
name = "eslint"
cmd = ["npx", "eslint", "--format", "json"]
timeout_secs = 90
output_format = "json"
optional = true
"#;
        std::fs::write(&path, toml_content).expect("failed to write temp config");
        let result = load_linter_config(path.to_str().unwrap());
        std::fs::remove_file(&path).ok();
        assert!(result.is_ok());
        let configs = result.unwrap();
        assert_eq!(configs.len(), 2);
        assert!(configs.contains_key("ruff"));
        assert!(configs.contains_key("eslint"));

        let ruff = &configs["ruff"];
        assert_eq!(ruff.name, "ruff");
        assert_eq!(ruff.cmd, vec!["ruff", "check"]);
        assert_eq!(ruff.timeout_secs, Some(60));
        assert_eq!(ruff.optional, Some(false));
    }

    // ── Factory Tests ──────────────────────────────────────────────────────

    #[test]
    fn test_create_ruff_tool() {
        let config = LinterConfig {
            name: "ruff".to_string(),
            cmd: vec!["ruff".to_string(), "check".to_string()],
            timeout_secs: Some(60),
            output_format: "json".to_string(),
            optional: Some(false),
        };
        let tool = create_ruff_tool(&config);
        assert_eq!(tool.name, "ruff");
        assert_eq!(tool.cmd, vec!["ruff", "check"]);
        assert_eq!(tool.timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_create_eslint_tool() {
        let config = LinterConfig {
            name: "eslint".to_string(),
            cmd: vec!["npx".to_string(), "eslint".to_string()],
            timeout_secs: Some(90),
            output_format: "json".to_string(),
            optional: Some(true),
        };
        let tool = create_eslint_tool(&config);
        assert_eq!(tool.name, "eslint");
        assert_eq!(tool.timeout, Duration::from_secs(90));
    }

    #[test]
    fn test_create_govet_tool() {
        let config = LinterConfig {
            name: "go vet".to_string(),
            cmd: vec!["go".to_string(), "vet".to_string(), "./...".to_string()],
            timeout_secs: None,
            output_format: "text".to_string(),
            optional: Some(true),
        };
        let tool = create_govet_tool(&config);
        assert_eq!(tool.name, "go vet");
        // Default timeout when None
        assert_eq!(tool.timeout, Duration::from_secs(60));
    }

    // ── Error Display Tests ────────────────────────────────────────────────

    #[test]
    fn test_linter_error_display() {
        let err = LinterError::TimeoutElapsed;
        assert_eq!(err.to_string(), "linter timed out");

        let err = LinterError::ParseFailed("bad output".to_string());
        assert_eq!(err.to_string(), "failed to parse linter output: bad output");

        let err = LinterError::NonZeroExit(1, "something broke".to_string());
        assert_eq!(err.to_string(), "linter exited with code 1: something broke");

        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "binary not found");
        let err = LinterError::SubprocessFailed(io_err);
        assert!(err.to_string().contains("linter subprocess failed"));
    }

    #[test]
    fn test_git_error_display() {
        let err = GitError::TimeoutElapsed;
        assert_eq!(err.to_string(), "git operation timed out");

        let err = GitError::NonZeroExit(128, "not a git repository".to_string());
        assert_eq!(
            err.to_string(),
            "git exited with code 128: not a git repository"
        );
    }

    #[test]
    fn test_config_error_display() {
        let err = ConfigError::ValidationError("bad config".to_string());
        assert_eq!(err.to_string(), "config validation error: bad config");

        let err = ConfigError::ParseError("bad toml".to_string());
        assert_eq!(err.to_string(), "config parse error: bad toml");
    }

    // ── From impl tests ────────────────────────────────────────────────────

    #[test]
    fn test_linter_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::Other, "oops");
        let linter_err: LinterError = io_err.into();
        match linter_err {
            LinterError::SubprocessFailed(_) => {}
            other => panic!("expected SubprocessFailed, got {other:?}"),
        }
    }

    #[test]
    fn test_git_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::Other, "oops");
        let git_err: GitError = io_err.into();
        match git_err {
            GitError::CommandFailed(_) => {}
            other => panic!("expected CommandFailed, got {other:?}"),
        }
    }
}
