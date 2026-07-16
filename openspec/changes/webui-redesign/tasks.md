# Tasks: Web UI Redesign

## Phase 1: Openspec Plan ✅
- [x] Create `openspec/changes/webui-redesign/proposal.md`
- [x] Create `openspec/changes/webui-redesign/design.md`
- [x] Create `openspec/changes/webui-redesign/tasks.md`
- [x] Create `specs/design-tokens/spec.md`
- [x] Create `specs/components/spec.md`
- [x] Create `specs/pages/spec.md`

## Phase 2: Design Token System ✅
- [x] Create `crates/crb-webui-frontend/css/tokens.css` — all CSS custom properties (colors, spacing, type, shadows, radii)
- [x] Create `crates/crb-webui-frontend/css/reset.css` — minimal CSS reset (box-sizing, margin/padding reset)
- [x] Create `crates/crb-webui-frontend/css/base.css` — base element styles (body, headings, links, code blocks)
- [x] Create `crates/crb-webui-frontend/css/styles.css` — single entry point importing all CSS modules
- [x] Verify: `cargo check --workspace` builds successfully after CSS file additions (no Rust changes)

## Phase 3: Layout System ✅
- [x] Create `crates/crb-webui-frontend/css/layout.css` — sidebar, main content grid, responsive breakpoints
- [x] Create `crates/crb-webui-frontend/css/components/nav.css` — sidebar navigation styles (expanded, collapsed, active states)
- [x] Implement sidebar component in Leptos — collapsible with toggle button
- [x] Implement mobile overlay sidebar (slide-in panel for <768px)
- [x] Wire up `App` root layout to use sidebar + main content area
- [x] Test responsive behavior at 3 breakpoints (≥1200px, 768–1199px, <768px)

## Phase 4: Base Components ✅
- [x] Create `crates/crb-webui-frontend/css/components/card.css` — card with header/body/footer slots, interactive hover
- [x] Create `crates/crb-webui-frontend/css/components/button.css` — all variants (primary, success, danger, secondary, ghost)
- [x] Create `crates/crb-webui-frontend/css/components/form.css` — inputs, labels, selects, checkboxes, sliders, validation states
- [x] Create `crates/crb-webui-frontend/css/components/badge.css` — status badges with semantic colors
- [x] Create `crates/crb-webui-frontend/css/components/table.css` — sortable table with sticky header, row hover
- [x] Create `crates/crb-webui-frontend/css/components/skeleton.css` — shimmer animation, skeleton shapes
- [x] Create `crates/crb-webui-frontend/css/components/progress.css` — progress bar with label
- [x] Refactor existing Leptos components to use new CSS classes and tokens
- [x] Verify visual consistency across all components

## Phase 5: State Indicators ✅
- [x] Implement loading skeleton component for each page type (home, run detail, live view, new run form)
- [x] Implement error boundary component — centered error message + retry button
- [x] Implement empty state component — icon, heading, message, CTA button
- [x] Add interaction states to all clickable elements (hover, focus, active, disabled)
- [x] Add `@media (prefers-reduced-motion: reduce)` support to disable skeleton shimmer
- [ ] Test: load pages with network throttling to verify skeleton appearance
- [ ] Test: mock API failure to verify error states with retry

## Phase 6: Page Redesigns 🔶 In Progress
- [x] Create `crates/crb-webui-frontend/css/pages/home.css` — home page layout (summary cards + run card grid)
- [ ] Redesign `HomePage` — metric summary row, run card grid [sparklines: NOT IMPLEMENTED, search: NOT IMPLEMENTED]
- [x] Create `crates/crb-webui-frontend/css/pages/run-detail.css` — run detail layout (metric cards + table + cost breakdown)
- [ ] Redesign `RunDetailPage` — metric cards, basic table [sorting/filtering: NOT IMPLEMENTED, cost breakdown: NOT IMPLEMENTED]
- [x] Create `crates/crb-webui-frontend/css/pages/new-run.css` — form with section dividers, aligned fields, validation
- [ ] Redesign `NewBenchmarkPage` — sectioned form with labels, helper text [sliders: NOT IMPLEMENTED, validation: PARTIAL]
- [x] Create `crates/crb-webui-frontend/css/pages/live-view.css` — agent pane grid, progress bar, live status
- [ ] Redesign `LiveViewPage` — 2×2 agent panes with status-colored borders, streaming content [per-agent metrics: NOT IMPLEMENTED]
- [x] Verify all pages render correctly with mock data

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
