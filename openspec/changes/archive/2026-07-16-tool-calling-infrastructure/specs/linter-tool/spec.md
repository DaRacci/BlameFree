# Delta for LinterTool

## ADDED Requirements

### Requirement: LinterTool Arguments
The system SHALL define typed `LinterArgs` via `schemars::JsonSchema` derive for automatic JSON Schema generation.

#### Scenario: Valid arguments
- GIVEN a `LinterArgs` struct with a valid `repo_path` field
- WHEN the struct is serialized for JSON Schema generation
- THEN the system SHALL produce a valid JSON Schema with a `repo_path` string parameter

### Requirement: Finding Struct
The system SHALL represent linter findings with a typed `Finding` struct supporting serde serialization.

#### Scenario: Full finding
- GIVEN a linter finding with severity, path, line, column, message, and code
- WHEN the finding is constructed
- THEN the `Finding` struct SHALL contain all fields: `severity: String`, `path: String`, `line: u32`, `column: u32`, `message: String`, `code: Option<String>`

### Requirement: LinterTool Implementation
Each linter SHALL be wrapped as a `rig::tool::Tool` implementation.

#### Scenario: Tool registration
- GIVEN a configured `LinterTool` instance with name, command, parser, and timeout
- WHEN the tool is registered with the agent
- THEN `definition()` SHALL return a `ToolDefinition` with auto-generated JSON Schema
- AND `call()` SHALL execute the linter subprocess and parse output

#### Scenario: Concurrent linting
- GIVEN multiple linter tools configured in `linters.toml`
- WHEN the harness runs all linters
- THEN each linter SHALL execute concurrently via `tokio::join!` or `tokio::task::JoinSet`
- AND results SHALL be collected as `Vec<(String, Result<Vec<Finding>, LinterError>)>`

### Requirement: Output Parsers
The system SHALL provide parser functions for each linter's output format.

#### Scenario: Ruff JSON parsing
- GIVEN ruff JSON output with `code`, `filename`, `location`, and `message` fields
- WHEN `parse_ruff_output()` is called
- THEN the system SHALL return a `Vec<Finding>` with severity, path, line, column, message, and code

#### Scenario: ESLint JSON parsing
- GIVEN ESLint JSON output with `filePath` and `messages` array containing `ruleId`, `severity`, `line`, `column`, `message`
- WHEN `parse_eslint_output()` is called
- THEN the system SHALL return a `Vec<Finding>` translated from the ESLint format

#### Scenario: Go vet text parsing
- GIVEN `go vet` text output in the format `./path/file.go:line:col: message`
- WHEN `parse_govet_output()` is called
- THEN the system SHALL parse each line with regex and return a `Vec<Finding>`
