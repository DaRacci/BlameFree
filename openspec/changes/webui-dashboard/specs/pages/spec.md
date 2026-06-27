# Pages & Components Specification

## Page Layouts

### Home (`/`)

```
┌─────────────────────────────────────────────────────────┐
│  📊 Review Harness Dashboard         [+ New Benchmark]  │
├─────────────────────────────────────────────────────────┤
│  Past Runs                                              │
│  ┌─────────────────────────────────────────────────────┐│
│  │  smoke-5  ● completed   2 PRs   F1 0.50   $0.015  ││
│  │  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  2m 0s             ││
│  ├─────────────────────────────────────────────────────┤│
│  │  ca-test-1  ● completed  3 PRs   F1 0.72   $0.042 ││
│  │  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  4m 30s            ││
│  └─────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────┘
```

### Run Detail (`/runs/:id`)

```
┌─────────────────────────────────────────────────────────┐
│  ← Back to Home    smoke-5                              │
├─────────────────────────────────────────────────────────┤
│  ┌────────────────┐  ┌────────────────┐                 │
│  │  F1 Score      │  │  Avg Precision │                 │
│  │  0.50          │  │  0.333         │                 │
│  └────────────────┘  └────────────────┘                 │
│  ┌────────────────┐  ┌────────────────┐                 │
│  │  Avg Recall    │  │  Total Cost    │                 │
│  │  1.0           │  │  $0.015        │                 │
│  └────────────────┘  └────────────────┘                 │
│                                                         │
│  Per-PR Results (sorted by F1 ▼)                        │
│  ┌─────────────────────────────────────────────────────┐│
│  │ PR Title       │ F1 │ Prec│ Rec │ Findings│ Cost   ││
│  ├────────────────┼────┼─────┼─────┼─────────┼────────┤│
│  │ scale-color... │0.50│0.33 │1.00 │    0    │ $0.008 ││
│  │ fix-button...  │0.45│0.29 │1.00 │    1    │ $0.007 ││
│  └────────────────┴────┴─────┴─────┴─────────┴────────┘│
└─────────────────────────────────────────────────────────┘
```

### New Benchmark (`/new`)

```
┌─────────────────────────────────────────────────────────┐
│  New Benchmark Run                                      │
├─────────────────────────────────────────────────────────┤
│  Model:         [gpt-4o          ▼]                     │
│  Judge Model:   [gpt-4o-mini     ▼]                     │
│  Dataset:       [golden_comments ▼]                     │
│  Concurrency:   [=====●===========]  4                  │
│  Max Findings:  [=====●===========]  20                 │
│  Prompts Dir:   [prompts/builtin      ]                 │
│  Cache Dir:     [                      ]                │
│  Roles:         ☑ SA  ☑ CL  ☑ AR  ☑ SEC                │
│  Skip Consensus: [  ]                                   │
│  Skip Linters:   [  ]                                   │
│                                                         │
│  [🚀 Start Benchmark]                                   │
└─────────────────────────────────────────────────────────┘
```

### Live View (`/live/:id`)

```
┌─────────────────────────────────────────────────────────┐
│  🔴 Live: smoke-test-1                                  │
├─────────────────────────────────────────────────────────┤
│  ┌─────── SA ───────┐  ┌─────── CL ───────┐             │
│  │ 🔄 reviewing...   │  │ ✅ 3 finding(s)  │             │
│  │ ───────────────── │  │ ───────────────── │             │
│  │ Analyzing PR...   │  │ Issue found in   │             │
│  │ Color function    │  │ button component │             │
│  │ uses wrong var    │  │ Line 42-45       │             │
│  └──────────────────┘  └──────────────────┘             │
│  ┌─────── AR ───────┐  ┌────── SEC ──────┐             │
│  │ ⏳ pending         │  │ ⏳ pending       │             │
│  └──────────────────┘  └──────────────────┘             │
│                                                         │
│  [██████████░░░░░░░░]  2/10 PRs  |  1m 23s  |  $0.03   │
│  PR: discourse-graphite/pull/7 → F1=0.33               │
└─────────────────────────────────────────────────────────┘
```

## Component Definitions

### AgentPane
- **Props:** `role: String`, `state: AgentPaneState`
- **State:** `status: enum { Pending, Reviewing, Done(usize), Failed }`
- **Display:** Title bar with emoji status, response lines, border color by status

### ProgressBar
- **Props:** `completed: usize, total: usize`
- **Display:** CSS progress bar with label "X/Y PRs"

### MetricsCard
- **Props:** `title: String, value: f64, subtitle: Option<String>`
- **Display:** Card with big number, label, optional colored indicator

### RunTable
- **Props:** `results: Vec<PrResult>`
- **Display:** Sortable table with columns for title, F1, precision, recall, findings, cost
- **Sorting:** Click column headers to sort ascending/descending

## Frontend State Management

```rust
// Global reactive state
#[derive(Clone, Debug)]
struct GlobalState {
    runs: Resource<Vec<RunSummary>>,
    config: Resource<Config>,
}

// Per-page state (created when page mounts)
#[derive(Clone, Debug)]
struct LivePageState {
    run_id: String,
    event_source: Option<EventSource>,
    agent_panes: RwSignal<Vec<AgentPaneState>>,
    progress: RwSignal<(usize, usize)>,
    elapsed: RwSignal<f64>,
    cost: RwSignal<f64>,
    current_pr: RwSignal<Option<String>>,
}
```
