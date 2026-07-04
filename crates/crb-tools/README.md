# crb-tools

Tool implementations for the code review benchmark harness — provides Rig `Tool` trait implementations for LLM-agent-in-the-loop tool calling.

- **Agent tools**: [`ShellTool`], [`ReadFileTool`], [`GitTool`], [`RigCoreMcpTool`] — callable by LLM agents during review
- **Linter tools**: [`LinterTool`] with parsers for ruff (JSON), ESLint (JSON), `go vet` (text), plus stubs for staticcheck, rubocop, and checkstyle
- **Budget tools**: [`ToolCallBudget`] / [`ToolCallTracker`] for limiting tool usage per agent
- **Per-role assignment**: [`tools_for_role()`] returns appropriate tools for each reviewer role; [`tool_prompt_section()`] renders the tool-calling preamble for the system prompt

## Key types

- [`LinterTool`](src/lib.rs) — Generic linter wrapper parameterized by command, parser function, and timeout
- [`LinterConfig`](src/lib.rs) — TOML-based linter configuration with name, cmd, timeout, output_format, optional flag
- [`load_linter_config()`](src/lib.rs) — Loads and validates linter configuration from a TOML file
- Parsers: `parse_ruff_output`, `parse_eslint_output`, `parse_govet_output`
