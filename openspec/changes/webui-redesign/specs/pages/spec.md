# Delta for Pages

## ADDED Requirements

### Requirement: Common App Shell Layout

The application shell SHALL provide a flexbox-based page layout with a fixed sidebar and scrollable main content area. The sidebar SHALL support two visual states: expanded (240px width) and collapsed (64px width icon-only). The main content area SHALL transition its left margin as the sidebar state changes.

Implementation Status: FULLY IMPLEMENTED. All CSS classes (`.app-shell`, `.main-content`, `.sidebar--collapsed`, `.content-container`) exist in `layout.css`.

#### Scenario: Sidebar expanded by default
- GIVEN the application loads
- WHEN the sidebar is in its default (expanded) state
- THEN `.main-content` SHALL have `margin-left: 240px`

#### Scenario: Sidebar collapsed
- GIVEN the application loads
- WHEN the sidebar transitions to the collapsed state via `.sidebar--collapsed`
- THEN `.main-content` SHALL have `margin-left: 64px`
- AND the transition SHALL animate using `var(--transition-normal)`

#### Scenario: Content container constraints
- GIVEN the main content area is rendered
- WHEN the `.content-container` element is present
- THEN it SHALL have `max-width: 1400px`, `margin: 0 auto`, and `padding: var(--spacing-2xl)`

#### Scenario: App shell structure
- GIVEN the page is rendered
- WHEN the DOM is inspected
- THEN `.app-shell` SHALL use `display: flex` with `min-height: 100vh`
- AND `.main-content` SHALL use `flex: 1` to fill remaining space

---

### Requirement: Home Page — Layout Structure

The home page SHALL display a dashboard layout at route `/` with a page header containing title and action buttons, a summary metrics row, a quick-actions section, a run card grid with interactive cards, an active-runs indicator with a pulse animation, filters, and loading / empty / error states.

Implementation Status: FULLY IMPLEMENTED. All CSS classes are present in `home.css` and `layout.css`: `.page-header`, `.content-grid--metrics` (auto-fit/minmax(200px,1fr)), `.content-grid--cards` (auto-fill/minmax(340px,1fr)), `.quick-actions`, `.home-page .card` (interactive), `.active-runs-indicator` (pulse animation), `.home-page__filters`, `.home-page .empty-state`, `.error-state`, `.state-container`.

#### Scenario: Page header display
- GIVEN the user navigates to `/`
- WHEN the home page renders
- THEN a `.page-header` SHALL display the title "Overview" (h1, `--text-2xl`)
- AND `.page-header__actions` SHALL contain action buttons (e.g., "New Benchmark", "Ad-hoc Review")
- AND the header SHALL use `display: flex` with `justify-content: space-between`

#### Scenario: Summary metrics row
- GIVEN the home page is loaded
- WHEN run data is available
- THEN a `.content-grid--metrics` grid SHALL display metric cards using `auto-fit / minmax(200px, 1fr)`
- AND each `.metric-card` SHALL show a label, a mono-font value (`--text-2xl`, `--font-mono`, `--weight-semibold`), and an optional delta indicator
- AND the grid SHALL display at least: Total Runs, Avg F1, PRs Reviewed

#### Scenario: Run card grid
- GIVEN run data is present
- WHEN the home page renders the run list
- THEN a `.content-grid--cards` grid SHALL display run cards using `auto-fill / minmax(340px, 1fr)`
- AND each card SHALL have `cursor: pointer` and hover state via `.home-page .card:hover` (`--bg-surface-hover`)
- AND each card SHALL show: name, status badge, meta row (PR count, cost in `.card__meta-row`), F1 value (mono `--text-2xl`), progress bar for running runs (`card__progress`), and duration
- AND `.card--active-run` SHALL highlight running runs with green border (1.5px) and green box-shadow on hover

#### Scenario: Active runs indicator
- GIVEN there are active (running) runs
- WHEN the home page displays the "Active Runs" section
- THEN the `.active-runs-indicator` SHALL show a pulsing green dot (10px, `--accent-green`) using the `active-pulse` animation (2s ease-in-out infinite)
- AND the `.active-runs-count` SHALL display the number of active runs

#### Scenario: Quick actions section
- GIVEN the home page is rendered
- WHEN the main content is loaded
- THEN the `.quick-actions` section SHALL display action buttons in a flex row with `gap: var(--spacing-lg)`
- AND each `.quick-actions__btn` SHALL use `flex: 1` with padding, semibold font, and centered content

#### Scenario: Filters area
- GIVEN the home page is rendered
- WHEN the run card section is visible
- THEN the `.home-page__filters` area SHALL display filter controls in a flex row with `gap: var(--spacing-md)` and wrapping

#### Scenario: Recent runs list
- GIVEN recent runs are available
- WHEN the home page renders
- THEN the `.home-page__recent-list` SHALL display recent run entries
- AND each `.home-page__recent-row` SHALL have a transparent left border that transitions to `--accent-blue` on hover

#### Scenario: Empty state
- GIVEN no run data exists
- WHEN the home page loads
- THEN the `.empty-state` container SHALL display an icon (48px), heading (`--text-lg`), message (secondary, max-width 400px), and an action button
- AND the empty state SHALL be visually bounded by `--bg-surface` background and `--border-default` border

#### Scenario: Error state
- GIVEN the run fetch request fails
- WHEN the error state is shown
- THEN the `.error-state` container SHALL display an error icon (48px), heading (`--text-lg`), descriptive message (secondary, max-width 400px), and a retry action button

#### Scenario: Loading state
- GIVEN the run data is being fetched
- WHEN the page is in a loading state
- THEN skeleton metric cards (4) and skeleton run cards (4) SHALL be displayed using `.skeleton-*` component classes

#### Scenario: Active actions row
- GIVEN there are active runs
- WHEN the page renders active run information
- THEN `.home-page__active-actions` SHALL display a link or action in `--accent-blue` below the active runs indicator

---

### Requirement: Home Page — Search Bar

The home page SHALL provide a search input to filter run cards client-side by name, with debounced input handling.

Implementation Status: NOT IMPLEMENTED. The `.search-bar`, `.search-bar__input-wrapper`, `.search-bar__icon`, and `.search-bar__input` CSS classes exist in `layout.css`, but no search input widget is rendered in the Rust `HomePage` component.

#### Scenario: Search input
- GIVEN the home page is loaded
- WHEN the user types in the search input
- THEN run cards SHALL be filtered client-side by name
- AND filtering SHALL be debounced at 300ms
- AND the search bar SHALL render with a search icon positioned absolutely on the left of the input (`padding-left: 32px`)

#### Scenario: No search results
- GIVEN the user has entered a search query
- WHEN no run cards match the query
- THEN a "no results" state SHALL be displayed showing the search term
- AND a clear search button SHALL be provided to reset the filter

---

### Requirement: Home Page — Sparkline SVG

Each run card SHALL display a mini bar-chart sparkline showing the F1 score trend across recent data points.

Implementation Status: NOT IMPLEMENTED. The `.sparkline` CSS (flex row, 30px height, 6px wide bars with blue fill, rounded top corners, height transition) exists in `layout.css`, but no sparkline SVG component is rendered in the Rust run cards.

#### Scenario: F1 trend visualization
- GIVEN a completed run has F1 score history
- WHEN the run card renders
- THEN a sparkline SHALL display the F1 trend with bar heights proportional to each data point
- AND bars SHALL use `--accent-blue` background with `transition: height var(--transition-fast)`

---

### Requirement: Home Page — Sort Dropdown

The run cards grid SHALL support changing the sort order via a dropdown control. Available sort modes: date descending/ascending, F1 descending/ascending, name.

Implementation Status: NOT IMPLEMENTED. Runs are sorted by date only (hardcoded). No sort dropdown exists in the Rust component.

#### Scenario: Sort by different criteria
- GIVEN the home page displays run cards
- WHEN the user selects a sort option from the dropdown
- THEN the run cards SHALL be reordered according to the selected criteria (date, F1, or name)
- AND each criteria SHALL support ascending and descending directions

---

### Requirement: Run Detail Page — Navigation and Header

The run detail page SHALL display a navigable header with a styled back link, run name (h1), and timestamp at route `/runs/:id`.

Implementation Status: FULLY IMPLEMENTED. The `.back-link` CSS is present in `run-detail.css` with hover effects. Page header includes name, status badge, model info.

#### Scenario: Back link navigation
- GIVEN the user navigates to `/runs/:id`
- WHEN the page renders
- THEN a `.run-detail-page .back-link` SHALL display as "← Dashboard" linking to `/`
- AND the back link SHALL use `display: inline-flex` with `gap: var(--spacing-xs)`, font-size `--text-sm`, color `--text-secondary`, and `margin-bottom: var(--spacing-lg)`
- AND on hover the back link SHALL change color to `--accent-blue`

#### Scenario: Run name and timestamp
- GIVEN a run is loaded
- WHEN the detail page renders
- THEN the page header SHALL display the run name (h1, `--text-2xl`)
- AND a status badge SHALL indicate the run's current status
- AND model info SHALL be displayed alongside the status

---

### Requirement: Run Detail Page — Summary Metrics Row

The run detail page SHALL display a row of metric cards for F1 Score, Precision, Recall, Avg Cost, and Duration, with color-coded values.

Implementation Status: FULLY IMPLEMENTED in `layout.css` via `.content-grid--metrics` and `.metric-card` components. Five metric cards are rendered.

#### Scenario: Metrics display
- GIVEN a run is loaded
- WHEN the detail page renders
- THEN a `.content-grid--metrics` grid SHALL display 5 metric cards
- AND each card SHALL have a label, mono-font value, and appropriate color: F1 (`--accent-blue`), Precision (`--accent-green`), Recall (`--accent-orange`), Avg Cost (`--text-primary`), Duration (`--text-primary`)

---

### Requirement: Run Detail Page — Progress Bar

The run detail page SHALL display a progress bar when the run is still in progress (running status).

Implementation Status: FULLY IMPLEMENTED using the `.progress` component CSS.

#### Scenario: Progress display for running runs
- GIVEN a run has a "running" status
- WHEN the detail page renders
- THEN a progress bar SHALL be displayed showing completion percentage
- AND the progress bar SHALL update as the run progresses

---

### Requirement: Run Detail Page — Per-PR Results Section

The run detail page SHALL display a Per-PR results section with a sortable table showing columns for PR (linked), F1, Precision, Recall, Findings, Cost, and Status. Styled table includes sortable headers, row hover, clickable rows, and an empty state.

Implementation Status: PARTIALLY IMPLEMENTED. The table renders all PR columns in a `.table-wrapper` with `.section-header` spacing (`margin-top: var(--spacing-2xl)`). Table CSS exists in `table.css` (`.table__th--sortable`, `.table__th--asc`, `.table__th--desc`, `.table__row`, `.table__row--clickable`, `.table__empty`). However, table sorting click handlers are NOT implemented in Rust — only CSS for visual sorting state is present. The filter dropdown is also NOT implemented.

#### Scenario: Section header
- GIVEN the results section is rendered
- WHEN the page displays
- THEN the `.section-header` SHALL display "Per-PR Results" with `font-size: var(--text-xl)`, `font-weight: var(--weight-semibold)`
- AND the section header SHALL use `display: flex`, `justify-content: space-between`, `margin-top: var(--spacing-2xl)`, `margin-bottom: var(--spacing-md)`

#### Scenario: Table wrapper
- GIVEN the results table is rendered
- WHEN the table is present
- THEN it SHALL be wrapped in a `.table-wrapper` with horizontal overflow (`overflow-x: auto`), border, border-radius, and `--bg-surface` background
- AND `.table-wrapper` SHALL have `margin-top: var(--spacing-md)`

#### Scenario: Sortable table headers
- GIVEN the results table is rendered
- WHEN the user views the table
- THEN each column header SHALL use `.table__th` with uppercase styling, `--text-secondary` color, `--weight-semibold`, and sticky positioning
- AND headers SHALL have `.table__th--sortable` with `cursor: pointer` and hover color transition to `--text-primary`
- AND `.table__th--asc` / `.table__th--desc` SHALL style the active sort column with `--accent-blue` color
- AND a sort icon SHALL display next to the active column using `.table__sort-icon`

#### Scenario: Table row interaction
- GIVEN a table row is rendered
- WHEN the user hovers over a row
- THEN `.table__row` SHALL change background to `--bg-surface-hover`
- AND `.table__row--clickable` SHALL show `cursor: pointer`

#### Scenario: Table sorting behavior (MISSING — click handlers)
- GIVEN the results table is rendered
- WHEN the user clicks a sortable column header
- THEN the table SHALL cycle sort state: none → ascending → descending
- AND the active sort column SHALL display a sort arrow icon
- **Note:** Only CSS (`--asc`/`--desc` styling) exists. Click handler logic is NOT implemented in Rust.

#### Scenario: PR filter dropdown (MISSING)
- GIVEN the results section is displayed
- WHEN the user interacts with the filter
- THEN a dropdown SHALL filter table rows client-side by: All, High F1 (>0.7), Low F1 (<0.3), Failed
- **Note:** No filter dropdown is implemented in the Rust component.

#### Scenario: Empty table state
- GIVEN there are no PR results for a run
- WHEN the table section renders
- THEN the `.table__empty` SHALL display centered text: "📭 No PRs in this run"

---

### Requirement: Run Detail Page — Cost Breakdown Section

The run detail page SHALL display a per-model cost breakdown section with a table showing Model, Tokens, Cost, and Calls columns, including a totals footer row.

Implementation Status: NOT IMPLEMENTED. No cost breakdown table exists in the Rust component. Table component CSS is available via `table.css` if needed, and `.section-header` styling is available in `layout.css`.

#### Scenario: Cost breakdown table
- GIVEN a run has cost data
- WHEN the detail page renders
- THEN a section with header "Cost Breakdown" SHALL display a table
- AND the table SHALL have columns: Model, Tokens, Cost, Calls
- AND a footer row SHALL display totals for the numeric columns

---

### Requirement: New Benchmark Page — Form Layout

The new benchmark page SHALL display a form at route `/new` with Configuration (model, dataset dropdowns), Execution (role selector checkbox group), and Advanced (inputs, checkboxes) form sections. The form SHALL be constrained to a maximum width of 720px and include form actions with a submit button.

Implementation Status: FULLY IMPLEMENTED. CSS classes `.new-run-page` (max-width: 720px), `.form-section`, `.form-actions` (with top border separator and margin-top: var(--spacing-2xl)) are present in `new-run.css`. Form section includes Configuration (model, judge model, dataset selects), Execution (role checkbox-group with Select All/Deselect All), Advanced (concurrency number input, max findings number input, cache text input, reasoning select), and PR Selection (checkboxes). Checkbox-group CSS in `form.css` supports checkbox labels with hover, checked state, and disabled state.

#### Scenario: Page header
- GIVEN the user navigates to `/new`
- WHEN the page renders
- THEN a page header SHALL display "New Benchmark Run" title (h1, `--text-2xl`)
- AND a "Cancel" ghost button SHALL link to `/`

#### Scenario: Configuration form section
- GIVEN the new run form is displayed
- WHEN the Configuration section is rendered
- THEN a `.form-section` with uppercase title "CONFIGURATION" SHALL contain dropdown selects for Model, Judge Model, and Dataset
- AND each select SHALL use `.select` styling with custom dropdown arrow SVG and required indicator

#### Scenario: Execution form section
- GIVEN the new run form is displayed
- WHEN the Execution section is rendered
- THEN a `.form-section` with uppercase title "EXECUTION" SHALL contain:
- AND a checkbox-group for Role to Run (SA, CL, AR, SEC) with `.checkbox-label` styled checkboxes
- AND number inputs for concurrency, max findings, and max turns

#### Scenario: Advanced form section
- GIVEN the new run form is displayed
- WHEN the Advanced section is rendered
- THEN a `.form-section` with uppercase title "ADVANCED" SHALL contain:
- AND text inputs for Prompts Dir and Cache Dir (optional)
- AND a checkbox-group for Roles to Run (SA, CL, AR, SEC)
- AND a checkbox-group for Options (Skip Consensus, Skip Linters, Dry Run)
- AND checkbox labels SHALL use `.checkbox-label` with rounded border, hover transition, and custom checkbox appearance with checkmark SVG

#### Scenario: PR Selection
- GIVEN the new run form is displayed
- WHEN the PR Selection section is rendered
- THEN the section SHALL display checkboxes for PR selection
- AND a "Select All" / "Deselect All" toggle SHALL be provided

#### Scenario: Form actions
- GIVEN the new run form is filled
- WHEN the user views the bottom of the form
- THEN the `.form-actions` section SHALL display a "🚀 Start Benchmark" primary button (`btn--primary btn--lg`)
- AND the form actions SHALL have a top border separator from `--border-muted`

---

### Requirement: New Benchmark Page — Slider Range Inputs

The Execution section SHALL provide slider (`<input type="range">`) controls for concurrency, max findings, and max turns, with real-time numeric value readouts displayed in a mono-font badge next to the slider track.

Implementation Status: NOT IMPLEMENTED. The `.slider` and `.slider-field` CSS classes exist in `form.css` with full styling (custom track, thumb with `--accent-blue`, hover glow). The `.slider-field__control` (flex row with gap) and `.slider-field__value` (mono-font `--accent-blue` readout) are also present. However, the Rust component uses `<input type="number">` instead of `<input type="range">`.

#### Scenario: Slider interaction
- GIVEN the new run form is displayed
- WHEN the user adjusts a slider
- THEN the `.slider-field__value` numeric readout SHALL update in real-time
- AND the slider thumb SHALL be styled with `--accent-blue` background, 18px diameter, rounded shape, and hover glow effect (`box-shadow: 0 0 0 3px color-mix(...)`)
- AND the slider track SHALL be 6px tall with `--radius-full` rounding

#### Scenario: Concurrency slider
- GIVEN the Execution section is displayed
- WHEN the user sees the concurrency control
- THEN the slider SHALL allow values from 1 to 8
- AND the default value SHALL be 4

#### Scenario: Max Findings slider
- GIVEN the Execution section is displayed
- WHEN the user sees the max findings control
- THEN the slider SHALL allow values from 1 to 50
- AND the default value SHALL be 20

#### Scenario: Max Turns slider
- GIVEN the Execution section is displayed
- WHEN the user sees the max turns control
- THEN the slider SHALL allow values from 1 to 10
- AND the default value SHALL be 3

---

### Requirement: New Benchmark Page — Form Validation

The new run form SHALL validate all required fields, enforce at least one role selection, display field-level error states with red borders, show inline error messages, and handle submission lifecycle (spinner during submit, error banner on failure, redirect on success to `/live/{run_id}`).

Implementation Status: PARTIALLY IMPLEMENTED. The `.form-field--error` CSS (red border, `.form-field__error` display block, `--accent-red` border color on `.input`, `.select`, `.textarea`) exists in `form.css`. However, only role non-empty validation is implemented in Rust; there are no field-level validation error states for Model, Judge Model, or Dataset fields.

#### Scenario: Required field validation
- GIVEN the user attempts to submit the form
- WHEN a required field (Model, Judge Model, or Dataset) is empty
- THEN the field SHALL display `.form-field--error` styling with `border-color: var(--accent-red)`
- AND a `.form-field__error` message SHALL appear below the field with color `--accent-red`
- AND an error banner SHALL display at the top of the form

#### Scenario: Role selection validation
- GIVEN the user attempts to submit the form
- WHEN no roles are selected
- THEN a validation error SHALL be shown preventing submission
- AND at least one role MUST be checked before submission

#### Scenario: Submission loading state
- GIVEN the form is valid and submitted
- WHEN the POST request to `/api/runs` is in flight
- THEN the submit button SHALL display a spinner
- AND all form fields SHALL be disabled (`input:disabled` with `opacity: 0.4`, `cursor: not-allowed`)

#### Scenario: Submission error
- GIVEN the form is submitted
- WHEN the server returns an error
- THEN an inline error banner SHALL display above the submit button
- AND all form fields SHALL be re-enabled for the user to correct

#### Scenario: Submission success
- GIVEN the form is submitted and the POST succeeds
- WHEN the server returns a run ID
- THEN the user SHALL be redirected to `/live/{run_id}`

---

### Requirement: Live View Page — Header with Live Indicator

The live view page SHALL display a header at route `/live/:id` with the run name prefixed by "Live:", a pulsing red dot indicator, and a back button.

Implementation Status: FULLY IMPLEMENTED. CSS classes `.live-view-page .live-header` and `.live-view-page .live-header__dot` are present in `live-view.css`. The header displays "Live: {run_id}" with a red dot.

#### Scenario: Live header display
- GIVEN the user navigates to `/live/:id`
- WHEN the page renders
- THEN a `.live-header` SHALL display the run name with "Live:" prefix
- AND a `.live-header__dot` SHALL show a 10px red (`--accent-red`) circle with a 1.5s infinite pulse animation
- AND a "⬅ Back" button SHALL link to the run detail page `/runs/:id`

#### Scenario: Run completion state
- GIVEN a live run completes
- WHEN the run transitions to completed status
- THEN the header SHALL update to "✅ {run_name} (completed)"
- AND the red dot SHALL be removed

#### Scenario: Run failure state
- GIVEN a live run fails
- WHEN the run transitions to failed status
- THEN the header SHALL update to "❌ {run_name} (failed)"
- AND error details SHALL display in a banner

---

### Requirement: Live View Page — Metrics Row

The live view page SHALL display a row of metric cards showing progress (e.g., "5/12 PRs"), elapsed time, cost, and current PR number.

Implementation Status: FULLY IMPLEMENTED via `.content-grid--metrics` and `.metric-card` components in `layout.css`.

#### Scenario: Live metrics
- GIVEN a live run is connected via SSE
- WHEN the page renders the metrics row
- THEN metric cards SHALL display: Progress ("5/12 PRs"), Elapsed ("3m 42s"), Cost ("$0.07"), Current PR ("#7" or "—" if idle)
- AND metrics SHALL update in real-time as events arrive via SSE

---

### Requirement: Live View Page — Agent Panes Grid

The live view page SHALL display a 2×2 grid of agent panes, one per role (SA, CL, AR, SEC), with status-colored borders that change in real-time. Each pane SHALL show a header with role name and status dot, a monospace content area with streaming text, and an optional footer with findings count.

Implementation Status: FULLY IMPLEMENTED. CSS classes in `layout.css`: `.content-grid--agent-panes` (2-column grid), `.agent-pane` (flex column, 200–350px height, 2px border), `.agent-pane--pending` (`--border-default`), `.agent-pane--running` (`--accent-blue`), `.agent-pane--completed` (`--accent-green`), `.agent-pane--failed` (`--accent-red`), `.agent-pane__header`, `.agent-pane__role`, `.agent-pane__status`, `.agent-pane__content` (mono, overflow-y auto, pre-wrap), `.agent-pane__footer`, `.agent-pane__findings`.

#### Scenario: Agent pane grid layout
- GIVEN the live view page is loaded
- WHEN the page renders
- THEN a `.content-grid--agent-panes` grid SHALL display 4 agent panes in a 2×2 layout with `gap: var(--spacing-lg)`

#### Scenario: Agent pane header
- GIVEN an agent pane is rendered
- WHEN the pane header displays
- THEN `.agent-pane__header` SHALL show the role acronym (SA, CL, AR, SEC) with `--weight-semibold`
- AND `.agent-pane__status` SHALL show the current status text (e.g., "reviewing", "3 finding(s)", "pending") right-aligned via `margin-left: auto`

#### Scenario: Status-colored borders
- GIVEN an agent pane is rendered
- WHEN the agent status changes
- THEN `.agent-pane--pending` SHALL use `border-color: var(--border-default)`
- AND `.agent-pane--running` SHALL use `border-color: var(--accent-blue)` with `transition: border-color var(--transition-normal)`
- AND `.agent-pane--completed` SHALL use `border-color: var(--accent-green)`
- AND `.agent-pane--failed` SHALL use `border-color: var(--accent-red)`

#### Scenario: Agent pane content
- GIVEN an agent pane is receiving streaming content
- WHEN content chunks arrive via SSE
- THEN the `.agent-pane__content` area SHALL display monospace text (`--font-mono`, `--text-sm`) with `white-space: pre-wrap` and `word-break: break-word`
- AND the content area SHALL have `overflow-y: auto` for scrollability within the 350px max-height

#### Scenario: Agent pane footer with findings
- GIVEN an agent pane has completed analysis
- WHEN the pane footer is rendered
- THEN `.agent-pane__footer` SHALL display a top-border separator
- AND `.agent-pane__findings` SHALL show the findings count

---

### Requirement: Live View Page — Bottom Bar

The live view page SHALL display a bottom bar containing a progress bar (overall run completion) and current PR information showing the PR link and F1 score.

Implementation Status: FULLY IMPLEMENTED. CSS classes `.live-view-page .bottom-bar` and `.live-view-page .bottom-bar__info` are present in `live-view.css`.

#### Scenario: Bottom bar display
- GIVEN the live view page is rendered
- WHEN the run is in progress
- THEN a `.bottom-bar` SHALL display with `--bg-surface` background, border, border-radius, padding, and `margin-top: var(--spacing-xl)`
- AND a progress bar SHALL fill proportionally as PRs complete
- AND `.bottom-bar__info` SHALL show the current PR identifier (e.g., "PR: discourse-graphite/pull/7 -> F1=0.33") with `display: flex`, `justify-content: space-between`, `font-size: var(--text-sm)`, `color: var(--text-secondary)`

---

### Requirement: Live View Page — PR Selector

The live view page SHALL provide a tab-based PR selector for switching between PRs within the run. Each tab SHALL indicate completion status.

Implementation Status: FULLY IMPLEMENTED. CSS classes in `live-view.css`: `.pr-selector` (border, border-radius, padding, margin-top, `--bg-surface`), `.pr-selector__label` (semibold, secondary, `white-space: nowrap`), `.pr-selector__tabs` (flex row, 6px gap, wrapping), `.pr-tab` (border, padding 4px 12px, border-radius, cursor pointer, transition), `.pr-tab--active` (`--accent-primary` bg, white text), `.pr-tab--completed` (75% opacity).

#### Scenario: Tab switching
- GIVEN the live view page is rendered
- WHEN the PR selector is visible
- THEN `.pr-tab` buttons SHALL display each PR in the run
- AND `.pr-tab--active` SHALL highlight the currently selected PR with `--accent-primary` background and white text
- AND `.pr-tab--completed` SHALL indicate completed PRs with reduced opacity (0.75)
- AND `.pr-tab:hover` SHALL show `--bg-hover` background

---

### Requirement: Live View Page — SSE Connection Handling

The live view page SHALL connect to a Server-Sent Events (SSE) endpoint at `/api/runs/{runId}/live` and handle the connection lifecycle: connecting, connected with streaming, disconnected, and reconnect. Events SHALL drive real-time updates to agent pane content, metrics, and progress.

Implementation Status: FULLY IMPLEMENTED in Rust component via `EventSource`. The SSE lifecycle manages `agent_chunk`, `run_progress`, and `run_finished` events.

#### Scenario: SSE connection
- GIVEN the user navigates to `/live/:id`
- WHEN the page initializes
- THEN an `EventSource` SHALL connect to `/api/runs/{runId}/live`
- AND the page SHALL show loading/connecting state with 4 skeleton agent panes, skeleton metrics, and skeleton progress bar

#### Scenario: Agent chunk event
- GIVEN an SSE connection is established
- WHEN an `agent_chunk` event is received
- THEN the corresponding agent pane SHALL update with the new text chunk
- AND the status indicators SHALL update as needed

#### Scenario: Progress event
- GIVEN an SSE connection is established
- WHEN a `run_progress` event is received
- THEN the progress metrics SHALL update (completed PRs, total PRs, elapsed time, total cost)

#### Scenario: Run finished event
- GIVEN a run is in progress
- WHEN a `run_finished` event is received
- THEN the SSE connection SHALL be closed cleanly
- AND the page SHALL transition to the completion state with final metrics

#### Scenario: Disconnection
- GIVEN an SSE connection was established
- WHEN 3 consecutive errors occur on the EventSource
- THEN a "Connection lost. [Reconnect]" error banner SHALL be displayed
- AND the user SHALL be able to click "Reconnect" to re-establish the connection

---

### Requirement: Live View Page — Per-Agent Metrics (MISSING)

Each agent pane SHALL display per-agent token count and cost metrics, either in the pane footer or as part of the status header.

Implementation Status: NOT IMPLEMENTED. Agent panes currently show only role name, status text, and response content. No per-agent token count or cost data is rendered in the panes.

#### Scenario: Agent metrics display
- GIVEN an agent pane is rendered and the agent has completed or is processing work
- WHEN the pane footer or status area is visible
- THEN the token count and cost for that agent SHALL be displayed
- AND metrics SHALL update in real-time as SSE chunks arrive

---

### Requirement: Live View Page — Auto-Scroll (MISSING)

Agent pane content areas SHALL automatically scroll to the bottom as new SSE content chunks arrive, ensuring the user always sees the latest streaming content without manual scrolling.

Implementation Status: NOT IMPLEMENTED. The `.agent-pane__content` area has `overflow-y: auto` for scrollability, but no JavaScript auto-scroll-to-bottom behavior is implemented.

#### Scenario: Streaming auto-scroll
- GIVEN an agent pane is receiving streaming content
- WHEN new content chunks are appended to the pane content area
- THEN the pane SHALL automatically scroll to the bottom to reveal the latest content
- AND if the user has manually scrolled up to inspect earlier content, auto-scroll SHALL pause until the user scrolls back to the bottom

---

### Requirement: Responsive Behavior — Tablet

The application SHALL adapt to tablet viewport widths (768px–1199px) by collapsing the sidebar to icon-only (64px), limiting content grids to 2 columns, and enabling horizontal table scroll for overflow content.

Implementation Status: FULLY IMPLEMENTED. CSS media query `@media (min-width: 768px) and (max-width: 1199px)` in `layout.css` handles sidebar and grid adjustments.

#### Scenario: Tablet sidebar collapsed by default
- GIVEN the viewport width is between 768px and 1199px
- WHEN the page renders
- THEN `.main-content` SHALL have `margin-left: 64px` by default (sidebar is icon-only)
- AND when the sidebar is expanded (`.sidebar:not(.sidebar--collapsed)`), margin SHALL return to 240px

#### Scenario: Tablet agent panes
- GIVEN the viewport is between 768px and 1199px
- WHEN the live view page renders
- THEN `.content-grid--agent-panes` SHALL maintain a 2-column layout

---

### Requirement: Responsive Behavior — Mobile

The application SHALL adapt to mobile viewport widths (<768px) by hiding the sidebar (margin-left: 0), collapsing all grids to single column, reducing content padding, stacking page header vertically, and making the search bar full-width.

Implementation Status: FULLY IMPLEMENTED. CSS media query `@media (max-width: 767px)` in `layout.css` handles all mobile adaptations.

#### Scenario: Mobile sidebar hidden
- GIVEN the viewport width is less than 768px
- WHEN the page renders
- THEN `.main-content` SHALL have `margin-left: 0` (sidebar is hidden)

#### Scenario: Mobile reduced padding
- GIVEN the viewport width is less than 768px
- WHEN the page renders
- THEN `.content-container` SHALL use `padding: var(--spacing-lg)` (reduced from `--spacing-2xl`)

#### Scenario: Mobile single-column grids
- GIVEN the viewport width is less than 768px
- WHEN any grid renders
- THEN `.content-grid--metrics` SHALL collapse to `grid-template-columns: 1fr`
- AND `.content-grid--cards` SHALL collapse to `grid-template-columns: 1fr`
- AND `.content-grid--agent-panes` SHALL collapse to `grid-template-columns: 1fr` (4 rows stacked vertically instead of 2×2)

#### Scenario: Mobile page header
- GIVEN the viewport width is less than 768px
- WHEN the page header renders
- THEN `.page-header` SHALL use `flex-direction: column` with `align-items: flex-start`
- AND `.search-bar` SHALL take full width (`width: 100%`)
- AND `.search-bar__input-wrapper` SHALL use `flex: 1`
