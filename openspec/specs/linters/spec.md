# linters Specification

## Purpose
Per-language linter integrations (ruff, eslint, govet, etc.) for static analysis of PR code via subprocess execution.
## Requirements
### Requirement: Subprocess Linter Execution
The system SHALL run static analysis linters as subprocesses on checked-out PR branches via Rust `tokio::process::Command`.

#### Scenario: Run ruff on Python PRs
- GIVEN a Python PR from the sentry dataset
- WHEN the harness evaluates the PR
- THEN it spawns `ruff check {repo_path}` via `tokio::process::Command`
- AND it captures stdout/stderr
- AND it converts findings to the standard `Finding` struct

#### Scenario: Run eslint on TypeScript PRs
- GIVEN a TypeScript PR from the cal.com dataset
- WHEN the harness evaluates the PR
- THEN it spawns `eslint {repo_path}/src/ --format json` via `tokio::process::Command`
- AND it parses the JSON output into standard `Finding` values

#### Scenario: Language-appropriate linting
- GIVEN a dataset entry for any of the 5 languages
- WHEN the harness evaluates the PR
- THEN it runs the appropriate linter(s) for that language:
  - Python -> ruff
  - TypeScript -> eslint
  - Go -> go vet + staticcheck
  - Ruby -> rubocop
  - Java -> checkstyle or spotbugs

### Requirement: Finding Translation
The system SHALL translate linter output into a Rust `Finding` struct with serde + schemars derives, matching the format used by LLM agents.

#### Scenario: ruff output parsing
- GIVEN ruff JSON output: `{"file": "src/api.py", "line": 42, "message": "Unused import", "code": "F401"}`
- WHEN the Rust linter:ruff Tool processes it
- THEN it produces: `Finding { source: "linter:ruff", file: Some("src/api.py"), line: Some(42), message: "Unused import: F401", severity: "Low", rule: Some("F401") }`
- AND it is stored alongside LLM agent findings

### Requirement: Tool Trait Integration
Each linter SHALL be wrapped as a `rig::tool::Tool` so it can be used both standalone and as an LLM-callable tool.

#### Scenario: Tool definition auto-generation
- GIVEN a `RuffLinter` struct implementing `Tool`
- WHEN the harness registers it with the agent
- THEN `definition()` returns a `ToolDefinition` with auto-generated JSON Schema (via `schemars`)
- AND no manual schema writing is required

#### Scenario: LLM-callable linter
- GIVEN an agent with registered linter Tools
- WHEN the LLM decides to run a linter mid-conversation
- THEN it invokes `Tool::call()` with `LinterArgs { path }`
- AND receives `Vec<Finding>` as output
- AND findings are incorporated into the agent's context for further reasoning

### Requirement: Concurrent Linter+Agent Execution
The system SHALL run linters concurrently with LLM agent calls for the same PR via `tokio::task::JoinSet`.

#### Scenario: Parallel evaluation
- GIVEN a PR being evaluated
- WHEN the harness runs
- THEN it spawns LLM agent calls AND linter Tool calls in the same `JoinSet`
- AND it waits for all to complete before judging
- AND it tags each finding with its source (`"llm:{role}"` or `"linter:{name}"`)
- AND it collects each `JoinSet` result with pattern matching (`Ok(Ok(f))`, `Ok(Err(e))`, `Err(e)`)

