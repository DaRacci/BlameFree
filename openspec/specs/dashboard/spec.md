# dashboard Specification

## Purpose
TUI dashboard rendering for live monitoring of agent progress during benchmark runs with multi-pane layout.
## Requirements
### Requirement: Dashboard Layout
The system SHALL render a live TUI dashboard with a title bar, four agent panes, a progress bar, and a footer.

#### Scenario: Standard layout
- GIVEN the dashboard is active with --dashboard flag
- WHEN the dashboard renders
- THEN it shows a title bar, 4 equal-width agent panes (SA, CL, AR, SEC), a progress bar, and a footer
- AND it requires a minimum terminal size of 80x24
- AND it warns on small terminals without crashing

### Requirement: Agent Pane Rendering
The system SHALL render each agent's real-time status in a bordered pane.

#### Scenario: Running agent display
- GIVEN an agent is actively processing a PR
- WHEN the pane renders
- THEN it shows the PR title, scrolling text buffer (2000 char cap), running duration, and cost
- AND the border is green

#### Scenario: Agent state transitions
- GIVEN an agent transitions between states
- WHEN the pane updates
- THEN idle agents show dim borders, running agents show green, finished show cyan, failed show red

### Requirement: Progress Bar
The system SHALL render a progress bar showing completed vs total PRs.

#### Scenario: Partial progress
- GIVEN 3 of 15 PRs completed
- WHEN the progress bar renders
- THEN it shows a cyan-filled gauge at 3/15
- AND the label shows 3/15

#### Scenario: All complete
- GIVEN all PRs completed
- WHEN the progress bar renders
- THEN the gauge turns fully green

### Requirement: Cost Display
The system SHALL display running cost and API metrics in real-time.

#### Scenario: Cost summary
- GIVEN agents have made API calls
- WHEN the footer renders
- THEN it shows total cost in USD, API call count, and cache hit/miss stats

### Requirement: Terminal Lifecycle Management
The system SHALL properly initialize and restore terminal state.

#### Scenario: Startup
- GIVEN --dashboard flag and a TTY
- WHEN the dashboard starts
- THEN it enables raw mode, enters alternate screen, and creates a 1024-capacity mpsc channel

#### Scenario: Shutdown
- GIVEN the user presses q or evaluation completes
- WHEN the dashboard shuts down
- THEN it leaves alternate screen and disables raw mode

#### Scenario: Panic safety
- GIVEN the dashboard task panics
- WHEN terminal guard's Drop runs
- THEN it restores the terminal to a usable state

### Requirement: Input Handling
The system SHALL handle user keyboard input during dashboard operation.

#### Scenario: Quit
- GIVEN the user presses q
- WHEN the dashboard processes input
- THEN it stops rendering and restores terminal

#### Scenario: Pause
- GIVEN the user presses p
- WHEN the dashboard processes input
- THEN it toggles rendering pause

