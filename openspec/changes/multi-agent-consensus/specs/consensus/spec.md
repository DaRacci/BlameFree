# Consensus Module Specification

**Type:** Behavioral/Architecture Spec
**Change:** multi-agent-consensus
**Status:** Draft

## 1. Purpose

Define the consensus module that orchestrates N independent LLM reviewer agents
(SA, CL, AR, SEC) to review a PR diff concurrently, then aggregates their
structured findings via a judge agent against golden comments. The module is
self-contained under `review-harness/src/consensus/` and depends only on rig's
`extractor::Extractor` trait and standard tokio concurrency primitives.

## 2. Shared Data Types

All types are defined in `consensus/types.rs` and re-exported from
`consensus/mod.rs`.

### 2.1 `Finding`

```rust
/// A single code review finding produced by any reviewer agent.
pub struct Finding {
    /// File path relative to repository root.
    pub file: String,
    /// 1-indexed line number.
    pub line: u32,
    /// Severity level: "error", "warning", or "info".
    pub severity: String,
    /// Human-readable description of the issue.
    pub message: String,
    /// Optional category code (e.g. "SA-001", "CL-003", "SEC-007").
    pub code: Option<String>,
}
```

Must derive `Deserialize`, `JsonSchema`, `Serialize`, `Debug`, `Clone`.

### 2.2 `GoldenComment`

```rust
/// A ground-truth annotation for evaluating review quality.
pub struct GoldenComment {
    /// File path relative to repository root.
    pub file: String,
    /// 1-indexed line number.
    pub line: u32,
    /// Regex pattern to match against finding.message.
    pub message_regex: String,
    /// Expected severity.
    pub severity: String,
    /// Which role(s) should catch this: "SA", "CL", "AR", "SEC", or "any".
    pub source: String,
}
```

Must derive `Deserialize`, `Debug`, `Clone`.

### 2.3 `MatchResult`

```rust
pub enum MatchResult {
    /// A candidate finding matches a golden comment.
    TruePositive,
    /// A candidate finding has no matching golden comment.
    FalsePositive,
    /// A golden comment has no matching candidate finding.
    FalseNegative,
}
```

### 2.4 `ConsensusReport`

```rust
/// Output of a full consensus run.
pub struct ConsensusReport {
    /// Findings from each agent, grouped by role.
    pub agents: Vec<(Role, Vec<Finding>)>,
    /// Goldens that were matched by at least one finding.
    pub true_positives: Vec<(GoldenComment, Finding)>,
    /// Findings that matched no golden.
    pub false_positives: Vec<Finding>,
    /// Goldens that matched no finding.
    pub false_negatives: Vec<GoldenComment>,
    /// TP / (TP + FP)
    pub precision: f64,
    /// TP / (TP + FN)
    pub recall: f64,
    /// Harmonic mean of precision and recall.
    pub f1: f64,
}
```

## 3. Reviewer Agent Specification

### 3.1 Role Definition

| Field | Type | Description |
|-------|------|-------------|
| `role` | `Role` | SA, CL, AR, or SEC |
| `model` | `String` | LLM model identifier (e.g. "gpt-4o") |
| `max_findings` | `usize` | Maximum number of findings this agent may produce |

### 3.2 Agent Construction

```rust
pub fn build_reviewer_agent(
    config: &ReviewerConfig,
    diff: &str,
) -> impl Extractor<Output = Vec<Finding>>
```

**Preconditions:**
- `config.role` is a valid `Role` variant.
- `diff` is a non-empty unified diff string.

**Postconditions:**
- Returns a rig `Extractor` configured with:
  - Role-specific system prompt.
  - Full diff as preamble/user message.
  - Output schema = `Vec<Finding>` (auto-derived via schemars).

**Error handling:**
- If `diff` is empty, the agent should still be callable but will likely return
  `Vec::new()` (not an error).

### 3.3 Role Prompts

Each role's system prompt must include:
1. The role's specialization area.
2. A list of issue categories to look for.
3. The instruction to output findings in structured format with a role-specific
   prefix for `code` (SA-NNN, CL-NNN, AR-NNN, SEC-NNN).

## 4. Concurrent Execution Specification

### 4.1 `run_reviewers()`

```rust
pub async fn run_reviewers(
    configs: Vec<ReviewerConfig>,
    diff: &str,
) -> Vec<(Role, Vec<Finding>)>
```

**Behavior:**
1. Create a `tokio::task::JoinSet`.
2. For each `ReviewerConfig`, spawn an async task that:
   a. Builds the agent via `build_reviewer_agent()`.
   b. Calls `agent.extract().await`.
   c. Caps output at `config.max_findings` (truncate or warn).
   d. Returns `(role, findings)`.
3. Await all tasks via `JoinSet::join_next()` loop.
4. Log and skip any task that returned `Err`.

**Timeout:** Each spawned task is wrapped in
`tokio::time::timeout(Duration::from_secs(120), ...)`. Timed-out tasks yield
`(role, Vec::new())` with a warning.

**Concurrency:** All reviewers run in parallel. No limit on concurrent agent
calls (typically 4). Use `JoinSet` for fair scheduling and bounded resource
usage.

## 5. Judge Specification

### 5.1 `judge_comment()`

```rust
pub async fn judge_comment(
    golden: &GoldenComment,
    candidates: &[Finding],
) -> MatchResult
```

**Heuristic matching (default, no LLM call):**
1. Filter candidates by `file == golden.file` and `line == golden.line`.
2. Among filtered, check if any `finding.message` matches
   `golden.message_regex` (regex match).
3. If match found → `TruePositive` for that finding.
4. All matched findings removed from candidate pool.
5. Goldens with no match → `FalseNegative`.
6. Remaining unmatched candidates → `FalsePositive`.

**LLM-based matching (alternative, gated by feature flag):**
- Send golden + candidate sublist to a judge Extractor.
- Judge returns `{ "match": bool, "matched_finding_index": Option<usize> }`.
- Only used when `--judge-model` is specified separately from reviewer model.

### 5.2 `JUDGE_PROMPT`

```
You are an impartial code review judge. You determine whether a candidate
finding from a reviewer agent matches a golden (ground-truth) comment.

GOLDEN: file={file}, line={line}, severity={severity}, message pattern={message_regex}
CANDIDATE: file={file}, line={line}, severity={severity}, message={message}

Does the candidate match the golden? Consider semantic equivalence even if
wording differs. Reply ONLY with JSON: {{"match": true|false}}
```

## 6. Metrics Computation

### 6.1 Definitions

| Metric | Formula | Description |
|--------|---------|-------------|
| True Positives (TP) | Count of goldens with ≥1 matching finding | Correctly identified issues |
| False Positives (FP) | Count of findings with no matching golden | False alarms |
| False Negatives (FN) | Count of goldens with 0 matching findings | Missed issues |
| Precision | `TP / (TP + FP)` | How many reported findings are real |
| Recall | `TP / (TP + FN)` | How many real issues are found |
| F1 | `2 * P * R / (P + R)` | Harmonic mean |

### 6.2 Edge Cases

| Scenario | Precision | Recall | F1 |
|----------|-----------|--------|----|
| No findings, no goldens | 1.0 | 1.0 | 1.0 |
| No findings, goldens > 0 | 1.0 | 0.0 | 0.0 |
| Findings > 0, no goldens | 0.0 | 1.0 | 0.0 |
| All findings match | 1.0 | 1.0 | 1.0 |
| No findings match | 0.0 | 0.0 | 0.0 |

## 7. Entry Point

```rust
/// Run the full consensus pipeline.
pub async fn run_consensus(
    diff: &str,
    goldens: Vec<GoldenComment>,
    reviewer_configs: Vec<ReviewerConfig>,
) -> ConsensusReport
```

**Flow:**
1. Validate inputs (non-null diff, non-empty configs).
2. Run `run_reviewers()` → get agent results.
3. Flatten all findings into one `Vec<Finding>`.
4. For each golden, call `judge_comment()`.
5. Track TPs, FPs, FNs.
6. Compute metrics.
7. Return `ConsensusReport`.

## 8. Configuration Schema

No external config file. Caller provides inline `Vec<ReviewerConfig>`:

```rust
let configs = vec![
    ReviewerConfig { role: Role::SA, model: "gpt-4o".into(), max_findings: 20 },
    ReviewerConfig { role: Role::CL, model: "gpt-4o".into(), max_findings: 20 },
    ReviewerConfig { role: Role::AR, model: "gpt-4o".into(), max_findings: 20 },
    ReviewerConfig { role: Role::SEC, model: "gpt-4o".into(), max_findings: 20 },
];
```

## 9. Dependencies

- `rig-core` (extractor feature) — structured LLM output.
- `tokio` (full features) — async runtime, JoinSet, timeouts.
- `schemars` — JSON schema generation for Finding.
- `serde` / `serde_json` — serialization.
- `regex` — optional, for heuristic message matching.

## 10. Out of Scope

- Inter-agent debate (agents responding to each other).
- Role-specific diff slicing.
- CI/CD pipeline integration.
- Non-rig model backends.
