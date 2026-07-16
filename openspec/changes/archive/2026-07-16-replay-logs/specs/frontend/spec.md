# Delta for Log Viewing - Frontend

## ADDED Requirements

### Requirement: Separate PR Detail Page

The log viewing UI SHALL be implemented as a dedicated page at `/runs/:id/prs/:pr_key`, not as a tab on the run detail page. The page provides breadcrumb navigation and a "Back to Run" button.

#### Scenario: Navigate to PR detail page

- GIVEN the user is on the run detail page for run `run-123`
- WHEN they click the "Logs" link on a PR row with `has_agents == true`
- THEN the router navigates to `/runs/run-123/prs/scale-color__lightness`
- AND the page shows breadcrumb: `Home / run-123 / scale-color__lightness`

#### Scenario: Breadcrumb navigation

- GIVEN the user is on the PrDetailPage
- WHEN the page renders
- THEN it shows breadcrumb links: "Home" → `/`, the run ID → `/runs/:id`, and the PR key as the current page label

#### Scenario: Back to Run button

- GIVEN the user is viewing logs for a PR
- WHEN they click the "&lt; Back to Run" button
- THEN the browser navigates to `/runs/:id`

#### Scenario: PR header with title and badge

- GIVEN the PR agents response loads successfully with `pr_title = "Scale color lightness"`
- WHEN the page renders
- THEN it shows the PR title as an h1 heading
- AND it shows a badge with the PR key as `PR #{pr_key}`
- AND it shows the run ID as secondary text

---

### Requirement: Multi-Step Data Loading

The page SHALL fetch agent availability first via `GET /api/runs/:id/prs/:pr_key`, then concurrently fetch individual agent logs for each available role via `GET /api/runs/:id/logs/:pr_key/:role`.

#### Scenario: Fetch agents then load logs concurrently

- GIVEN a run with cache available for agents SA and CL
- WHEN PrDetailPage mounts
- THEN it calls `GET /api/runs/:id/prs/:pr_key` to get `PrAgentsResponse` with 2 agents
- AND it displays the PR title and agent availability badges immediately
- THEN it spawns concurrent `GET /api/runs/:id/logs/:pr_key/SA` and `GET /api/runs/:id/logs/:pr_key/CL` requests via `wasm_bindgen_futures::spawn_local`
- AND stores results in a `HashMap<String, AgentLogResponse>`

#### Scenario: Loading state shown while fetching agents

- GIVEN the page has mounted but the `PrAgentsResponse` has not yet arrived
- WHEN the page renders
- THEN it shows a loading placeholder: "Loading PR details..."
- AND the agent log area is not yet visible

#### Scenario: Loading state shown while fetching individual logs

- GIVEN the `PrAgentsResponse` has arrived with 2 agents
- WHEN individual agent log fetches are in progress
- THEN the header shows "Loading agent logs..." in italic secondary text
- AND each agent card shows "Loading..." inside the card body

#### Scenario: Error state on failed agents fetch

- GIVEN the network returns a non-200 status for the agents endpoint
- WHEN the fetch fails
- THEN the page shows an error state with "Failed to load PR details" heading
- AND a "Retry" button that re-triggers the fetch

---

### Requirement: Agent Log Cards

Each agent SHALL be rendered as a card with a role-colored left border and dot indicator, availability badges for Prompt/Response/Reasoning, and expandable `<details>` sections for log content.

#### Scenario: Card structure with availability badges

- GIVEN a `PrAgentEntry` with `role = "SA", has_prompt = true, has_response = true, has_reasoning = false`
- WHEN the agent card renders
- THEN the card has a left border colored by `role_color("SA")`
- AND the header shows a colored dot indicator
- AND shows "✓ Prompt" (green), "✓ Response" (green), and no Reasoning badge

#### Scenario: Card shows missing availability

- GIVEN a `PrAgentEntry` with `role = "CL", has_prompt = false, has_response = false, has_reasoning = false`
- WHEN the agent card renders
- THEN the header shows "✗ Prompt" and "✗ Response" in muted gray
- AND the card body shows "No log data available."

#### Scenario: Expandable prompt section

- GIVEN an `AgentLogResponse` with `prompt = Some("You are a code reviewer...")`
- WHEN the card renders
- THEN the prompt content is inside a `<details>` element with `<summary>` labeled "Prompt"
- AND the details element is expanded by default (`open=true`)
- AND the content is rendered in a `<pre>` block with `white-space: pre-wrap`

#### Scenario: Expandable response section

- GIVEN an `AgentLogResponse` with `response = Some("## Summary\n\nThe PR...")`
- WHEN the card renders
- THEN the response content is inside a `<details>` element with `<summary>` labeled "Response"
- AND the details element is expanded by default
- AND the content is rendered in a `<pre>` block

#### Scenario: Expandable reasoning section

- GIVEN an `AgentLogResponse` with `reasoning = Some("The agent considered...")` (non-empty)
- WHEN the card renders
- THEN the reasoning content is inside a `<details>` element with `<summary>` labeled "Reasoning"
- AND the details element is collapsed by default
- AND the content is rendered in a `<pre>` block

#### Scenario: Cards arranged in responsive grid

- GIVEN there are 3 agents with log data
- WHEN the card grid renders
- THEN cards are laid out in a CSS grid with `grid-template-columns: repeat(auto-fit, minmax(450px, 1fr))`
- AND cards wrap responsively when the viewport narrows

#### Scenario: Role display name from config

- GIVEN the server config defines a role with `abbreviation = "SA"` and `name = "Security Auditor"`
- WHEN the agent card renders for role SA
- THEN the header shows the display name "Security Auditor" instead of the abbreviation "SA"
- AND the card falls back to the abbreviation when no display name is configured

---

### Requirement: Empty / No-Cache State

When no cache data exists for a PR, the page SHALL show a clear empty state message with a "Back to Run" button instead of attempting to load agents.

#### Scenario: Empty state when cache unavailable

- GIVEN the `PrAgentsResponse` returns `agents = []` (no cached agents)
- WHEN the page renders
- THEN it shows "No cached agent logs available for this PR." as the primary message
- AND a secondary explanation: "Agent logs are only available when the run was executed with caching enabled and the cache is still present."
- AND a "&lt; Back to Run" button styled as a primary button

#### Scenario: Empty state when agents list empty

- GIVEN the agents endpoint returns successfully with an empty agents array
- WHEN the page renders
- THEN it does NOT show agent cards
- AND it does NOT make individual log fetch requests
- AND it shows the empty state UI

---

### Requirement: Logs Link in Run Detail Page

The run detail page SHALL show a "Logs" link for each PR row that has agent data, and a disabled "Logs" button with tooltip for PRs without cached logs.

#### Scenario: Active logs link for PRs with agents

- GIVEN a `PrResult` with `has_agents = true` and `pr_key = "fix-bug-123"`
- WHEN the run detail page renders a table row for this PR
- THEN the actions column shows a clickable "Logs" link styled as a bordered button
- AND clicking the link navigates to `/runs/:id/prs/fix-bug-123`

#### Scenario: Disabled logs button for PRs without agents

- GIVEN a `PrResult` with `has_agents = false`
- WHEN the run detail page renders a table row for this PR
- THEN the actions column shows a "Logs" label in muted style with `cursor: not-allowed`
- AND the element has a `title` attribute set to "No cached logs available"
- AND clicking the label does nothing (no navigation)

---

## MODIFIED Requirements

*(None — all log viewing frontend functionality is newly added.)*

## REMOVED Requirements

*(None — no frontend functionality was removed.)*
