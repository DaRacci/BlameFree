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
        │  (cache → LLM → Jaccard fallback)                       │
        │  Output: Vec<(GoldenComment, TP|FP|FN)>                  │
        └────────────────────────┬─────────────────────────────────┘
                                 │
                                 ▼
        ┌──────────────────────────────────────────────────────────┐
        │           ConsensusReport { tp, fp, fn, analytics }     │
        │           Metrics computed via MetricsProvider trait     │
        └──────────────────────────────────────────────────────────┘
```

## 2. Agent Roles

### 2.1 Role Definitions

Roles are **dynamic strings** (`Role(String)`), not a fixed enum. They are loaded
at runtime from the prompt manifest (`prompts/agents/*.md`). Each agent markdown
file contains YAML frontmatter with metadata and a markdown body with the
role-specific prompt.

Typical roles include:

| Abbreviation | Role Name | Focus |
|--------------|-----------|-------|
| SA | Static Analysis | Code quality, lint-style issues |
| CL | Code Logic | Correctness, edge cases, bugs |
| AR | Architecture | Design patterns, coupling, structure |
| SEC | Security | Vulnerabilities, auth, injection |
| GEN | Generalist (optional) | General code review for small PRs |

### 2.2 Finding Struct

Located in `crates/crb-shared/src/finding.rs`:

```rust
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct Finding {
    /// Source file path where the issue was found, if available.
    pub file: Option<String>,
    /// Line number in the source file, if available.
    pub line: Option<u32>,
    /// Human-readable description of the finding.
    pub message: String,
    /// The agents claimed severity level of the finding.
    pub severity: Severity,
    /// Optional rule or check identifier (e.g. "S001", "R101").
    pub rule_code: Option<String>,
    /// Whether the severity has been audited/downgraded.
    pub severity_audited: bool,
    /// Reason for the severity audit result.
    pub severity_audit_reason: Option<String>,
    /// Evidence supporting the finding.
    pub evidence: Option<String>,
    /// Path trace / call chain.
    pub path_trace: Option<String>,
    /// Self reported Confidence level.
    pub confidence: Option<ConfidenceLevel>,
    /// Agent tag that found this issue.
    pub found_by: Option<String>,
    /// Number of agents that flagged this finding.
    pub agent_count: Option<u64>,
    /// Whether this finding was cross-validated.
    pub cross_validated: bool,
    /// How many agents cross-validated this finding.
    pub cross_validated_by: Option<u64>,
    /// How many original findings were merged.
    pub merged_from: Option<u64>,
}
```

All fields have serde aliases for robust LLM output parsing.

### 2.3 AgentEntry (replaces ReviewerConfig)

Agents are defined by `AgentEntry` records parsed from markdown files via `crb-agents`:

```rust
pub struct AgentEntry {
    pub role_name: String,
    pub role_abbreviation: String,    // e.g. "SA"
    pub role_domain: String,
    pub role_anti_hallucination_rules: Option<String>,
    pub role_review_methodology: Option<String>,
    pub generalist_agent: bool,
    pub incompatible_with_roles: Vec<String>,
    pub role_prompt: String,          // markdown body
}
```

### 2.4 Agent Construction via `crb-agents`

Agents are built through `crb_agents::build_agent()`, which uses the
`PromptLibrary` singleton to render agent prompts from embedded Handlebars
templates:

```rust
use crb_agents::build_agent;
use crb_agents::prompts::PromptLibrary;

let agent = build_agent(
    &client,        // Arc<openai::Client>
    &model,         // Model wrapper
    agent_entry,    // &AgentEntry from PromptLibrary
    rules_preamble, // Option<&str> (from RuleSet)
    template_vars,  // Option<&HashMap<String, Value>>
    extra_preamble, // Option<&str> (tool instructions)
    additional_params, // Option<serde_json::Value>
    tool_server_handle, // ToolServerHandle
)
.output_schema::<Vec<Finding>>()
.build();
```

Agent results are parsed as `Vec<Finding>` and capped at `config.max_findings`.

## 3. Consensus Flow

### 3.1 Parallel Agent Execution

Reviewer execution is handled in `crates/crb-harness/src/pipeline.rs` via the
`evaluate()` function and its private `run_reviewers()` helper:

```rust
pub async fn evaluate(mut diff: Diff, config: &EvalConfig) -> Result<Vec<Finding>> {
    diff::preprocess_diff(&mut diff);
    let linters = run_linters(config);
    let reviewers = run_reviewers(&diff, config);
    let (mut all_findings, reviewer_findings) = tokio::join!(linters, reviewers);
    // ...post-processing, metrics, events...
    Ok(all_findings)
}
```

The `run_reviewers()` function:

1. Calls `get_agents_for_diff()` for adaptive agent selection (small PR → generalist).
2. Spawns one `JoinSet` task per agent entry.
3. Each task calls `build_agent()` + `agent.prompt(diff.raw)`.
4. Parses JSON output as `Vec<Finding>`, caps at `max_findings`.
5. Logs and skips failures.
6. Returns flattened findings.

### 3.2 Adaptive Dispatch

Located in `crates/crb-consensus/src/adaptive.rs`:

```rust
pub fn get_agents_for_diff(
    diff: &Diff,
    selected_agents: &[&'static AgentEntry],
) -> Vec<&'static AgentEntry>
```

- Small diffs (≤3 files, ≤200 lines) with a generalist available → single GEN agent.
- Diffs touching `.go`, `.rs`, `.java`, `.cpp`, `.ts`, `.tsx` etc. → full panel.
- Otherwise → all non-generalist agents.

### 3.3 Golden Comment Matching

Golden comments are ground-truth annotations loaded from JSON dataset files:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenComment {
    pub comment: String,    // expected comment text
    pub severity: Severity, // expected severity
}
```

Golden comments have **no file/line/message_regex/source fields**. Matching is
done semantically against finding messages, not by positional attributes.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchResult {
    TruePositive,   // finding matches a golden
    FalsePositive,  // finding matches no golden
    FalseNegative,  // golden has no matching finding
}
```

### 3.4 Judge Agent

Located in `crates/crb-consensus/src/judge.rs`. Implements a **cache → LLM → Jaccard**
pipeline:

1. **Content-addressed cache**: Key = `sha256(judge_prompt_hash + finding_message + golden_comment + judge_model)`.
   Cache-first lookup avoids redundant API calls.

2. **LLM judge**: For each pre-filtered candidate, send a structured prompt asking
   whether the finding matches the golden. Judge returns `{ match: bool, reasoning, confidence }`.

3. **Jaccard fallback**: If LLM found no match, compute Jaccard word-overlap similarity.
   Threshold: **0.3** (matching Python `step3_judge_comments.py`).

```rust
pub async fn judge_comment(
    golden: &GoldenComment,
    candidates: &[Finding],
    judge: &Agent<ResponsesCompletionModel>,
    judge_model: &str,
    cache: Arc<dyn CacheBackend>,
    judge_prompt_hash: &str,
    judge_api_calls: &mut usize,
) -> MatchResult
```

### 3.5 Full Orchestration

Located in `crates/crb-consensus/src/pipeline.rs`:

```rust
pub struct ConsensusReport {
    pub agents: Vec<(Role, Vec<Finding>)>,
    pub true_positives: Vec<(GoldenComment, Finding)>,
    pub false_positives: Vec<Finding>,
    pub false_negatives: Vec<GoldenComment>,
    pub analytics: AnalyticsSnapshot,
}
```

Metrics are computed via the `MetricsProvider` trait (in `crb-types/src/benchmark.rs`)
rather than stored directly:

```rust
pub trait MetricsProvider {
    fn true_positives(&self) -> usize;
    fn false_positives(&self) -> usize;
    fn false_negatives(&self) -> usize;

    fn precision(&self) -> f64 { /* TP / (TP + FP), 0.0 on 0/0 */ }
    fn recall(&self) -> f64 { /* TP / (TP + FN), 0.0 on 0/0 */ }
    fn f1(&self) -> f64 { /* 2 * P * R / (P + R), 0.0 on 0/0 */ }
}
```

The pipeline entry point:

```rust
/// Run the consensus judging step on already-completed review findings.
pub async fn run_consensus_post(
    agents: Vec<(Role, Vec<Finding>)>,  // reviewer results
    goldens: Vec<GoldenComment>,        // ground truth
    judge: &Agent<ResponsesCompletionModel>,  // pre-built judge agent
    judge_model: &str,
    cache: Arc<dyn CacheBackend>,
    judge_prompt_hash: &str,
) -> ConsensusReport
```

**Flow:**
1. Pool and sort candidate findings from all agents.
2. For each golden, call `judge_comment()` (cache → LLM → Jaccard).
3. First match → `TruePositive` (matched finding removed from pool).
4. Remaining unmatched goldens → `FalseNegative`.
5. Remaining unmatched findings → `FalsePositive`.
6. Build `ConsensusReport`.

## 4. Module Structure

```
review-harness/
├── Cargo.toml                     # [workspace] members = ["crates/*"]
└── crates/
    ├── crb-consensus/             # Multi-agent orchestration & judging
    │   ├── Cargo.toml
    │   └── src/
    │       ├── lib.rs             # Role, MatchResult, ConsensusReport
    │       ├── judge.rs           # judge_comment() (cache + LLM + Jaccard)
    │       ├── pipeline.rs        # run_consensus_post()
    │       └── adaptive.rs        # get_agents_for_diff(), adaptive dispatch
    ├── crb-harness/
    │   └── src/
    │       ├── pipeline.rs        # evaluate(), run_reviewers(), run_linters()
    │       └── eval.rs            # EvalConfig
    ├── crb-agents/
    │   └── src/
    │       ├── lib.rs             # build_agent()
    │       ├── agent.rs           # AgentEntry
    │       └── prompts.rs         # PromptLibrary (singleton)
    ├── crb-shared/
    │   └── src/
    │       ├── finding.rs         # Finding struct
    │       ├── severity.rs        # Severity enum
    │       └── jaccard.rs         # jaccard_similarity()
    ├── crb-reporting/
    │   └── src/
    │       ├── golden.rs          # GoldenComment, GoldenCommentEntry
    │       └── cost.rs            # AnalyticsSnapshot, AnalyticsTracker
    └── crb-types/
        └── src/
            └── benchmark.rs       # MetricsProvider, JudgeVerdict
```

## 5. Error Handling

| Error Scenario | Behavior |
|----------------|----------|
| Agent call fails (network error) | Return empty `Vec<Finding>` for that agent; log warning |
| Agent returns malformed output | `serde_json::from_str` fails → empty `Vec`; log warning |
| Judge LLM call fails | Log warning; return `{ match: false }` verdict (falls through to Jaccard) |
| One agent times out | `JoinSet` timeout → log warning; continue with remaining agents |
| Empty diff | Agents return empty findings; precision/recall/F1 = 0.0 |
| No agents configured | `run_reviewers` returns `Vec::new()` immediately |

## 6. Configuration

Configuration is provided via `EvalConfig` in `crates/crb-harness/src/eval.rs`:

```rust
pub struct EvalConfig {
    pub identifier: String,
    pub strategy: EvalStrategy,         // Single | Panel
    pub model: Model,                   // Model wrapper (string + config)
    pub reasoning_effort: Option<ReasoningEffort>,
    pub client: Arc<openai::Client>,
    pub cache: Option<Arc<dyn CacheBackend>>,
    pub cost_tracker: Arc<AnalyticsTracker>,
    pub tool_handle: ToolServerHandle,
    pub dashboard_tx: Option<tokio::sync::broadcast::Sender<RunEvent>>,
    pub agents: &'static [&'static AgentEntry],  // from PromptLibrary
    pub repo_root: PathBuf,
    pub max_findings: usize,
    pub judge_model: String,
    pub judge: Agent<ResponsesCompletionModel>,  // pre-built judge agent
    pub linters_only: bool,
    pub linter_configs: Option<Arc<HashMap<String, LinterConfig>>>,
    pub ruleset: Option<Arc<RuleSet>>,
    pub template_vars: Option<HashMap<String, serde_json::Value>>,
}
```

The `EvalStrategy` determines whether to run a single generalist agent or a full
multi-agent panel. Agents are resolved from the `PromptLibrary` by abbreviation,
not hardcoded as `ReviewerConfig` instances.

Agents use `crb_agents::build_agent()` with prompts rendered through the
`PromptLibrary`'s Handlebars template, supporting variable substitution
(`{diff}`, `{file_list}`, `{language}`, etc.) and optional rule preamble injection.
