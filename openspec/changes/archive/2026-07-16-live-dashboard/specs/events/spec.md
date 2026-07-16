# Delta for Dashboard Events

## ADDED Requirements

### Requirement: DashboardEvent Enum
The system SHALL define a DashboardEvent enum for all agent-to-dashboard communications.

#### Scenario: Agent started
- GIVEN an agent begins processing a PR
- WHEN the agent code emits a DashboardEvent::AgentStarted
- THEN it includes the agent role and PR title

#### Scenario: Agent chunk
- GIVEN an agent streaming response receives a text chunk
- WHEN the agent code emits a DashboardEvent::AgentChunk
- THEN it includes the agent role and text chunk

#### Scenario: Agent finished
- GIVEN an agent completes its review
- WHEN the agent code emits a DashboardEvent::AgentFinished
- THEN it includes the agent role, PR title, findings count, and duration

#### Scenario: PR completed
- GIVEN all agents for a PR have finished
- WHEN the main loop emits a DashboardEvent::PrCompleted
- THEN it includes the PR title and total duration

#### Scenario: Run progress
- GIVEN a PR result is collected
- WHEN the main loop emits a DashboardEvent::RunProgress
- THEN it includes completed count, total count, and cost summary

### Requirement: Non-Blocking Event Sending
The system SHALL send events via non-blocking try_send to never block agent tasks.

#### Scenario: Channel full
- GIVEN the dashboard event channel is full
- WHEN an agent calls send_dashboard_event
- THEN the event is silently dropped (best-effort)

#### Scenario: Dashboard not active
- GIVEN the dashboard is not active (None sender)
- WHEN an agent calls send_dashboard_event
- THEN it is a no-op with zero overhead

### Requirement: Event Ordering Guarantees
The system SHALL maintain in-order delivery per agent role.

#### Scenario: Per-agent ordering
- GIVEN AgentStarted then AgentChunk then AgentFinished from the same agent
- WHEN the dashboard receives them
- THEN they are processed in order
- AND events from different agents may interleave

### Requirement: Supporting Types
The system SHALL define AgentRole and CostSummary types for dashboard use.

#### Scenario: AgentRole display
- GIVEN an AgentRole::SA variant
- WHEN name() is called
- THEN it returns "SA"

#### Scenario: CostSummary aggregation
- GIVEN API call records
- WHEN CostSummary is populated
- THEN it includes total_usd, per_role costs, api_calls, and cache_hits
