# Design: Web UI Redesign

## 1. Color Palette

Professional dark theme inspired by GitHub Dark. All colors defined as CSS custom properties.

### Surface & Background

| Token                 | Value     | Usage                                |
| --------------------- | --------- | ------------------------------------ |
| `--bg-page`           | `#0d1117` | Main page background                 |
| `--bg-surface`        | `#161b22` | Card, sidebar, panel backgrounds     |
| `--bg-surface-hover`  | `#1c2333` | Card/row hover state                 |
| `--bg-surface-raised` | `#1c2128` | Modals, dropdowns, elevated surfaces |
| `--bg-input`          | `#0d1117` | Form input fields                    |

### Accent Colors (Semantic)

| Token             | Value     | Usage                                            |
| ----------------- | --------- | ------------------------------------------------ |
| `--accent-blue`   | `#58a6ff` | Primary actions, links, active nav items         |
| `--accent-green`  | `#3fb950` | Success states, completed runs, positive metrics |
| `--accent-orange` | `#f0883e` | Warnings, pending states, medium metrics         |
| `--accent-red`    | `#f85149` | Errors, failures, negative metrics, stop buttons |
| `--accent-purple` | `#bc8cff` | Agent role indicators (optional distinction)     |

### Text

| Token              | Value           | Usage                           |
| ------------------ | --------------- | ------------------------------- |
| `--text-primary`   | `#c9d1d9`       | Body text, headings             |
| `--text-secondary` | `#8b949e`       | Labels, helper text, timestamps |
| `--text-tertiary`  | `#6e7681`       | Placeholders, disabled text     |
| `--text-inverse`   | `#ffffff`       | Text on accent backgrounds      |
| `--text-link`      | `--accent-blue` | Hyperlinks                      |

### Borders & Dividers

| Token              | Value           | Usage                                 |
| ------------------ | --------------- | ------------------------------------- |
| `--border-default` | `#30363d`       | Card borders, table borders, dividers |
| `--border-muted`   | `#21262d`       | Subtle dividers, separator lines      |
| `--border-accent`  | `--accent-blue` | Focus rings, active selection borders |

### Shadows

| Token         | Value                        |
| ------------- | ---------------------------- |
| `--shadow-sm` | `0 1px 2px rgba(0,0,0,0.3)`  |
| `--shadow-md` | `0 4px 12px rgba(0,0,0,0.4)` |
| `--shadow-lg` | `0 8px 24px rgba(0,0,0,0.5)` |

### Interaction States

- **Hover:** Background shift to `--bg-surface-hover`, optional opacity 0.9 on text borders
- **Focus:** `box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent-blue) 30%, transparent)` (CSS `outline` + `ring`)
- **Active:** `var(--bg-surface-hover)` with slight inset shadow
- **Disabled:** Opacity 0.4, `cursor: not-allowed`

---

## 2. Layout

### Grid System

```
┌──────────────────────────────────────────────────────────────┐
│  ┌──────────┐  ┌──────────────────────────────────────────┐ │
│  │          │  │                                          │ │
│  │ Sidebar  │  │          Main Content                    │ │
│  │ 240px    │  │          ────────────                    │ │
│  │          │  │   max-width: 1400px, centered            │ │
│  │ (coll-   │  │   padding: 32px                          │ │
│  │  apsible) │  │                                          │ │
│  └──────────┘  └──────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────┘
```

### Sidebar Navigation

- **Expanded:** 240px width, shows icon + label per nav item
- **Collapsed:** 64px width, shows icon only (tooltip on hover)
- **Toggle button:** Hamburger icon at top of sidebar
- **Nav items:** Home, Runs (with badge count), New Run, Live (if active)
- **Bottom section:** Version info, settings link (future)
- **Scroll:** Independent scroll if content overflows
- **Fixed positioning:** Sticky sidebar, main content scrolls

### Main Content Area

- `max-width: 1400px` with `margin: 0 auto`
- Horizontal padding: `32px` (desktop), `16px` (<768px)
- Responsive breakpoints:
  - ≥1200px: Full 4-column grid spaces
  - 768–1199px: 2-column, sidebar collapses to icon-only
  - <768px: Single column, sidebar overlay (slide-in)

### Content Grid (inside main area)

```css
.content-grid {
  display: grid;
  gap: var(--spacing-lg);
}
```

- **Card grid (Home):** `grid-template-columns: repeat(auto-fill, minmax(340px, 1fr))`
- **Metrics row:** `grid-template-columns: repeat(auto-fit, minmax(200px, 1fr))`
- **Agent panes (Live):** `grid-template-columns: repeat(2, 1fr)` (2×2 quad layout)

---

## 3. Typography

### Font Stack

```css
--font-sans:
  -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Oxygen, Ubuntu,
  Cantarell, "Helvetica Neue", Arial, sans-serif;
--font-mono:
  "SF Mono", "Fira Code", "Cascadia Code", "JetBrains Mono", Menlo, Consolas,
  monospace;
```

### Type Scale

| Token         | Size | Weight | Line Height | Usage                  |
| ------------- | ---- | ------ | ----------- | ---------------------- |
| `--text-xs`   | 12px | 400    | 1.4         | Captions, timestamps   |
| `--text-sm`   | 14px | 400    | 1.5         | Body text, table cells |
| `--text-base` | 16px | 400    | 1.6         | Default body text      |
| `--text-lg`   | 18px | 500    | 1.5         | Section titles         |
| `--text-xl`   | 20px | 600    | 1.4         | Page titles (h2)       |
| `--text-2xl`  | 24px | 700    | 1.3         | Main heading (h1)      |
| `--text-3xl`  | 32px | 700    | 1.3         | Hero / page heading    |

### Heading Hierarchy

- **Page title (h1):** `--text-3xl`, `--accent-blue` underline accent optional
- **Section heading (h2):** `--text-xl`, `--text-primary`, margin-top `--spacing-xl`
- **Card title (h3):** `--text-lg`, `--text-primary`
- **Label / subtitle:** `--text-sm`, `--text-secondary`, uppercase optional

### Code / Metrics

- **Metric values:** `--font-mono`, `--text-2xl`, weight 600
- **Inline code:** `--font-mono`, `--text-sm`, `--bg-input` background, `--border-default` border, `border-radius: 4px`

---

## 4. Spacing & Sizing

```css
--spacing-xs: 4px;
--spacing-sm: 8px;
--spacing-md: 12px;
--spacing-lg: 16px;
--spacing-xl: 24px;
--spacing-2xl: 32px;
--spacing-3xl: 48px;

--radius-sm: 4px;
--radius-md: 6px;
--radius-lg: 8px;
--radius-xl: 12px;
--radius-full: 9999px;
```

---

## 5. Component Redesign

### 5.1 Card

The fundamental building block.

```
┌──────────────────────────────────────┐
│  Card Header (optional)              │
│  ───────────────────────             │
│  Card Body                           │
│  ───────────────────────             │
│  Card Footer (optional)              │
└──────────────────────────────────────┘
```

- Background: `--bg-surface`
- Border: `1px solid --border-default`
- Radius: `--radius-lg`
- Padding: `--spacing-xl` (header/body/footer)
- Hover: `--bg-surface-hover` background shift (if interactive)
- Transition: `background-color 150ms ease`

### 5.2 Button

| Variant   | Background       | Text               | Border             | Hover                |
| --------- | ---------------- | ------------------ | ------------------ | -------------------- |
| Primary   | `--accent-blue`  | `--text-inverse`   | none               | Lighter blue         |
| Success   | `--accent-green` | `--text-inverse`   | none               | Lighter green        |
| Danger    | `--accent-red`   | `--text-inverse`   | none               | Lighter red          |
| Secondary | transparent      | `--text-primary`   | `--border-default` | `--bg-surface-hover` |
| Ghost     | transparent      | `--text-secondary` | none               | `--bg-surface-hover` |

- Padding: `--spacing-sm --spacing-lg`
- Radius: `--radius-md`
- Font: `--text-sm`, weight 500
- Icon + label spacing: `--spacing-sm`
- Focus ring: `--accent-blue` 3px outline offset

### 5.3 Form Input

- Background: `--bg-input`
- Border: `1px solid --border-default`
- Radius: `--radius-md`
- Padding: `--spacing-sm --spacing-md`
- Focus: border changes to `--accent-blue`, subtle glow ring
- Label: `--text-sm`, `--text-secondary`, margin-bottom `--spacing-xs`
- Error state: border `--accent-red`, helper text in `--accent-red`
- Disabled: opacity 0.4

### 5.4 Badge / Status Indicator

```
┌───────────────────┐
│ ● Completed  ✓    │
│ ● In Progress ◌   │
│ ● Failed      ✕   │
│ ● Pending     ○   │
└───────────────────┘
```

- Dot + label pattern
- Dot color matches semantic accent:
  - **Green dot:** completed / success
  - **Orange dot:** running / in-progress
  - **Red dot:** failed / error
  - **Gray dot:** pending / queued
- Background: `--bg-surface` with border or transparent
- Radius: `--radius-sm` or `--radius-full` (pill)
- Font: `--text-xs`, uppercase optional

### 5.5 Table

```
┌──────────┬──────┬───────┬──────┬─────────┬────────┐
│ PR Title │  F1  │ Prec  │ Rec  │ Findings│ Cost   │
├──────────┼──────┼───────┼──────┼─────────┼────────┤
│ fix-btn  │ 0.50 │ 0.333 │ 1.00 │    0    │ $0.008 │
│ scale-c  │ 0.45 │ 0.290 │ 1.00 │    1    │ $0.007 │
└──────────┴──────┴───────┴──────┴─────────┴────────┘
```

- Header: `--text-sm`, `--text-secondary`, uppercase, weight 600
- Body: `--text-sm`, `--text-primary`
- Row hover: `--bg-surface-hover`
- Sort icon: on hover of sortable header, arrow indicator
- Border: horizontal dividers with `--border-muted`
- Sticky header on scroll
- Empty state: centered message with icon (see below)

### 5.6 Loading Skeleton

- Background shimmer: linear gradient over `--bg-surface` with `--bg-surface-hover` sweep
- Keyframe animation: `shimmer` 1.5s infinite
- Shapes: rounded rectangles matching expected content dimensions
  - Card skeleton: full card outline with 2-3 text line placeholders
  - Table skeleton: header row + 3-5 body rows
  - Metric skeleton: 4 small metric card shapes
- No spinner fallback — skeleton is the primary loading indicator

### 5.7 Error State

```
┌──────────────────────────────────┐
│  ⚠️  Failed to load data        │
│  Something went wrong while     │
│  fetching benchmark runs.       │
│                                 │
│  [🔄 Retry]                     │
└──────────────────────────────────┘
```

- Icon: warning triangle (⚠️) or error circle (❌)
- Heading: `--text-lg`, `--text-primary`
- Message: `--text-sm`, `--text-secondary`
- Retry button: Primary variant, calls fetch again
- Appears inline (not a full-page overlay), centered in content area

### 5.8 Empty State

```
┌──────────────────────────────────┐
│  📂  No benchmark runs yet      │
│  Run your first benchmark to    │
│  see results here.              │
│                                 │
│  [🚀 Start Your First Run]      │
└──────────────────────────────────┘
```

- Icon: relevant emoji or SVG illustration
- Heading: `--text-lg`, `--text-primary`
- Message: `--text-sm`, `--text-secondary`
- CTA button: links to `/new` run form
- Centered in content area with generous padding

### 5.9 Sidebar Nav Item

```
   🏠 Home
   📊 Runs           (3)
   🆕 New Run
   🔴 Live View      (active)
```

- Padding: `--spacing-md --spacing-lg`
- Radius: `--radius-md`
- Active: `--bg-surface-raised` background, `--accent-blue` left border indicator (3px)
- Hover: `--bg-surface-hover`
- Badge count: `--radius-full` pill, `--accent-orange` background, `--text-inverse`
- Icon size: 18px

### 5.10 Toggle / Checkbox

- Custom checkbox styling (hidden native input + styled pseudo-element)
- Checked: `--accent-blue` background, white checkmark
- Unchecked: `--border-default` border, transparent background
- Label: `--text-sm`, `--text-primary`, clickable

---

## 6. Page-Specific Layouts

### 6.1 Home Page

```
┌──────────────────────────────────────────────────────────────┐
│  Dashboard                                    [🔍 Search] │
│                                                   [🆕 New] │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐    │
│  │ Total    │  │ Avg F1   │  │ Total    │  │ Total    │    │
│  │ Runs: 12 │  │ 0.48     │  │ Cost     │  │ PRs: 340 │    │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘    │
│                                                             │
│  Past Runs (sorted by date ▼)                               │
│  ┌─────────────────────┐  ┌─────────────────────┐           │
│  │ smoke-5             │  │ ca-test-1            │           │
│  │ ● 2 PRs | $0.015    │  │ ● 3 PRs | $0.042    │           │
│  │ F1: 0.50            │  │ F1: 0.72            │           │
│  │ ───╱╲───╱╲──      │  │ ───╱╲──╱╲╱╲───      │           │
│  │ 2m 0s              │  │ 4m 30s              │           │
│  └─────────────────────┘  └─────────────────────┘           │
│  ┌─────────────────────┐  ┌─────────────────────┐           │
│  │ full-suite-3         │  │ smoke-2 (running)   │           │
│  │ ● 12 PRs | $0.18    │  │ ◌ 5/12 PRs | $0.07 │           │
│  │ F1: 0.61            │  │ F1: — (in progress) │           │
│  │ ──╱╲╱╲──╱╲───      │  │ [progress bar]      │           │
│  │ 8m 12s              │  │ 3m 42s              │           │
│  └─────────────────────┘  └─────────────────────┘           │
└──────────────────────────────────────────────────────────────┘
```

- Top row: 4 metric summary cards (auto-fit grid)
- Card grid below: `auto-fill, minmax(340px, 1fr)` for past run cards
- Each run card shows: name, status badge, PR count, cost, avg F1, mini sparkline, duration
- Search bar in header area for filtering runs by name
- "New Benchmark" primary button in header area

### 6.2 Run Detail Page

```
┌──────────────────────────────────────────────────────────────┐
│  ← Back to Dashboard          smoke-5          [🔍 Search] │
│                                                             │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐    │
│  │ F1       │  │ Precision│  │ Recall   │  │ Cost     │    │
│  │ 0.50     │  │ 0.333    │  │ 1.00     │  │ $0.015   │    │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘    │
│                                                             │
│  Per-PR Results (sorted by F1 ▼)     [Filter: All PRs ▼]   │
│  ┌──────┬───────┬──────┬──────┬─────────┬────────┬────────┐│
│  │ PR   │  F1   │ Prec │ Rec  │ Findings│  Cost  │ Status ││
│  ├──────┼───────┼──────┼──────┼─────────┼────────┼────────┤│
│  │ #7   │ 0.50  │ 0.33 │ 1.00 │    0    │ $0.008 │ ✅    ││
│  │ #5   │ 0.45  │ 0.29 │ 1.00 │    1    │ $0.007 │ ✅    ││
│  └──────┴───────┴──────┴──────┴─────────┴────────┴────────┘│
│                                                             │
│  Cost Breakdown                                             │
│  ┌──────────┬────────┬──────────┬──────────┐                │
│  │ Model    │ Tokens │  Cost    │  Calls   │                │
│  ├──────────┼────────┼──────────┼──────────┤                │
│  │ gpt-4o   │ 12,500 │ $0.012  │    8     │                │
│  │ gpt-4o-m │  3,200 │ $0.003  │    4     │                │
│  └──────────┴────────┴──────────┴──────────┘                │
└──────────────────────────────────────────────────────────────┘
```

- Back link with arrow at top
- 4 metric summary cards (F1, Precision, Recall, Cost)
- Sortable, filterable table of per-PR results
- Filter dropdown: All PRs, Failed, High F1 (>0.7), Low F1 (<0.3)
- Cost breakdown section at bottom

### 6.3 New Run Page

```
┌──────────────────────────────────────────────────────────────┐
│  New Benchmark Run                                [Cancel] │
│                                                             │
│  ═══════════════════════════════════════════════════════════ │
│  Configuration                                               │
│                                                             │
│  Model:         [gpt-4o                          ▼]         │
│  Judge Model:   [gpt-4o-mini                     ▼]         │
│  Dataset:       [golden_comments                  ▼]         │
│                                                             │
│  ═══════════════════════════════════════════════════════════ │
│  Execution                                                   │
│                                                             │
│  Concurrency:   [══════════════●═══════════════]  4          │
│  Max Findings:  [════════●══════════════════════]  20        │
│  Max Turns:     [══════════●════════════════════]  3         │
│                                                             │
│  ═══════════════════════════════════════════════════════════ │
│  Advanced                                                    │
│                                                             │
│  Prompts Dir:   [prompts/builtin                    ]       │
│  Cache Dir:     [                                   ]       │
│                                                             │
│  Roles to Run:                                              │
│  ☑ SA (Security)  ☑ CL (Code Logic)                        │
│  ☑ AR (Architecture)  ☑ SEC (Security - extra)             │
│                                                             │
│  □ Skip Consensus   □ Skip Linters   □ Dry Run             │
│                                                             │
│  ═══════════════════════════════════════════════════════════ │
│                                                             │
│  [🚀 Start Benchmark]                                       │
│                                                             │
└──────────────────────────────────────────────────────────────┘
```

- Sections separated by visual dividers (thin horizontal rules)
- Form fields with proper labels, helper text below, validation indicators
- Sliders with numeric readout for numeric values
- Checkbox groups for role selection
- Submit button prominently at bottom

### 6.4 Live View Page

```
┌──────────────────────────────────────────────────────────────┐
│  🔴 Live: smoke-test-1     [🔗 Share]     [⬅ Back]         │
│                                                             │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐    │
│  │ Progress │  │ Elapsed  │  │ Cost     │  │ Current  │    │
│  │ 5/12 PRs │  │ 3m 42s   │  │ $0.07    │  │ PR: #7   │    │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘    │
│                                                             │
│  ┌──── SA ─────────────┐  ┌──── CL ─────────────┐          │
│  │ 🟢 reviewing...     │  │ 🟡 3 finding(s)     │          │
│  │ ─────────────────── │  │ ─────────────────── │          │
│  │ Analyzing PR #7...  │  │ Issue found in      │          │
│  │ Color function      │  │ button component    │          │
│  │ uses wrong...       │  │ Line 42-45...       │          │
│  └─────────────────────┘  └─────────────────────┘          │
│  ┌──── AR ─────────────┐  ┌──── SEC ────────────┐          │
│  │ ⏳ pending...        │  │ ⏳ pending...        │          │
│  │ ─────────────────── │  │ ─────────────────── │          │
│  │                     │  │                     │          │
│  └─────────────────────┘  └─────────────────────┘          │
│                                                             │
│  [████████████████░░░░░░░░░░]  5/12 PRs  |  Current: #7   │
│  PR: discourse-graphite/pull/7 -> F1=0.33                   │
└──────────────────────────────────────────────────────────────┘
```

- Top row: 4 live metric cards (progress, elapsed, cost, current PR)
- 2×2 agent pane grid with role labels
- Each pane: status indicator (colored dot), response streaming content, finding count
- Bottom: overall progress bar + current PR info line
- Auto-scrollable response areas within panes (overflow-y)
- Pane border color changes with agent status:
  - Pending: `--border-default`
  - Running: `--accent-blue`
  - Done (success): `--accent-green`
  - Failed: `--accent-red`

---

## 7. CSS Architecture

### File Structure

```
crates/crb-webui-frontend/
├── css/
│   ├── tokens.css          # All CSS custom properties (colors, spacing, type, shadows)
│   ├── reset.css           # Minimal reset (box-sizing, margin removal)
│   ├── base.css            # Base element styles (body, headings, links, code)
│   ├── layout.css          # Grid, sidebar, content area, responsive rules
│   ├── components/
│   │   ├── card.css
│   │   ├── button.css
│   │   ├── form.css
│   │   ├── table.css
│   │   ├── badge.css
│   │   ├── skeleton.css
│   │   ├── nav.css
│   │   └── progress.css
│   └── pages/
│       ├── home.css
│       ├── run-detail.css
│       ├── new-run.css
│       └── live-view.css
└── styles.css              # Single entry point importing all above
```

### CSS Custom Properties Pattern

```css
/* tokens.css */
:root {
  /* Surface */
  --bg-page: #0d1117;
  --bg-surface: #161b22;
  --bg-surface-hover: #1c2333;
  --bg-surface-raised: #1c2128;
  --bg-input: #0d1117;

  /* Accents */
  --accent-blue: #58a6ff;
  --accent-green: #3fb950;
  --accent-orange: #f0883e;
  --accent-red: #f85149;
  --accent-purple: #bc8cff;

  /* Text */
  --text-primary: #c9d1d9;
  --text-secondary: #8b949e;
  --text-tertiary: #6e7681;
  --text-inverse: #ffffff;
  --text-link: var(--accent-blue);

  /* Borders */
  --border-default: #30363d;
  --border-muted: #21262d;
  --border-accent: var(--accent-blue);

  /* Shadows */
  --shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.3);
  --shadow-md: 0 4px 12px rgba(0, 0, 0, 0.4);
  --shadow-lg: 0 8px 24px rgba(0, 0, 0, 0.5);

  /* Typography */
  --font-sans:
    -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Oxygen, Ubuntu,
    Cantarell, "Helvetica Neue", Arial, sans-serif;
  --font-mono:
    "SF Mono", "Fira Code", "Cascadia Code", "JetBrains Mono", Menlo, Consolas,
    monospace;
  --text-xs: 12px;
  --text-sm: 14px;
  --text-base: 16px;
  --text-lg: 18px;
  --text-xl: 20px;
  --text-2xl: 24px;
  --text-3xl: 32px;

  /* Spacing */
  --spacing-xs: 4px;
  --spacing-sm: 8px;
  --spacing-md: 12px;
  --spacing-lg: 16px;
  --spacing-xl: 24px;
  --spacing-2xl: 32px;
  --spacing-3xl: 48px;

  /* Radii */
  --radius-sm: 4px;
  --radius-md: 6px;
  --radius-lg: 8px;
  --radius-xl: 12px;
  --radius-full: 9999px;

  /* Transitions */
  --transition-fast: 150ms ease;
  --transition-normal: 200ms ease;
}
```

### Responsive Breakpoints

```css
/* Sidebar toggle */
@media (max-width: 1199px) {
  .sidebar {
    width: 64px;
  }
  .sidebar .nav-label {
    display: none;
  }
  .main-content {
    margin-left: 64px;
  }
}

/* Mobile */
@media (max-width: 767px) {
  .sidebar {
    display: none;
  }
  .mobile-nav {
    display: flex;
  }
  .content-grid {
    grid-template-columns: 1fr;
  }
  .main-content {
    padding: var(--spacing-lg);
  }
}
```

---

## 8. Accessibility Considerations

- Focus rings visible on all interactive elements (keyboard navigation)
- Color contrast ratios meet WCAG AA (minimum 4.5:1 for text, 3:1 for large text)
- Semantic HTML: `<nav>`, `<main>`, `<section>`, `<header>`, `<table>`, `<form>` with proper `<label>` elements
- `aria-label` on icon-only buttons (collapsed sidebar)
- `role="status"` on live updating status sections
- `prefers-reduced-motion` respects `@media (prefers-reduced-motion: reduce)` for skeleton animations

---

## 9. Implementation Status

### Color Palette Verification

All color tokens in this design match the actual `crates/crb-webui-frontend/css/tokens.css` exactly — no discrepancies in values or naming.

### Component Implementation: CSS vs Rust+CSS

| Component | Type | Status |
|-----------|------|--------|
| tokens.css | CSS-only | ✅ All 55 tokens present |
| reset.css / base.css | CSS-only | ✅ Present |
| layout.css | CSS-only | ✅ Present |
| nav.css | CSS-only | ✅ Present; Rust sidebar component in `app.rs` uses same CSS classes |
| card.css | CSS-only | ✅ Present; card BEM classes used directly in page views |
| button.css | CSS-only | ✅ Present; `.btn` variants used throughout |
| form.css | CSS-only | ✅ Present; includes slider styling (unused by Rust) |
| badge.css | CSS-only | ✅ Present; all semantic variants used |
| table.css | CSS-only | ✅ Present; `RunTable` component uses its sortable classes |
| skeleton.css | CSS-only | ✅ Present; skeleton shapes used in all pages |
| progress.css | CSS-only | ✅ Present; bound to `ProgressBar` Leptos component |
| home.css | CSS-only | ✅ Present; Rust `HomePage` uses these classes |
| run-detail.css | CSS-only | ✅ Present; Rust `RunDetailPage` uses these classes |
| new-run.css | CSS-only | ✅ Present; Rust `NewRunPage` uses these classes |
| live-view.css | CSS-only | ✅ Present; Rust `LivePage` uses these classes |

### Missing Rust Components

The following features described in this design have CSS but **no Rust/Leptos implementation**:

- **Search bar** on HomePage (Section 6.1)
- **Sparklines** on run cards (Section 6.1)
- **Cost breakdown** section on RunDetailPage (Section 6.2)
- **Table sorting** on RunDetailPage per-PR results table (Section 6.2)
- **Slider inputs** on NewBenchmarkPage (Section 6.3; uses number inputs instead)
- **Per-agent metrics** on LiveViewPage (Section 6.4; agent panes show status + response text only)
- **Auto-scroll** in agent panes on LiveViewPage (Section 6.4)
