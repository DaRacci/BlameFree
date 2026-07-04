# Delta for Dashboard Domain — Metadata in Dashboard Events

## ADDED Requirements

### Requirement: RunStarted event

The system SHALL add a `RunStarted` variant to the `DashboardEvent` enum (in both `crb-dashboard` and `crb-webui/src/events.rs`).

#### Scenario: RunStarted sent at start
- GIVEN a benchmark run begins
- WHEN all configuration is loaded and the first PR is about to be processed
- THEN a `DashboardEvent::RunStarted { metadata }` SHALL be sent over the dashboard channel

#### Scenario: RunStarted carries metadata
- GIVEN `DashboardEvent::RunStarted` is emitted
- WHEN it is serialized
- THEN it SHALL include a `metadata` field containing the full `RunMetadata`

### Requirement: RunFinished carries metadata

The system SHALL add a `metadata` field to the existing `RunFinished` variant in `DashboardEvent`.

#### Scenario: RunFinished includes metadata
- GIVEN a benchmark run completes
- WHEN `DashboardEvent::RunFinished` is emitted
- THEN it SHALL include a `metadata` field containing the full `RunMetadata` (with `duration_secs` populated)

## MODIFIED Requirements

### Requirement: RunFinished event (extended)

The `RunFinished` variant SHALL gain an additional `metadata: RunMetadata` field.
(Previously: `RunFinished` had `total_prs`, `aggregated`, `total_cost`, `total_tokens`, `total_agent_calls` only.)

#### Scenario: RunFinished metadata populated at end
- GIVEN `RunFinished` is constructed at the end of a benchmark run
- WHEN `duration_secs` is computed as `elapsed.as_secs_f64()`
- THEN `metadata.duration_secs` SHALL be set to that value

### Requirement: WebUI DashboardEvent alignment

The `DashboardEvent` enum in `crates/crb-webui/src/events.rs` SHALL gain `RunStarted` and the extended `RunFinished` variant, matching the `crb-dashboard` crate.

#### Scenario: WebUI handles RunStarted
- GIVEN the web UI receives a `DashboardEvent::RunStarted` from the harness subprocess
- WHEN it is parsed via `parse_event_line()`
- THEN it SHALL deserialize successfully
- AND the metadata SHALL be available for display

#### Scenario: WebUI handles extended RunFinished
- GIVEN the web UI receives a `DashboardEvent::RunFinished` with metadata
- WHEN it is parsed
- THEN it SHALL deserialize successfully
- AND the metadata SHALL be available alongside aggregate data

### Requirement: ActiveRun carries metadata

The `ActiveRun` struct in `crates/crb-webui/src/server.rs` SHALL store the `RunMetadata` for the currently running benchmark.

#### Scenario: Metadata tracked during run
- GIVEN an active benchmark run
- WHEN the `ActiveRun` state is read
- THEN `metadata` SHALL be populated with the run's `RunMetadata`

#### Scenario: Metadata available in run detail API
- GIVEN the run detail API endpoint returns an active run's state
- WHEN the response is constructed
- THEN `metadata` SHALL be included
