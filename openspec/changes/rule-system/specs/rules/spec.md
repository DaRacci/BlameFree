# Delta for Rule System

## ADDED Requirements

### Requirement: Rule File Discovery

The system SHALL discover rule files from a directory (default `.crb/rules/`) by scanning for files with the `.md` extension.

#### Scenario: Discover rules from default directory
- GIVEN a project with `.crb/rules/` containing `python-standards.md`, `typescript-rules.md`, and `security.md`
- WHEN `RuleSet::load_from_dir(".crb/rules/")` is called
- THEN it returns a `RuleSet` containing 3 rules
- AND each rule has its `source_file` set to the absolute path of the originating file

#### Scenario: Discover rules from custom directory
- GIVEN a project with `config/rules/` containing `custom.md`
- WHEN `RuleSet::load_from_dir("config/rules/")` is called
- THEN it returns a `RuleSet` with 1 rule sourced from `config/rules/custom.md`

#### Scenario: Empty rules directory
- GIVEN a directory with no `.md` files
- WHEN `RuleSet::load_from_dir()` is called
- THEN it returns an empty `RuleSet` (no error)
- AND `format_preamble()` returns an empty string

#### Scenario: Nonexistent rules directory
- GIVEN a path that does not exist
- WHEN `RuleSet::load_from_dir()` is called
- THEN it returns an error indicating the directory was not found

---

### Requirement: YAML Frontmatter Parsing

The system SHALL parse YAML frontmatter delimited by `---` at the start of each `.md` rule file, extracting `description`, `globs`, and `alwaysApply` fields.

#### Scenario: Parse valid frontmatter with single glob
- GIVEN a file with content:
  ```markdown
  ---
  description: "Python coding standards"
  globs: "**/*.py"
  alwaysApply: false
  ---
  Follow PEP 8 conventions.
  ```
- WHEN `parse_rule_file()` is called
- THEN it returns a `Rule` with `description = Some("Python coding standards")`
- AND `globs = ["**/*.py"]`
- AND `always_apply = false`
- AND `body = "Follow PEP 8 conventions."`

#### Scenario: Parse valid frontmatter with multiple globs
- GIVEN a file with content:
  ```markdown
  ---
  description: "TypeScript best practices"
  globs: ["**/*.ts", "**/*.tsx"]
  alwaysApply: true
  ---
  Use interfaces for object shapes.
  ```
- WHEN `parse_rule_file()` is called
- THEN it returns a `Rule` with `globs = ["**/*.ts", "**/*.tsx"]`
- AND `always_apply = true`

#### Scenario: Parse file without frontmatter
- GIVEN a file with no `---` delimiter
- WHEN `parse_rule_file()` is called
- THEN it returns a `Rule` with `globs = []`
- AND `always_apply = true`
- AND `body` contains the full file content
- AND `description = None`

#### Scenario: Parse malformed frontmatter (unclosed YAML)
- GIVEN a file with only one `---` delimiter
- WHEN `parse_rule_file()` is called
- THEN it returns an error indicating malformed frontmatter

#### Scenario: Parse invalid YAML in frontmatter
- GIVEN a file with `---\ninvalid: : yaml\n---`
- WHEN `parse_rule_file()` is called
- THEN it returns an error from `serde_yaml` deserialization

#### Scenario: Parse frontmatter with missing optional fields
- GIVEN a file with just `---\nalwaysApply: true\n---`
- WHEN `parse_rule_file()` is called
- THEN it returns a `Rule` with `description = None` and `globs = []`

---

### Requirement: Glob-Based File Path Matching

The system SHALL match rules against file paths using the `globs` field with the `glob` crate's pattern matching.

#### Scenario: Exact glob match
- GIVEN a rule with `globs: ["src/**/*.py"]`
- WHEN checking against path `src/api/handler.py`
- THEN `rule_matches_path()` returns `true`

#### Scenario: Wildcard glob match
- GIVEN a rule with `globs: ["**/*.py"]`
- WHEN checking against path `tests/test_api.py`
- THEN `rule_matches_path()` returns `true`

#### Scenario: Nested directory glob match
- GIVEN a rule with `globs: ["**/migrations/**/*.py"]`
- WHEN checking against path `src/db/migrations/001_initial.py`
- THEN `rule_matches_path()` returns `true`

#### Scenario: Multiple glob patterns
- GIVEN a rule with `globs: ["**/*.ts", "**/*.tsx"]`
- WHEN checking against path `src/components/Button.tsx`
- THEN `rule_matches_path()` returns `true`

#### Scenario: No glob match
- GIVEN a rule with `globs: ["**/*.py"]`
- WHEN checking against path `src/main.go`
- THEN `rule_matches_path()` returns `false`

#### Scenario: Empty globs never match file paths
- GIVEN a rule with `globs: []`
- WHEN checking against any path
- THEN `rule_matches_path()` returns `false`

---

### Requirement: Language Detection

The system SHALL detect programming language from file extensions and support filtering rules by language.

#### Scenario: Detect Python from .py
- GIVEN a path `src/main.py`
- WHEN `detect_language(path)` is called
- THEN it returns `Some("python")`

#### Scenario: Detect Rust from .rs
- GIVEN a path `src/lib.rs`
- WHEN `detect_language(path)` is called
- THEN it returns `Some("rust")`

#### Scenario: Detect TypeScript from .ts and .tsx
- GIVEN paths `app.ts` and `component.tsx`
- WHEN `detect_language()` is called for each
- THEN the first returns `Some("typescript")` and the second returns `Some("typescript")`

#### Scenario: Detect unknown extension
- GIVEN a path `data.json`
- WHEN `detect_language(path)` is called
- THEN it returns `None`

#### Scenario: Detect repo languages from file set
- GIVEN file paths `["src/main.py", "src/api.go", "src/util.py"]`
- WHEN `detect_repo_languages()` is called
- THEN it returns `{"python", "go"}`

---

### Requirement: Always-On Rules

The system SHALL include rules with `alwaysApply: true` in every matching result, regardless of file paths.

#### Scenario: Always-on rules included unconditionally
- GIVEN a `RuleSet` with a rule where `always_apply = true` and `globs = ["**/*.py"]`
- WHEN `matching(&[PathBuf::from("src/main.go")])` is called
- THEN the result includes the always-apply rule even though no `.go` files match its globs

#### Scenario: Always-on rules combined with glob-matched rules
- GIVEN a `RuleSet` with an always-apply rule and a glob-matched rule for `**/*.py`
- WHEN `matching(&[PathBuf::from("src/main.py")])` is called
- THEN the result includes both the always-apply rule and the glob-matched rule

#### Scenario: Non-always rules excluded when no globs match
- GIVEN a `RuleSet` with a non-always rule for `**/*.py`
- WHEN `matching(&[PathBuf::from("src/main.go")])` is called
- THEN the result does NOT include the non-always Python rule

---

### Requirement: Preamble Formatting

The system SHALL format matched rules into a string suitable for injection into agent system prompts.

#### Scenario: Format matched rules as preamble
- GIVEN a `RuleSet` with 2 matched rules:
  - Rule 1: `description = "Security"`, `body = "Never log secrets."`
  - Rule 2: `description = Some("Python")`, `body = "Use type hints."`
- WHEN `format_preamble(&[PathBuf::from("src/main.py")])` is called
- THEN it returns a string containing:
  ```
  ## Applicable Project Rules

  ### Security
  Never log secrets.

  ### Python
  Use type hints.
  ```

#### Scenario: No matched rules returns empty string
- GIVEN a `RuleSet` with no rules matching the given files
- WHEN `format_preamble()` is called
- THEN it returns an empty string

#### Scenario: Rules without description omit heading
- GIVEN a rule with `description = None` and `body = "Always indent with 4 spaces."`
- WHEN `format_preamble()` includes it
- THEN the output contains the body but no `###` heading line for it

---

### Requirement: Agent Integration

The system SHALL accept an optional rules preamble in `build_agent()` and prepend it to the role-specific system prompt.

#### Scenario: Rules preamble prepended to role preamble
- GIVEN `build_agent(client, "gpt-4", "SA", Some("## Rules\n\nBe careful."))`
- WHEN the agent is built
- THEN the full system prompt starts with `## Rules\n\nBe careful.\n\n` followed by the SA role preamble

#### Scenario: Empty or absent preamble produces normal agent
- GIVEN `build_agent(client, "gpt-4", "SA", None)`
- WHEN the agent is built
- THEN the system prompt is the standard SA preamble with no prepended content

#### Scenario: Empty string preamble treated as absent
- GIVEN `build_agent(client, "gpt-4", "SA", Some(""))`
- WHEN the agent is built
- THEN the system prompt is the standard SA preamble (empty string treated as absent)

---

### Requirement: CLI Integration

The system SHALL provide `--rules-dir` and `--skip-rules` CLI flags in `crb-harness` for controlling rule loading.

#### Scenario: Default rules directory loaded at startup
- GIVEN a project with `.crb/rules/` containing rule files
- WHEN the harness starts with no `--rules-dir` flag
- THEN it loads rules from `.crb/rules/` (default)
- AND passes matching rules to agent builders for each PR

#### Scenario: Custom rules directory via --rules-dir
- GIVEN a project with rules in `config/custom-rules/`
- WHEN the harness starts with `--rules-dir config/custom-rules/`
- THEN it loads rules from `config/custom-rules/`

#### Scenario: Skip rules via --skip-rules
- GIVEN a project with `.crb/rules/`
- WHEN the harness starts with `--skip-rules`
- THEN no rules are loaded
- AND `build_agent()` receives `None` for the preamble
