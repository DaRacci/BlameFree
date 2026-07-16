# Delta for Design Tokens

## ADDED Requirements

### Requirement: Background Colors

The application SHALL define five background color tokens as CSS custom properties on `:root`. These tokens establish a dark-theme color hierarchy with four distinct surface levels plus an input-specific variant. All background tokens SHALL use hex color values with no `var()` references.

| Token | Value | Purpose |
|-------|-------|---------|
| `--bg-page` | `#0d1117` | Main page background — darkest level |
| `--bg-surface` | `#161b22` | Card, sidebar, and panel surfaces — one level above page |
| `--bg-surface-hover` | `#1c2333` | Hover state for interactive surfaces |
| `--bg-surface-raised` | `#1c2128` | Elevated surfaces — modals, dropdowns, popovers |
| `--bg-input` | `#0d1117` | Form input backgrounds |

The background hierarchy SHALL maintain the following luminance relationship: `--bg-page` SHALL be the darkest (`#0d1117`), `--bg-surface` SHALL be lighter (`#161b22`), `--bg-surface-hover` SHALL be lighter still (`#1c2333`), and `--bg-surface-raised` SHALL be the lightest of the surface tokens (`#1c2128`). `--bg-input` SHALL match `--bg-page` to create a seamless inset appearance.

#### Scenario: Surface background applied to cards
- GIVEN a card component
- WHEN the card is rendered
- THEN its `background` SHALL be `var(--bg-surface)`
- AND its hover state SHALL use `var(--bg-surface-hover)`

#### Scenario: Input background applied to form fields
- GIVEN a form input component
- WHEN the input is rendered in its default state
- THEN its `background` SHALL be `var(--bg-input)`
- AND it SHALL appear inset relative to the surrounding surface

---

### Requirement: Accent Colors

The application SHALL define five accent color tokens as CSS custom properties on `:root`. These tokens SHALL provide semantic color for interactive states, status indicators, and role differentiation. All accent tokens SHALL use raw hex color values with no `var()` references.

| Token | Value | Purpose |
|-------|-------|---------|
| `--accent-blue` | `#58a6ff` | Primary actions, links, active/focus indicators |
| `--accent-green` | `#3fb950` | Success states, completed runs, positive metrics |
| `--accent-orange` | `#f0883e` | Warning states, in-progress runs, medium metrics |
| `--accent-red` | `#f85149` | Error states, failures, negative metrics, destructive actions |
| `--accent-purple` | `#bc8cff` | Secondary role indicator (agent pane distinction) |

#### Scenario: Accent color applied to primary button
- GIVEN a primary button component
- WHEN the button is rendered in its default state
- THEN its `background` SHALL be `var(--accent-blue)`
- AND its `color` SHALL be `var(--text-inverse)`

#### Scenario: Accent colors used for status indicators
- GIVEN a status indicator component
- WHEN the status is "success"
- THEN the indicator SHALL use `var(--accent-green)`
- WHEN the status is "warning" or "in-progress"
- THEN the indicator SHALL use `var(--accent-orange)`
- WHEN the status is "error" or "failure"
- THEN the indicator SHALL use `var(--accent-red)`

---

### Requirement: Text Colors

The application SHALL define five text color tokens as CSS custom properties on `:root`. These tokens SHALL provide a four-level text hierarchy plus a link color that references the accent palette. Tokens SHALL use either hex color values or `var()` references to other custom properties.

| Token | Value | Purpose |
|-------|-------|---------|
| `--text-primary` | `#c9d1d9` | Primary body text, headings |
| `--text-secondary` | `#8b949e` | Secondary text — labels, timestamps, helper text |
| `--text-tertiary` | `#6e7681` | Tertiary text — placeholders, disabled labels |
| `--text-inverse` | `#ffffff` | Inverse text — white on accent backgrounds |
| `--text-link` | `var(--accent-blue)` | Link text — references accent-blue by default |

The text hierarchy SHALL maintain descending luminance: `--text-primary` SHALL be the lightest (`#c9d1d9`), `--text-secondary` SHALL be medium (`#8b949e`), and `--text-tertiary` SHALL be the darkest of the text tokens (`#6e7681`). `--text-link` SHALL reference `--accent-blue` via `var()` to ensure link color stays synchronized with the accent palette.

#### Scenario: Text hierarchy applied to heading and body
- GIVEN a content region containing a heading and body text
- WHEN the region is rendered
- THEN the heading `color` SHALL be `var(--text-primary)`
- AND the body text `color` SHALL be `var(--text-primary)`
- AND any label or timestamp `color` SHALL be `var(--text-secondary)`
- AND any placeholder text `color` SHALL be `var(--text-tertiary)`

#### Scenario: Link color derived from accent palette
- GIVEN a link element
- WHEN the link is rendered
- THEN its `color` SHALL be `var(--text-link)`
- AND when `--accent-blue` is overridden by a theme, the link color SHALL update automatically

---

### Requirement: Borders

The application SHALL define three border tokens as CSS custom properties on `:root`. These tokens SHALL provide default, muted, and accent border styles. Tokens SHALL use either hex color values or `var()` references.

| Token | Value | Purpose |
|-------|-------|---------|
| `--border-default` | `#30363d` | Default border — cards, tables, inputs |
| `--border-muted` | `#21262d` | Muted border — subtle dividers, separator lines |
| `--border-accent` | `var(--accent-blue)` | Accent border — focus rings, active selections |

#### Scenario: Default and muted borders applied to card and divider
- GIVEN a card with a divider between sections
- WHEN the card is rendered
- THEN the card `border` SHALL be `var(--border-default)`
- AND the divider `border` SHALL be `var(--border-muted)`

#### Scenario: Accent border on focus
- GIVEN a form input in focus state
- WHEN the input receives keyboard focus
- THEN its `border-color` SHALL be `var(--border-accent)`

---

### Requirement: Shadows

The application SHALL define three box-shadow tokens as CSS custom properties on `:root`. These tokens SHALL provide a three-level elevation hierarchy using `rgba()` values with increasing blur radius and opacity.

| Token | Value | Purpose |
|-------|-------|---------|
| `--shadow-sm` | `0 1px 2px rgba(0, 0, 0, 0.3)` | Small — cards, buttons |
| `--shadow-md` | `0 4px 12px rgba(0, 0, 0, 0.4)` | Medium — dropdowns, modals |
| `--shadow-lg` | `0 8px 24px rgba(0, 0, 0, 0.5)` | Large — full-screen overlays, drawers |

Shadow elevation SHALL increase monotonically: `--shadow-sm` SHALL have the smallest offset and blur, `--shadow-md` SHALL be larger, and `--shadow-lg` SHALL be the largest. Shadow opacity SHALL also increase with elevation (0.3 → 0.4 → 0.5).

#### Scenario: Shadow applied to card
- GIVEN a card component
- WHEN the card is rendered in its default state
- THEN its `box-shadow` SHALL be `var(--shadow-sm)`

#### Scenario: Shadow applied to modal
- GIVEN a modal overlay component
- WHEN the modal is rendered
- THEN its `box-shadow` SHALL be `var(--shadow-md)`

---

### Requirement: Font Families

The application SHALL define two font-family tokens as CSS custom properties on `:root`. The sans-serif token SHALL use the system font stack for optimal native rendering. The monospace token SHALL prioritize developer-oriented coding typefaces.

| Token | Value |
|-------|-------|
| `--font-sans` | `-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Oxygen, Ubuntu, Cantarell, "Helvetica Neue", Arial, sans-serif` |
| `--font-mono` | `"SF Mono", "Fira Code", "Cascadia Code", "JetBrains Mono", Menlo, Consolas, monospace` |

#### Scenario: Font families applied to interface regions
- GIVEN a UI component
- WHEN the component renders body text
- THEN its `font-family` SHALL be `var(--font-sans)`
- WHEN the component renders code, metrics, or identifiers
- THEN its `font-family` SHALL be `var(--font-mono)`

---

### Requirement: Font Weights

The application SHALL define four font-weight tokens as CSS custom properties on `:root`. These tokens SHALL provide a weight scale using numeric values conforming to the CSS `font-weight` specification.

| Token | Value | Purpose |
|-------|-------|---------|
| `--weight-normal` | `400` | Body text, default weight |
| `--weight-medium` | `500` | Emphasized body, card titles |
| `--weight-semibold` | `600` | Section headings, strong emphasis |
| `--weight-bold` | `700` | Page headings, metric values |

#### Scenario: Font weights applied to heading hierarchy
- GIVEN a heading element
- WHEN the element is an `<h1>`
- THEN its `font-weight` SHALL be `var(--weight-bold)`
- WHEN the element is an `<h2>`
- THEN its `font-weight` SHALL be `var(--weight-semibold)`
- WHEN the element is an `<h3>`
- THEN its `font-weight` SHALL be `var(--weight-medium)`

---

### Requirement: Type Scale

The application SHALL define seven type-size tokens as CSS custom properties on `:root`. These tokens SHALL provide a modular type scale from 12px (captions) to 32px (hero titles). Each token SHALL specify only `font-size` in `px` units; line-height SHALL be applied separately.

| Token | Size | Usage |
|-------|------|-------|
| `--text-xs` | `12px` | Captions, timestamps, footnotes |
| `--text-sm` | `14px` | Body text, table cells, nav labels |
| `--text-base` | `16px` | Default body text (minimal use) |
| `--text-lg` | `18px` | Card titles, section subheadings |
| `--text-xl` | `20px` | Section headings (h2) |
| `--text-2xl` | `24px` | Page headings (h1) |
| `--text-3xl` | `32px` | Hero titles, empty state headings |

The heading-to-token mapping SHALL be as follows:

| Element | Token | Weight |
|---------|-------|--------|
| `<h1>` | `--text-2xl` | `var(--weight-bold)` |
| `<h2>` | `--text-xl` | `var(--weight-semibold)` |
| `<h3>` | `--text-lg` | `var(--weight-medium)` |
| `<h4>` | `--text-base` | `var(--weight-semibold)` |
| Card title | `--text-lg` | `var(--weight-medium)` |
| Metric value | `--text-2xl`, `--font-mono` | `var(--weight-semibold)` |
| Metric label | `--text-sm` | `var(--weight-normal)`, `var(--text-secondary)` |

#### Scenario: Type scale applied to heading structure
- GIVEN a page with an `<h1>` heading and body paragraph
- WHEN the page is rendered
- THEN the `<h1>` `font-size` SHALL be `var(--text-2xl)`
- AND the `<h1>` `font-weight` SHALL be `var(--weight-bold)`
- AND the body paragraph `font-size` SHALL be `var(--text-sm)`
- AND the body paragraph `font-weight` SHALL be `var(--weight-normal)`

#### Scenario: Type scale applied to card component
- GIVEN a card component with a title
- WHEN the card is rendered
- THEN the card title `font-size` SHALL be `var(--text-lg)`
- AND the card title `font-weight` SHALL be `var(--weight-medium)`

---

### Requirement: Spacing

The application SHALL define seven spacing tokens as CSS custom properties on `:root`. These tokens SHALL provide a spacing scale from 4px (tight) to 48px (large page breaks). All spacing tokens SHALL use `px` units.

| Token | Value | Usage |
|-------|-------|-------|
| `--spacing-xs` | `4px` | Tight inner padding |
| `--spacing-sm` | `8px` | Dense inner padding, icon gaps |
| `--spacing-md` | `12px` | Standard inner padding |
| `--spacing-lg` | `16px` | Card padding, element gaps |
| `--spacing-xl` | `24px` | Section gaps, card padding (generous) |
| `--spacing-2xl` | `32px` | Page section spacing |
| `--spacing-3xl` | `48px` | Large page breaks |

The spacing usage reference SHALL be as follows:

| Context | Token(s) |
|---------|----------|
| Card body padding | `--spacing-xl` |
| Button horizontal padding | `--spacing-lg` |
| Button vertical padding | `--spacing-sm` |
| Nav item padding | `--spacing-md` (horizontal), `--spacing-lg` (vertical) |
| Content area horizontal padding (desktop) | `--spacing-2xl` |
| Content area horizontal padding (<768px) | `--spacing-lg` |
| Gap between cards in grid | `--spacing-xl` |
| Gap between form fields | `--spacing-lg` |
| Margin below headings | `--spacing-lg` |
| Section divider margin | `--spacing-2xl` |
| Table cell padding | `--spacing-sm` (vertical), `--spacing-md` (horizontal) |
| Badge horizontal padding | `--spacing-sm` |
| Badge vertical padding | `--spacing-xs` |

#### Scenario: Spacing applied to card layout
- GIVEN a card component
- WHEN the card is rendered
- THEN the card body `padding` SHALL be `var(--spacing-xl)`

#### Scenario: Spacing applied to form layout
- GIVEN a form with multiple fields
- WHEN the form is rendered
- THEN the gap between form fields SHALL be `var(--spacing-lg)`

---

### Requirement: Border Radius

The application SHALL define five border-radius tokens as CSS custom properties on `:root`. These tokens SHALL provide a radius scale from 4px (badges) to 9999px (pills) plus a `--radius-full` token for fully rounded elements.

| Token | Value | Usage |
|-------|-------|-------|
| `--radius-sm` | `4px` | Badges, small indicators |
| `--radius-md` | `6px` | Buttons, inputs |
| `--radius-lg` | `8px` | Cards, panels |
| `--radius-xl` | `12px` | Modals, dropdown containers |
| `--radius-full` | `9999px` | Pills, avatars |

#### Scenario: Border radius applied to buttons and cards
- GIVEN a button component
- WHEN the button is rendered
- THEN its `border-radius` SHALL be `var(--radius-md)`
- GIVEN a card component
- WHEN the card is rendered
- THEN its `border-radius` SHALL be `var(--radius-lg)`

#### Scenario: Full radius applied to badges
- GIVEN a badge or pill component
- WHEN the badge is rendered
- THEN its `border-radius` SHALL be `var(--radius-full)`

---

### Requirement: Transitions

The application SHALL define two transition tokens as CSS custom properties on `:root`. These tokens SHALL provide `ease` timing functions with two duration tiers: fast (150ms) and normal (200ms).

| Token | Value | Purpose |
|-------|-------|---------|
| `--transition-fast` | `150ms ease` | Card hover, button hover, focus ring transitions |
| `--transition-normal` | `200ms ease` | Sidebar collapse, layout transitions |

Transition usage SHALL follow these mappings:

| Element | Property | Token |
|---------|----------|-------|
| Card hover | `background-color` | `--transition-fast` |
| Button hover | `background-color`, `opacity` | `--transition-fast` |
| Sidebar collapse | `width`, `transform` | `--transition-normal` |
| Focus ring | `box-shadow` | `--transition-fast` |
| Skeleton shimmer | `background-position` | 1.5s linear (infinite) — this SHALL NOT use transition tokens |

#### Scenario: Transition applied to button hover
- GIVEN a button component
- WHEN the user hovers over the button
- THEN the `background-color` SHALL transition using `var(--transition-fast)`

#### Scenario: Transition applied to sidebar collapse
- GIVEN a sidebar component
- WHEN the sidebar collapses or expands
- THEN the `width` and `transform` properties SHALL transition using `var(--transition-normal)`

---

### Requirement: Z-Index

The application SHALL define five z-index tokens as CSS custom properties on `:root`. These tokens SHALL establish a stacking hierarchy with 100-point increments, reserving space for sidebar, overlay, modal, tooltip, and toast layers.

| Token | Value | Purpose |
|-------|-------|---------|
| `--z-sidebar` | `100` | Sidebar layer |
| `--z-overlay` | `200` | Overlay / backdrop layer |
| `--z-modal` | `300` | Modal dialog layer |
| `--z-tooltip` | `400` | Tooltip / popover layer |
| `--z-toast` | `500` | Toast notification layer (topmost) |

The stacking order SHALL be: `--z-sidebar` < `--z-overlay` < `--z-modal` < `--z-tooltip` < `--z-toast`. Tokens SHALL use 100-point gaps to allow intermediate values (e.g., `150`) for sub-layers within each category when needed.

#### Scenario: Z-index applied to modal and toast
- GIVEN a modal dialog and a toast notification rendered simultaneously
- WHEN both are visible
- THEN the toast `z-index` SHALL be `var(--z-toast)`
- AND the modal `z-index` SHALL be `var(--z-modal)`
- AND `--z-toast` SHALL be greater than `--z-modal`

#### Scenario: Z-index applied to sidebar
- GIVEN a sidebar component
- WHEN the sidebar is rendered
- THEN its `z-index` SHALL be `var(--z-sidebar)`

---

### Requirement: Responsive Breakpoints

The application SHALL define three responsive breakpoints using `@custom-media` rules. These breakpoints SHALL control the sidebar behavior and content grid layout across desktop, tablet, and mobile viewports.

| Breakpoint | Rule | Behavior |
|------------|------|----------|
| `--sidebar-collapsed` | `(max-width: 1199px)` | Tablet / small desktop — sidebar collapses |
| `--mobile` | `(max-width: 767px)` | Mobile — sidebar hidden, overlay nav |
| `--wide` | `(min-width: 1200px)` | Wide desktop — full layout |

The layout behavior per breakpoint SHALL be:

| Breakpoint | Sidebar | Content Grid |
|------------|---------|--------------|
| `--wide` | Expanded (240px) | Maximum 4 columns |
| `--sidebar-collapsed` | Collapsed (64px, icon-only) | Maximum 2 columns |
| `--mobile` | Hidden (overlay toggle) | Single column |

#### Scenario: Sidebar collapses on tablet viewport
- GIVEN a viewport width of 900px
- WHEN the `--sidebar-collapsed` custom media query matches
- THEN the sidebar SHALL collapse to 64px width (icon-only)
- AND the content grid SHALL show a maximum of 2 columns

#### Scenario: Sidebar hidden on mobile viewport
- GIVEN a viewport width of 600px
- WHEN the `--mobile` custom media query matches
- THEN the sidebar SHALL be hidden
- AND the content grid SHALL show a single column

---

### Requirement: Animation Keyframes

The application SHALL define two `@keyframes` animations for skeleton loading states: `shimmer` and `pulse`. The shimmer animation SHALL create a horizontal sweep effect. The pulse animation SHALL create a gentle opacity fade. Both SHALL respect the `prefers-reduced-motion` user preference.

#### `shimmer`
```css
@keyframes shimmer {
  0%   { background-position: -200% 0; }
  100% { background-position: 200% 0; }
}
```

Duration: 1.5s, `linear`, `infinite`.

#### `pulse`
```css
@keyframes pulse {
  0%, 100% { opacity: 1; }
  50%      { opacity: 0.5; }
}
```

#### Reduced Motion
```css
@media (prefers-reduced-motion: reduce) {
  .skeleton {
    animation: none;
    opacity: 0.6;
  }
}
```

When `prefers-reduced-motion: reduce` is active, skeleton elements SHALL have their animation disabled and SHALL render at opacity `0.6` to indicate loading state without motion.

#### Scenario: Skeleton shimmer animation applied
- GIVEN a skeleton loading component
- WHEN the component is rendered and motion preference is not reduced
- THEN the shimmer animation SHALL play at 1.5s linear infinite
- AND `background-position` SHALL animate from `-200% 0` to `200% 0`

#### Scenario: Reduced motion respected
- GIVEN a skeleton loading component
- WHEN the user's system has `prefers-reduced-motion: reduce` set
- THEN the skeleton SHALL have `animation: none`
- AND the skeleton SHALL have `opacity: 0.6`

---

## Implementation Notes

1. All 55 design tokens (53 CSS custom properties + 2 `@keyframes` animations + 3 `@custom-media` breakpoints) SHALL be defined in `tokens.css` at `crates/crb-webui-frontend/css/tokens.css`.
2. No hardcoded color, spacing, type, or shadow value SHALL appear outside of `tokens.css`.
3. `tokens.css` SHALL be imported statically via `styles.css`. No Rust/WASM bindings are required for token access.
4. Theme overrides MAY redefine any semantic variable (e.g., `--accent-blue`) to create alternative themes; derivative tokens using `var()` references (e.g., `--text-link: var(--accent-blue)`) SHALL update automatically.
5. The heading-to-token mapping and spacing usage reference in this delta SHALL serve as the authoritative guide for component implementation — components MUST use the specified tokens rather than arbitrary values.
