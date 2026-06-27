# Design Tokens Specification

## Overview

All visual design tokens for the `crb-webui` redesign are defined as CSS custom properties on `:root`. This file documents every token, its value, and its intended usage. No hardcoded color, spacing, or type values should appear outside of `tokens.css`.

## Color Tokens

### Semantic Color Variables

Each color token references either a raw hex value or another variable. This enables future theme overrides by redefining the semantic variables.

```css
:root {
  /* ── Backgrounds ─────────────────────────────────── */

  /* Main page background — darkest level */
  --bg-page: #0d1117;
  /* Card, sidebar, and panel surfaces — one level above page */
  --bg-surface: #161b22;
  /* Hover state for interactive surfaces */
  --bg-surface-hover: #1c2333;
  /* Elevated surfaces — modals, dropdowns, popovers */
  --bg-surface-raised: #1c2128;
  /* Form input backgrounds */
  --bg-input: #0d1117;

  /* ── Accent Colors ───────────────────────────────── */

  /* Primary actions, links, active/focus indicators */
  --accent-blue: #58a6ff;
  /* Success states, completed runs, positive metrics */
  --accent-green: #3fb950;
  /* Warning states, in-progress runs, medium metrics */
  --accent-orange: #f0883e;
  /* Error states, failures, negative metrics, destructive actions */
  --accent-red: #f85149;
  /* Secondary role indicator (agent pane distinction) */
  --accent-purple: #bc8cff;

  /* ── Text ────────────────────────────────────────── */

  /* Primary body text, headings */
  --text-primary: #c9d1d9;
  /* Secondary text — labels, timestamps, helper text */
  --text-secondary: #8b949e;
  /* Tertiary text — placeholders, disabled labels */
  --text-tertiary: #6e7681;
  /* Inverse text — white on accent backgrounds */
  --text-inverse: #ffffff;
  /* Link text — uses accent-blue by default */
  --text-link: var(--accent-blue);

  /* ── Borders ─────────────────────────────────────── */

  /* Default border — cards, tables, inputs */
  --border-default: #30363d;
  /* Muted border — subtle dividers, separator lines */
  --border-muted: #21262d;
  /* Accent border — focus rings, active selections */
  --border-accent: var(--accent-blue);

  /* ── Shadows ─────────────────────────────────────── */

  /* Small — cards, buttons */
  --shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.3);
  /* Medium — dropdowns, modals */
  --shadow-md: 0 4px 12px rgba(0, 0, 0, 0.4);
  /* Large — full-screen overlays, drawers */
  --shadow-lg: 0 8px 24px rgba(0, 0, 0, 0.5);
}
```

### Color Usage Reference

| Element | Token | Example Value |
|---------|-------|---------------|
| Page body `background` | `--bg-page` | `#0d1117` |
| Card `background` | `--bg-surface` | `#161b22` |
| Card `border` | `--border-default` | `#30363d` |
| Input `background` | `--bg-input` | `#0d1117` |
| Input `border` (default) | `--border-default` | `#30363d` |
| Input `border` (focus) | `--border-accent` | `#58a6ff` |
| Input `border` (error) | `--accent-red` | `#f85149` |
| Primary button `background` | `--accent-blue` | `#58a6ff` |
| Primary button `color` | `--text-inverse` | `#ffffff` |
| Success indicator | `--accent-green` | `#3fb950` |
| Warning/pending indicator | `--accent-orange` | `#f0883e` |
| Error indicator | `--accent-red` | `#f85149` |
| Heading text `color` | `--text-primary` | `#c9d1d9` |
| Body text `color` | `--text-primary` | `#c9d1d9` |
| Label/helper text `color` | `--text-secondary` | `#8b949e` |
| Placeholder `color` | `--text-tertiary` | `#6e7681` |
| Table divider | `--border-muted` | `#21262d` |

## Typography Tokens

### Font Families

```css
:root {
  /* System font stack — primary interface typeface */
  --font-sans: -apple-system, BlinkMacSystemFont, 'Segoe UI',
               Roboto, Oxygen, Ubuntu, Cantarell,
               'Helvetica Neue', Arial, sans-serif;

  /* Monospace font stack — code, metrics, identifiers */
  --font-mono: 'SF Mono', 'Fira Code', 'Cascadia Code',
               'JetBrains Mono', Menlo, Consolas, monospace;
}
```

### Type Scale

| Token | Size | Weight | Line Height | Usage |
|-------|------|--------|-------------|-------|
| `--text-xs` | 12px | 400 | 1.4 | Captions, timestamps, footnotes |
| `--text-sm` | 14px | 400 | 1.5 | Body text, table cells, nav labels |
| `--text-base` | 16px | 400 | 1.6 | Default body text (minimal use) |
| `--text-lg` | 18px | 500 | 1.5 | Card titles, section subheadings |
| `--text-xl` | 20px | 600 | 1.4 | Section headings (h2) |
| `--text-2xl` | 24px | 700 | 1.3 | Page headings (h1) |
| `--text-3xl` | 32px | 700 | 1.3 | Hero titles, empty state headings |

### Font Weight Aliases

```css
--weight-normal: 400;
--weight-medium: 500;
--weight-semibold: 600;
--weight-bold: 700;
```

### Heading-to-Token Mapping

| Element | Token | Weight |
|---------|-------|--------|
| `<h1>` | `--text-2xl` | 700 |
| `<h2>` | `--text-xl` | 600 |
| `<h3>` | `--text-lg` | 500 |
| `<h4>` | `--text-base` | 600 (semibold) |
| Card title | `--text-lg` | 500 |
| Metric value | `--text-2xl`, `--font-mono` | 600 |
| Metric label | `--text-sm` | 400, `--text-secondary` |

## Spacing Tokens

```css
:root {
  --spacing-xs: 4px;    /* Tight inner padding */
  --spacing-sm: 8px;    /* Dense inner padding, icon gaps */
  --spacing-md: 12px;   /* Standard inner padding */
  --spacing-lg: 16px;   /* Card padding, element gaps */
  --spacing-xl: 24px;   /* Section gaps, card padding (generous) */
  --spacing-2xl: 32px;  /* Page section spacing */
  --spacing-3xl: 48px;  /* Large page breaks */
}
```

### Spacing Usage Reference

| Context | Token |
|---------|-------|
| Card body padding | `--spacing-xl` |
| Button horizontal padding | `--spacing-lg` |
| Button vertical padding | `--spacing-sm` |
| Nav item padding | `--spacing-md --spacing-lg` |
| Content area horizontal padding (desktop) | `--spacing-2xl` |
| Content area horizontal padding (<768px) | `--spacing-lg` |
| Gap between cards in grid | `--spacing-xl` |
| Gap between form fields | `--spacing-lg` |
| Margin below headings | `--spacing-lg` |
| Section divider margin | `--spacing-2xl` |
| Table cell padding | `--spacing-sm --spacing-md` |
| Badge horizontal padding | `--spacing-sm` |
| Badge vertical padding | `--spacing-xs` |

## Border Radius Tokens

```css
:root {
  --radius-sm: 4px;     /* Badges, small indicators */
  --radius-md: 6px;     /* Buttons, inputs */
  --radius-lg: 8px;     /* Cards, panels */
  --radius-xl: 12px;    /* Modals, dropdown containers */
  --radius-full: 9999px; /* Pills, avatars */
}
```

## Shadow Tokens

```css
:root {
  --shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.3);     /* Cards (subtle) */
  --shadow-md: 0 4px 12px rgba(0, 0, 0, 0.4);    /* Dropdowns, modals */
  --shadow-lg: 0 8px 24px rgba(0, 0, 0, 0.5);    /* Sidebar overlay, full-screen */
}
```

## Transition Tokens

```css
:root {
  --transition-fast: 150ms ease;
  --transition-normal: 200ms ease;
}
```

### Transition Usage

| Element | Property | Duration | Timing |
|---------|----------|----------|--------|
| Card hover | background-color | 150ms | ease |
| Button hover | background-color, opacity | 150ms | ease |
| Sidebar collapse | width, transform | 200ms | ease |
| Focus ring | box-shadow | 150ms | ease |
| Skeleton shimmer | background-position | 1.5s | linear (infinite) |

## Skeleton Shimmer Keyframes

```css
@keyframes shimmer {
  0% { background-position: -200% 0; }
  100% { background-position: 200% 0; }
}

@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.5; }
}

/* Respect reduced motion preferences */
@media (prefers-reduced-motion: reduce) {
  .skeleton {
    animation: none;
    opacity: 0.6;
  }
}
```

## Responsive Breakpoints

```css
/* Tablet / small desktop — sidebar collapses */
@custom-media --sidebar-collapsed (max-width: 1199px);

/* Mobile — sidebar hidden, overlay nav */
@custom-media --mobile (max-width: 767px);

/* Wide desktop — full layout */
@custom-media --wide (min-width: 1200px);
```

### Breakpoint Behavior

| Breakpoint | Sidebar | Content Grid |
|------------|---------|--------------|
| ≥1200px | Expanded (240px) | Max 4 columns |
| 768–1199px | Collapsed (64px, icon-only) | Max 2 columns |
| <768px | Hidden (overlay toggle) | Single column |

## Z-Index Tokens (for future use)

```css
:root {
  --z-sidebar: 100;
  --z-overlay: 200;
  --z-modal: 300;
  --z-tooltip: 400;
  --z-toast: 500;
}
```
