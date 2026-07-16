# Design: Runtime Feature Flags and Run Metadata

## 1. RuntimeConfig — Runtime Feature Flag Toggling

### 1.1 Location

`crates/crb-harness/src/config.rs` (extend existing `ReviewArgs` / add `RuntimeConfig` struct).

### 1.2 Struct Definition

```rust
/// Runtime-configurable experimental feature flags.
///
/// These replace `cfg!(feature = "exp14_*")` / `cfg!(feature = "exp16_*")`
/// compile-time gates. Defaults match `crates/crb-harness/Cargo.toml` defaults
/// (`default = []` — all flags off). The webui backend (`crb-webui-backend`)
/// enables some by default at its own Cargo.toml level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// Enable template variables (language, repo, role) injected into prompts.
    pub exp14_template_vars: bool,
    /// Enable submit-finding collector on consensus API.
    pub exp14_submit_finding: bool,
    /// Enable adaptive agent dispatch (single GEN agent for small PRs).
    pub exp16_adaptive_agents: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            exp14_template_vars: false,   // matches crb-harness Cargo.toml (default = [])
            exp14_submit_finding: false,  // matches crb-harness Cargo.toml (default = [])
            exp16_adaptive_agents: false, // matches crb-harness Cargo.toml (default = [])
        }
    }
}
```

> **Note on defaults:** `crb-webui-backend/Cargo.toml` enables `exp14_template_vars` and `exp16_adaptive_agents` in its `default` feature set. When the harness is compiled via the webui backend, those features are on by default at the Cargo level. After runtime conversion, the webui backend will set its own defaults via its API/CLI entry points instead of relying on Cargo features.

### 1.3 Global Accessor

```rust
use once_cell::sync::Lazy;
use std::sync::Mutex;

static RUNTIME_CONFIG: Lazy<Mutex<RuntimeConfig>> = Lazy::new(|| {
    Mutex::new(RuntimeConfig::default())
});

impl RuntimeConfig {
    pub fn global() -> &'static Mutex<RuntimeConfig> {
        &RUNTIME_CONFIG
    }

    pub fn init(flags: RuntimeConfig) {
        *Self::global().lock().unwrap() = flags;
    }
}
```

### 1.4 Configuration Paths — CLI, WebUI, Env Vars

RuntimeConfig is populated from three sources, listed in ascending priority order:

#### CLI path (`crb-harness` binary)

The `Review` subcommand accepts `--flag-*` args to control each feature:

```rust
// In ReviewArgs (extended):
#[arg(long)]
exp14_template_vars: bool,
#[arg(long)]
exp14_submit_finding: bool,
#[arg(long)]
exp16_adaptive_agents: bool,
```

All default to `false` via `RuntimeConfig::default()`.

#### WebUI path (`crb-webui`)

- **Ad-hoc review form**: The "Advanced options" section contains a checkbox/toggle for each flag (`exp14_template_vars`, `exp14_submit_finding`, `exp16_adaptive_agents`).
- **Benchmark run form**: Same toggles appear in the benchmark run configuration panel.
- The web UI sends the active flag values as fields in the API request to the harness, which populates `RuntimeConfig` before the run begins.

#### Env var path (CI / headless)

For automated/headless environments:

| Env var | Maps to field | Values |
|---------|---------------|--------|
| `CRB_EXP14_TEMPLATE_VARS` | `exp14_template_vars` | `0` / `1` |
| `CRB_EXP14_SUBMIT_FINDING` | `exp14_submit_finding` | `0` / `1` |
| `CRB_EXP16_ADAPTIVE_AGENTS` | `exp16_adaptive_agents` | `0` / `1` |

Env vars have the lowest priority — CLI args always override env vars.

#### Initialization Order

1. Start with `RuntimeConfig::default()` (hardcoded defaults — all flags off)
2. Overlay env vars (if present): each `CRB_EXP*` var overrides the corresponding field
3. Overlay CLI args / API request fields (final): explicit args win over everything

```rust
impl RuntimeConfig {
    pub fn from_env_and_args(env: Option<EnvVars>, args: Option<CliFlags>) -> Self {
        let mut config = RuntimeConfig::default();
        // Step 2: apply env vars
        if let Some(env) = env {
            if let Some(v) = env.exp14_template_vars { config.exp14_template_vars = v; }
            if let Some(v) = env.exp14_submit_finding { config.exp14_submit_finding = v; }
            if let Some(v) = env.exp16_adaptive_agents { config.exp16_adaptive_agents = v; }
        }
        // Step 3: apply CLI args (highest priority)
        if let Some(args) = args {
            config.exp14_template_vars = args.exp14_template_vars;
            config.exp14_submit_finding = args.exp14_submit_finding;
            config.exp16_adaptive_agents = args.exp16_adaptive_agents;
        }
        config
    }
}
```

### 1.5 Feature Gate Conversion Pattern

**Before (`cfg!()` macro):**
```rust
cfg!(feature = "exp14_template_vars")
```

**After (runtime check):**
```rust
RuntimeConfig::global().lock().unwrap().exp14_template_vars
```

The same pattern applies to all three experimental feature gates:
- `exp14_template_vars` → check `runtime_config.exp14_template_vars` before building template variables
- `exp14_submit_finding` → check `runtime_config.exp14_submit_finding` before wiring collector
- `exp16_adaptive_agents` → check `runtime_config.exp16_adaptive_agents` before adaptive dispatch

> **Note on `exp14_submit_finding`:** Currently used via `cfg!(feature = "exp14_submit_finding")` in `crb-agents/src/templates.rs` (inside a macro/template context). This is already a runtime-evaluated macro (not `#[cfg]` attribute) — the compile-time binding is only that the feature flag must be enabled at build time for the code to be compiled. After conversion, the template will use the runtime config value instead.

> **Note on `binary` flag:** `#[cfg(feature = "binary")]` is used to conditionally compile `config.rs` module and `build_review_config()` function. This is a pure compilation concern (binary vs library build) and is **not** converted to runtime.

### 1.6 Phased Removal

1. Phase 1: Replace `cfg!()` with runtime checks. All feature compile-time flags remain in `Cargo.toml` (no-op for the flags being converted; `binary` stays as-is).
2. Phase 2 (after validation): Remove experimental feature flag entries from `Cargo.toml`, remove `cfg!()` references when no longer needed.

---

## 2. RunMetadata — Structured Run Context

### 2.1 Struct Definition

```rust
/// Metadata describing the configuration context of a benchmark or ad-hoc run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RunMetadata {
    // ── Feature flags ──────────────────────────────────────────────
    /// Which runtime feature flags were enabled.
    pub enabled_features: Vec<String>,

    // ── Model configuration ────────────────────────────────────────
    /// Primary evaluation model.
    pub model: Option<String>,
    /// Judge model used for scoring.
    pub judge_model: Option<String>,
    /// Reasoning effort level ("low", "medium", "high", or None).
    pub reasoning_effort: Option<String>,

    // ── Agent configuration ────────────────────────────────────────
    /// Selected agent roles (comma-separated).
    pub roles: Option<String>,
    /// Maximum findings per agent.
    pub max_findings: Option<usize>,

    // ── Dataset ────────────────────────────────────────────────────
    /// Dataset directory or dataset identifier.
    pub dataset: Option<String>,

    // ── Prompt & Rules ─────────────────────────────────────────────
    /// Prompt library source (e.g., "builtin" or a path).
    pub prompt_library: Option<String>,
    /// Whether rules were loaded.
    pub rules_enabled: bool,

    // ── Harness version ────────────────────────────────────────────
    /// Git commit hash of the harness binary.
    pub harness_commit: Option<String>,

    // ── Timing ─────────────────────────────────────────────────────
    /// When the run was started (ISO 8601).
    pub started_at: Option<String>,
    /// Duration in seconds.
    pub duration_secs: Option<f64>,
}

impl Default for RunMetadata {
    fn default() -> Self {
        Self {
            enabled_features: vec![],  // no features enabled by default
            model: None,
            judge_model: None,
            reasoning_effort: None,
            roles: None,
            max_findings: None,
            dataset: None,
            prompt_library: Some("builtin".to_string()),
            rules_enabled: false,
            harness_commit: None,
            started_at: None,
            duration_secs: None,
        }
    }
}
```

### 2.2 Serialization

- **In benchmark per-PR JSON**: Added as a `metadata` key at the top level alongside `pr_title`, `url`, `metrics`, etc.
- **In benchmark summary (`_summary.json`)**: Added as top-level `metadata` key alongside `aggregate`, `total_cost`, etc.
- **In ad-hoc run summary**: Added as top-level `metadata` key alongside existing fields.
- **In dashboard events**: `RunMetadata` is embedded in `RunStarted` (new) and `RunFinished` (extended) events.

All new fields use `#[serde(default)]` so existing files without metadata deserialize silently.

### 2.3 Construction

Metadata is constructed at the start of a run and passed through the pipeline:

```
Benchmark CLI / WebUI
    │
    ▼
Build RunMetadata {
    model, judge_model, reasoning_effort,
    roles, max_findings, dataset,
    enabled_features: RuntimeConfig → Vec<enabled flag names>,
    prompt_library: prompts_dir.to_string(),
    rules_enabled: !skip_rules,
    harness_commit: git rev-parse HEAD,
    started_at: Utc::now().to_rfc3339(),
    duration_secs: None (set at end),
}
    │
    ├─► Per-PR result files (metadata added once to each)
    ├─► _summary.json (metadata at top level)
    ├─► DashboardEvent::RunStarted (new event)
    └─► DashboardEvent::RunFinished (updated with metadata)
```

### 2.4 Metadata Capture Points

| Field | Source | Where captured |
|-------|--------|----------------|
| `enabled_features` | `RuntimeConfig` → check each flag | Start of run |
| `model` | CLI arg / API request | Start of run |
| `judge_model` | CLI arg / API request | Start of run |
| `reasoning_effort` | CLI arg / API request | Start of run |
| `roles` | CLI arg / API request | Start of run |
| `max_findings` | CLI arg / API request | Start of run |
| `dataset` | CLI arg / API request | Start of run |
| `prompt_library` | `prompts_dir` arg or "builtin" | Start of run |
| `rules_enabled` | `skip_rules` flag inverted | Start of run |
| `harness_commit` | `git rev-parse HEAD` at build time via `build.rs` | Compile time (env var) |
| `started_at` | `Utc::now()` | Start of run |
| `duration_secs` | `Instant::now() - start_time` | End of run |

---

## 3. Data Flow

```
┌─────────────────────────────────────────────────────────┐
│                   Benchmark Run Start                    │
├─────────────────────────────────────────────────────────┤
│ 1. Parse CLI args / API request                         │
│ 2. Build RuntimeConfig from feature flags               │
│ 3. Build RunMetadata from runtime config + CLI args     │
│ 4. Send DashboardEvent::RunStarted { metadata }         │
│ 5. For each PR: evaluate → write result JSON            │
│    (result JSON includes metadata key)                  │
│ 6. Write _summary.json (includes metadata key)           │
│ 7. Send DashboardEvent::RunFinished { ..., metadata }    │
└─────────────────────────────────────────────────────────┘
```

---

## 4. Technical Decisions

| Decision | Option A | Option B | Chosen | Rationale |
|----------|----------|----------|--------|-----------|
| RuntimeConfig storage | Global `Lazy<Mutex<>>` | Thread-local | A | Simple, single-binary, no DI framework |
| RuntimeConfig init | Constructor at startup | Env-var-driven | A | Clear, testable, explicit |
| Feature gate replacement | `if` blocks throughout | Central dispatch fn | A | Minimal diff, preserves structure |
| Metadata location | Inline in result structs | Top-level `metadata` key | B | Backward-compatible, old parsers ignore it |
| Metadata serialization | Required field | `#[serde(default)]` | B | Graceful reading of old files |
| Harness commit | `build.rs` env var | Read `git` at runtime | A | Deterministic, no git dependency |
| `RunStarted` event | New variant | Reuse existing | A (new) | Enables timeline tracking; missing currently |

---

## 5. Migration

### 5.1 Reading Old Runs

All new `RunMetadata` fields use `#[serde(default)]`. When reading an old JSON file that lacks a `metadata` key, deserialization produces `RunMetadata::default()` with zero-values / empty vectors. Code that consumes metadata should check `!metadata.enabled_features.is_empty()` or similar to detect empty metadata.

### 5.2 Dashboard Event Backward Compatibility

The `DashboardEvent` enum in `crb-dashboard` already uses Serde. Adding `metadata` fields to `RunFinished` and a new `RunStarted` variant is backward compatible at the Rust type level (pattern matching must be updated). For JSON stdout / SSE consumers, `RunFinished` gains an optional `metadata` field — consumers that ignore unknown keys (or use `#[serde(deny_unknown_fields)]` on their side) will need updating.

### 5.3 CLI Flag Defaults

Defaults match `crates/crb-harness/Cargo.toml` feature defaults:
- `exp14_template_vars`: `false` (was not in `default = []`)
- `exp14_submit_finding`: `false` (was not in `default = []`)
- `exp16_adaptive_agents`: `false` (was not in `default = []`)

The webui backend (`crb-webui-backend`) currently enables `exp14_template_vars` and `exp16_adaptive_agents` in its own `default` feature set. After migration, the webui backend will pass its own defaults at the API/CLI level rather than relying on Cargo feature composition.
