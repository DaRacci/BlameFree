# Delta for Config Domain — Runtime Feature Toggling

## ADDED Requirements

### Requirement: RuntimeConfig struct

The system SHALL provide a `RuntimeConfig` struct that holds a boolean flag for each feature currently gated at compile time.

#### Scenario: RuntimeConfig initialized from defaults
- GIVEN the system starts with no CLI overrides for feature flags
- WHEN `RuntimeConfig::default()` is called
- THEN the struct SHALL have `reduce_diff = true`, `template_vars = true`, `submit_finding = false`, `adaptive_agents = true`

#### Scenario: RuntimeConfig overridden via CLI
- GIVEN a user passes `--reduce-diff=false` to the benchmark `Run` command
- WHEN the CLI is parsed and `RuntimeConfig` is built
- THEN `reduce_diff` SHALL be `false`

#### Scenario: RuntimeConfig overridden via env var
- GIVEN the environment sets `CRB_REDUCE_DIFF=0` and `CRB_TEMPLATE_VARS=0`
- WHEN `RuntimeConfig` is constructed from env and defaults
- THEN `reduce_diff` SHALL be `false`
- AND `template_vars` SHALL be `false`
- AND `submit_finding` SHALL be `false` (default)
- AND `adaptive_agents` SHALL be `true` (default)

#### Scenario: CLI overrides env var
- GIVEN `CRB_TEMPLATE_VARS=0` is set in the environment AND `--template-vars` is passed on the CLI
- WHEN `RuntimeConfig` is built
- THEN the CLI value SHALL take precedence over the env var value

#### Scenario: RuntimeConfig populated from webui API request
- GIVEN the webui sends an API request with `template_vars=false` and `adaptive_agents=false`
- WHEN the harness constructs `RuntimeConfig` from the request fields
- THEN `template_vars` SHALL be `false`
- AND `adaptive_agents` SHALL be `false`

### Requirement: Global accessor for RuntimeConfig

The system SHALL provide a thread-safe global accessor for the active `RuntimeConfig`.

#### Scenario: Feature functions read runtime state
- GIVEN `RuntimeConfig::init()` has been called with a config
- WHEN a function checks `RuntimeConfig::global().lock().unwrap().reduce_diff`
- THEN it SHALL read the value set by `init()`

### Requirement: RuntimeConfig initialization order

The system SHALL apply configuration sources in a documented priority order: defaults → env vars → CLI/API.

#### Scenario: Defaults used when no overrides present
- GIVEN no env vars are set and no CLI args are passed
- WHEN `RuntimeConfig::from_env_and_args(None, None)` is called
- THEN all fields SHALL equal `RuntimeConfig::default()`

#### Scenario: Env vars override defaults
- GIVEN `CRB_REDUCE_DIFF=0` is set and no CLI args are passed
- WHEN `RuntimeConfig::from_env_and_args(...)` is called
- THEN `reduce_diff` SHALL be `false` (env var overrides default `true`)

### Requirement: RuntimeConfig included in RunMetadata

The system SHALL emit the set of enabled feature flag names into `RunMetadata.enabled_features`.

#### Scenario: Flags recorded in metadata
- GIVEN a run starts with `reduce_diff = true` and `template_vars = false`
- WHEN the run metadata is serialized
- THEN `enabled_features` SHALL contain `["reduce-diff"]`

## MODIFIED Requirements

### Requirement: Feature gate behavior (reduce-diff)

The `reduce-diff` feature SHALL be checked at runtime instead of compile time.
(Previously: `#[cfg(feature = "reduce-diff")]`)

#### Scenario: Reduction enabled
- GIVEN `RuntimeConfig::global().reduce_diff == true`
- WHEN `preprocess_diff()` is called
- THEN it SHALL call `filter_files()` + `strip_diff_metadata()` — same as current `#[cfg(feature = "reduce-diff")]` path

#### Scenario: Reduction disabled
- GIVEN `RuntimeConfig::global().reduce_diff == false`
- WHEN `preprocess_diff()` is called
- THEN it SHALL return `raw_diff.to_string()` — same as current `#[cfg(not(feature = "reduce-diff"))]` path

### Requirement: Feature gate behavior (template_vars)

The `template_vars` feature SHALL be checked at runtime instead of compile time.
(Previously: `#[cfg(feature = "template_vars")]`)

#### Scenario: Template vars enabled
- GIVEN `RuntimeConfig::global().template_vars == true`
- WHEN `evaluate_pr_with_postprocessing()` runs
- THEN it SHALL build and pass `template_vars` to the consensus pipeline

#### Scenario: Template vars disabled
- GIVEN `RuntimeConfig::global().template_vars == false`
- WHEN `evaluate_pr_with_postprocessing()` runs
- THEN `template_vars` SHALL be `None`

### Requirement: Feature gate behavior (submit_finding)

The `submit_finding` feature in `crb-consensus` SHALL be checked at runtime instead of compile time.
(Previously: `#[cfg(feature = "submit_finding")]`)

#### Scenario: Submit finding enabled
- GIVEN `RuntimeConfig::global().submit_finding == true`
- WHEN `run_consensus()` is called
- THEN the collector parameter SHALL be wired

#### Scenario: Submit finding disabled
- GIVEN `RuntimeConfig::global().submit_finding == false`
- WHEN `run_consensus()` is called
- THEN the collector SHALL be `None`

### Requirement: Feature gate behavior (adaptive_agents)

The `adaptive_agents` feature SHALL be checked at runtime instead of compile time.
(Previously: `#[cfg(feature = "adaptive_agents")]`)

#### Scenario: Adaptive agents enabled
- GIVEN `RuntimeConfig::global().adaptive_agents == true`
- WHEN `evaluate_pr_with_postprocessing()` runs
- THEN it SHALL apply adaptive agent dispatch logic

#### Scenario: Adaptive agents disabled
- GIVEN `RuntimeConfig::global().adaptive_agents == false`
- WHEN `evaluate_pr_with_postprocessing()` runs
- THEN it SHALL skip adaptive dispatch and use user-selected roles
