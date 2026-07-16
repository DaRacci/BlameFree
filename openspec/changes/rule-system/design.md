# Design: Rule System (crb-rules)

## Architecture

### New crate: `crates/crb-rules/`

```
review-harness/
├── Cargo.toml                              # [workspace] members already covers "crates/*"
├── crates/
│   ├── crb-rules/                          # NEW — library crate
│   │   ├── Cargo.toml                      # deps: serde, serde_yaml, glob, anyhow, tracing
│   │   └── src/
│   │       ├── lib.rs                      # Public API: Rule, RuleSet, detect_language
│   │       ├── parser.rs                   # YAML frontmatter parsing logic
│   │       ├── matcher.rs                  # Glob + language matching
│   │       └── preamble.rs                 # System prompt formatting
│   ├── crb-agents/                         # MODIFIED — accepts rules_preamble
│   ├── crb-harness/                        # MODIFIED — --rules-dir flag, startup loading
│   └── ...
└── .crb/
    └── rules/                              # Default rules directory (project-level)
        ├── python-standards.md
        ├── typescript-rules.md
        ├── security.md
        └── go-conventions.md
```

### Inter-crate dependency

```
crb-harness (binary)
  ├── crb-rules          # NEW — load rules at startup, format preamble
  ├── crb-agents         # MODIFIED — build_agent() accepts rules_preamble
  ├── crb-consensus
  │     ├── crb-agents
  │     ├── crb-judge
  │     └── crb-tools
  ...
```

### Integration flow

```
┌─────────────────────────────────────────────────────────────┐
│                     crb-harness main loop                    │
│                                                              │
│  1. Parse CLI args (--rules-dir .crb/rules/)                 │
│  2. Load RuleSet::load_from_dir(rules_dir)                   │
│  3. For each PR:                                             │
│     a. Collect changed file paths from PR diff               │
│     b. Call ruleset.matching(&changed_paths) -> Vec<&Rule>    │
│     c. Call ruleset.format_preamble(&changed_paths) -> String │
│     d. Pass preamble string to build_agent(..., preamble)    │
│     e. Agent evaluates with enriched system prompt           │
│                                                              │
│  crb-agents::build_agent() now accepts optional preamble:    │
│    client.agent(model)                                       │
│      .preamble(format!("{rules_preamble}\n\n{role_preamble}"))│
│      .build()                                                │
└─────────────────────────────────────────────────────────────┘
```

## Core Types

```rust
/// A single rule loaded from a .md file with YAML frontmatter.
#[derive(Debug, Clone)]
pub struct Rule {
    pub description: Option<String>,
    pub globs: Vec<String>,
    pub always_apply: bool,
    pub body: String,             // Markdown content after frontmatter
    pub source_file: PathBuf,     // Origin file path
}

/// Intermediate deserialization target for YAML frontmatter.
#[derive(Debug, Clone, Deserialize)]
struct RuleMetadata {
    description: Option<String>,
    #[serde(default)]
    globs: Option<GlobsField>,
    #[serde(default)]
    always_apply: Option<bool>,
}

/// Accept single string or array of strings for globs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum GlobsField {
    Single(String),
    Multiple(Vec<String>),
}

/// A loaded ruleset, cached from a directory.
#[derive(Debug, Clone)]
pub struct RuleSet {
    pub rules: Vec<Rule>,
    pub always_rules: Vec<Rule>,   // Cached subset: always_apply == true
    pub source_dir: PathBuf,
}

impl RuleSet {
    /// Load all `.md` rule files from `dir`.
    ///
    /// If `dir` does not exist or contains no `.md` files, returns an empty
    /// [`RuleSet`] (no error) so that the harness works without any rules
    /// configured.
    pub fn load_from_dir(dir: &Path) -> Result<Self>;
    pub fn matching(&self, file_paths: &[PathBuf]) -> Vec<&Rule>;
    pub fn matching_language(&self, language: &Language) -> Vec<&Rule>;
    pub fn format_preamble(&self, file_paths: &[PathBuf]) -> String;
}
```

## Parsing Strategy

Use a simple `---` delimiter split to extract YAML frontmatter from markdown:

```rust
fn parse_rule_file(content: &str, source_file: &Path) -> Result<Rule> {
    if !content.starts_with("---") {
        // No frontmatter — treat as always-apply rule with empty metadata
        return Ok(Rule {
            description: None,
            globs: vec![],
            always_apply: true,
            body: content.trim().to_string(),
            source_file: source_file.to_path_buf(),
        });
    }

    let parts: Vec<&str> = content.splitn(3, "---").collect();
    if parts.len() < 3 {
        anyhow::bail!("Malformed frontmatter: expected --- blocks");
    }

    let yaml_str = parts[1];
    let body = parts[2].trim().to_string();
    let metadata: RuleMetadata = serde_yaml::from_str(yaml_str)?;

    let globs = match metadata.globs {
        GlobsField::Single(s) => vec![s],
        GlobsField::Multiple(v) => v,
    };

    Ok(Rule {
        description: metadata.description,
        globs,
        always_apply: metadata.always_apply,
        body,
        source_file: source_file.to_path_buf(),
    })
}
```

## Matching

### Glob matching
Use the `glob` crate's `Pattern::new()` and `matches()` for each rule's `globs` against each file path:

```rust
use glob::Pattern;

fn rule_matches_path(rule: &Rule, path: &Path) -> bool {
    if rule.globs.is_empty() {
        return false; // Empty globs means no file-path match (always-apply handles separately)
    }
    let path_str = path.to_string_lossy();
    rule.globs.iter().any(|g| {
        Pattern::new(g).map(|p| p.matches(&path_str)).unwrap_or(false)
    })
}
```

### Language detection
Map file extensions to language identifiers:

```rust
pub fn detect_language(path: &Path) -> Option<Language> {
    let ext = path.extension()?.to_str()?;
    EXTENSION_MAP
        .iter()
        .find(|(e, _)| *e == ext)
        .map(|(_, lang)| *lang)
}

pub fn detect_repo_languages(files: &[PathBuf]) -> HashSet<Language> {
    files.iter().filter_map(|f| detect_language(f)).collect()
}
```

### Rule matching composite
`RuleSet::matching()` returns always-apply rules PLUS glob-matched rules for the given file paths:

```rust
pub fn matching(&self, file_paths: &[PathBuf]) -> Vec<&Rule> {
    let mut matched: Vec<&Rule> = self.always_rules.iter().collect();
    for rule in &self.rules {
        if rule.always_apply { continue; } // already included
        if file_paths.iter().any(|p| rule_matches_path(rule, p)) {
            matched.push(rule);
        }
    }
    matched
}
```

### Preamble formatting
`format_preamble()` returns a formatted string suitable for agent system prompt injection:

```rust
pub fn format_preamble(&self, file_paths: &[PathBuf]) -> String {
    let matched = self.matching(file_paths);
    if matched.is_empty() {
        return String::new();
    }
    let mut preamble = String::from("## Applicable Project Rules\n\n");
    for rule in &matched {
        if let Some(desc) = &rule.description {
            preamble.push_str(&format!("### {}\n", desc));
        }
        preamble.push_str(&rule.body);
        preamble.push('\n');
        preamble.push('\n');
    }
    preamble
}
```

## Integration Points

### crb-agents: `build_agent()` signature change

```rust
// BEFORE
pub fn build_agent(client: &openai::Client, model: &str, role: &str) -> Agent<ResponsesCompletionModel>;

// AFTER
pub fn build_agent(
    client: &openai::Client,
    model: &str,
    role: &str,
    rules_preamble: Option<&str>,  // NEW
) -> Agent<ResponsesCompletionModel>;
```

Internally prepend the rules preamble before the role-specific preamble:
```rust
let full_preamble = match rules_preamble {
    Some(rp) if !rp.is_empty() => format!("{}\n\n{}", rp, preamble),
    _ => preamble.to_string(),
};
client.agent(model).preamble(&full_preamble).build()
```

### crb-harness: CLI changes

```rust
#[derive(clap::Parser)]
struct CliArgs {
    // ... existing fields ...

    /// Path to rules directory (e.g., .crb/rules/)
    #[arg(long, default_value = ".crb/rules/")]
    rules_dir: PathBuf,

    /// Skip rule loading entirely
    #[arg(long)]
    skip_rules: bool,
}
```

In `main()`:
```rust
let ruleset = if !args.skip_rules && args.rules_dir.exists() {
    Some(RuleSet::load_from_dir(&args.rules_dir)?)
} else {
    None
};
```

In `evaluate_pr()`:
```rust
let preamble = ruleset.as_ref().map(|rs| rs.format_preamble(&pr_files));
let agent = build_agent(client, model, role, preamble.as_deref());
```

## Dependencies

### Workspace root (`Cargo.toml`) — no changes needed

`members = ["crates/*"]` already picks up the new crate automatically.

### New crate (`crates/crb-rules/Cargo.toml`)

```toml
[package]
name = "crb-rules"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
serde_yaml = "0.9"
glob = "0.3"
anyhow = { workspace = true }
tracing = { workspace = true }

[dev-dependencies]
tempfile = "3"
```

## Key Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Rule file format | `.md` with YAML frontmatter | De facto industry standard (Cursor, Continue, Cline). No proprietary extension needed. |
| Rules directory | `.crb/rules/` | Follows `.cursor/rules/` and `.continue/rules/` pattern with `crb` prefix. |
| Glob field | String or `Vec<String>` | Supports both `globs: "**/*.py"` (single) and `globs: ["**/*.ts", "**/*.tsx"]` (multi). |
| No frontmatter default | always-apply rule with empty metadata | Graceful degradation — a `.md` file without `---` is treated as an always-on rule. |
| Preamble injection | Prepend to existing role pramable | Rules are project-level context, role pramable is role-level context. Rules come first. |
| Language detection | File extension mapping only | Simple, fast, no ML dependency. Sufficient for matching rules to repo languages. |
| Glob crate | `glob = "0.3"` | Lightweight, well-maintained, `Pattern::matches()` is the standard Rust glob matcher. |
| YAML crate | `serde_yaml = "0.9"` | Works with existing serde derives. No need for a custom YAML parser. |
| Caching | `always_rules` cached at load time | Avoids re-filtering always-apply rules on every `matching()` call. |
|| Empty / nonexistent rules directory | Returns empty RuleSet (no error) | Harness works without any rules configured. `load_from_dir` on nonexistent path returns `Ok(empty)` not `Err`. |
