# Spec: Frontend — Logs Tab, Replay Mode

## Tab Bar Component

Generic tab switcher used to toggle between Results and Logs views on the run detail page.

```
┌──────────────────────────────────────────────┐
│  [ Results ]  [ Logs ]                       │
├──────────────────────────────────────────────┤
│  (tab content)                               │
└──────────────────────────────────────────────┘
```

- Active tab has underlined/highlighted style
- Tab click switches the content view
- Initial state: Results tab active (backward compatible)

## Logs Tab

### Layout
```
┌──────────────────────────────────────────────┐
│  Logs for run-1234567890                     │
│  (3 PRs, 12 agent logs available)            │
├──────────────────────────────────────────────┤
│  ▼ PR #42: Scale color lightness...          │
│    ├── SA  [click to expand prompt/response] │
│    ├── CL  [click to expand prompt/response] │
│    ├── AR  [click to expand prompt/response] │
│    └── SEC [click to expand prompt/response] │
│                                              │
│  ▼ PR #128: Fix cache invalidation...        │
│    ├── SA  [click to expand prompt/response] │
│    ├── CL  [click to expand prompt/response] │
│    └── AR  [click to expand prompt/response] │
└──────────────────────────────────────────────┘
```

### Empty State
When no cache data exists:
```
┌──────────────────────────────────────────────┐
│  📋 No Cache Available                        │
│                                              │
│  This run was executed without --cache-dir.  │
│  Agent prompts and responses are not stored. │
│  To enable logging, re-run with caching.     │
└──────────────────────────────────────────────┘
```

### Data Fetching
- On tab switch: `GET /api/runs/:id/logs` → returns list of PRs with available agent roles
- On agent row click: `GET /api/runs/:id/logs/:pr_key/:role` → returns prompt + response
- Lazy loading: only fetch logs for agents the user expands

## Replay Mode

### Replay Button
- Shown on run detail page when cache is available
- Label: "▶ Replay Run"
- Disabled while a replay is already in progress

### Replay Overlay (Modal)
```
┌──────────────────────────────────────────────┐
│  Replaying Run                                │
│                                              │
│  ████████████░░░░░░░░░░  45%                 │
│                                              │
│  Replaying PR #3/7...                        │
│                                              │
│  [ Cancel ]                                  │
└──────────────────────────────────────────────┘
```

### Comparison Table (After completion)
```
┌──────────────────────────────────────────────┐
│  ✓ Replay Complete — Results Match!          │
│                                              │
│  PR     │ Original F1 │ Replay F1 │ Match?  │
│  ───────┼─────────────┼───────────┼──────── │
│  #42    │ 0.875       │ 0.875     │ ✅      │
│  #128   │ 0.667       │ 0.667     │ ✅      │
│  #256   │ 0.923       │ 0.923     │ ✅      │
│                                              │
│  Aggregate: F1=0.822 → F1=0.822 ✅          │
│                                              │
│  [ Close ]                                   │
└──────────────────────────────────────────────┘
```

### Data Flow for Replay
1. User clicks "Replay Run"
2. `POST /api/runs/:id/replay` → returns 202 with replay_id
3. Start polling `GET /api/runs/:id/replay/status` every 500ms
4. Progress bar updates with `progress_pct`
5. On status="completed": fetch original run detail + replay run detail
6. Build comparison table showing per-PR metrics side by side
7. Show hash-match indicators (✅/❌) per PR
