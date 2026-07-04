# LinterTool Specification

## Type: Implementation Spec

## 1. LinterArgs

```rust
#[derive(Deserialize, JsonSchema)]
struct LinterArgs {
    /// Absolute or relative filesystem path to the repository root.
    repo_path: String,
}
```

## 2. Finding

```rust
#[derive(Serialize, Debug, Clone)]
struct Finding {
    /// Severity: "error", "warning", or "info" (maps from linter conventions).
    severity: String,
    /// File path relative to repository root.
    path: String,
    /// 1-indexed line number.
    line: u32,
    /// 1-indexed column number.
    column: u32,
    /// Human-readable description of the finding.
    message: String,
    /// Linter-specific rule/check code (e.g. "F841", "no-unused-vars").
    code: Option<String>,
}
```

## 3. LinterError

```rust
#[derive(Debug)]
enum LinterError {
    SubprocessFailed(std::io::Error),
    NonZeroExit(i32, String),
    TimeoutElapsed,
    ParseFailed(String),
}
```

Implement `std::error::Error`, `std::fmt::Display`, and `From<std::io::Error>`.

## 4. LinterTool

```rust
struct LinterTool {
    name: &'static str,
    cmd: Vec<String>,
    parser: fn(&str) -> Result<Vec<Finding>, LinterError>,
    timeout: Duration,
}
```

Implements `rig::tool::Tool` with:
- `NAME` -> `"linter"` (overridden per-instance via constructor)
- `Args` -> `LinterArgs`
- `Output` -> `Vec<Finding>`
- `Error` -> `LinterError`

## 5. LinterConfig

```rust
#[derive(Deserialize)]
struct LinterConfig {
    name: String,
    cmd: Vec<String>,
    timeout_secs: Option<u64>,
    output_format: String,  // "json" or "text"
    optional: Option<bool>, // default false
}

#[derive(Deserialize)]
struct LinterConfigFile {
    linters: HashMap<String, LinterConfig>,
}
```

## 6. Parser Signatures

```rust
fn parse_ruff_output(stdout: &str) -> Result<Vec<Finding>, LinterError>;
fn parse_eslint_output(stdout: &str) -> Result<Vec<Finding>, LinterError>;
fn parse_govet_output(stdout: &str) -> Result<Vec<Finding>, LinterError>;
```

### Ruff Output Format (JSON)
```json
[
  {
    "code": "F841",
    "filename": "src/main.py",
    "location": { "row": 10, "column": 5 },
    "message": "Local variable `x` is assigned but never used"
  }
]
```

### ESLint Output Format (JSON)
```json
[
  {
    "filePath": "/repo/src/app.js",
    "messages": [
      {
        "ruleId": "no-unused-vars",
        "severity": 2,
        "line": 15,
        "column": 3,
        "message": "'x' is assigned but never used"
      }
    ]
  }
]
```

### go vet Output Format (text)
```
./src/main.go:25:2: unreachable code
./src/util.go:42:6: X is unused
```
