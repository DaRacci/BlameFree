# Change: Rule System (crb-rules Crate)

## Intent
Add a rule system to the benchmark harness that loads `.md` rule files with YAML frontmatter from a `.crb/rules/` directory, matches them by file path glob and language detection, and injects matching rules into agent system prompts at evaluation time.

## Scope
New `crates/crb-rules/` library crate with rule discovery, YAML frontmatter parsing, glob matching, language detection, and preamble formatting. Integration hooks in `crb-agents` (optional `rules_preamble` parameter) and `crb-harness` (CLI flag, startup loading).

## Approach
Follow the de facto industry convergence pattern established by Cursor, Continue, and Cline: YAML frontmatter + markdown body in `.md` files, directory-based discovery under `.crb/rules/`, `alwaysApply` and `globs` fields for matching. The crate exposes a `RuleSet` struct that loads and caches rules, then provides `matching()` and `format_preamble()` methods for the harness to inject into agent prompts at the `build_agent()` call site.
