# Delta for Components

## ADDED Requirements

### Requirement: Card

The Card component SHALL be the fundamental container component used throughout the UI. It SHALL use CSS custom properties from `tokens.css` — no hardcoded values.

The Card component MUST include the following CSS classes: `.card` (container), `.card--interactive` (hover-enabled state), `.card__header` (flex row, items center, space-between), `.card__body` (flex column with gap `--spacing-md`), `.card__footer` (flex row with border-top `1px solid --border-muted`), `.card__title` (`--text-lg`, `--text-primary`, `--weight-medium`), and `.card__meta` (`--text-sm`, `--text-secondary`).

Card properties SHALL use: background `--bg-surface`, border `1px solid --border-default`, border-radius `--radius-lg` (8px), box-shadow `--shadow-sm`, padding (body) `--spacing-xl` (24px).

Interactive cards SHALL show `background: var(--bg-surface-hover)` on hover with `transition: background-color var(--transition-fast)`.

The Card CSS SHALL be implemented and the Rust (Leptos) code SHALL use the `.card--interactive` class directly in page views. No standalone `<Card>` component wrapper is required — card slots (header/body/footer) SHALL be used as CSS classes.

#### Scenario: Default card renders with all slots
- GIVEN a page containing a card component
- WHEN the card is rendered with a header, body, and footer
- THEN the `.card` container SHALL display with the surface background, default border, large border-radius, and subtle shadow; the `.card__header` SHALL be a flex row with space-between alignment; the `.card__body` SHALL be a flex column with medium spacing gap; and the `.card__footer` SHALL be a flex row separated by a muted top border

#### Scenario: Interactive card responds to hover
- GIVEN a card with the `.card--interactive` class
- WHEN the user hovers over the card
- THEN the card background SHALL transition to `--bg-surface-hover` and the cursor SHALL change to pointer

### Requirement: Button

The Button component MUST provide a base `.btn` class with shared properties and semantic variant classes for different use cases.

Base button properties SHALL include: `display: inline-flex`, `align-items: center`, `gap: var(--spacing-sm)`, `padding: var(--spacing-sm) var(--spacing-lg)` (8px 16px), `border-radius: var(--radius-md)` (6px), `font-family: var(--font-sans)`, `font-size: var(--text-sm)`, `font-weight: var(--weight-medium)`, `cursor: pointer`, `transition: all var(--transition-fast)`, `line-height: 1.4`, `white-space: nowrap`, `text-decoration: none`.

Focus-visible SHALL show `box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent-blue) 30%, transparent)` with `outline: none`.

Disabled buttons (`.btn:disabled`, `.btn--disabled`) SHALL have `opacity: 0.4`, `cursor: not-allowed`, `pointer-events: none`.

The following semantic variants MUST be implemented:
- `.btn--primary`: background `--accent-blue`, color `--text-inverse`, hover `filter: brightness(1.1)`, active `filter: brightness(0.95)`
- `.btn--success`: background `--accent-green`, color `--text-inverse`, hover `filter: brightness(1.1)`, active `filter: brightness(0.95)`
- `.btn--danger`: background `--accent-red`, color `--text-inverse`, hover `filter: brightness(1.1)`, active `filter: brightness(0.95)`
- `.btn--secondary`: transparent background, color `--text-primary`, border `1px solid --border-default`, hover `background: --bg-surface-hover`, active `background: --bg-surface`
- `.btn--ghost`: transparent background, color `--text-secondary`, no border, hover `background: --bg-surface-hover` and `color: --text-primary`, active `background: --bg-surface`

The following size variants MUST be implemented:
- `.btn--sm`: padding `--spacing-xs --spacing-md`, font-size `--text-xs`
- `.btn--md` (default): padding `--spacing-sm --spacing-lg`, font-size `--text-sm`
- `.btn--lg`: padding `--spacing-md --spacing-xl`, font-size `--text-base`

Additional classes: `.btn--full` SHALL set `width: 100%`. `.btn__icon` SHALL render at `font-size: var(--text-lg)` with `line-height: 1`. `.btn__label` SHALL render with `line-height: 1`.

The Button CSS SHALL be implemented and the Rust (Leptos) code SHALL use `.btn`, `.btn--primary`, `.btn--secondary`, `.btn--ghost`, and `.btn--sm` throughout the application.

#### Scenario: Primary button renders with correct colors
- GIVEN a button element with class `btn btn--primary`
- WHEN the button is rendered on the page
- THEN the button SHALL display with `--accent-blue` background, `--text-inverse` text color, hover filter brightness 1.1, and active filter brightness 0.95

#### Scenario: Disabled button prevents interaction
- GIVEN a button with the `disabled` attribute or `.btn--disabled` class
- WHEN the button is rendered
- THEN the button SHALL show opacity 0.4, cursor not-allowed, and pointer-events none

#### Scenario: Small button applies reduced sizing
- GIVEN a button with class `btn btn--sm`
- WHEN the button is rendered
- THEN the button SHALL use `--spacing-xs --spacing-md` for padding and `--text-xs` for font size

### Requirement: Form Input

The Form Input component MUST provide labeled input fields with helper text and error states using the BEM convention `.form-field`.

The `.form-field` container SHALL have `margin-bottom: var(--spacing-lg)`.

The `.form-field__label` SHALL be a block element with color `--text-secondary`, font-size `--text-sm`, margin-bottom `--spacing-xs`, and `font-weight: var(--weight-medium)`.

The `.form-field__helper` SHALL use color `--text-tertiary`, font-size `--text-xs`, and margin-top `--spacing-xs`.

The `.form-field__error` SHALL use color `--accent-red`, font-size `--text-xs`, margin-top `--spacing-xs`, and SHALL be hidden (`display: none`) by default. When the parent has `.form-field--error`, the error SHALL be displayed.

Input elements (`.input`, `.select`, `.textarea`) SHALL use: `display: block`, `width: 100%`, background `--bg-input`, border `1px solid --border-default`, border-radius `--radius-md`, padding `--spacing-sm --spacing-md`, color `--text-primary`, font-size `--text-sm`, font-family `--font-sans`, and transitions on `border-color` and `box-shadow`.

Focus state SHALL show: `outline: none`, `border-color: var(--accent-blue)`, `box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent-blue) 20%, transparent)`.

Disabled state SHALL show `opacity: 0.4` and `cursor: not-allowed`.

Read-only state SHALL show `background: transparent`, `border-color: transparent`, and `cursor: default`.

Placeholder text SHALL use color `--text-tertiary`.

The `.select` element SHALL use `appearance: none` with a custom SVG chevron via `background-image`, `background-position: right 10px center`, and `padding-right: 32px`. The `.select--sm` variant SHALL use reduced sizing.

The `.textarea` SHALL have `min-height: 80px`, `resize: vertical`, and `line-height: 1.5`.

Error state (`.form-field--error`) SHALL cause `.input`, `.select`, and `.textarea` children to have `border-color: var(--accent-red)` and display the `.form-field__error` element.

The `.checkbox-label` SHALL be an inline-flex element with gap `--spacing-sm`, cursor pointer, border `1px solid --border-default`, border-radius `--radius-sm`, and hover state that changes border-color to `--accent-blue`. The checkbox input SHALL use custom styling with `appearance: none`, a custom checkmark SVG on `:checked`, and `--accent-blue` background when checked. `.checkbox-label--disabled` SHALL reduce opacity and disable hover effects.

The `.checkbox-group` SHALL display as flex with `gap: var(--spacing-sm)` and `flex-wrap: wrap`.

The Form Input CSS SHALL be implemented and the Rust (Leptos) code SHALL use `.input`, `.select`, `.textarea`, and `.checkbox-label` in page views (e.g., `NewRunPage`).

#### Scenario: Input field shows focus ring
- GIVEN an input element with class `.input`
- WHEN the input receives focus
- THEN the input SHALL show `border-color: var(--accent-blue)` and a `box-shadow` focus ring of 3px with 20% blue

#### Scenario: Form field shows validation error
- GIVEN a form-field with the `.form-field--error` class and an input child with invalid content
- WHEN the error state is applied
- THEN the input border SHALL change to `--accent-red` and the hidden `.form-field__error` element SHALL become visible with red text

### Requirement: Badge / Status Indicator

The Badge component MUST provide a visual status indicator with a colored dot and label, using the `.badge` BEM convention.

The `.badge` base SHALL be `inline-flex`, `align-items: center`, with `gap: var(--spacing-xs)`, padding `--spacing-xs --spacing-sm`, border-radius `--radius-sm`, font-size `--text-xs`, font-weight `--weight-medium`, background `--bg-surface`, border `1px solid --border-default`, and `line-height: 1`.

The `.badge__dot` SHALL be `8px` square with `border-radius: 50%` and `display: inline-block`.

The `.badge__label` SHALL have `line-height: 1`.

The following semantic variants MUST be implemented, each setting the dot color, border color, and text color:
- `.badge--success`: dot color `--accent-green`, border and text `--accent-green`
- `.badge--warning`: dot color `--accent-orange`, border and text `--accent-orange`
- `.badge--danger`: dot color `--accent-red`, border and text `--accent-red`
- `.badge--info`: dot color `--accent-blue`, border and text `--accent-blue`
- `.badge--neutral`: dot color `--text-tertiary`, border `--border-default`, text `--text-secondary`

The pill variant `.badge--pill` SHALL use `border-radius: var(--radius-full)` and padding `--spacing-xs --spacing-md`.

A standalone dot-only variant `.badge-dot` SHALL exist with `10px` width/height, `border-radius: 50%`, and semantic variants `.badge-dot--success`, `.badge-dot--warning`, `.badge-dot--danger`, `.badge-dot--info`, `.badge-dot--neutral` applying the corresponding accent colors.

The Badge CSS SHALL be implemented and the Rust (Leptos) code SHALL use `.badge`, `.badge--success`, `.badge--warning`, `.badge--danger`, and `.badge--neutral`.

#### Scenario: Badge renders with semantic color
- GIVEN a badge element with class `badge badge--success` containing a `badge__dot` and `badge__label`
- WHEN the badge is rendered
- THEN the dot SHALL be green (`--accent-green`), the border and label text SHALL also be green, and the layout SHALL be inline-flex with items centered

#### Scenario: Pill badge uses rounded styling
- GIVEN a badge with the additional class `.badge--pill`
- WHEN the badge is rendered
- THEN the border-radius SHALL be `--radius-full` (fully rounded) and padding SHALL be `--spacing-xs --spacing-md`

### Requirement: Table

The Table component MUST provide a structured data table with sortable headers, controls, and clickable rows.

The `.table-wrapper` SHALL have `overflow-x: auto`, border `1px solid --border-default`, border-radius `--radius-lg`, and background `--bg-surface`.

The `.table-controls` SHALL be a flex row with space-between alignment, gap `--spacing-md`, padding `--spacing-md --spacing-lg`, and a bottom border `1px solid --border-muted`.

The `.table-filter` SHALL be a flex row with items centered and gap `--spacing-sm`.

The `.table-count` SHALL use font-size `--text-sm` and color `--text-secondary`.

The `.table` SHALL have `width: 100%` and `border-collapse: collapse`.

Header cells (`.table__th`) SHALL use: color `--text-secondary`, font-size `--text-sm`, font-weight `--weight-semibold`, `text-transform: uppercase`, `letter-spacing: 0.05em`, padding `--spacing-sm --spacing-md`, text-align left, border-bottom `2px solid --border-default`, `white-space: nowrap`, `user-select: none`, sticky position on top, and background `--bg-surface`.

Sortable headers (`.table__th--sortable`) SHALL have `cursor: pointer` and a hover state transitioning color to `--text-primary`. Active sort columns (`.table__th--asc`, `.table__th--desc`) SHALL use `color: var(--accent-blue)`.

The `.table__sort-icon` SHALL have `margin-left: var(--spacing-xs)` and `font-size: 12px`.

Body cells (`.table__td`) SHALL use: padding `--spacing-sm --spacing-md`, font-size `--text-sm`, color `--text-primary`, `white-space: nowrap`.

Table rows (`.table__row`) SHALL have `border-bottom: 1px solid --border-muted` and `transition: background-color var(--transition-fast)`. The last row SHALL have no bottom border. Rows SHALL show `background: --bg-surface-hover` on hover.

Clickable rows (`.table__row--clickable`) SHALL have `cursor: pointer`.

The `.table__empty` state SHALL show centered text with padding `--spacing-3xl` and color `--text-secondary`.

Table states SHALL include: loading (skeleton rows shown), empty (centered empty state message), and error (error state component replaces table).

The Table CSS SHALL be implemented and the Rust (Leptos) code SHALL use `RunTable` component with full sort logic on the home page and raw table markup in `RunDetailPage` (CSS-only sortable headers without JavaScript).

#### Scenario: Table renders with sortable headers
- GIVEN a table with sortable header columns (`.table__th--sortable`)
- WHEN the table is rendered with data rows
- THEN each sortable header SHALL have cursor pointer, uppercase stylized text, sticky positioning, and a sort icon; the active sort column SHALL show accent-blue color; and body rows SHALL show hover highlighting

#### Scenario: Clickable row navigates on click
- GIVEN a table row with the `.table__row--clickable` class
- WHEN the user hovers over the row
- THEN the cursor SHALL change to pointer and the row background SHALL highlight with `--bg-surface-hover`

### Requirement: Loading Skeleton

The Skeleton component MUST provide shimmering placeholder shapes that mirror the dimensions of expected content during loading states.

A `@keyframes shimmer` animation SHALL exist, animating `background-position` from `-200% 0` to `200% 0`.

The `.skeleton` base class SHALL use a `linear-gradient(90deg, ...)` background with `--bg-surface` at 25%, `--bg-surface-hover` at 50%, and `--bg-surface` at 75%. It SHALL use `background-size: 200% 100%`, `animation: shimmer 1.5s infinite`, and `border-radius: var(--radius-md)`.

The following skeleton shape variants MUST be provided:
- `.skeleton--text`: height 14px, width 100%, margin-bottom `--spacing-sm`
- `.skeleton--heading`: height 24px, width 40%, margin-bottom `--spacing-md`
- `.skeleton--card`: height 120px, width 100%
- `.skeleton--metric`: height 80px, width 200px
- `.skeleton--avatar`: height 24px, width 24px, border-radius 50%
- `.skeleton--table-row`: height 32px, width 100%, margin-bottom `--spacing-xs`

Skeleton containers MUST receive `role="status"` and `aria-label="Loading..."` for screen reader accessibility.

For users who prefer reduced motion, a `@media (prefers-reduced-motion: reduce)` query SHALL disable the shimmer animation and set opacity to `0.6`.

A secondary `@keyframes pulse` animation SHALL also be defined for alternative use with opacity oscillation.

The Skeleton CSS SHALL be implemented and the Rust (Leptos) code SHALL use `.skeleton`, `.skeleton--metric`, `.skeleton--card`, and `.skeleton--text` in all pages.

#### Scenario: Skeleton card renders with shimmer animation
- GIVEN a container div with class `.skeleton.skeleton--card`, `role="status"`, and `aria-label="Loading..."`
- WHEN the page is in a loading state
- THEN the skeleton SHALL display at 120px height with full width, a shimmer gradient animation sweeping left to right, and the container SHALL be announced as a loading status to screen readers

#### Scenario: Reduced motion disables shimmer
- GIVEN a skeleton element on a page
- WHEN the user has `prefers-reduced-motion: reduce` set in their system preferences
- THEN the shimmer animation SHALL be disabled and the skeleton SHALL render with opacity 0.6 and no animation

### Requirement: Progress Bar

The Progress Bar component MUST provide a visual progress indicator with a track and fill bar.

The `.progress` container SHALL have `width: 100%`.

The `.progress__track` SHALL have `width: 100%`, height `8px`, background `--border-muted`, border-radius `--radius-full`, and `overflow: hidden`.

The `.progress__fill` SHALL have `height: 100%`, background `--accent-blue`, border-radius `--radius-full`, and `transition: width var(--transition-normal)`.

The `.progress__label` SHALL be a block element with font-size `--text-sm`, color `--text-secondary`, margin-top `--spacing-xs`, and `text-align: right`.

The following state variants MUST be implemented on `.progress`:
- `.progress--complete`: fill background SHALL change to `--accent-green`
- `.progress--error`: fill background SHALL change to `--accent-red`
- `.progress--indeterminate`: fill SHALL use a `repeating-linear-gradient` with `--accent-blue` stripes and a `shimmer` animation

The progress bar container SHALL use `role="progressbar"` with `aria-valuenow`, `aria-valuemin`, and `aria-valuemax` attributes for accessibility.

The Progress Bar CSS SHALL be implemented.

#### Scenario: Progress bar shows determinate progress
- GIVEN a `div.progress` element with `role="progressbar"`, `aria-valuenow="5"`, `aria-valuemin="0"`, `aria-valuemax="12"`
- WHEN the component is rendered
- THEN the track SHALL be 8px tall with muted background, the fill SHALL be blue (`--accent-blue`) at the corresponding width percentage, and the label SHALL display the fraction below the bar

#### Scenario: Progress bar shows completed state
- GIVEN a progress bar with the `.progress--complete` class
- WHEN the fill reaches 100%
- THEN the fill background SHALL change to `--accent-green`

### Requirement: Error State

The Error State component MUST provide a centered error display with icon, heading, message, and action button.

The `.error-state` SHALL be a flex column with centered alignment, padding `--spacing-3xl`, and text-align center. The container SHALL use `role="alert"`.

The `.error-state__icon` SHALL have `font-size: 48px`, `line-height: 1`, and `margin-bottom: var(--spacing-xl)`.

The `.error-state__heading` SHALL use `font-size: var(--text-lg)`, color `--text-primary`, and `margin-bottom: var(--spacing-sm)`.

The `.error-state__message` SHALL use `font-size: var(--text-sm)`, color `--text-secondary`, `max-width: 400px`, `text-align: center`, and `margin-bottom: var(--spacing-xl)`.

The `.error-state__action` SHALL have `margin-top: var(--spacing-xl)`.

The Error State CSS SHALL be implemented and the Rust (Leptos) code SHALL use `.error-state` in all pages with retry buttons.

#### Scenario: Error state displays with retry action
- GIVEN a div with class `.error-state` and `role="alert"` containing icon, heading, message, and action elements
- WHEN a data loading failure occurs
- THEN the error state SHALL render centered with a 48px icon, a heading describing the failure, a secondary message explaining details, and a primary retry button in the action slot

### Requirement: Empty State

The Empty State component MUST provide a centered empty-state display with icon, heading, message, and call-to-action button.

The `.empty-state` SHALL be a flex column with centered alignment, padding `--spacing-3xl`, and text-align center.

The `.empty-state__icon` SHALL have `font-size: 48px`, `line-height: 1`, and `margin-bottom: var(--spacing-xl)`.

The `.empty-state__heading` SHALL use `font-size: var(--text-lg)`, color `--text-primary`, and `margin-bottom: var(--spacing-sm)`.

The `.empty-state__message` SHALL use `font-size: var(--text-sm)`, color `--text-secondary`, `max-width: 400px`, `text-align: center`, and `margin-bottom: var(--spacing-xl)`.

The `.empty-state__action` SHALL have `margin-top: var(--spacing-xl)`.

The Empty State CSS SHALL be implemented. The Rust (Leptos) code SHALL use the empty state inline in `HomePage` — basic implementation without an icon is acceptable.

#### Scenario: Empty state displays with call to action
- GIVEN a div with class `.empty-state` containing a heading, message, and action elements
- WHEN a data view has no records to display
- THEN the empty state SHALL render centered with a heading describing the absence, a message with guidance, and a primary button linking to create the first record

### Requirement: Sidebar Navigation

The Sidebar Navigation component MUST provide a fixed, collapsible navigation sidebar with responsive breakpoints for desktop, tablet, and mobile viewports.

The `.sidebar` SHALL be `position: fixed`, `top: 0`, `left: 0`, have a default expanded width of `240px`, `height: 100vh`, background `--bg-surface`, border-right `1px solid --border-default`, flex column layout, z-index `--z-sidebar`, and transitions on `width` and `transform` using `--transition-normal`. It SHALL have `overflow: hidden`.

The collapsed state (`.sidebar--collapsed`) SHALL reduce width to `64px`.

The `.sidebar__header` SHALL be a flex row with items centered, gap `--spacing-md`, padding `--spacing-lg`, bottom border `1px solid --border-muted`, and `min-height: 56px`.

The `.sidebar__toggle` SHALL be a flex-shrink-0 button at 32x32px with border-radius `--radius-md` and hover background `--bg-surface-hover`.

The `.sidebar__brand` SHALL use `--text-base`, `font-weight: --weight-bold`, color `--text-primary`, `white-space: nowrap`, `overflow: hidden`.

The `.sidebar__nav` SHALL be a flex-1 scrolling column with padding `--spacing-sm` and 2px gap between items.

The `.sidebar__item` SHALL be a flex row with gap `--spacing-md`, padding `--spacing-md --spacing-lg`, border-radius `--radius-md`, color `--text-secondary`, text-decoration none, transition on all properties, and relative positioning. Hover SHALL show `background: --bg-surface-hover` and `color: --text-primary`.

The active item (`.sidebar__item--active`) SHALL have `background: --bg-surface-raised`, `color: --accent-blue`, and a `::before` pseudo-element creating a 3px blue left border indicator.

The `.sidebar__icon` SHALL be `font-size: 18px`, `width: 24px`, text-aligned center, flex-shrink-0.

The `.sidebar__label` SHALL use `font-size: --text-sm`, `font-weight: --weight-medium`, `overflow: hidden`, `text-overflow: ellipsis`.

The `.sidebar__footer` SHALL have padding `--spacing-md --spacing-lg` and a top border `1px solid --border-muted`. `.sidebar__version` SHALL use `--text-xs` and `--text-tertiary` color.

When collapsed, `.sidebar--collapsed` SHALL hide `.sidebar__brand`, `.sidebar__label`, `.sidebar__version`, and `.badge--pill` (with `display: none`). Items SHALL be center-justified with padding `--spacing-md`.

In collapsed mode, hover on `.sidebar__item` SHALL show a tooltip via `::after` pseudo-element using the `data-tooltip` attribute. The tooltip SHALL position to the right of the item with background `--bg-surface-raised`, border, shadow, and pointer-events none.

The `.sidebar__hamburger` SHALL be hidden by default (`display: none`). On mobile it SHALL be a fixed-position button with z-index `--z-overlay`.

The `.sidebar-overlay` SHALL be a fixed full-screen semi-transparent backdrop (`rgba(0, 0, 0, 0.5)`) with z-index `--z-overlay`, hidden by default and shown with `.sidebar-overlay--open`.

Responsive breakpoints SHALL be:
- Tablet (768px–1199px): Sidebar defaults to collapsed (64px). The toggle can expand it to 240px with `.sidebar:not(.sidebar--collapsed)`. All labels hidden by default.
- Mobile (≤767px): Sidebar slides off-screen via `transform: translateX(-100%)`. Width becomes 280px when open. The `.sidebar--mobile-open` class slides it into view with z-index 1000. The toggle button inside sidebar SHALL be hidden; the hamburger button SHALL be visible. Overlay SHALL be shown when sidebar is open.

The Sidebar Navigation CSS SHALL be implemented and the Rust (Leptos) code SHALL implement the sidebar in `app.rs` with collapse/expand functionality and mobile overlay support.

#### Scenario: Sidebar collapses to icon-only mode
- GIVEN a sidebar with the `.sidebar--collapsed` class
- WHEN the sidebar is toggled to collapsed state
- THEN the sidebar width SHALL reduce to 64px, labels and brand SHALL be hidden, navigation items SHALL be center-justified, and hovering over an item SHALL display a tooltip via the `data-tooltip` attribute

#### Scenario: Sidebar shows mobile hamburger overlay
- GIVEN a viewport width of 767px or less
- WHEN the page is rendered
- THEN the sidebar SHALL be hidden off-screen by default, a hamburger button SHALL be visible at the top-left, and clicking the hamburger SHALL slide in the sidebar with a dark overlay backdrop

### Requirement: Metric Card

The Metric Card component MUST provide a compact display card for numerical metrics with optional delta indicators.

The `.metric-card` SHALL use background `--bg-surface`, border `1px solid --border-default`, border-radius `--radius-lg`, padding `--spacing-xl`, and `text-align: center`.

The `.metric-card__label` SHALL use `font-size: var(--text-sm)`, color `--text-secondary`, and `margin-bottom: var(--spacing-xs)`.

The `.metric-card__value` SHALL use `font-size: var(--text-2xl)`, `font-family: var(--font-mono)`, `font-weight: var(--weight-semibold)`, color `--text-primary`, and `line-height: 1.3`.

The `.metric-card__delta` SHALL use `font-size: var(--text-xs)`, color `--text-secondary`, and `margin-top: var(--spacing-xs)`. Positive delta (`.metric-card__delta--positive`) SHALL use `color: var(--accent-green)`. Negative delta (`.metric-card__delta--negative`) SHALL use `color: var(--accent-red)`.

The `.metric-card__subtitle` SHALL use `font-size: var(--text-xs)`, color `--text-tertiary`, and `margin-top: var(--spacing-xs)`.

The Metric Card CSS SHALL be implemented.

#### Scenario: Metric card displays value with delta
- GIVEN a div with class `.card.metric-card` containing label, value, and delta elements
- WHEN the card is rendered
- THEN the value SHALL be displayed in large monospace font with semibold weight, the delta SHALL appear below in green (positive) or red (negative) text

### Requirement: Agent Pane (Live View)

The Agent Pane component MUST provide a live-status container for displaying agent activity with status-based border colors.

The `.agent-pane` SHALL have background `--bg-surface`, a `2px solid` border, border-radius `--radius-lg`, flex column layout, `min-height: 200px`, `max-height: 350px`, `overflow: hidden`, and `transition: border-color var(--transition-normal)`.

Status-based border colors SHALL be:
- `.agent-pane--pending`: border-color `--border-default`
- `.agent-pane--running`: border-color `--accent-blue`
- `.agent-pane--completed`: border-color `--accent-green`
- `.agent-pane--failed`: border-color `--accent-red`

The `.agent-pane__header` SHALL be a flex row with centered items, gap `--spacing-sm`, padding `--spacing-sm --spacing-md`, bottom border `1px solid --border-muted`, font-size `--text-sm`, font-weight `--weight-semibold`, and `flex-shrink: 0`.

The `.agent-pane__role` SHALL use color `--text-primary`.

The `.agent-pane__status` SHALL use color `--text-secondary`, `font-weight: var(--weight-normal)`, `font-size: var(--text-xs)`, and `margin-left: auto`.

The `.agent-pane__content` SHALL be flex-1 with padding `--spacing-md`, font-size `--text-sm`, font-family `--font-mono`, color `--text-primary`, `overflow-y: auto`, `line-height: 1.5`, `white-space: pre-wrap`, `word-break: break-word`.

The `.agent-pane__footer` SHALL have padding `--spacing-xs --spacing-md`, top border `1px solid --border-muted`, font-size `--text-xs`, color `--text-secondary`, and `flex-shrink: 0`.

The Agent Pane CSS SHALL be implemented.

#### Scenario: Agent pane shows running state with blue border
- GIVEN an agent-pane with class `.agent-pane--running` containing header, content, and footer
- WHEN the agent is actively processing
- THEN the container SHALL display a blue (`--accent-blue`) 2px border, the header SHALL show the agent role and status, the content area SHALL show streaming monospace text with scroll, and the footer SHALL show findings count

### Requirement: Form Section

The Form Section component MUST provide a titled visual divider for grouping related form fields.

The `.form-section` SHALL have `margin-bottom: var(--spacing-2xl)`.

The `.form-section__title` SHALL use font-size `--text-xs`, font-weight `--weight-semibold`, color `--text-secondary`, `text-transform: uppercase`, `letter-spacing: 0.5px`, padding-bottom `--spacing-md`, margin-bottom `--spacing-xl`, and a bottom border `1px solid --border-muted`.

The `.form-section__fields` SHALL be a flex column with `gap: var(--spacing-lg)`.

The Form Section CSS SHALL be implemented.

#### Scenario: Form section groups fields under a divider
- GIVEN a `section.form-section` element with a title and fields container
- WHEN the section is rendered on a form page
- THEN the title SHALL appear as an uppercase, secondary-colored label above a muted bottom border divider, and the fields SHALL be stacked below with large spacing gaps

### Requirement: Slider (Range Input)

The Slider component MUST provide a custom-styled range input with a track and thumb, along with a companion display value.

The `.slider` input SHALL use `-webkit-appearance: none`, `appearance: none`, `width: 100%`, height `6px`, background `--border-muted`, border-radius `--radius-full`, and `cursor: pointer`.

The slider track (WebKit) SHALL be `6px` tall with `--border-muted` background and `--radius-full` border-radius.

The slider thumb (WebKit) SHALL be `18px` square with `border-radius: 50%`, background `--accent-blue`, border `2px solid --text-inverse`, cursor pointer, and `margin-top: -6px`. Hover on the thumb SHALL show a `box-shadow` focus ring with 30% blue.

Moz variants SHALL be provided with equivalent styling.

The `.slider-field` SHALL have `margin-bottom: var(--spacing-lg)`.

The `.slider-field__control` SHALL be a flex row with centered items and `gap: var(--spacing-md)`.

The `.slider-field__value` SHALL have `min-width: 28px`, `text-align: center`, font-family `--font-mono`, font-size `--text-sm`, font-weight `--weight-semibold`, and color `--accent-blue`.

The Slider CSS SHALL be implemented. The Rust (Leptos) code currently uses `<input type="number">` instead of range inputs — the slider CSS is defined but NOT yet used in Rust.

#### Scenario: Slider renders with custom thumb and track
- GIVEN a range input with class `.slider` inside a `.slider-field` control
- WHEN the slider is rendered
- THEN the track SHALL be a 6px muted bar with full rounding, the thumb SHALL be an 18px blue circle with white border, and the current value SHALL be displayed next to the slider in monospace blue text

### Requirement: Search / Filter Bar

The Search / Filter Bar component MUST provide an inline search input paired with a filter dropdown for data filtering.

The `.search-bar` SHALL be a flex row with centered items and `gap: var(--spacing-sm)`.

The `.search-bar__input-wrapper` SHALL be position relative with flex display and centered items.

The `.search-bar__icon` SHALL be absolutely positioned at `left: var(--spacing-sm)`, with `font-size: 14px`, color `--text-tertiary`, and `pointer-events: none`.

The `.search-bar__input` SHALL have `padding-left: 32px !important`.

The Search / Filter Bar CSS SHALL be implemented.

#### Scenario: Search bar shows icon and filter dropdown
- GIVEN a div with class `.search-bar` containing an input wrapper with icon and an adjacent select filter
- WHEN the search bar is rendered
- THEN the search icon SHALL be positioned inside the left side of the input field, the input SHALL have left padding to accommodate the icon, and a filter dropdown SHALL appear next to the input with equal vertical alignment
