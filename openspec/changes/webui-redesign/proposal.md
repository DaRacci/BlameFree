# Proposal: Web UI Redesign

**Change ID:** webui-redesign
**Status:** Draft
**Author:** Hermes Agent
**Date:** 2026-06-27

## Summary

Redesign the `crb-webui` frontend with a professional dark-themed UI featuring a cohesive color system, proper responsive layout, consistent typography hierarchy, and polished component designs. This is a pure frontend redesign — no backend APIs, no new features, no data model changes.

## Motivation

The current `crb-webui` interface is functional but visually unpolished:

1. **No visual hierarchy** — flat, unstyled HTML-like appearance with minimal distinction between headings, content, and actions
2. **Inconsistent spacing** — ad-hoc margins and padding across pages and components
3. **No dark theme system** — basic dark colors without deliberate palette choices for backgrounds, surfaces, accents, and text
4. **Missing interaction feedback** — no hover states, focus rings, transitions, or loading skeletons
5. **Poor error/empty states** — raw error messages or blank pages when data is missing
6. **No responsive behavior** — fixed-width layout that breaks on smaller viewports

A polished UI directly improves the developer experience for everyone using the review harness. The dashboard serves as the primary interface for reviewing benchmark results, monitoring live agent runs, and launching new benchmarks — it should feel professional and trustworthy.

## Scope

- **In scope:**
  - CSS design token system (colors, spacing, typography, shadows, radii)
  - Responsive grid layout with collapsible sidebar navigation
  - Typography scale and heading hierarchy
  - Component redesign for all existing pages: Home, Run Detail, New Run, Live View
  - State indicators: loading skeletons, error states with retry, empty states with helpful messages
  - Hover, focus, active, and disabled interaction states
  - CSS variables approach (custom CSS with `:root` variables) — no CSS framework dependency

- **Out of scope:**
  - Backend API changes
  - New feature additions
  - Data model changes
  - Authentication / authorization UI
  - Mobile-first responsive (desktop-first with responsive fallback)
  - Animation library or full design system component library
  - Icon set overhaul (use existing icons with improved styling)

## Key Design Decisions

1. **CSS custom properties (variables)** — All design tokens defined as `:root` CSS variables in a single `tokens.css` file. No Tailwind or CSS framework dependency — pure custom CSS keeps the build lean and avoids framework churn. Variables also enable future theme switching (light mode, high-contrast).

2. **GitHub-dark inspired palette** — Uses `#0d1117` / `#161b22` surface colors proven in large-scale developer tools, with semantic accent colors (blue for primary actions, green for success, orange for warnings, red for errors).

3. **Collapsible sidebar navigation** — Replaces top navbar for better information density on widescreen monitors, with collapse to icon-only on narrow viewports. This matches the pattern used by VS Code, GitHub, and other developer tools.

4. **System font stack** — Uses `-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, sans-serif` for native OS feel and zero additional font weight.

5. **Progressive enhancement** — Loading skeleton screens for all data-fetching views, graceful error fallbacks, and meaningful empty states. No content jumps or flash-of-wrong-content.
