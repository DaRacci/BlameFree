# pages Specification

## Purpose
Frontend page layouts and components for the web UI dashboard, including overview, run detail, and PR detail pages.
## Requirements
### Requirement: Dashboard / Home Page (`/`)

**Description:** The home page SHALL show an overview of benchmark and ad-hoc runs with metrics cards, quick action buttons, a running reviews section, and a recent runs list.

**Layout:**
```
┌────────────────────────────────────────────────────────┐
│  Overview                                               │
│  [+ New Benchmark]  [+ Ad-hoc Review]                   │
├────────────────────────────────────────────────────────┤
│  ┌──────────┐  ┌──────────┐  ┌──────────┐              │
│  │Total Runs│  │ Avg F1   │  │PRs Rev'd │              │
│  │    12    │  │   0.72   │  │    84    │              │
│  └──────────┘  └──────────┘  └──────────┘              │
│                                                         │
│  Running Reviews                                        │
│  ┌────────────────────────────────────────────────────┐ │
│  │ ● smoke-test-1            Running   2 PRs  02:00  │ │
│  │ ────────────────────────────────────────────────── │ │
│  │ Model: gpt-4o                        Details >    │ │
│  └────────────────────────────────────────────────────┘ │
│                                                         │
│  Recent Runs                                            │
│  ┌────────────────────────────────────────────────────┐ │
│  │ smoke-5                 benchmark  completed       │ │
│  │ smoke-4                 benchmark  completed       │ │
│  │ discourse#7             ad-hoc     completed       │ │
│  └────────────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────┘
```

**Implementation file:** `crates/crb-webui-frontend/src/pages/home.rs`

**Data sources:**
- `GET /api/runs` for benchmark runs
- `GET /api/adhoc/runs` for ad-hoc runs
- Polls every 5 seconds while any run has `running` or `pending` status

**State:**
- `bench_runs: Vec<RunSummary>`, `adhoc_runs: Vec<AdhocRunSummary>`
- `loading`, `error`, `has_active` signals
- Recent runs: merged list of benchmark and ad-hoc, sorted by created_at, truncated to 10

#### Scenario: Loading state shows skeleton placeholders

Given the page is mounted and data is being fetched
When the `loading` signal is true
Then the view shows skeleton placeholder elements for metrics cards and run lists
And no error or empty state is shown

#### Scenario: Error state shows retry button

Given the API fetch fails
When the `error` signal contains an error message
Then an error state is displayed with the error text and a "Retry" button
That re-fetches both `/api/runs` and `/api/adhoc/runs`

#### Scenario: Empty state shows "No runs yet"

Given the API returns empty arrays for both benchmark and ad-hoc runs
When the page renders
Then it shows "No active reviews" under Running Reviews
And "No runs yet" under Recent Runs

#### Scenario: Active runs auto-poll every 5 seconds

Given at least one run has `status: "running"` in either benchmark or ad-hoc list
When the `has_active` signal is true
Then a 5-second interval is set up that periodically re-fetches both endpoints
And the interval is cleared when no runs remain active

#### Scenario: Metrics cards show aggregated stats

Given there are completed benchmark runs
When the page renders
Then the Total Runs card shows the count of completed (non-running) runs
And the Avg F1 card shows the mean F1 across completed benchmarks
And the PRs Reviewed card shows the sum of all PR counts

---

### Requirement: Run Detail Page (`/runs/:id`)

**Description:** The Run Detail page SHALL show detailed metrics and per-PR results for a specific benchmark or ad-hoc run.

**Layout:**
```
┌────────────────────────────────────────────────────────┐
│  < Dashboard    smoke-5                                 │
│                 ● completed    Model: gpt-4o            │
├────────────────────────────────────────────────────────┤
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌─────────┐│
│  │ F1 Score │  │ Precision│  │  Recall  │  │Tot Cost ││
│  │  0.500   │  │  0.333   │  │  1.000   │  │ $0.015  ││
│  └──────────┘  └──────────┘  └──────────┘  └─────────┘│
│  ┌──────────┐                                          │
│  │Duration  │                                          │
│  │  120s    │                                          │
│  └──────────┘                                          │
│                                                         │
│  Per-PR Results                                         │
│  ┌────┬──────────┬──────┬──────┬──────┬───────┬──────┐ │
│  │ #  │ Title    │ F1   │ Prec │ Rec  │ Cost  │Logs  │ │
│  ├────┼──────────┼──────┼──────┼──────┼───────┼──────┤ │
│  │ #7 │ scale..  │0.500 │0.333 │1.000 │$0.008 │Logs  │ │
│  └────┴──────────┴──────┴──────┴──────┴───────┴──────┘ │
└────────────────────────────────────────────────────────┘
```

**Implementation file:** `crates/crb-webui-frontend/src/pages/run_detail.rs`

**Data source:** `GET /api/runs/:id`

**Notes:**
- If the run is running, shows a progress bar and a "Live View" button
- Metrics card values use `MetricsProvider` traits (f1(), precision(), recall()) from aggregate
- Per-PR table shows pr_number, title, f1, precision, recall, cost, status badge, and Logs link
- Logs link is only clickable when `has_agents` is true

#### Scenario: Running run shows progress bar and Live View button

Given the run's status is "running" or "pending"
When the RunDetailPage renders
Then a progress bar shows completed vs total PRs
And a "Live View" button links to `/runs/:id/live`

#### Scenario: Completed run shows metrics and PR table

Given the run's status is "completed" or "done"
When the page renders
Then five metric cards show F1 Score, Precision, Recall, Total Cost, and Duration
And a table lists each PR with its metrics, cost, and a Logs link

#### Scenario: PR with agents shows clickable Logs link

Given a PR result has `has_agents: true`
When the PR table renders
Then the Logs cell is an active link to `/runs/:id/prs/:pr_key`
And clicking navigates to the PR detail page

#### Scenario: PR without agents shows disabled Logs link

Given a PR result has `has_agents: false`
When the PR table renders
Then the Logs cell is a disabled/non-clickable styled element with a "No cached logs" tooltip

---

### Requirement: PR Detail Page (`/runs/:id/prs/:pr_key`)

**Description:** The PR Detail page SHALL show agent log prompt/response/reasoning for a specific PR in a run. Fetches agent availability and then lazily loads each agent's log data.

**Layout:**
```
┌────────────────────────────────────────────────────────┐
│  Home / smoke-5 / discourse-graphite-pull-7             │
├────────────────────────────────────────────────────────┤
│  Update color function                     [< Back]    │
│  PR #discourse-graphite-pull-7    Run: smoke-5         │
├────────────────────────────────────────────────────────┤
│  Agent Logs                                             │
│                                                         │
│  ┌── SA ────────────────────────────────────────────┐  │
│  │ ● SA (Security Auditor)    ✓ Prompt  ✓ Response  │  │
│  │ ──────────────────────────────────────────────────│  │
│  │ ▶ Prompt                                            │
│  │ ▶ Response (open)                                   │
│  │   Analyzing the PR diff... Found issue...          │  │
│  │ ▶ Reasoning                                         │
│  └────────────────────────────────────────────────────┘  │
│  ┌── CL ────────────────────────────────────────────┐  │
│  │ ● CL (Code Linter)        ✓ Prompt  ✗ Response  │  │
│  └────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────┘
```

**Implementation file:** `crates/crb-webui-frontend/src/pages/pr_detail.rs`

**Data sources:**
- `GET /api/runs/:id/prs/:pr_key` for agent availability
- `GET /api/runs/:id/logs/:pr_key/:role` for each agent's prompt/response/reasoning

**Notes:**
- Breadcrumb navigation: Home > run_id > pr_key
- Each agent is rendered in a card with a color derived from its abbreviation
- Agent cards show availability indicators (✓/✗) for Prompt, Response, Reasoning
- Log content uses `<details>` expandable sections

#### Scenario: Fetches agent list then loads each agent's logs

Given the page mounts with valid run_id and pr_key
When the component loads
Then it fetches `GET /api/runs/:id/prs/:pr_key` to get the agent list
And for each available agent, fetches its individual log
And renders each agent's prompt, response, and reasoning in expandable sections

#### Scenario: No agents shows "No cached agent logs available"

Given the PR has no agent data (`agents` list is empty)
When the page renders
Then a message "No cached agent logs available for this PR" is displayed
With a "< Back to Run" button

#### Scenario: Breadcrumb navigation shows full path

Given the user is on the PR detail page
When the page renders
Then a breadcrumb shows "Home / {run_id} / {pr_key}" with each segment as a link
And the page header shows the PR title and details

---

### Requirement: New Benchmark Run Page (`/new`)

**Description:** The New Benchmark Run page SHALL provide a form to configure and start a new benchmark run with model, dataset, concurrency, max findings, roles, PR filter, and other options.

**Layout:**
```
┌────────────────────────────────────────────────────────┐
│  New Benchmark Run                          [Cancel]   │
├────────────────────────────────────────────────────────┤
│  Configuration                                          │
│  ┌────────────────────────────────────────────────────┐ │
│  │ Model:       [gpt-4o                         ▼]   │ │
│  │ Dataset:     [golden_comments (42 PRs)        ▼]  │ │
│  │ Effort:      [medium                         ▼]   │ │
│  │ Concurrency: [━━━━━━●━━━━━━━━━━━━━━]  4            │ │
│  │ Max Findings:[━━━━━━●━━━━━━━━━━━━━━]  20           │ │
│  │ Roles:       ☑ SA  ☑ CL  ☑ AR  ☑ SEC              │ │
│  │ PR Filter:   [feature/*                    ]       │ │
│  │ Use Cache:   [✓]                                   │ │
│  │                                                    │ │
│  │ [🚀 Start Benchmark]                               │ │
│  └────────────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────┘
```

**Implementation file:** `crates/crb-webui-frontend/src/pages/new_run.rs`

**Data sources:**
- `GET /api/config` for available models and roles
- `GET /api/config/datasets` for dataset list with PR counts
- `GET /api/config/reasoning-efforts` for effort levels
- `GET /api/datasets/:id/prs` for PR listings (when dataset changes)

**Form fields:**
- **Model:** dropdown, sourced from config; first model auto-selected
- **Dataset:** dropdown with PR count labels; changing it fetches PRs and applies dataset defaults
- **Reasoning Effort:** dropdown (low/medium/high/max), fetched from API, fallback to hardcoded list
- **Concurrency:** numeric input (default 4)
- **Max Findings:** numeric input (default 20)
- **Roles:** checkbox group via `RoleSelector` component
- **PR Filter:** text input for filtering PRs by pattern
- **Use Cache:** checkbox (default true)

#### Scenario: Fetches initial config and pre-selects first model/dataset

Given the page mounts
When the component initializes
Then it fetches `/api/config`, `/api/config/datasets`, and `/api/config/reasoning-efforts`
And auto-selects the first model and dataset from the responses
And populates the role selector with available roles

#### Scenario: Dataset change applies defaults and fetches PRs

Given a dataset is selected
When the user changes the dataset selection
Then the dataset's `dataset.toml` defaults (model, concurrency, max_findings, roles) are applied to the form
And `GET /api/datasets/:id/prs` is fetched to populate the PR list

#### Scenario: Submit creates run and navigates to detail page

Given all form fields are valid
When the user clicks "Start Benchmark"
Then a `POST /api/runs` request is made with the form values
And on success, the app navigates to `/runs/{run_id}`
And submitting state is managed (disabled button, loading text)

---

### Requirement: Live View Page (`/runs/:id/live`)

**Description:** The Live View page SHALL be a real-time monitoring page for an active benchmark run. Connects via SSE and displays agent panes for each PR, progress bar, and status metrics.

**Layout:**
```
┌────────────────────────────────────────────────────────┐
│  ● Live: smoke-test-1                    [< Back]      │
├────────────────────────────────────────────────────────┤
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌─────────┐│
│  │ Progress │  │  Status  │  │Complete %│  │ActivePRs││
│  │ 5/10     │  │ running  │  │   50%    │  │    3    ││
│  └──────────┘  └──────────┘  └──────────┘  └─────────┘│
│                                                         │
│  PR: [discourse-7] [discourse-12] [discourse-15]       │
│                                                         │
│  ┌────── SA ──────┐  ┌────── CL ──────┐               │
│  │ ✓ completed    │  │ ● reviewing... │               │
│  │ ────────────── │  │ ────────────── │               │
│  │ Found issue..  │  │ Analyzing PR..│               │
│  │ PR: discourse-7│  │ PR: discourse-12              │
│  └────────────────┘  └────────────────┘               │
│  ┌────── AR ──────┐  ┌───── SEC ─────┐               │
│  │ || pending     │  │ || pending    │               │
│  └────────────────┘  └────────────────┘               │
│                                                         │
│  [████████████░░░░░]  5/10 PRs (50%)                   │
│  PRs loaded: 3                                         │
└────────────────────────────────────────────────────────┘
```

**Implementation file:** `crates/crb-webui-frontend/src/pages/live.rs`

**Data source:** `GET /api/runs/:id/live` (SSE stream)

**Key state:**
- `pr_states: HashMap<String, PrState>` — mapping PR key to its agent states
- `pr_order: Vec<String>` — ordered list of PR keys for the tab bar
- `selected_pr: Option<String>` — currently selected PR in the tab bar
- `progress_done/progress_total` — for the bottom progress bar
- `status` — connection status string ("connecting", "running", "complete", "error: ...")

**Event handling:**
- `AgentStarted` → creates PrState, marks agent as reviewing, auto-selects PR
- `AgentChunk` → appends to agent's response text
- `AgentFinished` → marks agent as done/failed, checks if all done
- `RunProgress` → updates progress bar and total
- `RunFinished` → sets status to "complete"
- `ReviewCompleted` → marks PR as completed

#### Scenario: Connecting state shows skeleton placeholders

Given the page is mounted but SSE is not yet connected
When the status is "connecting"
Then skeleton placeholders are shown for metrics and agent panes

#### Scenario: Error state shows error message with reconnect button

Given the SSE connection fails or errors out
When the status starts with "error" or is "no_run_id"
Then an error state is displayed with the status message

#### Scenario: Agent panes update in real-time from SSE events

Given an active SSE connection with status "running"
When `AgentStarted` events arrive
Then new PR tabs appear in the tab bar
And agent panes show "reviewing..." status
When `AgentChunk` events arrive
Then the corresponding agent pane's response text updates incrementally
When `AgentFinished` events arrive
Then the agent pane shows a checkmark or X with the completion status
And the bottom progress bar advances

#### Scenario: PR tab bar auto-selects the first PR

Given PRs are being processed
When the first `AgentStarted` or `RunProgress` event arrives
Then that PR is auto-selected in the tab bar
And subsequent PRs don't change the selection unless the current PR is completed

#### Scenario: Run complete state shows completed status

Given all PRs have been processed
When a `RunFinished` event is received
Then the status becomes "complete"
And the page header shows the run as completed

---

### Requirement: Ad-hoc Review Page (`/adhoc/new`)

**Description:** The Ad-hoc Review page SHALL provide a form to start an ad-hoc PR review by entering owner/repo, selecting or entering a PR number, choosing a model and roles.

**Layout:**
```
┌────────────────────────────────────────────────────────┐
│  Ad-hoc PR Review                                       │
├────────────────────────────────────────────────────────┤
│  Repository                                             │
│  ┌────────────────────────────────────────────────────┐ │
│  │ Owner:   [discourse       ]                        │ │
│  │ Repo:    [discourse       ]   [Load PRs]           │ │
│  └────────────────────────────────────────────────────┘ │
│                                                         │
│  PR Selection                                            │
│  ┌────────────────────────────────────────────────────┐ │
│  │ ○ Open PRs   ● Manual Entry                        │ │
│  │ PR #:    [7                   ]                     │ │
│  └────────────────────────────────────────────────────┘ │
│                                                         │
│  Configuration                                          │
│  ┌────────────────────────────────────────────────────┐ │
│  │ Model:   [gpt-4o          ]                        │ │
│  │ Roles:   ☑ SA  ☑ CL  ☑ AR  ☑ SEC                  │ │
│  └────────────────────────────────────────────────────┘ │
│                                                         │
│  [🚀 Start Review]                                      │
└────────────────────────────────────────────────────────┘
```

**Implementation file:** `crates/crb-webui-frontend/src/pages/adhoc_review.rs`

**Data sources:**
- `GET /api/config` for available models and roles
- `GET /api/adhoc/prs/:owner/:repo` for listing open PRs

**Notes:**
- Two PR selection modes: "Open PRs" (dropdown of fetched PRs) and "Manual Entry" (text input)
- Submit calls `POST /api/adhoc/review` with the full GitHub URL

#### Scenario: Empty owner/repo shows "Please enter both" on PR load

Given the owner or repo field is empty
When the user clicks "Load PRs"
Then an error message "Please enter both owner and repo." is displayed

#### Scenario: Open PR mode fetches and displays PRs

Given owner and repo are non-empty
When the user clicks "Load PRs" in Open PR mode
Then `GET /api/adhoc/prs/:owner/:repo` is called
And the response populates a dropdown of PRs to select from

#### Scenario: Manual entry mode accepts any PR number

Given the user selects "Manual Entry" mode
When they enter a PR number and start the review
Then the constructed URL is `https://github.com/{owner}/{repo}/pull/{number}`

#### Scenario: Valid submission navigates to run detail

Given all form fields are valid
When the user clicks "Start Review"
Then `POST /api/adhoc/review` is called with `{ url, model, roles }`
And on success, the app navigates to `/adhoc/runs/{run_id}`

---

### Requirement: Ad-hoc Runs Page (`/adhoc`)

**Description:** The Ad-hoc Runs page SHALL list all previous ad-hoc review runs with a button to start a new one.

**Layout:**
```
┌────────────────────────────────────────────────────────┐
│  Ad-hoc Review Runs                       [+ New]      │
├────────────────────────────────────────────────────────┤
│  ┌────────────────────────────────────────────────────┐ │
│  │ discourse/discourse #7   gpt-4o   $0.005  complete│ │
│  │ discourse/discourse #12  gpt-4o   $0.012  complete│ │
│  └────────────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────┘
```

**Implementation file:** `crates/crb-webui-frontend/src/pages/adhoc_runs.rs`

#### Scenario: Lists all ad-hoc runs from API

Given there are ad-hoc runs on disk
When the page renders
Then each run is displayed with its PR info, model, cost, and status
And a "[+ New]" button links to `/adhoc/new`

---

### Requirement: Admin Page (`/admin`)

**Description:** The Admin page SHALL be a server admin page with a live log viewer that connects to the SSE log stream.

**Layout:**
```
┌────────────────────────────────────────────────────────┐
│  Admin                                                  │
├────────────────────────────────────────────────────────┤
│  Server Logs                           [console]        │
│  ┌────────────────────────────────────────────────────┐ │
│  │ 750 lines         ● Connected                      │ │
│  │ ────────────────────────────────────────────────── │ │
│  │ 2026-07-16 INFO Starting crb-webui on port 8080   │ │
│  │ 2026-07-16 INFO Listening on http://0.0.0.0:8080 │ │
│  │ ...live scrolling...                               │ │
│  └────────────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────┘
```

**Implementation file:** `crates/crb-webui-frontend/src/pages/admin.rs`

**Data sources:**
- `GET /api/admin/logs` for initial log content
- `GET /api/admin/logs/stream` for live SSE log stream

**Key features:**
- Initial load fetches existing logs via REST
- Then connects to SSE stream for live updates
- Auto-scrolls to bottom as new lines arrive
- Shows connection status indicator (connecting/connected/disconnected)
- Shows line count

#### Scenario: Loads initial logs then streams live updates

Given the admin page mounts
When the component initializes
Then it fetches `GET /api/admin/logs` for initial log content
And then connects to `GET /api/admin/logs/stream` for live updates
And new log lines are appended to the display

#### Scenario: Auto-scrolls to bottom on new content

Given the log viewer has content
When new log lines arrive via SSE
Then the scroll position automatically moves to the bottom

---

### Requirement: Sidebar Component

**Description:** The Sidebar component SHALL be a persistent sidebar navigation with links to Dashboard, Benchmarks, Ad-hoc Review, and Admin pages. Supports desktop collapse and mobile overlay modes.

**Implementation file:** `crates/crb-webui-frontend/src/app.rs`

**Links:**
- Dashboard (`/`)
- Benchmarks (`/`)
- Ad-hoc Review (`/adhoc`)
- Admin (`/admin`)

**Behavior:**
- Collapsible on desktop (toggle button in header)
- Overlay mode on mobile (< 1200px width)
- Active link highlighting via route prefix matching
- Version footer ("v0.1.0")
- OAuth auth section (login/logout/avatar) when `auth_enabled` is true

#### Scenario: Highlights current route in sidebar

Given the user is on a page under `/adhoc/...`
When the sidebar renders
Then the "Ad-hoc Review" link has the `sidebar__item--active` class

#### Scenario: Shows auth section when enabled

Given OAuth is enabled (`auth_enabled: true`)
And the user is logged in
Then the sidebar shows the user's avatar, name, and a "Log out" link
Given the user is not logged in
Then a "Log in" link is shown

#### Scenario: Mobile sidebar shows overlay

Given the viewport width is less than 1200px
When the sidebar renders
Then it starts in collapsed mode with a hamburger button
Clicking the hamburger opens an overlay sidebar with a backdrop

---

### Requirement: AgentPane Component

**Description:** The AgentPane component SHALL display the status and streaming response of a single agent role in the live view.

**Implementation file:** `crates/crb-webui-frontend/src/components/agent_pane.rs`

**Props:** `name: String`, `status: Signal<String>`, `response: Signal<Option<String>>`, `current_pr: Signal<Option<String>>`

**States:**
- `pending` (|| icon, "pending" text, "Waiting for task..." message) → default state
- `reviewing`/`running` (● icon, "reviewing..." text, "Processing..." message) → active agent
- `done`/`completed` (✓ icon, "completed" text, response content shown) → successful completion
- `failed` (✗ icon, "failed" text, response content if any) → agent error

**Visual:**
- Color-coded pane border via CSS class: pending, running, completed, failed
- Footer shows current PR key being processed

#### Scenario: Pending state shows waiting message

Given an AgentPane with status "pending" and no response
When it renders
Then it shows the || icon with "pending" label
And displays "Waiting for task..." message

#### Scenario: Reviewing state shows processing indicator

Given an AgentPane with status "reviewing" and no response yet
When it renders
Then it shows the ● icon with "reviewing..." label
And displays "Processing..." message

#### Scenario: Done state shows response and checkmark

Given an AgentPane with status "done" and a response string
When it renders
Then it shows the ✓ icon with "completed" label
And displays the agent's response text in a `<pre>` block

#### Scenario: Failed state shows error indicator

Given an AgentPane with status "failed"
When it renders
Then it shows the ✗ icon with "failed" label

---

### Requirement: ProgressBar Component

**Description:** The ProgressBar component SHALL be a horizontal progress bar with label for showing completion percentage.

**Implementation file:** `crates/crb-webui-frontend/src/components/progress_bar.rs`

**Props:** `value: u32`, `max: u32`, `label: String`

**Visual:**
- Track with animated fill bar whose width = `(value/max) * 100%`
- CSS class `progress--complete` when value >= max
- ARIA attributes: `role="progressbar"`, `aria-valuenow`, `aria-valuemin`, `aria-valuemax`

#### Scenario: Shows correct fill percentage

Given a ProgressBar with value=3 and max=10
When it renders
Then the fill bar has `width: 30%`
And the label shows the provided text
And `aria-valuenow` is 3

#### Scenario: Complete state adds CSS class

Given a ProgressBar with value=10 and max=10
When it renders
Then the container has the `progress--complete` CSS class

#### Scenario: Zero max does not cause division errors

Given a ProgressBar with value=0 and max=0
When it renders
Then the fill bar has `width: 0%` (no division by zero)

---

### Requirement: MetricsCard Component

**Description:** The MetricsCard component SHALL be a simple card displaying a label and a numeric/text value, used for dashboard metrics.

**Implementation file:** `crates/crb-webui-frontend/src/components/metrics_card.rs`

**Props:** `value: impl Into<String>`, `label: &'static str`, `value_style: Option<&'static str>`

**Visual:** A card with a large value (styled with optional `value_style` for color coding) and a smaller label below.

#### Scenario: Renders value and label

Given a MetricsCard with value="$0.015" and label="Total Cost"
When it renders
Then the value "$0.015" is displayed in the large value area
And the label "Total Cost" is displayed below

---

### Requirement: RunTable Component

**Description:** The RunTable component SHALL be a sortable table of benchmark runs used on the home page (though not currently wired there — used as a standalone component).

**Implementation file:** `crates/crb-webui-frontend/src/components/run_table.rs`

**Props:** `runs: Vec<RunSummary>`

**Columns:** Name, Status, Model, PRs, F1, Cost, Details

**Sorting:** Click column headers to toggle ascending/descending. Sortable columns: Name, Status, Model, PR count, F1, Cost, Date.

**Notes:**
- Running runs show a "Live" link in the Details column
- Status is rendered as a colored badge
- F1 and Cost use monospace font

#### Scenario: Sorts by clicked column

Given a RunTable with multiple runs
When the user clicks the "F1" column header
Then the runs are sorted by avg_f1 ascending (first click)
And clicking again reverses to descending
And a sort arrow (^/v) appears next to the column header

#### Scenario: Running run shows Live button

Given a run with status "running" or "pending"
When it renders in the RunTable
Then a "Live" button links to `/runs/:id/live`

---

### Requirement: RoleSelector Component

**Description:** The RoleSelector component SHALL be a checkbox group for selecting reviewer roles, with incompatibility enforcement. Disabled roles show a tooltip explaining the conflict.

**Implementation file:** `crates/crb-webui-frontend/src/components/role_selector.rs`

**Props:**
- `available_roles: Vec<RoleInfo>` — all roles with their incompatibility info
- `selected_roles: ReadSignal<Vec<String>>` — currently selected role abbreviations
- `set_selected_roles: WriteSignal<Vec<String>>` — write access

**Behavior:**
- Each role is a checkbox with the abbreviation as label
- Selecting a role that is incompatible with another already-selected role disables the incompatible one
- Disabled checkboxes have a tooltip "Incompatible with: {role1}, {role2}"
- Incompatibility is checked bidirectionally (role A incompatible with B, B gets disabled when A is selected)

#### Scenario: All roles selectable initially

Given no roles are selected yet
When the RoleSelector renders
Then all role checkboxes are enabled and unchecked

#### Scenario: Incompatible role becomes disabled with tooltip

Given the user selects role "FE"
And role "BE" is incompatible with "FE"
When the checkbox state updates
Then "BE" checkbox becomes disabled
And hovering shows a tooltip "Incompatible with: FE"

#### Scenario: Unselecting a role re-enables incompatible roles

Given "FE" is selected and "BE" is disabled
When the user unchecks "FE"
Then "BE" becomes enabled again

---

### Requirement: LogViewer Component

**Description:** The LogViewer component SHALL be an expandable viewer for agent logs per PR, used on the PR detail page. Shows prompt, response, and reasoning sections with lazy loading per agent.

**Implementation file:** `crates/crb-webui-frontend/src/components/log_viewer.rs`

**Props:** `logs: LogsListResponse`, `run_id: String`

**Behavior:**
- If `cache_available` is false, shows "No cache available" message
- If `prs` is empty, shows "No PR logs found" message
- Each PR is a `<details>` element; expanding it shows agent entries
- Each agent is a nested `<details>` element
- Agent log content is loaded lazily on first click via `GET /api/runs/:id/logs/:pr_key/:role`
- Loading state shows "Click to load logs", fetching shows "Loading...", loaded shows prompt/response/reasoning

#### Scenario: No cache shows explanatory message

Given `cache_available` is false
When the LogViewer renders
Then it shows "No cache available. Logs are only available when the run was executed with caching enabled."

#### Scenario: Loads agent log on first expand

Given a PR has agents with data available
When the user clicks to expand an agent section for the first time
Then a request is made to `GET /api/runs/:id/logs/:pr_key/:role`
And the response populates prompt, response, and reasoning sections
And subsequent clicks do not re-fetch

#### Scenario: Agent with no response shows "not available"

Given the log response has `available: false`
When the agent log section renders
Then it shows "Agent log data not available."

