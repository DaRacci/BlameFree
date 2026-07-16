# consensus Specification

## Purpose
Multi-agent consensus pipeline with judge aggregation for combining findings across reviewer roles into a unified report.
## Requirements
### Requirement: Shared Data Types

The consensus module SHALL define and re-export several shared types that flow across crate boundaries. These types underpin all agent reviews, matching, and reporting.

Types are distributed across crates:

- `Role` is defined in `crates/crb-consensus/src/lib.rs` and re-exported.
- `Finding` is defined in `crates/crb-shared/src/finding.rs`.
- `GoldenComment` is defined in `crates/crb-reporting/src/golden.rs`.
- `MatchResult` and `ConsensusReport` are defined in `crates/crb-consensus/src/lib.rs`.

**`Role`:**

```rust
/// The role of a reviewer agent.
///
/// This is a dynamic newtype around a string abbreviation.
/// Valid values are loaded at runtime from the agent manifest (`prompts/agents/*.md`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize, Serialize)]
pub struct Role(pub String);
```

Constructed via `Role::from_abbreviation(abbreviation)`, which validates against
the loaded `PromptLibrary`. Also has `From<&str>` and `From<String>` impls.

Roles are **not** a fixed enum. Agents are loaded dynamically from the prompt
manifest. Typical abbreviations include "SA", "CL", "AR", "SEC", "GEN", etc.

**`Finding`:**

```rust
/// A finding that has been reported by an agent.
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
    /// Evidence supporting the finding (command output, code snippet, etc.).
    pub evidence: Option<String>,
    /// Path trace / call chain showing how the issue was reached.
    pub path_trace: Option<String>,
    /// Self reported Confidence level from the agent.
    pub confidence: Option<ConfidenceLevel>,
    /// Agent tag that found this issue.
    pub found_by: Option<String>,
    /// Number of agents that flagged this finding.
    pub agent_count: Option<u64>,
    /// Whether this finding was cross-validated by multiple agents.
    pub cross_validated: bool,
    /// How many agents/occurrences cross-validated this finding.
    pub cross_validated_by: Option<u64>,
    /// How many original findings were merged to produce this one.
    pub merged_from: Option<u64>,
}
```

Must derive `Deserialize`, `JsonSchema`, `Serialize`, `Debug`, `Clone`, `Default`.
Each field has multiple serde aliases for robustness against varying LLM output formats.

`Severity` is an enum with variants like `Critical`, `High`, `Medium`, `Low`, `Info`, `None`.

**`GoldenComment`:**

```rust
/// A single golden comment for a PR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenComment {
    /// The expected comment text.
    pub comment: String,
    /// The expected severity of the comment.
    pub severity: Severity,
}
```

Golden comments are loaded from JSON dataset files via `load_golden_datasets()`.
They represent ground-truth expected findings. Matching is done semantically
(LLM + Jaccard), not by file/line/regex.

**`MatchResult`:**

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

**`ConsensusReport`:**

```rust
/// Output of a full consensus run.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ConsensusReport {
    /// Findings from each agent, grouped by role.
    pub agents: Vec<(Role, Vec<Finding>)>,
    /// Goldens that were matched by at least one finding.
    pub true_positives: Vec<(GoldenComment, Finding)>,
    /// Findings that matched no golden.
    pub false_positives: Vec<Finding>,
    /// Goldens that matched no finding.
    pub false_negatives: Vec<GoldenComment>,
    /// Analytics usage for the agent LLM calls.
    pub analytics: AnalyticsSnapshot,
}
```

Precision, recall, and F1 are computed via the `MetricsProvider` trait rather than
being stored directly:

```rust
impl MetricsProvider for ConsensusReport {
    fn true_positives(&self) -> usize { self.true_positives.len() }
    fn false_positives(&self) -> usize { self.false_positives.len() }
    fn false_negatives(&self) -> usize { self.false_negatives.len() }
}
```

With default methods on the trait for `precision()`, `recall()`, and `f1()`.

#### Scenario: Shared data types are deserialized from LLM output with serde aliases
- GIVEN an LLM returns a Finding with non-standard field names
- WHEN the Finding is deserialized
- THEN the serde aliases on each field match the alternative names
- AND unknown fields are ignored
- AND the struct is correctly populated

#### Scenario: ConsensusReport computes metrics via the MetricsProvider trait
- GIVEN a ConsensusReport with known true_positives, false_positives, and false_negatives counts
- WHEN MetricsProvider::precision(), recall(), and f1() are called
- THEN precision = TP / (TP + FP)
- AND recall = TP / (TP + FN)
- AND F1 is the harmonic mean of precision and recall

---

### Requirement: Reviewer Agent Specification

Reviewer agents SHALL be constructed dynamically from a `PromptLibrary` registry.
Agent configuration comes from `AgentEntry` records loaded from markdown files (`prompts/agents/*.md`):

```rust
pub struct AgentEntry {
    pub role_name: String,
    pub role_abbreviation: String,   // e.g. "SA", "CL", "AR", "SEC"
    pub role_domain: String,
    pub role_anti_hallucination_rules: Option<String>,
    pub role_review_methodology: Option<String>,
    pub generalist_agent: bool,
    pub incompatible_with_roles: Vec<String>,
    pub role_prompt: String,         // markdown body after YAML frontmatter
}
```

Agents are built via `crb_agents::build_agent()` using the `PromptLibrary`:

```rust
use crb_agents::build_agent;
use crb_agents::prompts::PromptLibrary;

let prompt_lib = PromptLibrary::get_instance();
let agent_entry = prompt_lib.config("SA").unwrap();
let agent = build_agent(
    &client,
    &model,
    agent_entry,
    rules_preamble,
    template_vars,
    extra_preamble,
    additional_params,
    tool_server_handle,
)
.output_schema::<Vec<Finding>>()
.build();
```

**Preconditions:**
- `agent_entry` is a valid `AgentEntry` from the `PromptLibrary`.
- `diff` is passed as a user message / prompt (not as a preamble).

**Error handling:**
- Agent `Err` results are logged as warnings. The agent's findings are treated as `Vec::new()`.
- Output is capped to `config.max_findings` (truncated with a warning).

#### Scenario: Agent is built from PromptLibrary entry and produces structured findings
- GIVEN a valid AgentEntry from the PromptLibrary for a given role abbreviation
- WHEN build_agent() is called with the entry, client, model, and templates
- THEN the agent is configured with the role's system prompt and output_schema::<Vec<Finding>>
- AND calling agent.prompt(diff_str) returns structured Finding results

#### Scenario: Agent errors are tolerated as empty findings
- GIVEN an agent returns an Err result during prompting
- WHEN the pipeline processes the agent's output
- THEN the error is logged as a warning
- AND the agent's findings are treated as Vec::new()
- AND the pipeline continues without failing

#### Scenario: Agent output is capped at the configured maximum
- GIVEN an agent produces more findings than config.max_findings
- WHEN the agent output is processed
- THEN the findings are truncated to max_findings
- AND a warning is logged about the truncation

---

### Requirement: Concurrent Reviewer Execution

The concurrent reviewer execution SHALL be located in `crates/crb-harness/src/pipeline.rs` (private function):

```rust
async fn run_reviewers(diff: &Diff, config: &EvalConfig) -> Vec<Finding>
```

**Behavior:**
1. Calls `get_agents_for_diff(diff, config.agents)` for adaptive agent selection.
2. Creates a `tokio::task::JoinSet`.
3. For each agent entry, spawns an async task that:
   a. Builds the agent via `crb_agents::build_agent()`.
   b. Calls `agent.prompt(diff_str).extended_details().await`.
   c. Parses output as `Vec<Finding>`, caps at `config.max_findings`.
   d. On `Err`, logs and returns `Vec::new()`.
4. Awaits all tasks via `JoinSet::join_next()` loop.
5. Returns flattened `Vec<Finding>`.

**Timeout:** Per-agent tasks are spawned with `tokio::time::timeout` (duration is set
at the pipeline level, not in this function).

**Concurrency:** All reviewers run in parallel. No explicit concurrency limit
(typically 4-10 agents). Uses `JoinSet` for fair scheduling.

#### Scenario: All review agents execute concurrently for a diff
- GIVEN a PR diff and an EvalConfig with multiple agent entries
- WHEN run_reviewers() is called
- THEN each agent runs as a separate tokio task in a JoinSet
- AND all tasks execute concurrently
- AND per-agent errors are logged and replaced with Vec::new()
- AND the flattened Vec<Finding> from all agents is returned

#### Scenario: Per-agent timeout prevents individual agents from blocking the pipeline
- GIVEN an agent task that exceeds the configured timeout duration
- WHEN run_reviewers() awaits the task
- THEN tokio::time::timeout triggers for the slow agent
- AND that agent's findings are treated as empty
- AND the remaining agents' results are collected normally

---

### Requirement: Adaptive Agent Dispatch

The adaptive agent dispatch SHALL be located in `crates/crb-consensus/src/adaptive.rs`:

```rust
pub fn get_agents_for_diff(
    diff: &Diff,
    selected_agents: &[&'static AgentEntry],
) -> Vec<&'static AgentEntry>
```

For small diffs (≤3 files, ≤200 lines) where a generalist agent exists, returns
only the generalist. Otherwise returns all non-generalist agents. Diffs touching
languages in `FULL_PANEL_LANGUAGES` always trigger the full panel.

#### Scenario: Small diffs use only the generalist agent
- GIVEN a diff with ≤3 files and ≤200 lines touching no FULL_PANEL_LANGUAGES
- WHEN get_agents_for_diff() is called
- THEN only the generalist agent is returned (if one exists)
- AND no domain-specific agents are dispatched

#### Scenario: Small diffs in FULL_PANEL_LANGUAGES always trigger full dispatch
- GIVEN a diff with ≤3 files and ≤200 lines touching a language in FULL_PANEL_LANGUAGES
- WHEN get_agents_for_diff() is called
- THEN all non-generalist agents are returned regardless of diff size

#### Scenario: Large diffs always use the full agent panel
- GIVEN a diff with >3 files or >200 lines
- WHEN get_agents_for_diff() is called
- THEN all non-generalist agents from the selection are returned

---

### Requirement: LLM Judge with Cache and Jaccard Fallback

The LLM judge with cache and Jaccard fallback SHALL be located in `crates/crb-consensus/src/judge.rs`:

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

**Algorithm (cache + LLM + Jaccard):**

1. **LLM judge pass**: For each candidate, compute a content-addressed cache key
   from `(judge_prompt_hash, finding.message, golden.comment, judge_model)`.
   Look up in the cache first. On cache miss, call the judge LLM with:
   ```text
   You are evaluating AI code review tools.
   Determine if the candidate issue matches the golden (expected) comment.

   Golden Comment (the issue we're looking for):
   {golden_comment}

   Candidate Issue (from the tool's review):
   {candidate}
   ...
   ```
   Judge returns `{ "reasoning": "...", "match": bool, "confidence": 0.0-1.0 }`.
   First LLM match → `TruePositive`.

2. **Jaccard fallback**: If LLM found no match, compute Jaccard word-overlap
   similarity between `finding.message` and `golden.comment` with threshold **0.3**
   (matching the Python `step3_judge_comments.py` behavior).
   First candidate scoring ≥ 0.3 → `TruePositive`.

3. No match → `FalseNegative`.

**`JUDGE_PROMPT`:**

```text
You are evaluating AI code review tools.
Determine if the candidate issue matches the golden (expected) comment.

Golden Comment (the issue we're looking for):
{golden_comment}

Candidate Issue (from the tool's review):
{candidate}

Instructions:
- Determine if the candidate identifies the SAME underlying issue as the golden comment
- Accept semantic matches - different wording is fine if it's the same problem
- Focus on whether they point to the same bug, concern, or code issue

Respond with ONLY a JSON object:
{"reasoning": "brief explanation", "match": true/false, "confidence": 0.0-1.0}
```

#### Scenario: Cache hit skips the LLM judge call
- GIVEN a cached verdict exists for (judge_prompt_hash, finding.message, golden.comment, judge_model)
- WHEN judge_comment() is called
- THEN the cached result is returned immediately
- AND no LLM call is made
- AND judge_api_calls is not incremented

#### Scenario: LLM judge finds the first matching candidate
- GIVEN a golden comment and multiple candidate findings
- WHEN judge_comment() is called and none are cached
- THEN each candidate is evaluated in order against the golden via the LLM
- AND the first candidate that the LLM matches returns TruePositive
- AND no further candidates are evaluated

#### Scenario: Jaccard fallback catches matches missed by the LLM judge
- GIVEN a golden comment and candidates where no LLM match was found
- WHEN judge_comment() reaches the Jaccard fallback
- THEN word-overlap Jaccard similarity is computed between each finding.message and golden.comment
- AND the first candidate with Jaccard ≥ 0.3 threshold returns TruePositive
- AND candidates below the threshold continue to the next check

#### Scenario: No match produces FalseNegative
- GIVEN a golden comment where no candidate passes LLM or Jaccard matching
- WHEN judge_comment() completes
- THEN the result is FalseNegative

---

### Requirement: Metrics Computation

Metrics SHALL be computed via the `MetricsProvider` trait in `crb-types/src/benchmark.rs`:

| Metric | Formula | Description |
|--------|---------|-------------|
| True Positives (TP) | Count of goldens with ≥1 matching finding | Correctly identified issues |
| False Positives (FP) | Count of findings with no matching golden | False alarms |
| False Negatives (FN) | Count of goldens with 0 matching findings | Missed issues |
| Precision | `TP / (TP + FP)` | How many reported findings are real |
| Recall | `TP / (TP + FN)` | How many real issues are found |
| F1 | `2 * P * R / (P + R)` | Harmonic mean |

#### Scenario: Standard precision, recall, and F1 computation
- GIVEN a ConsensusReport with TP > 0, FP > 0, and FN > 0
- WHEN precision(), recall(), and f1() are computed
- THEN precision = TP / (TP + FP)
- AND recall = TP / (TP + FN)
- AND F1 = 2 * precision * recall / (precision + recall)

#### Scenario: Edge case — zero denominators return 0.0
- GIVEN a ConsensusReport where TP = 0 and FP = 0 (zero-division case)
- WHEN precision() is called
- THEN the result is 0.0 (not 1.0 vacuously)

#### Scenario: Edge case — all findings match (perfect score)
- GIVEN a ConsensusReport where all findings are true positives and no findings are false
- WHEN precision(), recall(), and f1() are computed
- THEN precision = 1.0
- AND recall = 1.0
- AND F1 = 1.0

**Edge cases (current implementation):**

| Scenario | Precision | Recall | F1 |
|----------|-----------|--------|----|
| No findings, no goldens | 0.0 (0/0 → 0.0) | 0.0 (0/0 → 0.0) | 0.0 |
| No findings, goldens > 0 | 0.0 (0/0 → 0.0) | 0.0 | 0.0 |
| Findings > 0, no goldens | 0.0 | 0.0 (0/0 → 0.0) | 0.0 |
| All findings match | 1.0 | 1.0 | 1.0 |
| No findings match | 0.0 | 0.0 | 0.0 |

Note: The current `MetricsProvider` trait returns `0.0` for the `0/0` case
(rather than 1.0 vacuously), which differs from the original spec.

---

### Requirement: Consensus Pipeline Entry Point

The consensus pipeline entry point SHALL be located in `crates/crb-consensus/src/pipeline.rs`:

```rust
/// Run the consensus judging step on already-completed review findings.
pub async fn run_consensus_post(
    agents: Vec<(Role, Vec<Finding>)>,
    goldens: Vec<GoldenComment>,
    judge: &Agent<ResponsesCompletionModel>,
    judge_model: &str,
    cache: Arc<dyn CacheBackend>,
    judge_prompt_hash: &str,
) -> ConsensusReport
```

**Flow:**
1. Accepts already-completed review results from each agent (reviewers run
   independently via `crb-harness/src/pipeline.rs::evaluate()`).
2. Pools findings into a sorted `unmatched` list.
3. For each golden, call `judge_comment()` (LLM + cache + Jaccard).
4. First match per golden → `TruePositive` (matched finding removed from pool).
5. Unmatched goldens → `FalseNegative`.
6. Remaining unmatched findings → `FalsePositive`.
7. Compute metrics via `MetricsProvider` trait.
8. Return `ConsensusReport`.

#### Scenario: Full consensus pipeline runs end-to-end
- GIVEN completed review findings from multiple agents and a set of golden comments
- WHEN run_consensus_post() is called
- THEN each golden is matched against the pooled findings via judge_comment()
- AND matched findings are removed from the pool (TruePositive)
- AND unmatched goldens are recorded as FalseNegative
- AND remaining unmatched findings are recorded as FalsePositive
- AND a ConsensusReport with all categories and analytics is returned

#### Scenario: First matching finding consumes a golden
- GIVEN a golden comment and three candidate findings
- WHEN judge_comment() finds the second candidate matches
- THEN the match is recorded as TruePositive with the matching finding
- AND the matched finding is removed from the remaining candidate pool
- AND no further candidates are evaluated for that golden

---

### Requirement: Configuration Schema

Configuration SHALL be provided via `EvalConfig` in `crates/crb-harness/src/eval.rs`:

```rust
pub struct EvalConfig {
    pub identifier: String,
    pub strategy: EvalStrategy,           // Single or Panel
    pub model: Model,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub client: Arc<openai::Client>,
    pub cache: Option<Arc<dyn CacheBackend>>,
    pub cost_tracker: Arc<AnalyticsTracker>,
    pub tool_handle: ToolServerHandle,
    pub dashboard_tx: Option<tokio::sync::broadcast::Sender<RunEvent>>,
    pub agents: &'static [&'static AgentEntry],
    pub repo_root: PathBuf,
    pub max_findings: usize,
    pub judge_model: String,
    pub judge: Agent<openai::responses_api::ResponsesCompletionModel>,
    pub linters_only: bool,
    pub linter_configs: Option<Arc<HashMap<String, LinterConfig>>>,
    pub ruleset: Option<Arc<RuleSet>>,
    pub template_vars: Option<HashMap<String, serde_json::Value>>,
}
```

The `EvalStrategy` enum supports:
- `Single` — single generalist agent evaluation.
- `Panel` — full multi-agent evaluation with domain experts.

Agents are resolved from the `PromptLibrary` by abbreviation, not hardcoded.

#### Scenario: EvalConfig with Single strategy uses a generalist agent
- GIVEN an EvalConfig with strategy = Single
- WHEN the pipeline evaluates a PR diff
- THEN only the generalist agent is selected
- AND no domain-expert agents are dispatched

#### Scenario: EvalConfig with Panel strategy uses domain-expert agents
- GIVEN an EvalConfig with strategy = Panel
- WHEN the pipeline evaluates a PR diff
- THEN the full panel of domain-expert agents is dispatched
- AND agents are resolved from PromptLibrary by abbreviation

---

### Requirement: Module Structure

The module structure SHALL follow the defined crate layout.

```
review-harness/
├── Cargo.toml                     # [workspace] members = ["crates/*"]
└── crates/
    ├── crb-consensus/             # Multi-agent orchestration & judging
    │   ├── Cargo.toml
    │   └── src/
    │       ├── lib.rs             # Role, MatchResult, ConsensusReport, re-exports
    │       ├── judge.rs           # judge_comment() with cache + LLM + Jaccard
    │       ├── pipeline.rs        # run_consensus_post()
    │       └── adaptive.rs        # get_agents_for_diff() adaptive dispatch
    ├── crb-harness/
    │   ├── src/
    │   │   ├── pipeline.rs        # evaluate(), run_reviewers()
    │   │   └── eval.rs            # EvalConfig
    ├── crb-agents/
    │   ├── src/
    │   │   ├── lib.rs             # build_agent()
    │   │   ├── agent.rs           # AgentEntry
    │   │   └── prompts.rs         # PromptLibrary (singleton, embedded prompts)
    ├── crb-shared/
    │   └── src/
    │       └── finding.rs         # Finding struct
    ├── crb-reporting/
    │   └── src/
    │       └── golden.rs          # GoldenComment, GoldenCommentEntry
    └── crb-types/
        └── src/
            └── benchmark.rs       # MetricsProvider trait, JudgeVerdict
```

**Crate Dependencies:**

- `rig-core` (extractor feature) — structured LLM output and agent builder.
- `crb-agents` — `build_agent()`, `PromptLibrary`, `AgentEntry`.
- `crb-shared` — `Finding`, `Severity`, `jaccard_similarity`.
- `crb-reporting` — `GoldenComment`, `GoldenCommentEntry`, `AnalyticsSnapshot`.
- `crb-cache` — `CacheBackend`, content-addressed caching.
- `crb-types` — `MetricsProvider`, `JudgeVerdict`, `EvalStrategy`.
- `crb-harness` — `EvalConfig`, pipeline orchestration.
- `tokio` (full features) — async runtime, JoinSet, timeouts.
- `schemars` — JSON schema generation for `Finding`.
- `serde` / `serde_json` — serialization.

#### Scenario: Module structure follows the defined crate layout
- GIVEN the review-harness workspace
- WHEN the project is built
- THEN each crate in crates/ compiles independently
- AND crb-consensus depends on crb-agents, crb-shared, crb-reporting, crb-cache, and crb-types
- AND crb-harness depends on crb-consensus for consensus orchestration

---

**Out of Scope**

- Inter-agent debate (agents responding to each other).
- Role-specific diff slicing.
- CI/CD pipeline integration.
- Non-rig model backends.
- Judge agent being built by this module (provided externally via EvalConfig).

