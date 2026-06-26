# Tasks: Multi-Agent Consensus

## Phase 1: Core Types & Agent Infrastructure

- [ ] **1.1 Create `review-harness/src/consensus/types.rs`**
  - Define `Finding` struct (`file: String`, `line: u32`, `severity: String`,
    `message: String`, `code: Option<String>`).
  - Implement `JsonSchema` + `Deserialize` + `Serialize` (for Extractor + output).
  - Define `GoldenComment` struct (`file`, `line`, `message_regex`, `severity`, `source`).
  - Define `MatchResult` enum (`TruePositive`, `FalsePositive`, `FalseNegative`).
  - Define `ConsensusReport` struct with agent results, TP/FP/FN lists, and metrics.
  - Define `Role` enum (`SA`, `CL`, `AR`, `SEC`) with `system_prompt()` method.
  - Define `ReviewerConfig` struct (`role`, `model`, `max_findings`).

- [ ] **1.2 Create `review-harness/src/consensus/agent.rs`**
  - Implement `build_reviewer_agent(config: &ReviewerConfig, diff: &str)`
    returning a rig `Extractor<Output = Vec<Finding>>`.
  - Use `openai::Client::from_env()` (or configurable provider).
  - Each agent gets the role-specific system prompt + full diff as preamble.
  - Enforce `max_findings` cap on output.

- [ ] **1.3 Create `review-harness/src/consensus/mod.rs`**
  - Re-export all public types.
  - Re-export `run_consensus()` entry point.
  - Re-export `run_reviewers()` for parallel execution.

## Phase 2: Judge & Matching

- [ ] **2.1 Create `review-harness/src/consensus/judge.rs`**
  - Define `JUDGE_PROMPT` constant.
  - Implement `judge_comment(golden: &GoldenComment, candidates: &[Finding])`
    returning `MatchResult`.
  - Initial implementation: heuristic matching (exact file + line + severity,
    substring message match).
  - Advanced implementation: LLM-based semantic matching via another
    `Extractor`.

- [ ] **2.2 Implement `run_consensus()` orchestration**
  - Run all agents via `tokio::JoinSet`.
  - Flatten findings.
  - Run judge against golden comments.
  - Aggregate TP/FP/FN.
  - Compute precision, recall, F1.

## Phase 3: Integration

- [ ] **3.1 Wire into main entry point**
  - Load PR diff from git.
  - Load golden comments from file or hardcoded set.
  - Call `run_consensus()`.
  - Print or serialize `ConsensusReport` to JSON.

- [ ] **3.2 Add CLI flags**
  - `--roles` to select subset of agents (default: all four).
  - `--model` to override model for all agents.
  - `--judge-model` to set judge model independently.
  - `--max-findings` per agent.

## Phase 4: Testing

- [ ] **4.1 Unit tests for types**
  - Test `Finding` deserialization from JSON.
  - Test `Role::system_prompt()` returns expected strings.
  - Test `ConsensusReport` metric calculations.

- [ ] **4.2 Unit tests for agent builder**
  - Verify `build_reviewer_agent()` constructs correct system prompt.
  - Verify `max_findings` cap is applied.

- [ ] **4.3 Judge tests**
  - Test heuristic matching: exact match, partial match, no match.
  - Test LLM-based matching with known golden/finding pairs (requires
    integration test setup).

- [ ] **4.4 Integration tests**
  - Run mock diff through full pipeline with known golden comments.
  - Verify metrics are correct: all TPs → 1.0/1.0/1.0.
  - Verify all FNs → 0.0/0.0/0.0.
  - Verify mixed case produces correct intermediate metrics.

## Phase 5: Edge Cases & Hardening

- [ ] **5.1 Handle agent failures gracefully**
  - If an agent returns `Err`, log warning and continue with `Vec::new()`.
  - Ensure `ConsensusReport` records agent-level errors.

- [ ] **5.2 Empty inputs**
  - Empty diff → all agents return empty vectors → precision=1.0, recall=1.0,
    F1=1.0 (no findings matches no goldens vacuously).
  - Empty golden set → all findings are FPs → precision=0.0, recall=1.0, F1=0.0.

- [ ] **5.3 Large findings output**
  - Cap total findings across all agents (configurable, default 100).
  - Skip findings beyond the cap with a warning.

- [ ] **5.4 Timeout handling**
  - Per-agent timeout via `tokio::time::timeout` (default 120s).
  - Timed-out agents recorded as empty with a warning.

## Phase 6: Documentation

- [ ] **6.1 Module-level docs**
  - Document all public types and functions with `///` doc comments.
  - Include example usage for `run_consensus()`.

- [ ] **6.2 Developer guide**
  - How to add a new reviewer role.
  - How to add golden comments.
  - How to run the consensus pipeline standalone.
