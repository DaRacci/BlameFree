# crb-agents

Agent construction and prompt management for LLM-based code review agents.

- Provides `build_agent()` to construct Rig `Agent` instances with role-specific preambles, supporting both a template engine with agent manifest rendering and an embedded prompt library
- Includes a [`PromptLibrary`] for file-based prompt loading with `{variable}` template substitution, falling back to built-in defaults
- [`TemplateEngine`](src/templates.rs) for Handlebars-based template rendering with section file composition
- [`AgentManifest`](src/manifest.rs) for role-specific agent definitions loaded from `prompts/agents/*.md`
- Re-exports the [`Finding`] struct from `crb-tools` — the core structured output type for all code review findings across the harness

## Key types

- [`Finding`](src/lib.rs) — Structured finding: `file`, `line`, `message`, `severity`, `rule_code`, `severity_audited`, `severity_audit_reason` (re-exported from `crb-tools`)
- [`build_agent()`](src/lib.rs) — Factory function accepting client, model, role, rules preamble, prompt library, optional template engine + agent manifest, template vars, extra preamble, optional workdir, additional params, and optional `exp14_submit_finding` collector
- [`PromptLibrary`](src/prompts.rs) — Manages role-specific prompts with `load_from_dir()`, `get()`, and `render()` methods
- [`TemplateEngine`](src/templates.rs) — Handlebars engine with section file composition for `agent.hbs` rendering
- [`AgentManifest`](src/manifest.rs) — Loaded from `prompts/agents/*.md`, maps role names to preamble variables
