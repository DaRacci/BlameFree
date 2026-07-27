# Design: MVP Core Harness

## Architecture

BlameFree/                          # Cargo workspace root
├── Cargo.toml                           # [workspace] members = ["crates/*"]
├── crates/
│   ├── riv-harness/                     # Binary — orchestrates benchmark
│   │   ├── Cargo.toml                   # deps: riv-agents, riv-judge, riv-consensus, riv-tools, riv-reporting, rig-core, tokio, clap, tracing
│   │   └── src/main.rs
│   ├── riv-aggregator/                  # aggregate_findings.py port
│   │   ├── Cargo.toml                   # deps: serde, serde_json, regex
│   │   └── src/{lib.rs, main.rs}
│   ├── riv-auditor/                     # severity_auditor.py port
│   │   ├── Cargo.toml                   # deps: serde, serde_json, regex
│   │   └── src/{lib.rs, main.rs}
│   ├── riv-agents/                      # Agent builders + prompt templates
│   │   ├── Cargo.toml                   # deps: rig-core, serde, schemars
│   │   └── src/lib.rs
│   ├── riv-judge/                       # Judge + metrics
│   │   ├── Cargo.toml                   # deps: rig-core, serde, schemars
│   │   └── src/lib.rs
│   ├── riv-consensus/                   # Multi-agent orchestration
│   │   ├── Cargo.toml                   # deps: rig-core, tokio, riv-agents, riv-judge
│   │   └── src/lib.rs
│   ├── riv-tools/                       # Tool trait implementations
│   │   ├── Cargo.toml                   # deps: rig-core, tokio, serde, schemars
│   │   └── src/lib.rs
│   └── riv-reporting/                   # Output formatting
│       ├── Cargo.toml                   # deps: serde, serde_json, csv
│       └── src/lib.rs
└── datasets/
    └── golden_comments/

## Inter-crate dependency graph

riv-harness (binary)
  ├── riv-consensus
  │     ├── riv-agents
  │     ├── riv-judge
  │     └── riv-tools
  ├── riv-reporting
  ├── riv-aggregator (used as library, also has standalone CLI)
  └── riv-auditor (used as library, also has standalone CLI)

## Key decisions
- riv-aggregator and riv-auditor: dual lib+bin — usable as `cargo run -p riv-aggregator` standalone
- riv-agents: defines Finding type (shared across consensus, tools, judge)
- riv-judge: defines JudgeVerdict type and Martian prompt
- riv-consensus: orchestration — depends on riv-agents for agent creation, riv-judge for evaluation, riv-tools for linter integration
- riv-tools: linter/git tool implementations via rig Tool trait
- riv-reporting: pure output — JSON + CSV, no LLM deps

## Core Loop (Pseudocode)

```rust
use rig::prelude::*;
use rig::providers::openai::{self, Client};
use tokio::task::JoinSet;
use std::collections::HashMap;

#[derive(serde::Deserialize, schemars::JsonSchema)]
struct Finding {
    file: Option<String>,
    line: Option<u32>,
    message: String,
    severity: String,
    rule_code: Option<String>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
struct JudgeVerdict {
    reasoning: String,
    match_: bool,
    confidence: f64,
}

async fn evaluate_pr(
    pr_entry: &GoldenCommentEntry,
    client: &Client,
    model: &str,
    sem: &tokio::sync::Semaphore,
) -> Result<PrResult> {
    let _permit = sem.acquire().await?;
    let diff = load_diff(&pr_entry.url)?;

    // 4 concurrent agent calls via JoinSet
    let mut set = JoinSet::new();
    for role in ["SA", "CL", "AR", "SEC"] {
        let agent = build_agent(client, model, role, &diff);
        set.spawn(async move { agent.extract::<Vec<Finding>>(&diff).await });
    }
    let mut findings = Vec::new();
    while let Some(result) = set.join_next().await {
        findings.extend(result??);
    }

    // Judge each finding against golden comments
    let judge = build_judge(client, config.judge_model);
    let mut verdicts = Vec::new();
    for finding in &findings {
        for gc in &pr_entry.comments {
            let prompt = format_judge_prompt(gc, finding);
            verdicts.push(judge.extract::<JudgeVerdict>(&prompt).await?);
        }
    }

    Ok(compute_metrics(verdicts))
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = CliArgs::parse();
    let prs = load_golden_comments(&config.dataset_dir)?;
    let client = Client::from_env()?;
    let sem = tokio::sync::Semaphore::new(config.concurrency);

    let mut set = JoinSet::new();
    for pr in &prs {
        let client = client.clone();
        let sem = sem.clone();
        set.spawn(async move { evaluate_pr(pr, &client, &config.model, &sem).await });
    }
    let mut results = Vec::new();
    while let Some(result) = set.join_next().await {
        results.push(result??);
    }
    write_report(&results, &config.output_dir)?;
    Ok(())
}
```

## Key Design Decisions

| Decision | Choice | Rationale |
||----------|--------|-----------|
|| Async runtime | tokio (rt-multi-thread, macros) | Industry standard Rust async. JoinSet for concurrent PR eval |
|| LLM client | rig-core 0.39 | Provider-agnostic traits (CompletionModel, Extractor), published on crates.io, MIT licensed |
|| Workspace layout | `crates/*` pattern | Each component independently compilable, testable, publishable |
|| Dual lib+bin | riv-aggregator, riv-auditor | Reusable as library, usable as standalone CLI |
|| Shared types | `riv-agents` crate | Finding, Severity, Candidate shared across all crates |
|| Diff source | Pre-scaffolded files | Avoid git dependency in MVP. Add git operations later |
|| Judge prompt | Martian JUDGE_PROMPT verbatim | Proven effective, MIT licensed |
|| Config model | clap derive + struct | Type-safe CLI parsing, env var fallback via clap env feature |
|| Output format | JSON via serde + CSV | Machine-readable + human-readable |
|| Error handling | anyhow::Result + skip-on-failure | Don't fail entire run on one PR failure. Log for post-hoc audit |

## Dependencies

### Workspace root (`Cargo.toml`)
```toml
[workspace]
members = ["crates/*"]
```

### Main binary (`crates/riv-harness/Cargo.toml`)
```toml
[dependencies]
riv-agents = { path = "../riv-agents" }
riv-judge = { path = "../riv-judge" }
riv-consensus = { path = "../riv-consensus" }
riv-tools = { path = "../riv-tools" }
riv-reporting = { path = "../riv-reporting" }
riv-aggregator = { path = "../riv-aggregator" }
riv-auditor = { path = "../riv-auditor" }
rig-core = "0.39"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync"] }
clap = { version = "4", features = ["derive", "env"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

## Prompt Template Approach

Each agent role gets a fixed system prompt + the PR diff injected as context:

```rust
use rig::providers::openai::Client;
use rig::agent::AgentBuilder;

fn build_agent(client: &Client, model: &str, role: &str, diff: &str) -> rig::agent::Agent<_> {
    let preamble = match role {
        "SA" => "You are a static analysis specialist...",
        "CL" => "You are a code logic expert...",
        "AR" => "You are an architecture reviewer...",
        "SEC" => "You are a security specialist...",
        _ => "You are a code reviewer.",
    };
    client
        .agent(model)
        .preamble(preamble)
        .build()
}
```
