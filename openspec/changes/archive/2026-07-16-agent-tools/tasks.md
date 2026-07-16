# Tasks: Agent Tool Calling

## Phase 1: Tool Implementations

- [x] **1.1 Create `ShellTool`** — `crates/crb-tools/src/shell.rs`
  - Command execution via `sh -c`, 30s timeout, 100KB output cap, typed errors.
- [x] **1.2 Create `ReadFileTool`** — `crates/crb-tools/src/read_file.rs`
  - Path safety (canonicalization, prefix check), 1MB size limit, line-range reading.
- [x] **1.3 Create `GitTool`** — `crates/crb-tools/src/git.rs`
  - Unified `GitOperation` enum for log, diff, show, status.
- [x] **1.4 Create `MCPTool`** — `crates/crb-tools/src/mcp.rs`
  - HTTP POST with JSON-RPC 2.0, server config, tool definition caching.

## Phase 2: Budget & Assignment

- [x] **2.1 Create `ToolCallBudget` / `ToolCallTracker`** — `crates/crb-tools/src/budget.rs`
  - Per-session budget with per-tool and total limits, hard/soft stop.
- [x] **2.2 Implement `tools_for_role()`** — per-role tool assignment
  - SA: `[shell, read_file]`; CL/AR/SEC: `[shell, read_file, git]`
- [x] **2.3 Implement `tool_prompt_section()`** — renders tool-calling preamble
  - Includes tool descriptions, usage rules, and budget constraints.

## Phase 3: Integration

- [x] **3.1 Fix `mcp.rs` `.send()` timeout bug** — move `.send()` inside `tokio::time::timeout()`.
- [x] **3.2 Wire into harness single-agent path** — `evaluate_pr_single_agent()` computes and passes `tool_preamble`.
- [x] **3.3 Wire into consensus pipeline** — Thread `tool_preamble` through `build_reviewer_agent()` -> `run_reviewers()` -> `run_consensus()` -> `evaluate_pr_with_consensus()`.
- [ ] **3.4 Prompt updates** — Ensure agent prompts mention tool usage effectively.
- [ ] **3.5 Test with real PR** — Run a real PR through the harness to verify tool instructions appear in agent output.

## Phase 4: Documentation

- [x] **4.1 Create OpenSpec docs** — proposal.md, design.md, tasks.md, specs/tools/spec.md
- [x] **4.2 API docs** — Module-level and function-level `///` docs in all tool source files.
- [ ] **4.3 Developer guide** — How to add a new tool, how tools are assigned to roles.
