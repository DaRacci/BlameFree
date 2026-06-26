# crb-eval Crate Design — Evaluation System for Code Review Benchmark Harness

> **Status**: Design Draft  
> **Date**: 2026-06-26  
> **Inspired by**: Kodus AI evaluation system (promptfoo-based deterministic replay, golden datasets, multi-metric scoring)

---

## Table of Contents

1. [Kodus Eval System Analysis](#1-kodus-eval-system-analysis)
2. [Gap Analysis](#2-gap-analysis)
3. [Architecture Design for `crb-eval` Crate](#3-architecture-design-for-crb-eval-crate)
4. [Implementation Plan](#4-implementation-plan)
5. [CLI Flag Additions](#5-cli-flag-additions)
6. [Integration Points with Existing Crates](#6-integration-points-with-existing-crates)
7. [Effort Estimate](#7-effort-estimate)

---

## 1. Kodus Eval System Analysis

### 1.1 Structure

Kodus's eval system lives under `evals/` in their monorepo. It has three main areas:

```
evals/
├── investigation/          # Tool-use/recall evals (the primary focus)
│   ├── agent-provider.js      # Builds live generalist prompt from source
│   ├── prompt-loader.js       # Loads case payloads
│   ├── trace-shape-assertion.js   # Verifies trace payload shape
│   ├── tool-expectation-assertion.js  # Verifies required/forbidden tools
│   ├── recall-assertion.js     # Judges recall/precision/fairness/fidelity
│   ├── recall-judge.js         # Sonnet-based matcher for golden bugs
│   ├── promptfoo-recall.yaml   # RECALL eval config
│   ├── extract-replay-from-trace.js  # Builds replay datasets from Langfuse
│   └── ... (golden datasets, replay scripts)
├── promotion/              # Finding promotion/suppression evals
└── promptfoo-safeguard/    # Safeguard verification pipeline evals
```

### 1.2 Key Capabilities

#### A. Deterministic Replay

Kodus captures LLM request/response pairs (prompt + response) from production Langfuse traces and replays them during evaluation **without making new API calls**. This is critical for:

- **Deterministic scoring**: No variance from LLM nondeterminism
- **CI stability**: Eval runs always produce the same result
- **Cost reduction**: Zero inference cost during eval runs
- **Debugging**: Replay a specific trace to debug scoring logic

The mechanism: `extract-replay-from-trace.js` builds replay datasets from Langfuse traces. The promptfoo config references these replay files, and promptfoo's built-in replay provider serves cached responses.

#### B. Golden Datasets

- Extracted from Langfuse production traces (real PR reviews)
- Stored as JSON fixture files per test case
- Each golden has: `file`, `line`, `message_regex`, `severity`, `source` (which agent role should catch it)
- Multiple formats supported: full golden comments, tool-use expectations, trace shape expectations

#### C. Multi-Metric Scoring

Tracked in `recall-assertion.js`:

| Metric | Definition |
|--------|-----------|
| **Recall** | Fraction of goldens covered by ≥1 finding |
| **Precision** | Fraction of findings that hit ≥1 golden |
| **F1** | Harmonic mean of precision and recall |
| **Fair-recall** | Recall excluding replay artifacts |
| **Loop-fidelity** | % of tool calls the replay could serve |

#### D. Investigation Pipeline

- promptfoo YAML configs define test suites
- Each test case has: prompt, golden responses, assertions (LLM-judged or deterministic)
- Run via `promptfoo eval` command
- Results exported as JSON for CI integration

#### E. Structured Output Validation

- Uses Zod schemas for finding format validation
- `trace-shape-assertion.js` verifies LLM response shapes

### 1.3 Promptfoo Integration Pattern

The key design pattern Kodus uses:

```
┌─────────────────┐     ┌──────────────────┐     ┌───────────────┐
│ Golden Datasets │────▶│ promptfoo Config │────▶│ Eval Runner   │
│ (Langfuse       │     │ (YAML + JS       │     │ (CLI + CI)    │
│  traces)        │     │  providers)      │     │               │
└─────────────────┘     └──────────────────┘     └───────┬───────┘
                                                          │
                                                          ▼
                                                  ┌───────────────┐
                                                  │ Reports       │
                                                  │ (JSON +       │
                                                  │  assertions)  │
                                                  └───────────────┘
```

---

## 2. Gap Analysis

### 2.1 What We Have vs. What Kodus Has vs. What We Need

| Capability | We Have | Kodus Has | We Need |
|-----------|---------|-----------|---------|
| **LLM judging** | ✅ crb-judge with Martian JUDGE_PROMPT | ✅ recall-judge.js (Sonnet matcher) | ✅ Adequate |
| **Precision/Recall/F1** | ✅ crb-judge::compute_metrics() | ✅ recall-assertion.js | ✅ Adequate |
| **Golden datasets** | ✅ crb-reporting loads `GoldenCommentEntry` | ✅ Langfuse-extracted fixtures | **Format expansion needed** |
| **Consensus pipeline** | ✅ crb-consensus (heuristic + LLM fallback) | ✅ Multi-agent orchestration | ✅ Adequate |
| **Baseline validation** | ✅ validation.rs (threshold comparison) | ❌ Not in Kodus | ✅ Adequate |
| **CI mode** | ✅ `--ci` flag | ❌ Not in Kodus | ✅ Adequate |
| **Deterministic replay** | ❌ **Missing** | ✅ extract-replay-from-trace.js | **Critical gap** |
| **Record/replay layer** | ❌ **Missing** | ✅ promptfoo replay provider | **Critical gap** |
| **Per-agent metrics** | ❌ **Missing** | ✅ recall-assertion.js | **High priority** |
| **Per-category metrics** | ❌ **Missing** | ✅ (via golden source field) | **High priority** |
| **Cost tracking** | ❌ **Missing** | ❌ Not in Kodus | **Medium priority** |
| **Confidence calibration** | ❌ **Missing** | ❌ Not in Kodus | **Medium priority** |
| **Golden dataset validation** | ❌ **Missing** | ✅ promptfoo schema validation | **Medium priority** |
| **Multi-format golden input** | ✅ JSON only | ✅ JSON + YAML + promptfoo | **Low priority** |
| **Regression detection** | ⚠️ Basic threshold check | ❌ Not in Kodus | **High priority** |

### 2.2 Critical Gaps

1. **Deterministic replay is the #1 gap** — Our T10 regression test (judge determinism) remains unsolved because every eval run makes real LLM calls. We need a record/replay layer.

2. **No per-agent/per-category metrics** — We aggregate everything into a single precision/recall/F1. We can't tell which agent role is overperforming or underperforming.

3. **No cost tracking** — We have no visibility into token usage or cost per finding.

4. **No confidence calibration** — The judge returns confidence scores but we never validate them.

5. **Golden dataset integrity** — We load goldens from JSON files but never validate for duplicates, valid regex patterns, or file coverage.

---

## 3. Architecture Design for `crb-eval` Crate

### 3.1 Crate Overview

```
crates/crb-eval/
├── Cargo.toml
├── src/
│   ├── lib.rs                    # Public API
│   ├── record/                   # Trace recording layer
│   │   ├── mod.rs
│   │   ├── recorder.rs            # TraceRecorder — captures LLM calls to disk
│   │   └── formats.rs             # Trace file format (JSON, MessagePack)
│   ├── replay/                   # Trace replay layer
│   │   ├── mod.rs
│   │   ├── player.rs              # TracePlayer — serves cached responses
│   │   └── matcher.rs             # Request matching (prompt hash, model)
│   ├── golden/                   # Golden dataset management
│   │   ├── mod.rs
│   │   ├── loader.rs              # Multi-format golden loader
│   │   ├── validator.rs           # Golden integrity checks
│   │   └── format.rs              # Supported formats (Martian JSON, TOML, promptfoo YAML)
│   ├── metrics/                  # Multi-metric scoring engine
│   │   ├── mod.rs
│   │   ├── core.rs                # Core metric computation
│   │   ├── per_agent.rs           # Per-role metrics
│   │   ├── per_category.rs        # Per-category (bug/security/performance)
│   │   ├── per_severity.rs        # Per-severity breakdown
│   │   ├── confidence.rs          # Confidence calibration
│   │   └── cost.rs                # Cost tracking (tokens, $ per finding)
│   ├── regression/              # Regression detection
│   │   ├── mod.rs
│   │   ├── detector.rs            # Regression detector (F1 drop > threshold)
│   │   └── reporter.rs            # Regression report formatting
│   ├── report/                   # Eval report output
│   │   ├── mod.rs
│   │   ├── json.rs                # JSON report writer
│   │   ├── markdown.rs            # Markdown summary writer
│   │   └── html.rs                # HTML detail writer (optional)
│   └── config/                   # Eval-specific config
│       ├── mod.rs
│       └── types.rs               # EvalConfig, RecordConfig, ReplayConfig
└── tests/
    ├── record_replay_test.rs     # Integration: record then replay
    ├── golden_validation_test.rs # Golden dataset integrity checks
    └── metrics_test.rs           # Multi-metric computation tests
```

### 3.2 Core Data Types

```rust
// ── Trace Recording ────────────────────────────────────────────

/// A single captured LLM call (request + response).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmTrace {
    /// Timestamp of the call (ISO 8601).
    pub timestamp: String,
    /// Model identifier (e.g. "gpt-4o", "claude-sonnet-4-20250514").
    pub model: String,
    /// The full prompt sent to the LLM.
    pub prompt: String,
    /// The full response received from the LLM.
    pub response: String,
    /// Token usage if available.
    pub usage: Option<TokenUsage>,
    /// Duration of the call in milliseconds.
    pub duration_ms: u64,
    /// Logical label for the call (e.g. "judge:PR-42:golden-3", "agent:SA:PR-42").
    pub label: String,
    /// SHA-256 hash of the prompt for fast matching.
    pub prompt_hash: String,
    /// Arbitrary metadata (caller crate, role, attempt number).
    pub metadata: HashMap<String, String>,
}

/// Token usage information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// A session trace — one eval run's worth of LLM calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSession {
    pub session_id: String,
    pub started_at: String,
    pub model: String,
    pub judge_model: String,
    pub traces: Vec<LlmTrace>,
    pub metadata: HashMap<String, String>,
}


// ── Trace Replay ───────────────────────────────────────────────

/// Result of matching a prompt against recorded traces.
#[derive(Debug, Clone)]
pub enum ReplayMatch {
    /// Exact match found (prompt hash matches).
    Exact(LlmTrace),
    /// No match found.
    None,
}


// ── Golden Dataset Management ──────────────────────────────────

/// Validated golden comment with integrity metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatedGolden {
    pub entry: GoldenCommentEntry,
    pub valid: bool,
    pub issues: Vec<GoldenIssue>,
}

/// Issues found during golden dataset validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GoldenIssue {
    DuplicateComment { pr_title: String, comment: String },
    InvalidRegex { comment: String, error: String },
    EmptyComment { pr_title: String },
    MissingFile { pr_title: String, expected_path: String },
}


// ── Multi-Metric Scoring ───────────────────────────────────────

/// Extended metrics with per-agent and per-category breakdowns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedMetrics {
    /// Overall metrics (existing precision/recall/F1).
    pub overall: Metrics,
    /// Per-agent-role breakdown.
    pub per_agent: HashMap<String, Metrics>,
    /// Per-severity breakdown.
    pub per_severity: HashMap<String, Metrics>,
    /// Per-category breakdown (if categories are annotated on goldens).
    pub per_category: HashMap<String, Metrics>,
    /// Confidence calibration metrics.
    pub confidence: ConfidenceMetrics,
    /// Cost tracking.
    pub cost: CostMetrics,
}

/// Confidence calibration metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceMetrics {
    /// Mean confidence of true positives.
    pub mean_confidence_tp: f64,
    /// Mean confidence of false positives.
    pub mean_confidence_fp: f64,
    /// Expected Calibration Error (ECE).
    pub expected_calibration_error: f64,
    /// Number of confidence bins used.
    pub bins: usize,
}

/// Cost tracking metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostMetrics {
    /// Total tokens used (prompt + completion).
    pub total_tokens: u64,
    /// Tokens per finding.
    pub tokens_per_finding: f64,
    /// Estimated cost (USD), configurable per-model pricing.
    pub estimated_cost_usd: f64,
    /// Cost per true positive.
    pub cost_per_tp: f64,
    /// Cost per finding.
    pub cost_per_finding: f64,
}


// ── Regression Detection ───────────────────────────────────────

/// Result of regression detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionResult {
    pub has_regression: bool,
    pub deltas: HashMap<String, f64>,
    pub details: Vec<RegressionDetail>,
}

/// A single regression finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionDetail {
    pub metric_name: String,
    pub previous_value: f64,
    pub current_value: f64,
    pub threshold: f64,
    pub delta: f64,
    pub is_regression: bool,
}
```

### 3.3 Trace Recording Approach

The recording layer wraps rig-core's completion calls:

```rust
/// Decorator that wraps a rig completion model to record traces.
///
/// Usage:
/// ```rust,ignore
/// let client = rig_core::providers::openai::Client::from_env()?;
/// let recorder = TraceRecorder::new("traces/latest.json");
/// let recording_client = recorder.wrap_client(client);
/// // All LLM calls through recording_client are captured to disk
/// ```
pub struct TraceRecorder {
    output_path: PathBuf,
    session: TraceSession,
    buffer: Arc<Mutex<Vec<LlmTrace>>>,
    flush_interval: Duration,
}

impl TraceRecorder {
    /// Create a new recorder that writes traces to `output_path`.
    /// If `output_path` is a directory, creates `output_path/<session_id>.json`.
    pub fn new(output_path: PathBuf) -> Self;

    /// Wrap a rig-core Client to record all completion calls.
    pub fn wrap_client<C: CompletionClient>(&self, client: C) -> RecordingClient<C>;

    /// Wrap an Agent to record all its prompts.
    pub fn wrap_agent<M: CompletionModel>(&self, agent: Agent<M>) -> RecordingAgent<M>;

    /// Flush buffered traces to disk.
    pub fn flush(&self) -> Result<()>;

    /// Finalize the session and write all remaining traces.
    pub fn finalize(self) -> Result<TraceSession>;
}
```

**File format**: One JSON file per session:
```json
{
  "session_id": "2026-06-26T10-30-00_run-003",
  "started_at": "2026-06-26T10:30:00Z",
  "model": "gpt-4o",
  "judge_model": "gpt-4o-mini",
  "traces": [
    {
      "timestamp": "2026-06-26T10:30:01Z",
      "model": "gpt-4o",
      "prompt": "...",
      "response": "...",
      "usage": {"prompt_tokens": 1500, "completion_tokens": 200, "total_tokens": 1700},
      "duration_ms": 2340,
      "label": "agent:SA:fix-null-pointer-in-user-service",
      "prompt_hash": "abc123def456",
      "metadata": {"role": "SA", "pr_title": "fix-null-pointer-in-user-service"}
    }
  ],
  "metadata": {"dataset": "martian-50", "concurrency": "4"}
}
```

### 3.4 Trace Replay Approach

```rust
/// Loads a trace session and serves cached responses instead of calling LLM.
pub struct TracePlayer {
    session: TraceSession,
    match_strategy: MatchStrategy,
}

/// Strategy for matching prompts to cached traces.
pub enum MatchStrategy {
    /// Match by SHA-256 hash of the full prompt (fastest, most precise).
    Exact,
    /// Match by label prefix (for partial replay).
    LabelPrefix(String),
    /// Match by prompt similarity threshold (for fuzzy matching).
    Similar(f64),
}

impl TracePlayer {
    /// Load a trace session from a JSON file.
    pub fn load(path: &Path) -> Result<Self>;

    /// Attempt to match a prompt against recorded traces.
    pub fn match_prompt(&self, prompt: &str, label: Option<&str>) -> Option<&LlmTrace>;

    /// Create a rig Client that replays traces instead of calling the API.
    pub fn replay_client<C: CompletionClient>(
        &self,
        fallback_client: C,
    ) -> ReplayClient<C>;
}
```

### 3.5 Multi-Metric Scoring Engine

```rust
/// Compute all extended metrics from judge verdicts and trace data.
pub fn compute_extended_metrics(
    verdicts: &[JudgeVerdict],
    traces: &[LlmTrace],
    goldens: &[GoldenCommentEntry],
    verdict_annotations: &[VerdictAnnotation],  // which agent/severity each verdict belongs to
    pricing: &ModelPricing,
) -> ExtendedMetrics;

/// Per-agent breakdown: re-compute metrics filtered by agent role.
pub fn compute_per_agent_metrics(
    verdicts: &[JudgeVerdict],
    annotations: &[VerdictAnnotation],
    golden_count: usize,
) -> HashMap<String, Metrics>;

/// Per-severity breakdown: metrics for critical, high, medium, low.
pub fn compute_per_severity_metrics(
    verdicts: &[JudgeVerdict],
    annotations: &[VerdictAnnotation],
    golden_count: usize,
) -> HashMap<String, Metrics>;

/// Confidence calibration: bin predictions and compute ECE.
pub fn compute_confidence_metrics(
    verdicts: &[JudgeVerdict],
) -> ConfidenceMetrics;

/// Cost tracking: sum token usage and apply pricing model.
pub fn compute_cost_metrics(
    traces: &[LlmTrace],
    pricing: &ModelPricing,
    true_positives: usize,
    total_findings: usize,
) -> CostMetrics;

/// Model pricing table (configurable via TOML).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    pub per_model: HashMap<String, ModelPrice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPrice {
    pub input_per_1k: f64,    // $ per 1K input tokens
    pub output_per_1k: f64,   // $ per 1K output tokens
}
```

### 3.6 Golden Dataset Validation

```rust
pub fn validate_goldens(
    entries: &[GoldenCommentEntry],
    config: &GoldenValidationConfig,
) -> Vec<ValidatedGolden>;

pub struct GoldenValidationConfig {
    /// Check for duplicate comment text within the same PR.
    pub check_duplicates: bool,
    /// Check that all `message_regex` patterns are valid regexes.
    pub check_regex_validity: bool,
    /// Check that no comments have empty text.
    pub check_empty: bool,
    /// If set, check that the specified files exist in the repo path.
    pub check_file_exists: Option<PathBuf>,
    /// Maximum allowed comment length (chars). 0 = no limit.
    pub max_comment_length: usize,
    /// Minimum number of goldens per PR (0 = no minimum).
    pub min_goldens_per_pr: usize,
}
```

### 3.7 Regression Detection

```rust
pub fn detect_regressions(
    current_metrics: &ExtendedMetrics,
    baseline_metrics: &ExtendedMetrics,
    thresholds: &HashMap<String, f64>,
) -> RegressionResult;

impl RegressionResult {
    pub fn is_failing(&self) -> bool;
    pub fn summary(&self) -> String;
    pub fn markdown_report(&self) -> String;
}
```

### 3.8 Markdown Report Example

```
## Eval Report: run-2026-06-26

### Overall Metrics
| Metric    | Value   | Baseline | Delta    | Status |
|-----------|---------|----------|----------|--------|
| Precision | 0.7234  | 0.7100   | +0.0134  | ✅     |
| Recall    | 0.6542  | 0.6500   | +0.0042  | ✅     |
| F1        | 0.6868  | 0.6788   | +0.0080  | ✅     |

### Per-Agent Metrics
| Agent | Precision | Recall | F1     |
|-------|-----------|--------|--------|
| SA    | 0.8100    | 0.5200 | 0.6335 |
| CL    | 0.6900    | 0.6100 | 0.6478 |
| AR    | 0.5500    | 0.3800 | 0.4493 |
| SEC   | 0.7800    | 0.4400 | 0.5628 |

### Per-Severity Metrics
| Severity | Precision | Recall | F1     |
|----------|-----------|--------|--------|
| Critical | 0.9200    | 0.4500 | 0.6041 |
| High     | 0.7400    | 0.5800 | 0.6503 |
| Medium   | 0.5800    | 0.7200 | 0.6425 |
| Low      | 0.4000    | 0.3500 | 0.3733 |

### Confidence Calibration
| Bin        | Accuracy | Confidence | Gap  |
|------------|----------|------------|------|
| 0.0-0.2    | 0.05     | 0.10       | 0.05 |
| 0.2-0.4    | 0.30     | 0.32       | 0.02 |
| 0.4-0.6    | 0.52     | 0.51       | 0.01 |
| 0.6-0.8    | 0.71     | 0.73       | 0.02 |
| 0.8-1.0    | 0.94     | 0.92       | 0.02 |
| **ECE**    |          |            | 0.03 |

### Cost
| Metric              | Value     |
|---------------------|-----------|
| Total Tokens        | 1,234,567 |
| Tokens per Finding  | 12,345    |
| Estimated Cost (USD)| $12.34    |
| Cost per TP         | $0.42     |

### Regression Check: ✅ PASS
All metrics within thresholds.
```

---

## 4. Implementation Plan

### Phase 1: Deterministic Replay (Record/Replay LLM Calls)

**Goal**: Allow the harness to record all LLM calls to disk and replay them on subsequent runs for deterministic, cost-free scoring.

**Files to create**:
- `crates/crb-eval/src/lib.rs` — crate root, re-exports
- `crates/crb-eval/src/record/mod.rs` — TraceRecorder, LlmTrace, TraceSession types
- `crates/crb-eval/src/record/recorder.rs` — RecordingClient, RecordingAgent implementations
- `crates/crb-eval/src/replay/mod.rs` — TracePlayer, ReplayClient
- `crates/crb-eval/src/replay/matcher.rs` — prompt hashing, exact/label/similarity matching

**Implementation details**:
- TraceRecorder wraps `rig_core::providers::openai::Client` by creating a new client with a middleware layer
- RecordingAgent wraps `rig_core::agent::Agent` to intercept `prompt()` calls
- TracePlayer loads a session and returns cached responses matched by prompt hash
- Prompt hashing: SHA-256 of the normalized prompt string (strip whitespace, normalize line endings)
- File format: single JSON file per session, append-only during recording, full rewrite on finalize
- Flush every N traces (configurable, default 100) to avoid data loss on crash

**Integration points**:
- `crb-harness/src/main.rs` — add `--record-traces` and `--replay-traces` flags
- Main evaluation loop wraps client in RecordingClient when `--record-traces` is set
- Main evaluation loop wraps client in ReplayClient when `--replay-traces` is set

**Deliverable**: Running `cargo run -- --record-traces traces/session1.json` captures all LLM calls to `traces/session1.json`. Running `cargo run -- --replay-traces traces/session1.json` reruns the same evaluation using cached responses, producing identical metrics.

### Phase 2: Golden Dataset Management

**Goal**: Add golden dataset validation, multi-format loading, and integrity checks.

**Files to create**:
- `crates/crb-eval/src/golden/mod.rs`
- `crates/crb-eval/src/golden/loader.rs` — load goldens from JSON, TOML, promptfoo YAML
- `crates/crb-eval/src/golden/validator.rs` — duplicate detection, regex validation, empty checks
- `crates/crb-eval/src/golden/format.rs` — format detection and conversion

**Implementation details**:
- Validator runs as a pre-check before evaluation starts
- `--validate-goldens` flag runs validation only (lint mode for golden datasets)
- Multiple format support: Martian JSON (current), TOML (for hand-written goldens), promptfoo YAML (for interop)
- Integrity report printed to stdout with issues found

**Integration points**:
- `crb-reporting/src/lib.rs` — can optionally delegate golden loading to crb-eval
- `crb-harness/src/main.rs` — add `--validate-goldens` and `--golden-dir` flags

**Deliverable**: `cargo run -- --validate-goldens datasets/golden_comments` validates all goldens and reports issues. `cargo run -- --golden-dir datasets/golden_comments --format toml` loads TOML-format goldens.

### Phase 3: Multi-Metric Scoring

**Goal**: Add per-agent, per-category, per-severity metrics, confidence calibration, and cost tracking.

**Files to create**:
- `crates/crb-eval/src/metrics/mod.rs`
- `crates/crb-eval/src/metrics/core.rs` — ExtendedMetrics, compute_extended_metrics()
- `crates/crb-eval/src/metrics/per_agent.rs` — per-role breakdown
- `crates/crb-eval/src/metrics/per_category.rs` — per-category breakdown
- `crates/crb-eval/src/metrics/per_severity.rs` — per-severity breakdown
- `crates/crb-eval/src/metrics/confidence.rs` — ECE and calibration plots
- `crates/crb-eval/src/metrics/cost.rs` — token counting and USD estimation
- `crates/crb-eval/src/report/mod.rs`
- `crates/crb-eval/src/report/json.rs` — extended JSON report writer
- `crates/crb-eval/src/report/markdown.rs` — human-readable markdown report

**Implementation details**:
- `ExtendedMetrics` wraps the existing `Metrics` with additional breakdowns
- Verdict annotations track which agent/severity/category each verdict belongs to
- Confidence calibration: bin predictions by confidence (0.0-0.2, 0.2-0.4, ...), compute accuracy per bin, compute ECE
- Cost tracking: trace session provides token counts; pricing model from `evals.toml`
- Reports written alongside existing output (`.json`, `.md` files)

**Integration points**:
- `crb-judge/src/lib.rs` — `compute_metrics()` extended to accept annotations
- `crb-harness/src/main.rs` — post-evaluation calls `compute_extended_metrics()`
- `crb-harness/src/main.rs` — writes markdown report alongside JSON results

**Deliverable**: After evaluation, `output/report.md` contains per-agent, per-severity, confidence, and cost metrics. `output/report.json` contains structured extended metrics.

### Phase 4: Regression Detection

**Goal**: Auto-detect regressions across extended metrics, not just overall F1.

**Files to create**:
- `crates/crb-eval/src/regression/mod.rs`
- `crates/crb-eval/src/regression/detector.rs` — multi-metric regression detection
- `crates/crb-eval/src/regression/reporter.rs` — regression report formatting

**Implementation details**:
- Extends `validation.rs` from crb-harness to check per-agent, per-severity metrics
- Baseline format extended to include per-agent and per-severity thresholds
- CI mode auto-generates baseline from current run when `--write-baseline` is set
- Regression report highlights which specific metric(s) regressed

**Integration points**:
- `crb-harness/src/validation.rs` — extended `Baseline` struct with per-agent thresholds
- `crb-harness/src/main.rs` — `--ci` mode calls regression detection
- `crb-harness/src/config.rs` — new `--thresholds` flag for custom thresholds

**Deliverable**: `cargo run -- --ci` checks all extended metrics against baseline, exits nonzero on any regression. `cargo run -- --write-baseline baselines/5.15.json` captures current metrics as baseline.

---

## 5. CLI Flag Additions

### New Flags in `crb-harness`

```rust
// ── Recording / Replay ────────────────────────────────────────

/// Path to save LLM trace recordings (enables recording mode).
/// All LLM calls are captured to this file for later replay.
#[arg(long, env = "RECORD_TRACES")]
pub record_traces: Option<PathBuf>,

/// Path to load trace recordings for replay (enables replay mode).
/// Instead of making LLM calls, cached responses are served.
#[arg(long, env = "REPLAY_TRACES")]
pub replay_traces: Option<PathBuf>,

/// Match strategy for replay: "exact" (default), "label:<prefix>", or "similar:<threshold>".
#[arg(long, default_value = "exact")]
pub replay_match: String,

/// If true and replay misses a prompt, fall back to real LLM call.
#[arg(long, default_value_t = false)]
pub replay_fallback: bool,


// ── Golden Dataset Management ─────────────────────────────────

/// Golden dataset format: "json" (default), "toml", "promptfoo-yaml".
#[arg(long, env = "GOLDEN_FORMAT", default_value = "json")]
pub golden_format: String,

/// Validate golden datasets only, no evaluation.
#[arg(long, default_value_t = false)]
pub validate_goldens: bool,


// ── Multi-Metric Scoring ──────────────────────────────────────

/// Path to pricing config TOML (model costs).
#[arg(long, env = "PRICING_CONFIG", default_value = "pricing.toml")]
pub pricing_config: PathBuf,

/// Enable detailed per-agent metrics.
#[arg(long, default_value_t = true)]
pub per_agent_metrics: bool,

/// Enable per-severity breakdown.
#[arg(long, default_value_t = true)]
pub per_severity_metrics: bool,

/// Enable confidence calibration metrics.
#[arg(long, default_value_t = false)]
pub confidence_metrics: bool,


// ── Regression Detection ──────────────────────────────────────

/// Path to write the current metrics as a new baseline JSON file.
#[arg(long)]
pub write_baseline: Option<PathBuf>,

/// Path to custom thresholds TOML file for regression detection.
#[arg(long, env = "THRESHOLDS_CONFIG")]
pub thresholds_config: Option<PathBuf>,
```

### Summary of All CLI Flags (Current + New)

```
FLAGS:
      --dry-run                    Dry run: load config, print stats, exit
      --resume                     Skip already-evaluated PRs
      --skip-linters               Skip linter execution
      --linters-only               Only run linters
      --skip-consensus             Skip multi-agent consensus
      --skip-rules                 Skip rule loading
      --validate                   Validate against stored baselines
      --ci                         CI mode: full pipeline with exit code
      --cached-diffs               Use pre-extracted diffs
      --replay-fallback            Fall back to real LLM if replay misses
      --validate-goldens           Validate golden datasets only
      --per-agent-metrics          Enable per-agent metrics (default: true)
      --per-severity-metrics       Enable per-severity breakdown (default: true)
      --confidence-metrics         Enable confidence calibration (default: false)

OPTIONS:
  -o, --output-dir <PATH>          Output directory (default: "output")
      --dataset-dir <PATH>         Golden comments directory (default: "datasets/golden_comments")
      --repos-dir <PATH>           Repos directory (default: "repos")
      --model <MODEL>              Model for agent reviews (default: "gpt-4o")
      --judge-model <MODEL>        Model for LLM judge (default: "gpt-4o-mini")
      --concurrency <N>            Max concurrent PR evaluations (default: 4)
      --linters-config <PATH>      Linter TOML config (default: "linters.toml")
      --rules-dir <PATH>           Rules directory (default: ".crb/rules/")
      --prompts-dir <PATH>         Prompts directory (default: "prompts/builtin")
      --roles <ROLES>              Agent roles (default: "SA,CL,AR,SEC")
      --max-findings <N>           Max findings per agent (default: 20)
      --record-traces <PATH>       Save LLM traces to file (enables recording)
      --replay-traces <PATH>       Load traces for replay (enables replay)
      --replay-match <STRATEGY>    Replay match strategy (default: "exact")
      --golden-format <FORMAT>     Golden dataset format (default: "json")
      --pricing-config <PATH>      Pricing TOML config (default: "pricing.toml")
      --write-baseline <PATH>      Write current metrics as baseline
      --thresholds-config <PATH>   Custom thresholds TOML file
```

---

## 6. Integration Points with Existing Crates

### 6.1 Dependency Graph

```
crb-eval
  │
  ├── depends on: crb-judge (Metrics, JudgeVerdict)
  │               crb-reporting (GoldenCommentEntry, PrResult)
  │               rig-core (Client, Agent, CompletionClient)
  │               serde, serde_json, sha2, anyhow, tracing
  │
  └── used by: crb-harness (eval pipeline extension)
```

### 6.2 Integration with `crb-harness/src/main.rs`

**Current flow:**
```rust
load_datasets() → evaluate_prs() → write_report() → [--ci] validate()
```

**Extended flow:**
```rust
// Pre-check
if args.validate_goldens { return run_validate_goldens(&dataset_dir); }

// Client setup
let client = create_client(); // rig OpenAI client
let recorder = args.record_traces.map(|p| TraceRecorder::new(p));
let player = args.replay_traces.map(|p| TracePlayer::load(p));

// Wrap client based on mode
let eval_client = match (&recorder, &player) {
    (Some(r), None) => r.wrap_client(client),         // recording mode
    (None, Some(p)) => p.replay_client(client),       // replay mode
    _ => client,                                      // normal mode
};

// Evaluate
let results = evaluate_all_prs(eval_client, ...).await;

// Post-evaluation: extended metrics
if recorder { recorder.finalize()?; }
let extended = compute_extended_metrics(
    &results, recorder_traces, &goldens, &annotations, &pricing
);
write_eval_report(&extended, &output_dir)?;

// [--ci] Regression detection
if args.ci {
    let baseline = load_baseline(...);
    let regression = detect_regressions(&extended, &baseline, &thresholds);
    print_regression_report(&regression);
    if regression.is_failing() { exit(1); }
}

// [--write-baseline]
if let Some(path) = args.write_baseline {
    write_baseline_file(&path, &extended)?;
}
```

### 6.3 Integration with `crb-judge/src/lib.rs`

- `compute_metrics()` in crb-judge becomes the building block for crb-eval's `compute_extended_metrics()`
- crb-eval calls crb-judge's `compute_metrics()` per agent/severity/category slice
- `JudgeVerdict` gets optional `annotation` field for agent/severity/category metadata

### 6.4 Integration with `crb-reporting/src/lib.rs`

- `PrResult` gets optional `extended_metrics: Option<ExtendedMetrics>` field
- `write_report()` extended to write `.md` and optional `.html` reports
- `load_golden_datasets()` optionally delegates to crb-eval's multi-format loader

### 6.5 Integration with `crb-harness/src/validation.rs`

- `Baseline` struct extended to hold per-agent, per-severity thresholds
- `ExtendedBaseline` type added alongside existing `Baseline`
- `validate_against_baseline()` overloaded for extended metrics

### 6.6 Integration with `crb-consensus/src/lib.rs`

- Consensus pipeline produces `VerdictAnnotation` per verdict (which agent, severity, category)
- These annotations feed into crb-eval's per-agent/per-severity metrics

---

## 7. Effort Estimate

### Lines of Rust (approximate)

| Module | Files | Lines | Complexity |
|--------|-------|-------|------------|
| **Phase 1: Record/Replay** | | | |
| `record/mod.rs` | 1 | 80 | Low — types + re-exports |
| `record/recorder.rs` | 1 | 250 | Medium — async middleware, file I/O |
| `replay/mod.rs` | 1 | 150 | Medium — session loading, client wrapping |
| `replay/matcher.rs` | 1 | 120 | Low — hashing + matching logic |
| Subtotal | 4 | 600 | **Medium** |
| **Phase 2: Golden Mgmt** | | | |
| `golden/mod.rs` | 1 | 50 | Low |
| `golden/loader.rs` | 1 | 180 | Medium — multi-format parsing |
| `golden/validator.rs` | 1 | 200 | Medium — integrity checks |
| `golden/format.rs` | 1 | 80 | Low — format detection |
| Subtotal | 4 | 510 | **Medium** |
| **Phase 3: Metrics** | | | |
| `metrics/mod.rs` | 1 | 60 | Low |
| `metrics/core.rs` | 1 | 150 | Medium — orchestration |
| `metrics/per_agent.rs` | 1 | 80 | Low — filtered compute |
| `metrics/per_category.rs` | 1 | 80 | Low |
| `metrics/per_severity.rs` | 1 | 80 | Low |
| `metrics/confidence.rs` | 1 | 120 | Medium — ECE computation |
| `metrics/cost.rs` | 1 | 100 | Low — token math |
| `report/mod.rs` | 1 | 60 | Low |
| `report/json.rs` | 1 | 100 | Low |
| `report/markdown.rs` | 1 | 250 | Medium — template + formatting |
| Subtotal | 10 | 1,080 | **Medium** |
| **Phase 4: Regression** | | | |
| `regression/mod.rs` | 1 | 50 | Low |
| `regression/detector.rs` | 1 | 150 | Medium — multi-metric comparison |
| `regression/reporter.rs` | 1 | 100 | Low |
| Subtotal | 3 | 300 | **Low-Medium** |
| **Crate infrastructure** | | | |
| `Cargo.toml` | 1 | 30 | Low |
| `lib.rs` (re-exports) | 1 | 80 | Low |
| `config/mod.rs` | 1 | 60 | Low |
| `config/types.rs` | 1 | 100 | Low |
| Subtotal | 4 | 270 | **Low** |
| **Tests** | | | |
| `tests/record_replay_test.rs` | 1 | 150 | Medium |
| `tests/golden_validation_test.rs` | 1 | 120 | Low |
| `tests/metrics_test.rs` | 1 | 200 | Medium |
| Subtotal | 3 | 470 | **Low-Medium** |
| **Total** | **28 files** | **~3,230 lines** | |

### Integration Changes in Existing Crates

| Crate | Changes | Lines | Complexity |
|-------|---------|-------|------------|
| `crb-harness/src/config.rs` | Add ~15 new CLI flags | 80 | Low |
| `crb-harness/src/main.rs` | Extended eval pipeline | 100 | Medium |
| `crb-harness/src/validation.rs` | Extended baseline types | 150 | Medium |
| `crb-judge/src/lib.rs` | Optional annotation in JudgeVerdict | 20 | Low |
| `crb-reporting/src/lib.rs` | Optional extended fields | 30 | Low |
| `Cargo.toml` | Add crb-eval dependency | 5 | Low |
| **Subtotal** | | **385** | |

### Total: ~3,615 lines of Rust (new crate) + ~385 lines (existing crate changes)

### Timeline Estimate (single developer)

| Phase | Lines | Estimated Time | Dependencies |
|-------|-------|---------------|--------------|
| Phase 1: Record/Replay | 600 | 3-4 days | None (self-contained) |
| Phase 2: Golden Mgmt | 510 | 2-3 days | Phase 1 (reuses trace types) |
| Phase 3: Metrics | 1,080 | 4-5 days | Phase 1, 2 (needs traces + goldens) |
| Phase 4: Regression | 300 | 1-2 days | Phase 3 (needs ExtendedMetrics) |
| Integration | 385 | 1-2 days | All phases |
| Testing + Polish | 470 | 2-3 days | All phases |
| **Total** | **3,615** | **13-19 days** | |

---

## Appendix A: Baseline File Format (Extended)

Current (`baselines/5.14.json`):
```json
{
  "version": "5.14",
  "expected": {
    "total_prs": 50,
    "avg_precision": 0.71,
    "avg_recall": 0.65,
    "avg_f1": 0.6788
  },
  "thresholds": {
    "precision_delta": 0.05,
    "recall_delta": 0.05,
    "f1_delta": 0.05
  }
}
```

Extended (`baselines/5.15.json`):
```json
{
  "version": "5.15",
  "expected": {
    "total_prs": 50,
    "avg_precision": 0.71,
    "avg_recall": 0.65,
    "avg_f1": 0.6788,
    "per_agent": {
      "SA": { "precision": 0.80, "recall": 0.52, "f1": 0.63 },
      "CL": { "precision": 0.69, "recall": 0.61, "f1": 0.65 },
      "AR": { "precision": 0.55, "recall": 0.38, "f1": 0.45 },
      "SEC": { "precision": 0.78, "recall": 0.44, "f1": 0.56 }
    },
    "per_severity": {
      "critical": { "precision": 0.92, "recall": 0.45, "f1": 0.60 },
      "high": { "precision": 0.74, "recall": 0.58, "f1": 0.65 },
      "medium": { "precision": 0.58, "recall": 0.72, "f1": 0.64 },
      "low": { "precision": 0.40, "recall": 0.35, "f1": 0.37 }
    }
  },
  "thresholds": {
    "precision_delta": 0.05,
    "recall_delta": 0.05,
    "f1_delta": 0.05,
    "per_agent": {
      "SA": { "f1_delta": 0.08 },
      "CL": { "f1_delta": 0.08 },
      "AR": { "f1_delta": 0.10 },
      "SEC": { "f1_delta": 0.08 }
    },
    "per_severity": {
      "critical": { "f1_delta": 0.10 },
      "high": { "f1_delta": 0.08 },
      "medium": { "f1_delta": 0.08 },
      "low": { "f1_delta": 0.12 }
    }
  }
}
```

## Appendix B: Pricing Config Format (`pricing.toml`)

```toml
[model.gpt-4o]
input_per_1k = 0.0025
output_per_1k = 0.0100

[model."gpt-4o-mini"]
input_per_1k = 0.00015
output_per_1k = 0.00060

[model."claude-sonnet-4-20250514"]
input_per_1k = 0.0030
output_per_1k = 0.0150

[model."claude-haiku-3-5"]
input_per_1k = 0.0008
output_per_1k = 0.0040
```

---

## Appendix C: References

1. **Kodus AI Eval System** — `evals/investigation/` in [kodustech/kodus-ai](https://github.com/kodustech/kodus-ai)
   - `recall-assertion.js` — recall/precision/fairness/fidelity metrics
   - `extract-replay-from-trace.js` — Langfuse trace → replay dataset
   - `promptfoo-recall.yaml` — promptfoo eval configuration
2. **Existing Kodus Analysis** — `/data/workspace/projects/code-review-benchmark-research/research/kodus-ai-analysis.md`
3. **Our Harness** — [/data/workspace/projects/review-harness/](/data/workspace/projects/review-harness/)
   - `crb-harness/src/main.rs` — CLI entry point
   - `crb-harness/src/validation.rs` — baseline comparison
   - `crb-judge/src/lib.rs` — current metric computation
   - `crb-consensus/src/lib.rs` — consensus pipeline
   - `crb-reporting/src/lib.rs` — golden loading + output
4. **Promptfoo** — [promptfoo.dev](https://www.promptfoo.dev/) — deterministic replay provider, assertions, multi-model evals
