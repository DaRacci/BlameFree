# Delta for Data

## ADDED Requirements

### Requirement: Golden Comments Loading
The system SHALL load golden comment datasets from Martian-format JSON files.

#### Scenario: Load all 5 language datasets
- GIVEN 5 JSON files in `datasets/golden_comments/` (sentry.json, grafana.json, cal_dot_com.json, discourse.json, keycloak.json)
- WHEN the harness loads the datasets
- THEN it validates each entry has: pr_title, url, comments array with {comment, severity}
- AND it reports dataset stats per file (count, language, severity distribution)

#### Scenario: Invalid entry handling
- GIVEN a JSON file with a malformed entry (missing required field)
- WHEN the harness attempts to load it
- THEN it logs the error with file path and entry index
- AND it skips that entry
- AND it continues loading the rest

### Requirement: Diff Loading
The system SHALL load PR diffs from pre-scaffolded repositories or cached diff files.

#### Scenario: Load from local checkout
- GIVEN a PR URL mapped to a local repo path
- WHEN the harness needs the diff
- THEN it reads the diff file from `repos/{owner}/{repo}/{pr_num}/diff.diff`
- OR it falls back to `git diff` against the base branch

### Requirement: Results Output
The system SHALL output evaluation results in structured JSON and summary CSV formats.

#### Scenario: Per-PR results
- GIVEN a completed evaluation run
- WHEN results are written
- THEN each PR gets a JSON file with: agent findings, judge decisions, TP/FP/FN counts
- AND a summary JSONL file aggregates all PR results

#### Scenario: Summary report
- GIVEN a completed evaluation run across N PRs
- WHEN the reporting module runs
- THEN it outputs precision, recall, F1 per language and overall
- AND it outputs a CSV table with per-PR metrics
