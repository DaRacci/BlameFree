# Delta for Orchestration

## ADDED Requirements

### Requirement: Concurrent Agent Evaluation
The system SHALL evaluate code review PRs by running multiple agent roles (SA, CL, AR, SEC) concurrently against the same PR diff.

#### Scenario: Full evaluation of a single PR
- GIVEN a PR entry with a diff and golden comments
- WHEN the harness runs an evaluation
- THEN it calls 4 LLM agents concurrently (SA, CL, AR, SEC) for the PR
- AND it collects all structured findings from each agent
- AND it passes findings to the judge for comparison against golden comments

#### Scenario: Batch evaluation of PRs
- GIVEN a dataset of N PR entries
- WHEN the harness runs in batch mode
- THEN it evaluates up to M PRs concurrently (configurable concurrency)
- AND it respects rate limits via semaphore
- AND it produces an aggregated report

### Requirement: Deterministic Execution
The system SHALL produce the same output for the same inputs (same model, temperature, prompts, and PR data).

#### Scenario: Reproducible run
- GIVEN a fixed set of PRs, models, prompts, and temperature=0
- WHEN the harness runs twice
- THEN both runs produce identical results (modulo LLM nondeterminism at temperature=0)

## REMOVED Requirements

### Requirement: Subagent Orchestration
(No longer needed — subagent spawning, tool-use loops, and LLM-native multistep reasoning are replaced by direct API calls with fixed prompt templates.)
