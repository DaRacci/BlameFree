# Design: Migration & Validation

## Architecture

```
review-harness/
├── Cargo.toml                           # [workspace] members = ["crates/*"]
└── crates/
    ├── crb-harness/                     # Binary — CLI entrypoint: --validate, --ci, --cached-diffs
    │   ├── Cargo.toml                   # deps: crb-tools, crb-reporting, crb-agents, clap
    │   └── src/main.rs
    ├── crb-tools/                       # Git tool operations (scaffolding port)
    │   ├── Cargo.toml                   # deps: serde
    │   └── src/lib.rs                   # git helpers: clean_repo, checkout_pr, extract_diff
    └── crb-reporting/                   # Validation (baseline comparison + output)
        ├── Cargo.toml                   # deps: serde, serde_json
        └── src/lib.rs                   # load_baseline, compute_delta, validate_run
```

## Scaffolding Module (in `crates/crb-tools/src/lib.rs`)

```rust
// crates/crb-tools/src/lib.rs — git helper functions
use std::path::Path;
use std::process::Command;

pub struct PrInfo {
    pub owner: String,
    pub repo: String,
    pub pr_num: u32,
    pub base_branch: String,
    pub pr_branch: String,
}

/// Run `git clean -fdx` to ensure pristine state.
pub fn clean_repo(repo_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .args(["clean", "-fdx"])
        .current_dir(repo_path)
        .output()?;
    anyhow::ensure!(output.status.success(), "git clean failed: {}", String::from_utf8_lossy(&output.stderr));
    Ok(())
}

/// Checkout base branch, then fetch PR head.
pub fn checkout_pr(repo_path: &Path, pr_info: &PrInfo) -> Result<(), Box<dyn std::error::Error>> {
    Command::new("git")
        .args(["checkout", &pr_info.base_branch])
        .current_dir(repo_path)
        .output()?;
    Command::new("git")
        .args(["fetch", "origin", &format!("pull/{}/head:{}", pr_info.pr_num, pr_info.pr_branch)])
        .current_dir(repo_path)
        .output()?;
    Ok(())
}

/// Return unified diff between base and PR head.
pub fn extract_diff(repo_path: &Path, base: &str, head: &str) -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .args(["diff", &format!("{}...{}", base, head)])
        .current_dir(repo_path)
        .output()?;
    anyhow::ensure!(output.status.success(), "git diff failed: {}", String::from_utf8_lossy(&output.stderr));
    Ok(String::from_utf8(output.stdout)?)
}
```

## Validation Module (in `crates/crb-reporting/src/lib.rs`)

```rust
// crates/crb-reporting/src/lib.rs — baseline comparison
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Baseline {
    pub precision: f64,
    pub recall: f64,
    pub f1: f64,
    pub per_pr: HashMap<String, PrMetrics>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrMetrics {
    pub precision: f64,
    pub recall: f64,
    pub f1: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BaselineDelta {
    pub metric: String,
    pub expected: f64,
    pub actual: f64,
    pub delta: f64,
    pub threshold: f64,
    pub passed: bool,
}

const THRESHOLDS: &[(&str, f64)] = &[
    ("precision", 0.03),
    ("recall", 0.03),
    ("f1", 0.02),
];

pub fn load_baseline(path: &Path) -> Result<Baseline, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

pub fn compute_delta(
    results: &HashMap<String, f64>,
    baseline: &Baseline,
) -> Vec<BaselineDelta> {
    let mut deltas = Vec::new();
    for &(metric, threshold) in THRESHOLDS {
        let actual = *results.get(metric).unwrap_or(&0.0);
        let expected = match metric {
            "precision" => baseline.precision,
            "recall" => baseline.recall,
            "f1" => baseline.f1,
            _ => continue,
        };
        let delta = actual - expected;
        deltas.push(BaselineDelta {
            metric: metric.to_string(),
            expected,
            actual,
            delta,
            threshold,
            passed: delta.abs() <= threshold,
        });
    }
    deltas
}

pub fn validate_run(
    results_path: &Path,
    baseline_path: &Path,
) -> Result<bool, Box<dyn std::error::Error>> {
    let results: HashMap<String, f64> = load_results(results_path)?;
    let baseline = load_baseline(baseline_path)?;
    let deltas = compute_delta(&results, &baseline);
    let all_passed = deltas.iter().all(|d| d.passed);
    Ok(all_passed)
}

fn load_results(path: &Path) -> Result<HashMap<String, f64>, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}
```

## CI Integration

```bash
# ./run_ci.sh — intended for cron or GitHub Actions
export OPENROUTER_API_KEY="${OPENROUTER_API_KEY}"
cd review-harness
cargo build --release -p crb-harness
./target/release/crb-harness --ci \
    --dataset datasets/golden_comments/ \
    --repos repos/ \
    --output results/$(date +%Y%m%d_%H%M%S)/ \
    --validate --baseline baselines/v5.14.json \
    --model deepseek/deepseek-v4-flash \
    --judge-model openai/gpt-4o-mini \
    --concurrency 24
```

## Key Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Git operations | `std::process::Command` in `crates/crb-tools` | Reduces dependency weight; git CLI is stable and available in all CI environments |
| Serialization | serde + serde_json | Standard Rust JSON toolkit; zero extra deps for the project |
| Baseline format | JSON file with summary metrics + per-PR breakdown | Matches existing v5.14 output format |
| Noise threshold | ±2pp F1, ±3pp precision/recall | Based on empirical variance observed in v5.12–v5.14 runs |
| CI output | JSON summary to stdout | Machine-parseable for downstream dashboards |
| Cache strategy | Directory timestamp + manifest hash | Skip re-scaffolding if manifest unchanged and diff files exist |
| CLI parsing | clap v4 | Industry-standard Rust argument parser with derive macros |
