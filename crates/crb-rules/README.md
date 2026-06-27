# crb-rules

YAML frontmatter rules system for the code review benchmark harness — inspired by Cursor, Continue, and Cline-style project rules.

- Markdown files with optional YAML frontmatter, loaded from a directory (default: `.crb/rules/`)
- Each rule has `globs` (glob patterns for file matching) and/or `always_apply: true` for universal rules
- [`RuleSet::matching()`] returns all rules whose globs match given file paths; [`RuleSet::format_preamble()`] builds a formatted string for injection into agent system prompts

## Key types

- [`Rule`](src/lib.rs) — Single rule: `description`, `globs`, `always_apply`, `body`, `source_file`
- [`RuleSet`](src/lib.rs) — Loaded rule collection with `load_from_dir()`, `matching()`, `matching_language()`, `format_preamble()`
- [`parse_rule_file()`](src/parser.rs) — Parses markdown with YAML frontmatter into a `Rule`
- [`detect_language()`](src/matcher.rs) — Language detection from file extensions

## Example rule file (`.crb/rules/python-standards.md`)

```markdown
---
description: Python Standards
globs: "**/*.py"
---
Use type hints for all function signatures.
Prefer dataclasses over manual `__init__` methods.
```
