# Delta for Data Domain — RunMetadata Schema

## ADDED Requirements

### Requirement: RunMetadata struct

The system SHALL define a `RunMetadata` struct with the following fields, all optional or defaulted for backward compatibility:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled_features` | `Vec<String>` | `[]` | Names of runtime feature flags that were enabled |
| `model` | `Option<String>` | `None` | Primary evaluation model |
| `judge_model` | `Option<String>` | `None` | Judge model used for scoring |
| `reasoning_effort` | `Option<String>` | `None` | Reasoning effort level (`"low"`, `"medium"`, `"high"`, or `None`) |
| `roles` | `Option<String>` | `None` | Comma-separated agent roles |
| `max_findings` | `Option<usize>` | `None` | Max findings per agent |
| `dataset` | `Option<String>` | `None` | Dataset directory or identifier |
| `prompt_library` | `Option<String>` | `"builtin"` | Prompt library source |
| `rules_enabled` | `bool` | `false` | Whether rules were loaded |
| `harness_commit` | `Option<String>` | `None` | Git commit of the harness binary |
| `started_at` | `Option<String>` | `None` | ISO 8601 timestamp of run start |
| `duration_secs` | `Option<f64>` | `None` | Total run duration in seconds |

#### Scenario: Metadata created from CLI args
- GIVEN a benchmark run is started with `MODEL=deepseek/deepseek-v4-pro`, roles "SA,CL", and `exp14_template_vars` enabled
- WHEN `RunMetadata` is constructed
- THEN `model` SHALL be `Some("deepseek/deepseek-v4-pro")`
- AND `roles` SHALL be `Some("SA,CL")`
- AND `enabled_features` SHALL include `"exp14_template_vars"`
- AND `started_at` SHALL be a valid ISO 8601 timestamp

#### Scenario: Metadata created from ad-hoc API request
- GIVEN an ad-hoc review is requested with `model="deepseek/deepseek-v4-flash"` and `roles=["SA"]`
- WHEN `RunMetadata` is constructed
- THEN `model` SHALL be `Some("deepseek/deepseek-v4-flash")`
- AND `roles` SHALL be `Some("SA")`

### Requirement: RunMetadata serialization

The system SHALL serialize `RunMetadata` as a top-level `metadata` key in all run-related JSON output files.

#### Scenario: Benchmark per-PR JSON includes metadata
- GIVEN a benchmark run produces per-PR result files
- WHEN each result JSON is written
- THEN a top-level `metadata` key SHALL contain the serialized `RunMetadata`

#### Scenario: Benchmark summary JSON includes metadata
- GIVEN a benchmark run writes a `_summary.json` file
- WHEN the summary is serialized
- THEN a top-level `metadata` key SHALL contain the serialized `RunMetadata`

#### Scenario: Ad-hoc summary JSON includes metadata
- GIVEN an ad-hoc review run writes a summary file
- WHEN the summary is serialized
- THEN a top-level `metadata` key SHALL contain the serialized `RunMetadata`

### Requirement: Backward-compatible deserialization

The system SHALL deserialize existing run files (which lack a `metadata` key) without error.

#### Scenario: Old file without metadata
- GIVEN a JSON file from a previous run that has no `metadata` key
- WHEN it is deserialized into a struct containing `RunMetadata`
- THEN deserialization SHALL succeed
- AND `metadata` SHALL be `RunMetadata::default()`

#### Scenario: Old file with partial metadata
- GIVEN a JSON file that has a `metadata` key with only `model` set
- WHEN it is deserialized
- THEN `model` SHALL be the stored value
- AND all other fields SHALL be their defaults

### Requirement: Harness commit capture

The system SHALL capture the Git commit hash of the harness binary at build time.

#### Scenario: Commit baked into binary
- GIVEN the harness is built from a Git repository
- WHEN `RunMetadata::harness_commit` is populated
- THEN it SHALL contain the output of `git rev-parse HEAD` at build time

#### Scenario: Build outside Git
- GIVEN the harness is built outside a Git repository (e.g., CI without `.git`)
- WHEN `RunMetadata::harness_commit` is populated
- THEN it SHALL be `None` or `"unknown"`
