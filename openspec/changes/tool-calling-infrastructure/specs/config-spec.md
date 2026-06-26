# Linter Configuration Specification

## Type: Schema Spec

## 1. File Location

The linter configuration lives in the harness config directory as
`linters.toml`, loaded at startup by `load_linter_config()`.

## 2. Schema

```toml
# linters.toml — defines all linters the harness can invoke

[linters.ruff]
name = "ruff"                        # Unique identifier used in logs/UI
cmd = ["ruff", "check"]             # Command + initial args (array, not string)
timeout_secs = 60                    # Per-file timeout (optional, default 60)
output_format = "json"              # "json" or "text" (selects parser)
optional = false                     # If true, missing binary is a warning not error

[linters.eslint]
name = "eslint"
cmd = ["npx", "eslint", "--format", "json"]
timeout_secs = 90
output_format = "json"
optional = true                      # npx may not be available everywhere

[linters.govet]
name = "go vet"
cmd = ["go", "vet", "./..."]
timeout_secs = 120
output_format = "text"
optional = true

[linters.rust-analyzer-check]
name = "cargo check"
cmd = ["cargo", "check", "--message-format=json"]
timeout_secs = 300
output_format = "json"
optional = true
```

## 3. Rust Representation

```rust
/// A single linter definition from the TOML config.
#[derive(Deserialize, Debug, Clone)]
struct LinterConfig {
    /// Display name for the linter.
    pub name: String,
    /// Command and arguments (never a shell string).
    pub cmd: Vec<String>,
    /// Per-invocation timeout in seconds.
    pub timeout_secs: Option<u64>,
    /// Output format: "json" or "text".
    pub output_format: String,
    /// If true, a missing binary is non-fatal.
    pub optional: Option<bool>,
}

/// Top-level config file structure.
#[derive(Deserialize, Debug)]
struct LinterConfigFile {
    pub linters: HashMap<String, LinterConfig>,
}

/// Load and validate the configuration.
fn load_linter_config(path: &str) -> Result<HashMap<String, LinterConfig>, ConfigError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| ConfigError::IoError(e))?;
    let config: LinterConfigFile = toml::from_str(&content)
        .map_err(|e| ConfigError::ParseError(e.to_string()))?;
    Ok(config.linters)
}
```

## 4. Validation Rules

- `cmd` must have at least one element.
- `output_format` must be `"json"` or `"text"`.
- Linter names must be non-empty and unique.
- A default `linters.toml` is generated if not present.
