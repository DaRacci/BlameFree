# crb-agents

Agent construction and prompt management for LLM-based code review agents.

- Provides `build_agent()` to construct Rig `Agent` instances with role-specific preambles (SA, CL, AR, SEC)
- Includes a [`PromptLibrary`] for file-based prompt loading with `{variable}` template substitution, falling back to built-in defaults
- Defines the [`Finding`] struct — the core structured output type for all code review findings across the harness

## Key types

- [`Finding`](src/lib.rs) — Structured finding: `file`, `line`, `message`, `severity`, `rule_code`, `severity_audited`, `severity_audit_reason`
- [`build_agent()`](src/lib.rs) — Factory function taking a client, model, role, optional rules preamble, optional prompt library, template vars, and extra preamble
- [`PromptLibrary`](src/prompts.rs) — Manages role-specific prompts with `load_from_dir()`, `get()`, and `render()` methods
- [`AGENT_ROLES`](src/lib.rs) — Constant `&[&str]` listing the four role identifiers: `["SA", "CL", "AR", "SEC"]`
