# Design: Multi-Agent Consensus

## 1. Overview

The consensus pattern orchestrates N independent LLM agents (reviewers) that
each review the same PR diff and produce structured findings. A separate judge
agent then evaluates those findings against golden comments to compute
precision, recall, and F1 scores.

The flow is purely **parallel generation + aggregation** — agents do not
communicate with each other.

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│  SA Agent    │     │  CL Agent    │     │  AR Agent    │     │  SEC Agent   │
│ (Static An.) │     │ (Code Logic) │     │ (Architect.) │     │ (Security)   │
└──────┬───────┘     └──────┬───────┘     └──────┬───────┘     └──────┬───────┘
       │                    │                    │                    │
       └─────────┬──────────┴──────────┬─────────┘                    │
                 │                     │                              │
                 ▼                     ▼                              ▼
        ┌──────────────────────────────────────────────────────────┐
        │              Vec<Vec<Finding>> (all agents)              │
        └────────────────────────┬─────────────────────────────────┘
                                 │ flatten
                                 ▼
        ┌──────────────────────────────────────────────────────────┐
        │              Vec<Finding> (candidates)                   │
        └────────────────────────┬─────────────────────────────────┘
                                 │
                                 ▼
        ┌──────────────────────────────────────────────────────────┐
        │  Judge Agent: match candidates against golden comments   │
        │  Output: Vec<(GoldenComment, TP|FP|FN)>                  │
        └────────────────────────┬─────────────────────────────────┘
                                 │
                                 ▼
        ┌──────────────────────────────────────────────────────────┐
        │           ConsensusReport { precision, recall, f1 }     │
        └──────────────────────────────────────────────────────────┘
```

## 2. Agent Roles

### 2.1 Role Definitions

| Role | Code | Focus | System Prompt Theme |
|------|------|-------|--------------------|
| Static Analysis | SA | Code quality, lint-style issues | 'Expert code reviewer focused on code quality, style, formatting, and best practices' |
| Code Logic | CL | Correctness, edge cases, bugs | 'Expert code reviewer focused on logic errors, correctness, off-by-one, race conditions' |
| Architecture | AR | Design patterns, coupling, structure | 'Expert code reviewer focused on architecture, coupling, cohesion, design patterns' |
| Security | SEC | Vulnerabilities, auth, injection | 'Expert code reviewer focused on security vulnerabilities, auth, injection, crypto' |

### 2.2 ReviewerAgent Struct

```rust
use rig::extractor::Extractor;
use schemars::JsonSchema;
use serde::Deserialize;

/// A single finding from any reviewer agent.
#[derive(Deserialize, JsonSchema, Debug, Clone)]
pub struct Finding {
    /// File path relative to repo root.
    pub file: String,
    /// Line number (1-indexed).
    pub line: u32,
    /// Severity: "error", "warning", "info".
    pub severity: String,
    /// Human-readable description of the issue.
    pub message: String,
    /// Optional rule/category code (e.g. "SA-001", "SEC-003").
    pub code: Option<String>,
}

/// Role identifier for reviewer agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    SA,  // Static Analysis
    CL,  // Code Logic
    AR,  // Architecture
    SEC, // Security
}

impl Role {
    pub fn system_prompt(self) -> &'static str {
        match self {
            Role::SA => concat!(
                "You are a senior code reviewer specialized in STATIC ANALYSIS. ",
                "Focus on code quality issues: unused variables, dead code, ",
                "formatting problems, naming conventions, duplicated code, ",
                "and deviations from best practices. ",
                "For each issue, output a structured finding with file, line, ",
                "severity (error/warning/info), message, and a category code ",
                "prefix 'SA-'."
            ),
            Role::CL => concat!(
                "You are a senior code reviewer specialized in CODE LOGIC. ",
                "Focus on correctness: off-by-one errors, null pointer ",
                "dereferences, incorrect conditionals, missing edge cases, ",
                "race conditions, and incorrect assumptions about data flow. ",
                "For each issue, output a structured finding with file, line, ",
                "severity (error/warning/info), message, and a category code ",
                "prefix 'CL-'."
            ),
            Role::AR => concat!(
                "You are a senior code reviewer specialized in ARCHITECTURE. ",
                "Focus on design: tight coupling, low cohesion, inappropriate ",
                "abstraction boundaries, violation of separation of concerns, ",
                "over-engineering, and architectural anti-patterns. ",
                "For each issue, output a structured finding with file, line, ",
                "severity (error/warning/info), message, and a category code ",
                "prefix 'AR-'."
            ),
            Role::SEC => concat!(
                "You are a senior code reviewer specialized in SECURITY. ",
                "Focus on vulnerabilities: SQL injection, XSS, command injection, ",
                "path traversal, insecure cryptographic usage, authentication ",
                "bypasses, and unsafe deserialization. ",
                "For each issue, output a structured finding with file, line, ",
                "severity (error/warning/info), message, and a category code ",
                "prefix 'SEC-'."
            ),
        }
    }
}

/// Configuration for a single reviewer agent.
pub struct ReviewerConfig {
    pub role: Role,
    pub model: String,        // e.g. "gpt-4o"
    pub max_findings: usize,  // max findings per agent
}
```

### 2.3 Agent Construction via rig

```rust
use rig::providers::openai;
use rig::extractor::Extractor;

/// Build a reviewer agent for the given role.
fn build_reviewer_agent(config: &ReviewerConfig, diff: &str) -> impl Extractor<Output = Vec<Finding>> {
    let client = openai::Client::from_env();
    client
        .extractor(config.model.clone())
        .system_prompt(config.role.system_prompt())
        .preamble(format!(
            "Review the following PR diff. Output findings in JSON format:\n\n{}",
            diff
        ))
        .build::<Vec<Finding>>()
}
```

## 3. Consensus Flow

### 3.1 Parallel Agent Execution

```rust
use tokio::task::JoinSet;

/// Run all four reviewer agents concurrently.
async fn run_reviewers(
    configs: Vec<ReviewerConfig>,
    diff: &str,
) -> Vec<(Role, Vec<Finding>)> {
    let mut set = JoinSet::new();

    for cfg in configs {
        let diff = diff.to_string();
        set.spawn(async move {
            let agent = build_reviewer_agent(&cfg, &diff);
            let findings = agent.extract().await.unwrap_or_default();
            (cfg.role, findings)
        });
    }

    let mut results = Vec::new();
    while let Some(res) = set.join_next().await {
        results.push(res.unwrap());
    }
    results
}
```

### 3.2 Golden Comment Matching

Golden comments are ground-truth annotations for a given PR diff. Each golden
comment has a file, line, expected message (or regex), and severity.

```rust
#[derive(Debug, Clone)]
pub struct GoldenComment {
    pub file: String,
    pub line: u32,
    pub message_regex: String,  // regex to match against finding.message
    pub severity: String,
    pub source: String,         // role that created this golden (or "any")
}

#[derive(Debug, Clone, PartialEq)]
pub enum MatchResult {
    TruePositive,   // finding matches a golden (correct)
    FalsePositive,  // finding matches no golden (extra)
    FalseNegative,  // golden has no matching finding (missed)
}
```

### 3.3 Judge Agent

The judge receives the list of golden comments and candidate findings (flattened
from all agents). For each golden, it evaluates candidate findings and
determines TP/FP/FN.

```rust
/// The judge prompt template.
const JUDGE_PROMPT: &str = r#"
You are an impartial judge evaluating code review quality.
You are given:
1. GOLDEN COMMENTS: the correct, ground-truth findings that should have been produced.
2. CANDIDATE FINDINGS: findings produced by N independent reviewer agents.

For each GOLDEN COMMENT, determine:
- If ANY candidate finding matches it (file, line, semantic content match) → TRUE POSITIVE
- If NO candidate finding matches it → FALSE NEGATIVE

Then, for any CANDIDATE FINDING that did NOT match any golden → FALSE POSITIVE.

Output: JSON array of { "golden_index": int, "candidate_index": int | null, "result": "TP" | "FP" | "FN" }
"#;

/// Run the judge for a single golden comment against all candidate findings.
async fn judge_comment(
    golden: &GoldenComment,
    candidates: &[Finding],
) -> MatchResult {
    // Build a judge extractor with both golden and candidates in context,
    // or use a simpler heuristic: exact file + line + severity match with
    // fuzzy message matching via the LLM.
    todo!()
}
```

### 3.4 Full Orchestration

```rust
pub struct ConsensusReport {
    pub agents: Vec<(Role, Vec<Finding>)>,
    pub true_positives: Vec<(GoldenComment, Finding)>,
    pub false_positives: Vec<Finding>,
    pub false_negatives: Vec<GoldenComment>,
    pub precision: f64,
    pub recall: f64,
    pub f1: f64,
}

/// Run the full consensus pipeline.
pub async fn run_consensus(
    diff: &str,
    goldens: Vec<GoldenComment>,
    reviewer_configs: Vec<ReviewerConfig>,
) -> ConsensusReport {
    // 1. Run all agents concurrently
    let agent_results = run_reviewers(reviewer_configs, diff).await;

    // 2. Flatten all findings
    let all_findings: Vec<Finding> = agent_results
        .iter()
        .flat_map(|(_, findings)| findings.clone())
        .collect();

    // 3. Judge: match candidates against goldens
    let mut tp = Vec::new();
    let mut fn = Vec::new();
    let mut fp_candidates: Vec<Finding> = all_findings.clone();

    for golden in &goldens {
        match judge_comment(golden, &all_findings).await {
            MatchResult::TruePositive => {
                // Find the matching finding
                tp.push((golden.clone(), /* matched finding */));
            }
            MatchResult::FalseNegative => {
                fn.push(golden.clone());
            }
            _ => {}
        }
    }

    // 4. Remaining candidates that matched nothing are FPs
    // (logic depends on judge output)

    // 5. Compute metrics
    let precision = if tp.len() + fp_candidates.len() > 0 {
        tp.len() as f64 / (tp.len() + fp_candidates.len()) as f64
    } else {
        1.0
    };
    let recall = if tp.len() + fn.len() > 0 {
        tp.len() as f64 / (tp.len() + fn.len()) as f64
    } else {
        1.0
    };
    let f1 = if precision + recall > 0.0 {
        2.0 * precision * recall / (precision + recall)
    } else {
        0.0
    };

    ConsensusReport {
        agents: agent_results,
        true_positives: tp,
        false_positives: fp_candidates,
        false_negatives: fn,
        precision,
        recall,
        f1,
    }
}
```

## 4. Module Structure

```
review-harness/src/consensus/
├── mod.rs           # Re-exports: run_consensus, ConsensusReport, Role, Finding
│                    # Re-exports: GoldenComment, MatchResult, ReviewerConfig
├── agent.rs        # Role enum, ReviewerConfig, build_reviewer_agent()
├── judge.rs        # JudgeAgent, JUDGE_PROMPT, judge_comment()
└── types.rs        # Finding, GoldenComment, MatchResult, ConsensusReport
```

## 5. Error Handling

| Error Scenario | Behavior |
|----------------|----------|
| Agent call fails (network error) | Return empty Vec<Finding> for that agent; log warning |
| Agent returns malformed output | Extractor returns Err; log and skip |
| Judge call fails | Log warning; fall back to heuristic matching (exact file+line+severity) |
| One agent times out | JoinSet yields timeout error; continue with remaining agents |
| Empty diff | All agents return empty findings; precision/recall/F1 = 1.0 |

## 6. Configuration

Configuration is minimal — no TOML file needed. The caller provides
`Vec<ReviewerConfig>` inline:

```rust
let configs = vec![
    ReviewerConfig { role: Role::SA, model: "gpt-4o".into(), max_findings: 20 },
    ReviewerConfig { role: Role::CL, model: "gpt-4o".into(), max_findings: 20 },
    ReviewerConfig { role: Role::AR, model: "gpt-4o".into(), max_findings: 20 },
    ReviewerConfig { role: Role::SEC, model: "gpt-4o".into(), max_findings: 20 },
];
```

An optional consensus pass can be enabled where the judge receives all agent
outputs collectively and decides if disagreement resolution is needed (TBD in
future iteration).
