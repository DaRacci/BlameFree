# Components Specification

## Overview

This document specifies the visual design, states, and DOM structure for every reusable UI component in the `crb-webui` redesign. All components use CSS custom properties from `tokens.css` — no hardcoded values.

---

## 1. Card

The fundamental container component used throughout the UI.

### Visual Design

```
┌──────────────────────────────────────────────┐
│  ┌──────────────────────────────────────────┐ │
│  │  Card Header (optional)                  │ │
│  │  title, actions, badge                   │ │
│  ├──────────────────────────────────────────┤ │
│  │  Card Body                               │ │
│  │  primary content area                    │ │
│  ├──────────────────────────────────────────┤ │
│  │  Card Footer (optional)                  │ │
│  │  secondary actions, metadata             │ │
│  └──────────────────────────────────────────┘ │
└──────────────────────────────────────────────┘
```

### CSS Properties

| Property | Token | Value |
|----------|-------|-------|
| background | `--bg-surface` | #161b22 |
| border | `--border-default` | 1px solid #30363d |
| border-radius | `--radius-lg` | 8px |
| box-shadow | `--shadow-sm` | 0 1px 2px rgba(0,0,0,0.3) |
| padding (body) | `--spacing-xl` | 24px |

### States

- **Default:** As above.
- **Hover (interactive cards):** `background: var(--bg-surface-hover)`, `transition: background-color var(--transition-fast)`, optional `cursor: pointer`.
- **Clickable (run cards):** Entire card is a link — no separate button inside.

### DOM Structure

```html
<div class="card">
  <div class="card__header">
    <h3 class="card__title">Run Name</h3>
    <span class="badge badge--success">Completed</span>
  </div>
  <div class="card__body">
    <!-- content -->
  </div>
  <div class="card__footer">
    <span class="card__meta">2m 30s</span>
  </div>
</div>
```

### CSS Classes

- `.card` — container
- `.card--interactive` — hover state enabled
- `.card__header` — flex row, items center, space-between
- `.card__body` — flex column, gap `--spacing-md`
- `.card__footer` — flex row, items center, border-top `1px solid --border-muted`, padding-top `--spacing-md`
- `.card__title` — `--text-lg`, `--text-primary`, `--weight-medium`
- `.card__meta` — `--text-sm`, `--text-secondary`

---

## 2. Button

### Variants

| Variant | Background | Text Color | Border | Hover | Active |
|---------|-----------|------------|--------|-------|--------|
| `primary` | `--accent-blue` | `--text-inverse` | none | `filter: brightness(1.1)` | `filter: brightness(0.95)` |
| `success` | `--accent-green` | `--text-inverse` | none | `filter: brightness(1.1)` | `filter: brightness(0.95)` |
| `danger` | `--accent-red` | `--text-inverse` | none | `filter: brightness(1.1)` | `filter: brightness(0.95)` |
| `secondary` | transparent | `--text-primary` | `1px solid --border-default` | `bg: --bg-surface-hover` | `bg: --bg-surface` |
| `ghost` | transparent | `--text-secondary` | none | `bg: --bg-surface-hover`, `color: --text-primary` | `bg: --bg-surface` |

### Shared Properties

- `padding: var(--spacing-sm) var(--spacing-lg)` (8px 16px)
- `border-radius: var(--radius-md)` (6px)
- `font: var(--text-sm) var(--font-sans)`, `font-weight: 500`
- `cursor: pointer`
- `transition: all var(--transition-fast)`
- Focus ring: `box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent-blue) 30%, transparent)`
- Disabled: `opacity: 0.4`, `cursor: not-allowed`, `pointer-events: none`

### Size Variants

| Size | Padding | Font Size |
|------|---------|-----------|
| `sm` | `--spacing-xs --spacing-md` | `--text-xs` |
| `md` (default) | `--spacing-sm --spacing-lg` | `--text-sm` |
| `lg` | `--spacing-md --spacing-xl` | `--text-base` |

### DOM Structure

```html
<button class="btn btn--primary btn--md" type="button">
  <span class="btn__icon">🚀</span>
  <span class="btn__label">Start Benchmark</span>
</button>
```

### CSS Classes

- `.btn` — base button reset + shared properties
- `.btn--primary`, `.btn--success`, `.btn--danger`, `.btn--secondary`, `.btn--ghost`
- `.btn--sm`, `.btn--md`, `.btn--lg`
- `.btn--disabled` (for link/div elements that aren't `<button>`)
- `.btn__icon` — `--text-lg`, margin-right `--spacing-sm`
- `.btn__label`

---

## 3. Form Input

### Visual Design

```
┌────────────────────────────────────────────┐
│  Model                                      │  ← label
│  ┌────────────────────────────────────────┐ │
│  │ gpt-4o                          [▼]   │ │  ← input
│  └────────────────────────────────────────┘ │
│  The model used for review agents           │  ← helper text
└────────────────────────────────────────────┘
```

### CSS Properties

| Element | Property | Token |
|---------|----------|-------|
| Label | color | `--text-secondary` |
| Label | font | `--text-sm` |
| Label | margin-bottom | `--spacing-xs` |
| Input | background | `--bg-input` |
| Input | border | `1px solid --border-default` |
| Input | border-radius | `--radius-md` |
| Input | padding | `--spacing-sm --spacing-md` |
| Input | color | `--text-primary` |
| Input | font | `--text-sm` |
| Helper text | color | `--text-tertiary` |
| Helper text | font | `--text-xs` |
| Helper text | margin-top | `--spacing-xs` |

### States

- **Focus:** `border-color: var(--accent-blue)`, `box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent-blue) 20%, transparent)`
- **Error:** `border-color: var(--accent-red)`, helper text color `var(--accent-red)`
- **Disabled:** `opacity: 0.4`, `cursor: not-allowed`
- **Read-only:** `background: transparent`, `border-color: transparent`

### Input Types

All standard input types use the same base styles:

- `.input` — `<input>` text, email, number, url
- `.select` — `<select>` with custom chevron via `background-image`
- `.textarea` — `<textarea>` with min-height 80px, resizable vertical
- `.slider` — range input with custom track/thumb styling (accent-colored)

### DOM Structure

```html
<div class="form-field">
  <label class="form-field__label" for="model-select">Model</label>
  <select id="model-select" class="input select">
    <option>gpt-4o</option>
    <option>gpt-4o-mini</option>
  </select>
  <p class="form-field__helper">The model used for review agents</p>
  <p class="form-field__error" hidden>This field is required</p>
</div>
```

---

## 4. Badge / Status Indicator

### Visual Design

```
 ● Completed    (green)
 ● In Progress  (orange)
 ● Failed       (red)
 ● Pending      (gray/dim)
```

### CSS Properties

| Property | Token |
|----------|-------|
| display | `inline-flex`, `align-items: center` |
| padding | `--spacing-xs --spacing-sm` |
| border-radius | `--radius-sm` |
| font | `--text-xs`, `--weight-medium` |
| gap | `--spacing-xs` |
| background | `--bg-surface` |
| border | `1px solid --border-default` |

### Semantic Variants

| Variant | Dot Color |
|---------|-----------|
| `badge--success` | `--accent-green` |
| `badge--warning` | `--accent-orange` |
| `badge--danger` | `--accent-red` |
| `badge--info` | `--accent-blue` |
| `badge--neutral` | `--text-tertiary` |

Also supports pill variant: `badge--pill` with `border-radius: --radius-full` and `padding: --spacing-xs --spacing-md`.

### DOM Structure

```html
<span class="badge badge--success">
  <span class="badge__dot"></span>
  <span class="badge__label">Completed</span>
</span>
```

Alternative compact form (dot only):

```html
<span class="badge-dot badge-dot--success" title="Completed"></span>
```

---

## 5. Table

### Visual Design

```
┌───────────┬──────┬───────┬──────┬─────────┬────────┐
│ ↕ PR      │ ↕ F1 │ ↕ Prec│ ↕ Rec│ ↕ Findi │ ↕ Cost │  ← sortable headers
├───────────┼──────┼───────┼──────┼─────────┼────────┤
│ fix-btn   │ 0.50 │ 0.333 │ 1.00 │    0    │ $0.008 │
│ scale-…   │ 0.45 │ 0.290 │ 1.00 │    1    │ $0.007 │  ← hover row
└───────────┴──────┴───────┴──────┴─────────┴────────┘
```

### CSS Properties

| Element | Property | Token |
|---------|----------|-------|
| Table | width | 100% |
| Table | border-collapse | collapse |
| Header cell | color | `--text-secondary` |
| Header cell | font | `--text-sm`, `--weight-semibold` |
| Header cell | text-transform | uppercase |
| Header cell | padding | `--spacing-sm --spacing-md` |
| Header cell | border-bottom | `2px solid --border-default` |
| Body cell | color | `--text-primary` |
| Body cell | font | `--text-sm` |
| Body cell | padding | `--spacing-sm --spacing-md` |
| Body row | border-bottom | `1px solid --border-muted` |
| Body row hover | background | `--bg-surface-hover` |

### Interaction

- **Sortable header:** Hover shows sort arrow (↕->↑/↓), click toggles sort direction. Active sort column header has `color: var(--accent-blue)`.
- **Clickable row:** If row links to detail page, entire row is clickable with `cursor: pointer`. Hover state applies.

### States

- **Loading:** Table skeleton rows shown instead of data.
- **Empty:** Centered message in table area (handled by empty state component, not a "no data" row).
- **Error:** Error state component replaces table.

### DOM Structure

```html
<div class="table-wrapper">
  <div class="table-controls">
    <div class="table-filter">
      <label class="form-field__label" for="filter">Filter:</label>
      <select id="filter" class="input select select--sm">
        <option>All PRs</option>
        <option>High F1 (>0.7)</option>
        <option>Low F1 (<0.3)</option>
      </select>
    </div>
    <span class="table-count">12 results</span>
  </div>
  <table class="table">
    <thead>
      <tr>
        <th class="table__th table__th--sortable table__th--asc">
          F1 <span class="table__sort-icon">↑</span>
        </th>
        <!-- ... -->
      </tr>
    </thead>
    <tbody>
      <tr class="table__row table__row--clickable" data-href="/runs/...">
        <td class="table__td">0.50</td>
        <!-- ... -->
      </tr>
    </tbody>
  </table>
</div>
```

---

## 6. Loading Skeleton

### Visual Design

Shimmering placeholder shapes that mirror the dimensions of the expected content.

```
┌──────────────────────────────────┐
│  ┌──────────────────────────┐    │  ← shimmer gradient
│  │ ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓ │    │     sweep left->right
│  │ ▓▓▓▓▓▓▓▓▓▓▓▓▓▓         │    │
│  │ ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓ │    │
│  └──────────────────────────┘    │
└──────────────────────────────────┘
```

### CSS Keyframes

```css
@keyframes shimmer {
  0% { background-position: -200% 0; }
  100% { background-position: 200% 0; }
}

.skeleton {
  background: linear-gradient(
    90deg,
    var(--bg-surface) 25%,
    var(--bg-surface-hover) 50%,
    var(--bg-surface) 75%
  );
  background-size: 200% 100%;
  animation: shimmer 1.5s infinite;
  border-radius: var(--radius-md);
}
```

### Skeleton Shapes

| Class | Mimics | Dimensions |
|-------|--------|------------|
| `.skeleton--text` | Line of text | height: 14px, width: 60–100% |
| `.skeleton--heading` | Heading | height: 24px, width: 40% |
| `.skeleton--card` | Card | height: 120px, full width |
| `.skeleton--metric` | Metric card | height: 80px, width: 200px |
| `.skeleton--avatar` | Dot/icon | height: 24px, width: 24px, round |
| `.skeleton--table-row` | Table row | height: 32px, full width |

### DOM Structure

```html
<!-- Card skeleton -->
<div class="card skeleton skeleton--card" aria-label="Loading..." role="status">
  <div class="skeleton skeleton--heading" style="width: 40%"></div>
  <div class="skeleton skeleton--text" style="width: 80%"></div>
  <div class="skeleton skeleton--text" style="width: 60%"></div>
</div>
```

Each skeleton container receives `role="status"` and `aria-label="Loading..."` for screen readers.

---

## 7. Error State

### Visual Design

```
┌────────────────────────────────────────┐
│                                        │
│           ⚠️                            │  ← icon (48px)
│                                        │
│     Failed to load data                │  ← heading
│                                        │
│     Something went wrong while         │
│     fetching benchmark results.        │  ← message
│                                        │
│       [🔄 Retry]                       │  ← button
│                                        │
└────────────────────────────────────────┘
```

### CSS Classes

- `.error-state` — flex column, center, padding `--spacing-3xl`
- `.error-state__icon` — `font-size: 48px`, `line-height: 1`, margin-bottom `--spacing-xl`
- `.error-state__heading` — `--text-lg`, `--text-primary`
- `.error-state__message` — `--text-sm`, `--text-secondary`, max-width `400px`, text-align center
- `.error-state__action` — margin-top `--spacing-xl`

### DOM Structure

```html
<div class="error-state" role="alert">
  <div class="error-state__icon">⚠️</div>
  <h3 class="error-state__heading">Failed to load data</h3>
  <p class="error-state__message">Something went wrong while fetching benchmark results.</p>
  <div class="error-state__action">
    <button class="btn btn--primary" onclick="retry()">🔄 Retry</button>
  </div>
</div>
```

---

## 8. Empty State

### Visual Design

```
┌────────────────────────────────────────┐
│                                        │
│           📂                            │  ← icon (48px)
│                                        │
│     No benchmark runs yet              │  ← heading
│                                        │
│     Run your first benchmark to        │
│     see results here.                  │  ← message
│                                        │
│    [🚀 Start Your First Run]           │  ← CTA button
│                                        │
└────────────────────────────────────────┘
```

### CSS Classes

- `.empty-state` — flex column, center, padding `--spacing-3xl`
- `.empty-state__icon` — `font-size: 48px`, `line-height: 1`, margin-bottom `--spacing-xl`
- `.empty-state__heading` — `--text-lg`, `--text-primary`
- `.empty-state__message` — `--text-sm`, `--text-secondary`, max-width `400px`, text-align center
- `.empty-state__action` — margin-top `--spacing-xl`

### DOM Structure

```html
<div class="empty-state">
  <div class="empty-state__icon">📂</div>
  <h3 class="empty-state__heading">No benchmark runs yet</h3>
  <p class="empty-state__message">Run your first benchmark to see results here.</p>
  <div class="empty-state__action">
    <a href="/new" class="btn btn--primary">🚀 Start Your First Run</a>
  </div>
</div>
```

---

## 9. Sidebar Navigation

### Visual Design (Expanded)

```
┌────────────────────┐
│  ☰                  │  ← toggle button
│                     │
│  🏠 Home            │
│  📊 Runs        (3) │  ← badge
│  🆕 New Run         │
│  🔴 Live View       │  ← active (blue left border)
├────────────────────┤
│                     │
│  ⚙️ Settings        │  ← future
│  v0.1.0             │  ← version
└────────────────────┘
```

### Visual Design (Collapsed)

```
┌────────┐
│  ☰      │
│         │
│  🏠     │  ← tooltip "Home" on hover
│  📊 (3) │  ← truncated badge
│  🆕     │
│  🔴     │  ← active indicator (blue left border)
├────────┤
│  ⚙️     │
│  v0.1.0 │
└────────┘
```

### CSS Properties

| Property | Token |
|----------|-------|
| background | `--bg-surface` |
| border-right | `1px solid --border-default` |
| width (expanded) | 240px |
| width (collapsed) | 64px |
| transition | `width var(--transition-normal)` |
| Item padding | `--spacing-md --spacing-lg` |
| Item border-radius | `--radius-md` |
| Item active bg | `--bg-surface-raised` |
| Item active left border | `3px solid --accent-blue` |
| Item hover bg | `--bg-surface-hover` |

### DOM Structure

```html
<nav class="sidebar" aria-label="Main navigation">
  <div class="sidebar__header">
    <button class="btn btn--ghost sidebar__toggle" aria-label="Toggle sidebar">☰</button>
    <span class="sidebar__brand sidebar__label">Dashboard</span>
  </div>

  <ul class="sidebar__nav">
    <li>
      <a href="/" class="sidebar__item sidebar__item--active">
        <span class="sidebar__icon">🏠</span>
        <span class="sidebar__label">Home</span>
      </a>
    </li>
    <li>
      <a href="/runs" class="sidebar__item">
        <span class="sidebar__icon">📊</span>
        <span class="sidebar__label">Runs</span>
        <span class="badge badge--pill badge--warning">3</span>
      </a>
    </li>
    <!-- ... -->
  </ul>

  <div class="sidebar__footer">
    <span class="sidebar__version sidebar__label">v0.1.0</span>
  </div>
</nav>
```

---

## 10. Progress Bar

### Visual Design

```
[████████████████░░░░░░░░░░]  5/12 PRs
```

### CSS Properties

| Element | Property | Token |
|---------|----------|-------|
| Track | background | `--border-muted` |
| Track | border-radius | `--radius-full` |
| Track | height | 8px |
| Fill | background | `--accent-blue` |
| Fill | border-radius | `--radius-full` |
| Fill | transition | `width var(--transition-normal)` |
| Label | font | `--text-sm`, `--text-secondary` |
| Label | margin-top | `--spacing-xs` |

### States

- **Indeterminate (no total yet):** Fill uses striped animation pattern via repeating gradient.
- **Complete (100%):** Fill changes to `--accent-green`.
- **Error:** Fill changes to `--accent-red`.

### DOM Structure

```html
<div class="progress" role="progressbar" aria-valuenow="5" aria-valuemin="0" aria-valuemax="12">
  <div class="progress__track">
    <div class="progress__fill" style="width: 41.6%"></div>
  </div>
  <span class="progress__label">5/12 PRs</span>
</div>
```

---

## 11. Metric Card

### Visual Design

```
┌──────────────────────┐
│  F1 Score             │  ← label
│  0.50                 │  ← value (mono, large)
│  +12% from last run  │  ← delta (optional, green)
└──────────────────────┘
```

### CSS Properties

| Property | Token |
|----------|-------|
| background | `--bg-surface` |
| border | `1px solid --border-default` |
| border-radius | `--radius-lg` |
| padding | `--spacing-xl` |
| text-align | center |

### Sub-elements

- `.metric-card__label` — `--text-sm`, `--text-secondary`
- `.metric-card__value` — `--text-2xl`, `--font-mono`, `--weight-semibold`, `--text-primary`
- `.metric-card__delta` — `--text-xs`, `--text-secondary`; positive: `--accent-green`, negative: `--accent-red`
- `.metric-card__subtitle` — `--text-xs`, `--text-tertiary`

### DOM Structure

```html
<div class="card metric-card">
  <p class="metric-card__label">F1 Score</p>
  <p class="metric-card__value">0.50</p>
  <p class="metric-card__delta metric-card__delta--positive">↑ 12%</p>
</div>
```

---

## 12. Agent Pane (Live View)

### Visual Design

```
┌──── SA ──────────────────┐
│ 🟢 reviewing...           │  ← status header
│ ─────────────────────── │
│ Analyzing PR #7...       │  ← streaming response
│ Color function uses      │
│ wrong variable in...     │
│                          │
│ Findings: 0              │  ← finding count
└──────────────────────────┘
```

### Border Color by Status

| Status | Border Color |
|--------|-------------|
| Pending | `--border-default` |
| Running | `--accent-blue` |
| Completed (success) | `--accent-green` |
| Failed | `--accent-red` |

### CSS Properties

| Element | Property | Token |
|---------|----------|-------|
| Container | background | `--bg-surface` |
| Container | border | `2px solid (varies by status)` |
| Container | border-radius | `--radius-lg` |
| Header | font | `--text-sm`, `--weight-semibold` |
| Header | padding | `--spacing-sm --spacing-md` |
| Header | border-bottom | `1px solid --border-muted` |
| Content | padding | `--spacing-md` |
| Content | font | `--text-sm`, `--font-mono` |
| Content | max-height | 300px |
| Content | overflow-y | auto |

### DOM Structure

```html
<div class="agent-pane agent-pane--running">
  <div class="agent-pane__header">
    <span class="badge-dot badge-dot--success"></span>
    <span class="agent-pane__role">SA</span>
    <span class="agent-pane__status">reviewing...</span>
  </div>
  <div class="agent-pane__content">
    <p>Analyzing PR #7...</p>
    <p>Color function uses wrong variable...</p>
  </div>
  <div class="agent-pane__footer">
    <span class="agent-pane__findings">Findings: 0</span>
  </div>
</div>
```

---

## 13. Form Section

### Visual Design

```
═ Configuration ═══════════════════════════

  Model:         [________________________] ▼
  Judge:         [________________________] ▼
  Dataset:       [________________________] ▼

═ Execution ═══════════════════════════════

  Concurrency:   [═══●═══════════════════]  4

═ Advanced ════════════════════════════════
```

### CSS Properties

| Element | Property | Token |
|---------|----------|-------|
| Section divider | border-top | `1px solid --border-muted` |
| Section divider | margin | `--spacing-2xl 0` |
| Section title | color | `--text-secondary` |
| Section title | font | `--text-xs`, `--weight-semibold`, uppercase |
| Section title | letter-spacing | 0.5px |

### DOM Structure

```html
<section class="form-section">
  <h2 class="form-section__title">Configuration</h2>
  <div class="form-section__fields">
    <div class="form-field">...</div>
    <div class="form-field">...</div>
  </div>
</section>
```

---

## 14. Slider (Range Input)

### Visual Design

```
  Concurrency:   [══════●═══════════════]  4
```

### CSS Properties (custom range styling)

```css
/* Track */
input[type="range"]::-webkit-slider-runnable-track {
  height: 6px;
  background: var(--border-muted);
  border-radius: var(--radius-full);
}

/* Fill (left of thumb) */
input[type="range"]::-webkit-slider-runnable-track {
  background: linear-gradient(
    to right,
    var(--accent-blue) 0%,
    var(--accent-blue) var(--fill-percentage),
    var(--border-muted) var(--fill-percentage),
    var(--border-muted) 100%
  );
}

/* Thumb */
input[type="range"]::-webkit-slider-thumb {
  width: 18px;
  height: 18px;
  background: var(--accent-blue);
  border-radius: 50%;
  border: 2px solid var(--text-inverse);
  cursor: pointer;
  margin-top: -6px;
}
```

### DOM Structure

```html
<div class="slider-field">
  <label class="form-field__label">Concurrency</label>
  <div class="slider-field__control">
    <input type="range" min="1" max="8" value="4" class="slider" />
    <output class="slider-field__value">4</output>
  </div>
</div>
```

---

## 15. Search / Filter Bar

### Visual Design

```
  🔍 Search runs...                    [All Status ▼]
```

### DOM Structure

```html
<div class="search-bar">
  <div class="search-bar__input-wrapper">
    <span class="search-bar__icon">🔍</span>
    <input type="search" class="input search-bar__input" placeholder="Search runs..." />
  </div>
  <select class="input select search-bar__filter">
    <option>All Status</option>
    <option>Completed</option>
    <option>Running</option>
    <option>Failed</option>
  </select>
</div>
```
