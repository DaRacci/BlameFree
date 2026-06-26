# Tasks: Tool Calling Infrastructure

## Phase 1: Foundation

- [x] **1.1 Create `crates/crb-tools/Cargo.toml` and `src/lib.rs`**
  - Define the `Finding` struct.
  - Re-export all public types from the module.
  - Define `LinterError`, `GitError` error types.

- [x] **1.2 Implement linter tools in `crates/crb-tools/src/lib.rs`**
  - Define `LinterArgs`, `LinterError`.
  - Implement `LinterTool` struct with `Tool` trait.
  - Implement `LinterConfig` deserialization from TOML.
  - Implement `load_linter_config()`.
  - Write `parse_ruff_output()` parser.
  - Write `parse_eslint_output()` parser.
  - Write `parse_govet_output()` parser.
  - Write factories: `create_ruff_tool()`, `create_eslint_tool()`, `create_govet_tool()`.

- [x] **1.3 Implement git tools in `crates/crb-tools/src/lib.rs`**
  - Define `GitCleanArgs`, `GitDiffArgs`, `GitError`.
  - Implement `GitCleanTool` with `Tool` trait.
  - Implement `GitDiffTool` with `Tool` trait.

## Phase 2: Testing

- [x] **2.1 Linter parser unit tests**
  - Test `parse_ruff_output` with known ruff JSON output.
  - Test `parse_eslint_output` with known ESLint JSON output.
  - Test `parse_govet_output` with known `go vet` text output.
  - Test each parser with empty input.
  - Test each parser with malformed input (should return `LinterError::ParseFailed`).

- [ ] **2.2 Git tool integration tests**
  - Create a temp git repo in tests.
  - Test `GitCleanTool`: create dirty files, run clean, verify they're removed.
  - Test `GitDiffTool`: create commits, verify diff output matches expected.
  - Test timeout behavior with a mock slow git command.

- [ ] **2.3 Linter tool integration tests**
  - Install ruff, run on a known file, verify findings.
  - Install eslint, run on a known file, verify findings.
  - Test timeout via `tokio::time::pause()`.

## Phase 3: Configuration & Wiring

- [x] **3.1 Create sample `linters.toml`**
  - Define ruff, eslint, and govet entries.
  - Validate with the `load_linter_config()` function.

- [x] **3.2 Wire into harness orchestration**
  - Create `run_all_linters(repo_path)` function.
  - Create `aggregate_results()` for producing a `LintReport`.
  - Integrate with the main review loop in `crb-harness`.

## Phase 4: Edge Cases & Hardening

- [x] **4.1 Handle missing linter binaries gracefully**
  - If `ruff` is not installed, skip or warn instead of hard error.
  - Config option: `optional = true` per linter.

- [x] **4.2 Shell injection prevention**
  - Verify all commands use argument arrays, not shell strings.
  - Audit `LinterTool.cmd` usage for `sh -c` or string concatenation.

- [ ] **4.3 Large output handling**
  - Stream/chunk stdout for linters that produce megabytes of output.
  - Cap `Vec<Finding>` size to prevent OOM.

- [x] **4.4 Concurrent execution limits**
  - Add a semaphore to limit concurrent linter processes (default: 4).

## Phase 5: Documentation

- [x] **5.1 API docs**
  - Document public types and functions with `///` docs.
  - Include example usage for each tool.

- [ ] **5.2 Developer guide**
  - How to add a new linter (create parser, add TOML entry).
  - How to add a new git tool.
