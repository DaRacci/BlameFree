# Design: Runtime Feature Flags and Run Metadata

## 1. RuntimeConfig — Runtime Feature Flag Toggling

### 1.1 Location

`crates/crb-harness/src/config.rs` (extend existing `ReviewArgs` / add `RuntimeConfig` struct).

### 1.2 Struct Definition

```rust
/// Runtime-configurable feature flags.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// Enable diff reduction (strip metadata, -U1 context).
    pub reduce_diff: bool,
    /// Enable template variables (language, repo, role) injected into prompts.
    pub template_vars: bool,
    /// Enable submit-finding collector on consensus API.
    pub submit_finding: bool,
    /// Enable adaptive agent dispatch (single GEN agent for small PRs).
    pub adaptive_agents: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            reduce_diff: true,        // matches current Cargo.toml `default = ["reduce-diff"]`
            template_vars: true, // matches webui default
            submit_finding: false, // not in defaults
            adaptive_agents: true, // matches webui default
        }
    }
}
```

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

#### CLI path (`crb-benchmark` binary)

The `Run` subcommand accepts `--flag-*` args to control each feature:

```rust
// In Cli Run subcommand:
#[arg(long)]
reduce_diff: bool,            // defaults to true via RuntimeConfig::default()
#[arg(long)]
no_reduce_diff: bool,         // --no-reduce-diff to disable (reduce_diff defaults on)
#[arg(long)]
template_vars: bool,
#[arg(long)]
submit_finding: bool,
#[arg(long)]
adaptive_agents: bool,
```

These are parsed into a `RuntimeConfig` struct at startup and stored in `RUNTIME_CONFIG` before any benchmark iteration begins. `reduce_diff` defaults to `true`; a `--no-reduce-diff` flag allows explicitly disabling it without requiring `--reduce-diff=false`.

#### WebUI path (`crb-webui`)

- **Ad-hoc review form**: The "Advanced options" section contains a checkbox/toggle for each flag (`template_vars`, `submit_finding`, `adaptive_agents`, `reduce_diff`).
- **Benchmark run form**: Same toggles appear in the benchmark run configuration panel.
- The web UI sends the active flag values as fields in the API request to the harness, which populates `RuntimeConfig` before the run begins.

#### Env var path (CI / headless)

For automated/headless environments:

| Env var | Maps to field | Values |
|---------|---------------|--------|
| `CRB_REDUCE_DIFF` | `reduce_diff` | `0` / `1` |
| `CRB_TEMPLATE_VARS` | `template_vars` | `0` / `1` |
| `CRB_SUBMIT_FINDING` | `submit_finding` | `0` / `1` |
| `CRB_ADAPTIVE_AGENTS` | `adaptive_agents` | `0` / `1` |

Env vars have the lowest priority — CLI args always override env vars.

#### Initialization Order

1. Start with `RuntimeConfig::default()` (hardcoded defaults matching current Cargo.toml feature defaults)
2. Overlay env vars (if present): each `CRB_*` var overrides the corresponding field
3. Overlay CLI args / API request fields (final): explicit args win over everything

```rust
impl RuntimeConfig {
    pub fn from_env_and_args(env: Option<EnvVars>, args: Option<CliFlags>) -> Self {
        let mut config = RuntimeConfig::default();
        // Step 2: apply env vars
        if let Some(env) = env {
            if let Some(v) = env.reduce_diff { config.reduce_diff = v; }
            if let Some(v) = env.template_vars { config.template_vars = v; }
            if let Some(v) = env.submit_finding { config.submit_finding = v; }
            if let Some(v) = env.adaptive_agents { config.adaptive_agents = v; }
        }
        // Step 3: apply CLI args (highest priority)
        if let Some(args) = args {
            config.reduce_diff = args.reduce_diff;
            config.template_vars = args.template_vars;
            config.submit_finding = args.submit_finding;
            config.adaptive_agents = args.adaptive_agents;
        }
        config
    }
}
```

### 1.5 Feature Gate Conversion Pattern

**Before (`#[cfg]`):**
```rust
#[cfg(feature = "reduce-diff")]
{
    let filtered = filter_files(raw_diff);
    strip_diff_metadata(&filtered)
}
#[cfg(not(feature = "reduce-diff"))]
{
    raw_diff.to_string()
}
```

**After (runtime check):**
```rust
if RuntimeConfig::global().lock().unwrap().reduce_diff {
    let filtered = filter_files(raw_diff);
    strip_diff_metadata(&filtered)
} else {
    raw_diff.to_string()
}
```

The same pattern applies to all four feature gates:
- `template_vars` → check `runtime_config.template_vars` before building template variables
- `submit_finding` → check `runtime_config.submit_finding` before wiring collector
- `adaptive_agents` → check `runtime_config.adaptive_agents` before adaptive dispatch

### 1.6 Phased Removal

1. Phase 1: Replace `#[cfg]` with runtime checks. All feature compile-time flags remain in `Cargo.toml` (no-op).
2. Phase 2 (after validation): Remove feature flag entries from `Cargo.toml`, remove `#[cfg(feature)]` attributes when no longer referenced.

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
            enabled_features: vec!["reduce-diff".to_string()],
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

Defaults are chosen to match the current Cargo.toml feature defaults:
- `reduce-diff`: `true` (was default feature)
- `template_vars`: `true` (webui default)
- `submit_finding`: `false` (not in defaults)
- `adaptive_agents`: `true` (webui default)
