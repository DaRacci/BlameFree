# Design: Agent Tool Calling

## 1. Architecture

Tool implementations live in `crb-tools` crate. They are wired into agents via `build_agent()`'s `extra_preamble` parameter — a string containing tool descriptions, usage rules, and budget constraints is appended to the agent's system prompt.

```
┌─────────────────┐     tool_prompt_section()     ┌──────────────┐
│  crb-harness     │ ──────────────────────────►  │  crb-tools   │
│  main.rs         │                               │  lib.rs      │
│  evaluate_pr_*() │ ◄──────────────────────────── │  shell.rs    │
│                  │      tool_preamble String     │  read_file.rs│
└────────┬────────┘                               │  git.rs      │
         │                                        │  mcp.rs      │
         │ build_agent(..., extra_preamble)        │  budget.rs   │
         ▼                                        └──────────────┘
┌─────────────────┐
│  crb-agents     │
│  lib.rs         │
│  build_agent()  │
└─────────────────┘
```

## 2. Per-Role Tool Assignment

| Role | Code | Tools | Rationale |
|------|------|-------|-----------|
| Static Analysis | SA | `shell`, `read_file` | Grep for patterns, read specific files |
| Code Logic | CL | `shell`, `read_file`, `git` | Same + check blame/log for change history |
| Architecture | AR | `shell`, `read_file`, `git` | Same + inspect module structure |
| Security | SEC | `shell`, `read_file`, `git` | Same + grep for dangerous APIs |

MCPTool is available to all roles when an MCP server is configured, but is listed separately in the prompt.

## 3. Budget System

Default budget:

| Parameter | Default | Description |
|-----------|---------|-------------|
| `max_total_calls` | 50 | Total tool invocations across all tools |
| `max_per_tool` | 20 | Per-tool-type maximum |
| `hard_stop` | false | When true, over-budget calls return `Err`; when false, warn + allow |

Budget limits are embedded in the tool prompt section, telling the agent how many calls it has.

## 4. Tool Prompt Section Format

```
You have access to the following tools during this review: shell, read_file, git.

Tool usage rules:
- Each tool invocation returns text output. Use tools to inspect files, run commands, or check git history.
- You may call tools multiple times but are limited to a total of 20 calls per tool and 50 overall.
- If a tool fails, try again with different arguments or skip that check.
- Use `read_file` to examine specific files, `shell` to run commands like grep/build/tests, and `git` to inspect commit history or diffs.
- Keep your tool usage targeted and efficient — prefer `read_file` over `shell cat`.

Available tools:
- **read_file**: Read a file from the repository. Specify path (relative to repo root), optional start_line (1-indexed), and optional max_lines.
- **shell**: Run a shell command in the repository working directory. Use for building, testing, grepping, or any CLI operation.
- **git**: Run git operations on the repository: log, diff, show, status.
```

## 5. Safety

### 5.1 Path Traversal Protection (ReadFileTool)

- All paths are canonicalized via `dunce::canonicalize`.
- Any path whose canonical form does not start with `repo_root` is rejected with `PathOutsideRepo`.

### 5.2 Command Deny-List (ShellTool)

- Currently no deny-list — any shell command is allowed within the configured working directory.
- A deny-list can be added in the future as a field on `ShellTool`.

### 5.3 Timeouts

| Tool | Default Timeout | Configurable |
|------|----------------|--------------|
| ShellTool | 30s | Yes (field) |
| ReadFileTool | N/A (no subprocess) | N/A |
| GitTool | 30s | Yes (field) |
| MCPTool | Per-config (default 30s) | Yes (config) |

### 5.4 Output Caps

| Tool | Max Output | Behavior When Exceeded |
|------|-----------|------------------------|
| ShellTool | 100 KB | Returns `Err(OutputTooLarge)` |
| ReadFileTool | 1 MB file size / 200 default lines | Rejects large files, truncates lines |
| GitTool | N/A (git commands are bounded by timeout) | Times out if endless |
| MCPTool | N/A | Timed out by HTTP client |

## 6. Integration Points

### Harness (main.rs)

- `evaluate_pr_single_agent()`: Computes `tool_preamble` via `crb_tools::tool_prompt_section()` and passes as `extra_preamble` to `build_agent()`.
- `evaluate_pr_consensus()`: Computes `tool_preamble` and passes through consensus pipeline.

### Consensus (lib.rs)

- `build_reviewer_agent()`: New `tool_preamble` parameter, passed to `build_agent()`.
- `run_reviewers()`: Accepts `tool_preamble`, passes to `build_reviewer_agent()`.
- `run_consensus()`: Accepts `tool_preamble`, passes to `run_reviewers()`.
- `evaluate_pr_with_consensus()`: Accepts `tool_preamble`, passes to `run_consensus()`.

## 7. Tool Implementations

### ShellTool

- Executes commands via `sh -c`.
- 30s default timeout, 100 KB output cap.
- Returns `SpawnFailed`, `NonZeroExit`, `TimeoutElapsed`, or `OutputTooLarge`.

### ReadFileTool

- Reads files with path-safety checks (canonicalization, prefix check).
- 1 MB file size limit, 200 default max lines.
- Supports line-range reading via `start_line` and `max_lines`.
- Returns `IoError`, `PathOutsideRepo`, or `FileTooLarge`.

### GitTool

- Supports `log`, `diff`, `show`, `status` operations via unified `GitOperation` enum.
- Runs git via `std::process::Command` inside `tokio::task::spawn_blocking`.
- Returns `CommandFailed`, `NonZeroExit`, or `TimeoutElapsed`.

### MCPTool

- HTTP POST to MCP server's `call_tool` endpoint.
- JSON-RPC 2.0 protocol.
- Tool definitions fetched via `list_tools` endpoint.
- Configurable per-server (URL, auth, timeout, optional flag).
