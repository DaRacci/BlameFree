# Pages Layout Specification

## Overview

This document specifies the wireframe layout, content structure, and interactive behavior for each page in the `crb-webui` redesign. Every page follows the common shell: sidebar (left) + main content area (right, scrollable).

---

## Common Shell

```
┌──────────────────────────────────────────────────────────────┐
│  ┌──────────┐  ┌──────────────────────────────────────────┐ │
│  │ Sidebar  │  │  Main Content                            │ │
│  │          │  │  ──────────────────────                  │ │
│  │ 240px    │  │  max-width: 1400px                       │ │
│  │ (coll-   │  │  margin: 0 auto                          │ │
│  │  apsible) │  │  padding: 32px 32px                     │ │
│  │          │  │                                          │ │
│  │          │  │  Page-specific content here              │ │
│  │          │  │                                          │ │
│  └──────────┘  └──────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────┘
```

### DOM Structure (App Root)

```html
<div class="app-shell">
  <Sidebar />  <!-- See spec: components > sidebar -->
  <main class="main-content">
    <div class="content-container">
      <!-- Router outlet for page content -->
    </div>
  </main>
</div>
```

---

## Page 1: Home (`/`)

### Layout Wireframe

```
┌──────────────────────────────────────────────────────────────┐
│  ┌──────────┐  ┌──────────────────────────────────────────┐ │
│  │          │  │  Dashboard                    🔍  [🆕]  │ │
│  │  SIDEBAR │  │  ─────────────────────────────────────     │ │
│  │          │  │                                             │ │
│  │          │  │  ┌──────────┐ ┌──────────┐ ┌──────────┐   │ │
│  │          │  │  │Total Runs│ │  Avg F1  │ │Total Cost│   │ │
│  │          │  │  │   12     │ │  0.48    │ │  $0.28   │   │ │
│  │          │  │  └──────────┘ └──────────┘ └──────────┘   │ │
│  │          │  │                                             │ │
│  │          │  │  Past Runs (sorted by date ▼)               │ │
│  │          │  │                                             │ │
│  │          │  │  ┌──────────────────┐ ┌──────────────────┐ │ │
│  │          │  │  │ smoke-5          │ │ ca-test-1        │ │ │
│  │          │  │  │ ● Completed      │ │ ● Completed      │ │ │
│  │          │  │  │ 2 PRs | $0.015   │ │ 3 PRs | $0.042   │ │ │
│  │          │  │  │ F1: 0.50         │ │ F1: 0.72         │ │ │
│  │          │  │  │ ───╱╲───╱╲──   │ │ ───╱╲──╱╲╱╲─── │ │ │
│  │          │  │  │ 2m 0s            │ │ 4m 30s           │ │ │
│  │          │  │  └──────────────────┘ └──────────────────┘ │ │
│  │          │  │  ┌──────────────────┐ ┌──────────────────┐ │ │
│  │          │  │  │ full-suite-3     │ │ smoke-2          │ │ │
│  │          │  │  │ ● Completed      │ │ ◌ Running        │ │ │
│  │          │  │  │ 12 PRs | $0.18   │ │ 5/12 PRs | $0.07│ │ │
│  │          │  │  │ F1: 0.61         │ │ F1: —            │ │ │
│  │          │  │  │ ──╱╲╱╲──╱╲───  │ │ [████░░░░░]      │ │ │
│  │          │  │  │ 8m 12s           │ │ 3m 42s           │ │ │
│  │          │  │  └──────────────────┘ └──────────────────┘ │ │
│  │          │  └──────────────────────────────────────────┘ │
│  └──────────┘                                               │
└──────────────────────────────────────────────────────────────┘
```

### Layout Structure

```
Main Content
├── Page Header (flex row, space-between)
│   ├── Title: "Dashboard" (h1, --text-2xl)
│   └── Header Actions
│       ├── SearchInput (placeholder: "Search runs...")
│       └── Button: "🆕 New Benchmark" (btn--primary, links to /new)
│
├── Summary Metrics Row (.content-grid with auto-fit, minmax(200px, 1fr))
│   ├── MetricCard: "Total Runs" → value: number
│   ├── MetricCard: "Avg F1" → value: float (2dp)
│   ├── MetricCard: "Total Cost" → value: currency
│   └── MetricCard: "Avg Duration" → value: time string
│
└── Run Card Grid (.card-grid with auto-fill, minmax(340px, 1fr))
    ├── RunCard × N (sorted by date descending, filterable by search)
    │   ├── Card Header: name (h3), status badge
    │   ├── Card Body:
    │   │   ├── Meta row: PR count, total cost
    │   │   ├── F1 value (large mono text)
    │   │   ├── Sparkline (mini SVG F1 trend)
    │   │   └── (if running) Progress bar
    │   └── Card Footer: duration, timestamp
    │
    ├── Empty State (when no runs exist)
    └── Error State (when fetch fails)
```

### Interactive Behavior

- **Search input:** Filters run cards client-side by name (debounced, 300ms)
- **Sort dropdown:** Changes sort order of cards (date desc/asc, F1 desc/asc, name)
- **RunCard click:** Navigates to `/runs/:id` (full card is a link)
- **New Benchmark button:** Navigates to `/new`
- **Sparkline:** 15×30px inline SVG showing F1 trend across last N data points
- **Status badge colors:** Completed=green, Running=orange, Failed=red

### States

| State | Behavior |
|-------|----------|
| Loading | 4 skeleton metric cards + 4 skeleton run cards in grid |
| Empty | "📂 No benchmark runs yet" empty state with CTA to /new |
| Error | "⚠️ Failed to load runs" error state with retry button |
| Search (no results) | "🔍 No runs matching 'query'" with clear search button |

---

## Page 2: Run Detail (`/runs/:id`)

### Layout Wireframe

```
┌──────────────────────────────────────────────────────────────┐
│  ┌──────────┐  ┌──────────────────────────────────────────┐ │
│  │          │  │  ← Dashboard        smoke-5         🕒  │ │
│  │  SIDEBAR │  │  ─────────────────────────────────────     │ │
│  │          │  │                                             │ │
│  │          │  │  ┌──────────┐ ┌──────────┐ ┌──────────┐   │ │
│  │          │  │  │   F1     │ │Precision │ │  Recall  │   │ │
│  │          │  │  │  0.50    │ │  0.333   │ │  1.00    │   │ │
│  │          │  │  └──────────┘ └──────────┘ └──────────┘   │ │
│  │          │  │  ┌──────────┐ ┌──────────┐                │ │
│  │          │  │  │Avg Cost  │ │Duration  │                │ │
│  │          │  │  │  $0.015  │ │  2m 0s   │                │ │
│  │          │  │  └──────────┘ └──────────┘                │ │
│  │          │  │                                             │ │
│  │          │  │  Per-PR Results (sorted by F1 ▼)            │ │
│  │          │  │  ┌────────────────────────────────────────┐ │ │
│  │          │  │  │ ↕ PR Title │ ↕ F1 │ ↕ Prec│ ↕ Rec│ C │ │ │
│  │          │  │  ├────────────────────────────────────────┤ │ │
│  │          │  │  │ fix-btn   │ 0.50 │ 0.333 │ 1.00 │ ✅│ │ │
│  │          │  │  │ scale-col │ 0.45 │ 0.290 │ 1.00 │ ✅│ │ │
│  │          │  │  └────────────────────────────────────────┘ │ │
│  │          │  │                                             │ │
│  │          │  │  Cost Breakdown                             │ │
│  │          │  │  ┌────────────────────────────────────────┐ │ │
│  │          │  │  │ Model    │ Tokens  │ Cost    │ Calls   │ │ │
│  │          │  │  ├────────────────────────────────────────┤ │ │
│  │          │  │  │ gpt-4o   │ 12,500  │ $0.012  │   8     │ │ │
│  │          │  │  │ gpt-4o-m │  3,200  │ $0.003  │   4     │ │ │
│  │          │  │  └────────────────────────────────────────┘ │ │
│  │          │  └──────────────────────────────────────────┘ │
│  └──────────┘                                               │
└──────────────────────────────────────────────────────────────┘
```

### Layout Structure

```
Main Content
├── Page Header (flex row, space-between)
│   ├── Back link: "← Dashboard" (links to /)
│   ├── Title: run name (h1, --text-2xl)
│   └── Timestamp (--text-sm, --text-secondary)
│
├── Summary Metrics Row (.content-grid with auto-fit, minmax(200px, 1fr))
│   ├── MetricCard: F1 Score (color: --accent-blue)
│   ├── MetricCard: Precision (color: --accent-green)
│   ├── MetricCard: Recall (color: --accent-orange)
│   ├── MetricCard: Avg Cost (color: --text-primary)
│   └── MetricCard: Duration (color: --text-primary)
│
├── Results Section
│   ├── Section header: "Per-PR Results" with sort dropdown + filter dropdown
│   └── Sortable Table
│       ├── Columns: PR (linked), F1, Precision, Recall, Findings, Cost, Status
│       ├── Sortable by: F1, Precision, Recall, Cost (click header to toggle asc/desc)
│       ├── Filterable by: All, High F1 (>0.7), Low F1 (<0.3), Failed
│       └── Clickable rows → link to /runs/:id/pr/:pr_key (future)
│
└── Cost Breakdown Section
    ├── Section header: "Cost Breakdown"
    └── Table
        ├── Columns: Model, Tokens, Cost, Calls
        └── Footer row: totals
```

### Interactive Behavior

- **Sort:** Click column header → cycle: none → ascending → descending. Active sort column shows arrow and accent color.
- **Filter:** Dropdown filters table rows client-side.
- **Back link:** Smooth scroll to top, then navigate to `/`.
- **Row click:** Navigate to per-PR detail view (if implemented in future).

### States

| State | Behavior |
|-------|----------|
| Loading | 5 skeleton metric cards + skeleton table (header + 4 rows) |
| Error | "⚠️ Failed to load run details" error state with retry |
| No PRs (empty run) | "📭 No PRs in this run" empty state |

---

## Page 3: New Benchmark (`/new`)

### Layout Wireframe

```
┌──────────────────────────────────────────────────────────────┐
│  ┌──────────┐  ┌──────────────────────────────────────────┐ │
│  │          │  │  New Benchmark Run            [Cancel]   │ │
│  │  SIDEBAR │  │  ─────────────────────────────────────     │ │
│  │          │  │                                             │ │
│  │          │  │  ═══════════════════════════════════════     │ │
│  │          │  │  CONFIGURATION                              │ │
│  │          │  │                                             │ │
│  │          │  │  Model:       [gpt-4o              ▼]      │ │
│  │          │  │  Judge Model: [gpt-4o-mini          ▼]     │ │
│  │          │  │  Dataset:     [golden_comments       ▼]     │ │
│  │          │  │                                             │ │
│  │          │  │  ═══════════════════════════════════════     │ │
│  │          │  │  EXECUTION                                  │ │
│  │          │  │                                             │ │
│  │          │  │  Concurrency:   [══════●════════════]   4   │ │
│  │          │  │  Max Findings:  [════●══════════════]  20   │ │
│  │          │  │  Max Turns:     [══════●════════════]   3   │ │
│  │          │  │                                             │ │
│  │          │  │  ═══════════════════════════════════════     │ │
│  │          │  │  ADVANCED                                   │ │
│  │          │  │                                             │ │
│  │          │  │  Prompts Dir:  [prompts/builtin       ]     │ │
│  │          │  │  Cache Dir:    [                     ]     │ │
│  │          │  │                                             │ │
│  │          │  │  Roles to Run:                              │ │
│  │          │  │  ☑ SA (Security)                            │ │
│  │          │  │  ☑ CL (Code Logic)                          │ │
│  │          │  │  ☑ AR (Architecture)                        │ │
│  │          │  │  ☑ SEC (Security - extra)                   │ │
│  │          │  │                                             │ │
│  │          │  │  Options:                                   │ │
│  │          │  │  □ Skip Consensus   □ Skip Linters         │ │
│  │          │  │  □ Dry Run                                 │ │
│  │          │  │                                             │ │
│  │          │  │  ═══════════════════════════════════════     │ │
│  │          │  │                                             │ │
│  │          │  │  [🚀 Start Benchmark]                       │ │
│  │          │  │                                             │ │
│  │          │  └──────────────────────────────────────────┘ │
│  └──────────┘                                               │
└──────────────────────────────────────────────────────────────┘
```

### Layout Structure

```
Main Content
├── Page Header (flex row, space-between)
│   ├── Title: "New Benchmark Run" (h1, --text-2xl)
│   └── Cancel button (btn--ghost, links to /)
│
└── Form (.new-run-form, max-width: 720px)
    ├── Form Section: "Configuration"
    │   ├── Select: Model (dropdown, required)
    │   ├── Select: Judge Model (dropdown, required)
    │   └── Select: Dataset (dropdown, required)
    │
    ├── Form Section: "Execution"
    │   ├── Slider: Concurrency (1–8, default 4)
    │   ├── Slider: Max Findings (1–50, default 20)
    │   └── Slider: Max Turns (1–10, default 3)
    │
    ├── Form Section: "Advanced"
    │   ├── Input: Prompts Dir (text, optional, placeholder)
    │   ├── Input: Cache Dir (text, optional, placeholder)
    │   ├── Checkbox Group: Roles to Run
    │   │   ├── ☑ SA (Security Analyst)
    │   │   ├── ☑ CL (Code Logic)
    │   │   ├── ☑ AR (Architecture)
    │   │   └── ☑ SEC (Security - extra)
    │   └── Checkbox Group: Options
    │       ├── □ Skip Consensus
    │       ├── □ Skip Linters
    │       └── □ Dry Run
    │
    └── Form Actions
        └── Button: "🚀 Start Benchmark" (btn--primary btn--lg, full-width)
```

### Interactive Behavior

- **Slider:** Moving the slider thumb updates the numeric readout in real-time
- **Checkbox toggles:** Click label or checkbox to toggle; at least one role must be checked
- **Form validation:** On submit:
  - Required fields highlighted with error state if empty
  - At least one role selected, or validation error shown
  - On success: POST to `/api/runs`, navigate to `/live/{run_id}`
  - On error: inline error message above submit button
- **Cancel:** Navigate to `/` without confirmation (form state not persisted)

### States

| State | Behavior |
|-------|----------|
| Initial | Empty form with defaults pre-filled (sliders at defaults, all roles checked) |
| Validation error | Red border on invalid fields, error message per field, error banner at top |
| Submitting | Submit button shows spinner, all fields disabled |
| Submission error | Error banner above submit button with message, fields re-enabled |
| Success | Redirect to live view |

---

## Page 4: Live View (`/live/:id`)

### Layout Wireframe

```
┌──────────────────────────────────────────────────────────────┐
│  ┌──────────┐  ┌──────────────────────────────────────────┐ │
│  │          │  │  🔴 Live: smoke-test-1       [⬅ Back]   │ │
│  │  SIDEBAR │  │  ─────────────────────────────────────     │ │
│  │          │  │                                             │ │
│  │          │  │  ┌──────────┐ ┌──────────┐ ┌──────────┐   │ │
│  │          │  │  │Progress  │ │ Elapsed  │ │   Cost   │   │ │
│  │          │  │  │  5/12    │ │  3m 42s  │ │  $0.07   │   │ │
│  │          │  │  └──────────┘ └──────────┘ └──────────┘   │ │
│  │          │  │  ┌──────────┐                              │ │
│  │          │  │  │ Current  │                              │ │
│  │          │  │  │   PR #7  │                              │ │
│  │          │  │  └──────────┘                              │ │
│  │          │  │                                             │ │
│  │          │  │  ┌──── SA ─────────┐ ┌──── CL ─────────┐   │ │
│  │          │  │  │ 🟢 reviewing    │ │ 🟡 3 finding(s) │   │ │
│  │          │  │  │ ─────────────── │ │ ─────────────── │   │ │
│  │          │  │  │ Analyzing PR #7│ │ Issue found in  │   │ │
│  │          │  │  │ Color function │ │ button component│   │ │
│  │          │  │  │ uses wrong...  │ │ Line 42-45...   │   │ │
│  │          │  │  └────────────────┘ └─────────────────┘   │ │
│  │          │  │  ┌──── AR ─────────┐ ┌─── SEC ─────────┐   │ │
│  │          │  │  │ ⏳ pending       │ │ ⏳ pending       │   │ │
│  │          │  │  │ ─────────────── │ │ ─────────────── │   │ │
│  │          │  │  │                 │ │                 │   │ │
│  │          │  │  └────────────────┘ └─────────────────┘   │ │
│  │          │  │                                             │ │
│  │          │  │  [██████████████░░░░░░░░]  5/12 PRs        │ │
│  │          │  │  Current: discourse-graphite/pull/7        │ │
│  │          │  │  → F1=0.33                                 │ │
│  │          │  └──────────────────────────────────────────┘ │
│  └──────────┘                                               │
└──────────────────────────────────────────────────────────────┘
```

### Layout Structure

```
Main Content
├── Page Header (flex row, space-between)
│   ├── Title: "🔴 Live: {run_name}" (h1, --text-2xl)
│   ├── Status dot (animated pulse when running)
│   └── Back button: "⬅ Back" (links to /runs/:id)
│
├── Live Metrics Row (.content-grid with auto-fit, minmax(160px, 1fr))
│   ├── MetricCard: "Progress" → "5/12 PRs" (value: --text-base)
│   ├── MetricCard: "Elapsed" → "3m 42s"
│   ├── MetricCard: "Cost" → "$0.07"
│   └── MetricCard: "Current PR" → "#7" (or "—" if idle)
│
├── Agent Panes Grid (2×2 grid, --spacing-lg gap)
│   ├── AgentPane: SA (Security Analyst) — top-left
│   │   ├── Border color by status
│   │   ├── Status header with dot
│   │   └── Streaming response content (auto-scroll, --font-mono)
│   ├── AgentPane: CL (Code Logic) — top-right
│   ├── AgentPane: AR (Architecture) — bottom-left
│   └── AgentPane: SEC (Security - extra) — bottom-right
│
└── Bottom Bar
    ├── Progress Bar (overall run progress)
    └── Current PR info line: "PR: discourse-graphite/pull/7 → F1=0.33"
```

### Interactive Behavior

- **Auto-scroll:** Agent pane content auto-scrolls to bottom as new chunks arrive
- **Pane border color** changes in real-time based on agent status:
  - Pending: `--border-default` (gray)
  - Running: `--accent-blue` (blue, animated pulse)
  - Completed: `--accent-green` (green)
  - Failed: `--accent-red` (red)
- **Progress bar** fills proportionally as PRs complete
- **No user input** on this page — it's a read-only monitoring view

### States

| State | Behavior |
|-------|----------|
| Loading (connecting) | 4 skeleton agent panes, skeleton metrics, skeleton progress bar |
| Connected, running | Live updates via SSE stream, metrics and panes update in real-time |
| Disconnected (SSE lost) | Error banner at top: "Connection lost. [Reconnect]" button |
| Run complete | Header updates to "✅ smoke-test-1 (completed)", final metrics, panes show final status |
| Run failed | Header updates to "❌ smoke-test-1 (failed)", error details in a banner |

### SSE Connection Handling

```javascript
// Pseudocode for SSE connection lifecycle
function connectLiveView(runId) {
  const es = new EventSource(`/api/runs/${runId}/live`);

  es.addEventListener('agent_chunk', (e) => {
    const data = JSON.parse(e.data);
    updateAgentPane(data.role, data.chunk);
  });

  es.addEventListener('run_progress', (e) => {
    const data = JSON.parse(e.data);
    updateProgress(data.completed_prs, data.total_prs);
    updateElapsed(data.elapsed_secs);
    updateCost(data.total_cost);
  });

  es.addEventListener('run_finished', (e) => {
    disconnect();  // Clean up
    showCompletionState();
  });

  es.onerror = () => {
    // Show "reconnect" button after 3 consecutive errors
    showReconnectPrompt();
  };
}
```

---

## Responsive Behavior

### Tablet (768–1199px)

- Sidebar collapses to icon-only (64px)
- Content grid: max 2 columns
- Agent panes: still 2×2 but smaller
- Form: full width (no max-width constraint)
- Table: horizontal scroll if content overflows (`overflow-x: auto`)

### Mobile (<768px)

- Sidebar hidden entirely; hamburger toggle opens overlay slide-in sidebar
- All grids collapse to single column
- Agent panes: single column (4 rows instead of 2×2)
- Metric cards: full width
- Table: horizontal scroll wrapper
- Form: full width, no side-by-side fields
- Page header padding reduces to 16px
- Run card grid: single column full-width cards
