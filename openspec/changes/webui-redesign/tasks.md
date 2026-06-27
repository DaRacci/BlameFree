# Tasks: Web UI Redesign

## Phase 1: Openspec Plan ✅
- [x] Create `openspec/changes/webui-redesign/proposal.md`
- [x] Create `openspec/changes/webui-redesign/design.md`
- [x] Create `openspec/changes/webui-redesign/tasks.md`
- [x] Create `specs/design-tokens/spec.md`
- [x] Create `specs/components/spec.md`
- [x] Create `specs/pages/spec.md`

## Phase 2: Design Token System
- [ ] Create `frontend/css/tokens.css` — all CSS custom properties (colors, spacing, type, shadows, radii)
- [ ] Create `frontend/css/reset.css` — minimal CSS reset (box-sizing, margin/padding reset)
- [ ] Create `frontend/css/base.css` — base element styles (body, headings, links, code blocks)
- [ ] Create `frontend/css/styles.css` — single entry point importing all CSS modules
- [ ] Verify: `cargo check --workspace` builds successfully after CSS file additions (no Rust changes)

## Phase 3: Layout System
- [ ] Create `frontend/css/layout.css` — sidebar, main content grid, responsive breakpoints
- [ ] Create `frontend/css/components/nav.css` — sidebar navigation styles (expanded, collapsed, active states)
- [ ] Implement sidebar component in Leptos — collapsible with toggle button
- [ ] Implement mobile overlay sidebar (slide-in panel for <768px)
- [ ] Wire up `App` root layout to use sidebar + main content area
- [ ] Test responsive behavior at 3 breakpoints (≥1200px, 768–1199px, <768px)

## Phase 4: Base Components
- [ ] Create `frontend/css/components/card.css` — card with header/body/footer slots, interactive hover
- [ ] Create `frontend/css/components/button.css` — all variants (primary, success, danger, secondary, ghost)
- [ ] Create `frontend/css/components/form.css` — inputs, labels, selects, checkboxes, sliders, validation states
- [ ] Create `frontend/css/components/badge.css` — status badges with semantic colors
- [ ] Create `frontend/css/components/table.css` — sortable table with sticky header, row hover
- [ ] Create `frontend/css/components/skeleton.css` — shimmer animation, skeleton shapes
- [ ] Create `frontend/css/components/progress.css` — progress bar with label
- [ ] Refactor existing Leptos components to use new CSS classes and tokens
- [ ] Verify visual consistency across all components

## Phase 5: State Indicators
- [ ] Implement loading skeleton component for each page type (home, run detail, live view, new run form)
- [ ] Implement error boundary component — centered error message + retry button
- [ ] Implement empty state component — icon, heading, message, CTA button
- [ ] Add interaction states to all clickable elements (hover, focus, active, disabled)
- [ ] Add `@media (prefers-reduced-motion: reduce)` support to disable skeleton shimmer
- [ ] Test: load pages with network throttling to verify skeleton appearance
- [ ] Test: mock API failure to verify error states with retry

## Phase 6: Page Redesigns
- [ ] Create `frontend/css/pages/home.css` — home page layout (summary cards + run card grid)
- [ ] Redesign `HomePage` — metric summary row, run card grid with sparklines, search
- [ ] Create `frontend/css/pages/run-detail.css` — run detail layout (metric cards + table + cost breakdown)
- [ ] Redesign `RunDetailPage` — metric cards, sortable/filterable table, cost breakdown section
- [ ] Create `frontend/css/pages/new-run.css` — form with section dividers, aligned fields, validation
- [ ] Redesign `NewBenchmarkPage` — sectioned form with proper labels, validation, helper text
- [ ] Create `frontend/css/pages/live-view.css` — agent pane grid, progress bar, live status
- [ ] Redesign `LiveViewPage` — 2×2 quad agent panes with status-colored borders, streaming content
- [ ] Verify all pages render correctly with mock data

## Phase 7: Polish
- [ ] Add smooth transitions to sidebar collapse/expand
- [ ] Add fade transitions on route changes
- [ ] Add tooltip styles for collapsed sidebar icon-only mode
- [ ] Verify minimum WCAG AA color contrast ratios (use tool: `@contrast` or manual check)
- [ ] Add `aria-label` attributes to all icon-only interactive elements
- [ ] Keyboard navigation audit — tab through all interactive elements, verify focus visibility
- [ ] Cross-browser check (Chromium, Firefox, Safari)
- [ ] Dark theme consistency pass — no light-colored backgrounds leaking through

## Phase 8: Verification
- [ ] `cargo check --workspace`
- [ ] `cargo test --workspace`
- [ ] Visual review of each page against design.md wireframes
- [ ] Resize browser to verify responsive breakpoints
- [ ] Test with empty dataset (no past runs) — verify empty state
- [ ] Test with API error (kill backend) — verify error state with retry
- [ ] Test live view with SSE stream — verify agent pane updates and auto-scroll
