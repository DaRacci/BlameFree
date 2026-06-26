# Design: Linter Integration (Rust + rig Tool Trait)

## Architecture

The linter runner lives in `linters.rs` under `crb-harness/src/`. It is called from `evaluate_pr()` in `main.rs` or `agents.rs` alongside LLM agent calls.

```
crb-harness/src/
├── main.rs          # evaluate_pr() entry point
├── agents.rs        # LLM agent definitions
├── judge.rs         # finding evaluation pipeline
├── reporting.rs     # output formatting
├── config.rs        # TOML config loading
├── linters.rs       # NEW — Tool trait implementations
│   ├── struct LinterConfig { name, cmd, parser_kind, timeout_secs }
│   ├── impl Tool for RuffLinter
│   ├── impl Tool for EslintLinter
│   ├── impl Tool for GoVetLinter
│   ├── impl Tool for RubocopLinter
│   ├── impl Tool for CheckstyleLinter
│   └── fn run_linters(pr_path, language) -> Vec<Finding>
└── findings.rs      # NEW — shared Finding schema
```

### `rig::tool::Tool` Trait (from `rig-core`)

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    const NAME: &'static str;
    type Error: std::error::Error + Send + Sync;
    type Args: DeserializeOwned + JsonSchema + Send + Sync;
    type Output: Serialize + Send + Sync;
    async fn definition(&self, prompt: String) -> ToolDefinition;
    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error>;
}
```

Each linter struct implements this trait. `Args` is a struct with the repo path; `Output` is a `Vec<Finding>`. The `definition()` method returns a `ToolDefinition` with an auto-generated JSON Schema (via `schemars`) describing the argument shape and return type — no manual schema writing.

### Integration in `evaluate_pr()`

```rust
// In main.rs or agents.rs
async fn evaluate_pr(pr: PrEntry, config: &Config) -> PrResult {
    let repo_path = checkout_pr(&pr.url).await?;

    // Concurrent: LLM agents + linters via JoinSet
    let mut join_set = JoinSet::new();
    for role in ["SA", "CL", "AR", "SEC"] {
        let agent = create_llm_agent(role, &pr.diff);
        join_set.spawn(agent.call());
    }
    for linter in get_linters_for_language(&pr.language, &config) {
        join_set.spawn(run_linter_tool(linter, &repo_path));
    }

    let mut findings = Vec::new();
    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(f)) => findings.extend(f),
            Ok(Err(e)) => warn!("Linter failed: {e}"),
            Err(e) => warn!("Task panicked: {e}"),
        }
    }
    // ... judge findings against golden comments
}
```

## Finding Schema (Rust)

```rust
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
pub struct Finding {
    pub source: String,         // "linter:ruff" | "llm:SA"
    pub file: Option<String>,   // source file path (relative to repo)
    pub line: Option<u32>,      // line number
    pub message: String,        // human-readable description
    pub severity: String,       // "Critical" | "High" | "Medium" | "Low"
    pub rule: Option<String>,   // linter rule code (e.g., "F401")
}
```

The struct derives `JsonSchema` from `schemars`, which means the rig `Tool` trait automatically generates a JSON Schema for the output type — no manual schema maintenance.

## Linter Configuration (TOML)

Linters are configured in a `linters.toml` file (loaded by `config.rs`), not hardcoded:

```toml
[linters.python]
[[linters.python.tools]]
name = "ruff"
cmd = ["ruff", "check", "{repo_path}", "--output-format=json"]
parser_kind = "ruff"
timeout_secs = 60

[linters.typescript]
[[linters.typescript.tools]]
name = "eslint"
cmd = ["eslint", "{repo_path}/src/", "--format=json"]
parser_kind = "eslint"
timeout_secs = 60

[linters.go]
[[linters.go.tools]]
name = "go-vet"
cmd = ["go", "vet", "./..."]
parser_kind = "govet"
timeout_secs = 120

[[linters.go.tools]]
name = "staticcheck"
cmd = ["staticcheck", "./..."]
parser_kind = "staticcheck"
timeout_secs = 120

[linters.ruby]
[[linters.ruby.tools]]
name = "rubocop"
cmd = ["rubocop", "--format=json", "{repo_path}/"]
parser_kind = "rubocop"
timeout_secs = 60

[linters.java]
[[linters.java.tools]]
name = "checkstyle"
cmd = ["java", "-jar", "checkstyle.jar", "-c", "sun_checks.xml", "{repo_path}/"]
parser_kind = "checkstyle"
timeout_secs = 120
```

## LinterTool Struct

```rust
#[derive(Deserialize, Clone)]
pub struct LinterToolConfig {
    pub name: String,
    pub cmd: Vec<String>,
    pub parser_kind: String,
    pub timeout_secs: u64,
}

pub struct LinterTool {
    config: LinterToolConfig,
    repo_path: PathBuf,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct LinterArgs {
    pub path: String,
}

impl Tool for LinterTool {
    const NAME: &'static str = "linter";  // overridden per instance
    type Error = Box<dyn std::error::Error + Send + Sync>;
    type Args = LinterArgs;
    type Output = Vec<Finding>;

    async fn definition(&self, prompt: String) -> ToolDefinition {
        // Auto-generates JSON Schema from Finding + LinterArgs — uses schemars
        ToolDefinition {
            name: self.config.name.clone(),
            description: format!("Run {} linter on the repository", self.config.name),
            parameters: serde_json::to_value(LinterArgs::schema()).unwrap(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let cmd_str: Vec<String> = self.config.cmd.iter()
            .map(|s| s.replace("{repo_path}", &self.repo_path.to_string_lossy()))
            .collect();

        let output = tokio::time::timeout(
            Duration::from_secs(self.config.timeout_secs),
            tokio::process::Command::new(&cmd_str[0])
                .args(&cmd_str[1..])
                .output()
        ).await??;

        // Route to parser_kind-specific parser
        parse_linter_output(&self.config.parser_kind, &output.stdout, &output.stderr)
    }
}
```

## Key Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Language | Rust | Same lang as harness; Tool trait gives auto JSON Schema; native perf |
| Subprocess approach | `tokio::process::Command` | Async, non-blocking, idiomatic Rust |
| Tool trait | `rig::tool::Tool` | Auto-generated tool definitions for LLM; no manual schema writing |
| Linter configuration | External TOML | No recompilation to add/remove linters |
| Linter installation | Pre-installed in container | Avoid cargo/apt overhead per run |
| Linter vs. LLM weighting | Equal (no source priority) | Measure each source independently |
| Timeout per linter | 60-120s (configurable in TOML) | Eslint/checkstyle on large repos can be slow |
| Caching | None for MVP | Linters on clean checkouts only; add caching later |
| Missing linter handling | Warn + skip | Don't fail the run if a linter isn't installed |
| Parser dispatch | Match on `parser_kind` string | Simple routing; extensible without trait objects |
