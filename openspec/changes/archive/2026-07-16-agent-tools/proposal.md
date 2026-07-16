# Proposal: Agent Tool Calling

**Change ID:** agent-tools
**Status:** Draft
**Author:** Hermes Agent
**Date:** 2026-06-27

## Summary

Give each LLM reviewer agent access to a curated set of tools (ShellTool, ReadFileTool, GitTool, MCPTool) during code review, enabling them to grep, read files, check git history, and search external documentation — with per-role tool assignment and a 5-call budget to prevent runaway tool use.

## Why

LLM review agents currently have no tool access — they cannot grep, read files, check git history, or search docs. This severely limits their ability to find bugs across all four review roles (SA, CL, AR, SEC). Without tools, agents rely entirely on the diff text provided in their prompt, missing critical context that exists in the wider repository.

## What Changes

Implement four tool types (ShellTool, ReadFileTool, GitTool, MCPTool) with per-role assignment and a call budget system. Tool descriptions are injected into agent system prompts via the extra_preamble parameter on build_agent(). Per-role tool sets: SA gets [shell, read_file], CL/AR/SEC get [shell, read_file, git]. Budget: default 50 total calls, 20 per-tool.

## Scope

- **In scope:** 4 tool implementations (Shell, ReadFile, Git, MCP), per-role tool assignment, budget system, tool prompt section injected into agent system prompts.
- **Out of scope:** Actual attachment of tools via Rig's `Tool` trait (rig-core 0.39 doesn't support it cleanly at agent creation time), backend service for MCP, authentication for MCP servers.

## Key Design Decisions

1. **Prompt-based tool instructions** — Since rig-core 0.39 doesn't easily support attaching tools at agent creation, we inject tool descriptions into the system prompt. The agent "knows" what tools exist and their JSON schema, even though the tool backends aren't wired up yet.
2. **Per-role tool sets** — SA gets `[shell, read_file]`, CL/AR/SEC get `[shell, read_file, git]`. MCP tools are available to all roles when configured.
3. **Call budget** — Default 50 total calls, 20 per-tool, soft-stop (warns but allows over-budget calls).
4. **Build_agent extra_preamble** — Tool instructions are injected via the existing `extra_preamble` parameter on `build_agent()`.

## Directory Structure

```
review-harness/
└── crates/
    └── crb-tools/
        ├── src/
        │   ├── lib.rs        # tools_for_role(), tool_prompt_section()
        │   ├── shell.rs      # ShellTool
        │   ├── read_file.rs  # ReadFileTool
        │   ├── git.rs        # GitTool
        │   ├── mcp.rs        # MCPTool
        │   └── budget.rs     # ToolCallBudget, ToolCallTracker
        └── Cargo.toml
```
