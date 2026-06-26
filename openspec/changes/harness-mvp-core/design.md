# Design: MVP Core Harness

## Architecture

crb-harness/
├── Cargo.toml
├── src/
│   ├── main.rs           # ~150 lines — CLI (clap derive), main loop, JoinSet orchestration
│   ├── agents.rs          # ~120 lines — prompt templates, rig AgentBuilder, provider routing
│   ├── judge.rs           # ~150 lines — Martian-compatible judge via rig Extractor
│   ├── reporting.rs       # ~100 lines — aggregation, CSV/JSON output via serde
│   └── config.rs          # ~80 lines  — CliArgs, provider config, environment handling
└── datasets/
    └── golden_comments/   # copied from Martian (MIT license)

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
|----------|--------|-----------|
| Async runtime | tokio (rt-multi-thread, macros) | Industry standard Rust async. JoinSet for concurrent PR eval |
| LLM client | rig-core 0.39 | Provider-agnostic traits (CompletionModel, Extractor), published on crates.io, MIT licensed |
| Diff source | Pre-scaffolded files | Avoid git dependency in MVP. Add git operations later |
| Judge prompt | Martian JUDGE_PROMPT verbatim | Proven effective, MIT licensed |
| Config model | clap derive + struct | Type-safe CLI parsing, env var fallback via clap env feature |
| Output format | JSON via serde + CSV | Machine-readable + human-readable |
| Error handling | anyhow::Result + skip-on-failure | Don't fail entire run on one PR failure. Log for post-hoc audit |

## Dependencies

```toml
[dependencies]
rig-core = "0.39"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync"] }
clap = { version = "4", features = ["derive", "env"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
schemars = "0.8"
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
csv = "1.3"
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
