# Tasks: Rule System (crb-rules Crate)

## Phase 1 — Crate Scaffold

- [x] Create `crates/crb-rules/` directory with `Cargo.toml`:
  - Package name: `crb-rules`, version `0.1.0`, edition `2021`
  - Dependencies: `serde` (workspace), `serde_json` (workspace), `serde_yaml = "0.9"`, `glob = "0.3"`, `anyhow` (workspace), `tracing` (workspace)
  - Dev-dependencies: `tempfile = "3"`
- [x] Add to workspace — no change needed: `members = ["crates/*"]` already covers it
- [x] Create `src/lib.rs` with module declarations: `mod parser; mod matcher; mod preamble;`
- [x] Create `src/parser.rs`, `src/matcher.rs`, `src/preamble.rs` as empty stubs
- [x] Verify `cargo check -p crb-rules` succeeds

## Phase 2 — Core Types and Parsing

- [x] Define `Rule` struct in `lib.rs`:
  - Fields: `description: Option<String>`, `globs: Vec<String>`, `always_apply: bool`, `body: String`, `source_file: PathBuf`
  - Derives: `Debug, Clone, Serialize, Deserialize`
- [x] Define `RuleMetadata` struct in `parser.rs`:
  - Fields: `description: Option<String>`, `globs: Option<GlobsField>`, `always_apply: Option<bool>`
  - Derives: `Debug, Clone, Serialize, Deserialize`
- [x] Define `GlobsField` untagged enum in `parser.rs`:
  - Variants: `Single(String)`, `Multiple(Vec<String>)`
  - Derives: `Debug, Clone, Serialize, Deserialize`
- [x] Implement `parse_rule_file()` in `parser.rs`:
  - Check for `---` prefix — if absent, return always-apply rule with full content as body
  - Split on `---` with `splitn(3, "---")` — bail on fewer than 3 parts
  - Deserialize YAML frontmatter via `serde_yaml::from_str::<RuleMetadata>()`
  - Convert `GlobsField` to `Vec<String>`
  - Return fully populated `Rule`
- [x] Write unit tests:
  - Valid frontmatter with single glob
  - Valid frontmatter with multiple globs
  - File without frontmatter (defaults to always-apply)
  - Malformed frontmatter (single `---`)
  - Invalid YAML in frontmatter
  - Missing optional fields

## Phase 3 — Rule Discovery and Set Loading

- [x] Define `RuleSet` struct in `lib.rs`:
  - Fields: `rules: Vec<Rule>`, `always_rules: Vec<Rule>`, `source_dir: PathBuf`
  - Derives: `Debug, Clone`
- [x] Implement `RuleSet::load_from_dir()`:
  - Validate directory exists (or return empty set if optional behavior)
  - Walk directory for `*.md` files
  - Read each file, call `parse_rule_file()`
  - Separate always-apply rules into `always_rules` cache
  - Handle I/O errors gracefully (log + skip, or return error)
- [x] Write unit tests:
  - Load rules from directory with 3 files
  - Empty directory returns empty `RuleSet`
  - Nonexistent directory returns error

## Phase 4 — Matching

- [x] Implement `rule_matches_path()` in `matcher.rs`:
  - If `rule.globs` is empty, return `false`
  - Use `glob::Pattern::new()` and `Pattern::matches()` for each glob
  - Return `true` if any glob matches
- [x] Implement `detect_language()` in `matcher.rs`:
  - Map file extensions to language strings (python, rust, typescript, javascript, go, ruby, java, kotlin, swift, csharp, cpp, c, php, scala)
  - Return `Option<&'static str>`
- [x] Implement `detect_repo_languages()` in `matcher.rs`:
  - Collect unique languages from a slice of `PathBuf`
  - Return `HashSet<String>`
- [x] Implement `RuleSet::matching()`:
  - Start with all `always_rules`
  - For each non-always rule, check if any file path matches its globs
  - Return deduplicated `Vec<&Rule>`
- [x] Implement `RuleSet::matching_language()`:
  - Filter rules where at least one of the files of the given language matches
- [x] Write unit tests:
  - Exact glob match
  - Wildcard (`**/*.py`) match
  - Nested directory glob match
  - Multiple glob patterns (any match succeeds)
  - No match returns empty
  - Empty globs never match
  - Language detection for all supported extensions
  - Unknown extension returns `None`
  - Repo language detection from multiple files
  - Always-on rules included regardless of file paths
  - Always-on + glob-matched rules combined

## Phase 5 — Preamble Formatting

- [x] Implement `RuleSet::format_preamble()` in `preamble.rs`:
  - Call `self.matching(file_paths)`
  - If empty, return empty string
  - Build string: `## Applicable Project Rules\n\n`
  - For each rule, if `description` is `Some`, add `### {description}\n`
  - Append `body`, then `\n\n`
  - Return the complete preamble string
- [x] Write unit tests:
  - Format matched rules as preamble with headings
  - No matched rules returns empty string
  - Rules without description omit heading

## Phase 6 — Integration: crb-agents

- [x] Modify `build_agent()` in `crates/crb-agents/src/lib.rs`:
  - Add `rules_preamble: Option<&str>` parameter
  - If `rules_preamble` is `Some` and non-empty, prepend to role preamble:
    ```rust
    let full_preamble = match rules_preamble {
        Some(rp) if !rp.is_empty() => format!("{}\n\n{}", rp, role_preamble),
        _ => role_preamble.to_string(),
    };
    ```
  - Use `full_preamble` instead of `preamble` in `client.agent(model).preamble(...)`
- [x] Update all call sites of `build_agent()` in the workspace to pass `None` (backward compatible)
- [x] Verify `cargo check` passes across workspace

## Phase 7 — Integration: crb-harness

- [x] Add `crb-rules` dependency to `crates/crb-harness/Cargo.toml`
- [x] Add CLI flags to `CliArgs`:
  - `--rules-dir <PATH>` with default `.crb/rules/`
  - `--skip-rules` (flag, disables rule loading)
- [x] In `main()`:
  - Load `RuleSet` at startup if not `--skip-rules` and directory exists
  - Pass `ruleset` reference into `evaluate_pr()`
- [x] In `evaluate_pr()`:
  - Collect changed file paths from PR diff
  - Call `ruleset.format_preamble(&pr_files)` to get preamble string
  - Pass `Some(preamble_str)` to each `build_agent()` call
- [x] Verify `cargo check -p crb-harness` succeeds

## Phase 8 — Testing

- [x] Create `crates/crb-rules/tests/` integration tests:
  - End-to-end: create temp directory with rule files, load, match, format
  - Empty directory, mix of matching and non-matching rules
  - Rules with no frontmatter treated as always-apply
  - Rules with single vs multiple glob patterns
- [x] Create test fixture `.md` files in `tests/fixtures/`:
  - `python-standards.md`
  - `typescript-rules.md`
  - `security.md` (alwaysApply)
  - `no-frontmatter.md` (plain markdown)
- [x] Write harness-level integration test:
  - Start harness with `--rules-dir test_fixtures/ --skip-linters`
  - Verify preamble appears in agent system prompt
- [x] Run full `cargo test -p crb-rules` and verify all tests pass

## Future Work (Out of Scope)

- [ ] Conditional rules (`when`, `if` clauses à la Cline)
- [ ] AGENTS.md single-file support
- [ ] Rule file hot-reloading during long-running sessions
- [ ] Rule priority/ordering beyond always-apply override
- [ ] Subdirectory scanning within `.crb/rules/`
- [ ] Rule validation/`check` subcommand
