# Delta for Config Domain — Runtime Feature Toggling

## ADDED Requirements

### Requirement: RuntimeConfig struct

The system SHALL provide a `RuntimeConfig` struct that holds a boolean flag for each experimental feature currently gated at compile time (`cfg!(feature = "exp14_*")` / `cfg!(feature = "exp16_*")`).

#### Scenario: RuntimeConfig initialized from defaults
- GIVEN the system starts with no CLI overrides for feature flags
- WHEN `RuntimeConfig::default()` is called
- THEN the struct SHALL have `exp14_template_vars = false`, `exp14_submit_finding = false`, `exp16_adaptive_agents = false`

#### Scenario: RuntimeConfig overridden via CLI
- GIVEN a user passes `--exp14-template-vars` to the `review` command
- WHEN the CLI is parsed and `RuntimeConfig` is built
- THEN `exp14_template_vars` SHALL be `true`

#### Scenario: RuntimeConfig overridden via env var
- GIVEN the environment sets `CRB_EXP14_TEMPLATE_VARS=0` and `CRB_EXP16_ADAPTIVE_AGENTS=0`
- WHEN `RuntimeConfig` is constructed from env and defaults
- THEN `exp14_template_vars` SHALL be `false`
- AND `exp16_adaptive_agents` SHALL be `false`
- AND `exp14_submit_finding` SHALL be `false` (default)

#### Scenario: CLI overrides env var
- GIVEN `CRB_EXP14_TEMPLATE_VARS=0` is set in the environment AND `--exp14-template-vars` is passed on the CLI
- WHEN `RuntimeConfig` is built
- THEN the CLI value SHALL take precedence over the env var value

#### Scenario: RuntimeConfig populated from webui API request
- GIVEN the webui sends an API request with `exp14_template_vars=false` and `exp16_adaptive_agents=true`
- WHEN the harness constructs `RuntimeConfig` from the request fields
- THEN `exp14_template_vars` SHALL be `false`
- AND `exp16_adaptive_agents` SHALL be `true`

### Requirement: Global accessor for RuntimeConfig

The system SHALL provide a thread-safe global accessor for the active `RuntimeConfig`.

#### Scenario: Feature functions read runtime state
- GIVEN `RuntimeConfig::init()` has been called with a config
- WHEN a function checks `RuntimeConfig::global().lock().unwrap().exp14_template_vars`
- THEN it SHALL read the value set by `init()`

### Requirement: RuntimeConfig initialization order

The system SHALL apply configuration sources in a documented priority order: defaults → env vars → CLI/API.

#### Scenario: Defaults used when no overrides present
- GIVEN no env vars are set and no CLI args are passed
- WHEN `RuntimeConfig::from_env_and_args(None, None)` is called
- THEN all fields SHALL equal `RuntimeConfig::default()`

#### Scenario: Env vars override defaults
- GIVEN `CRB_EXP16_ADAPTIVE_AGENTS=1` is set and no CLI args are passed
- WHEN `RuntimeConfig::from_env_and_args(...)` is called
- THEN `exp16_adaptive_agents` SHALL be `true` (env var overrides default `false`)

### Requirement: RuntimeConfig included in RunMetadata

The system SHALL emit the set of enabled feature flag names into `RunMetadata.enabled_features`.

#### Scenario: Flags recorded in metadata
- GIVEN a run starts with `exp14_template_vars = true` and `exp16_adaptive_agents = true`
- WHEN the run metadata is serialized
- THEN `enabled_features` SHALL contain `["exp14_template_vars", "exp16_adaptive_agents"]`

## MODIFIED Requirements

### Requirement: Feature gate behavior (exp14_template_vars)

The `exp14_template_vars` feature SHALL be checked at runtime instead of compile time.
(Previously: `cfg!(feature = "exp14_template_vars")`)

#### Scenario: Template vars enabled
- GIVEN `RuntimeConfig::global().exp14_template_vars == true`
- WHEN `evaluate_pr_with_postprocessing()` runs
- THEN it SHALL build and pass template variables to the consensus pipeline

#### Scenario: Template vars disabled
- GIVEN `RuntimeConfig::global().exp14_template_vars == false`
- WHEN `evaluate_pr_with_postprocessing()` runs
- THEN template variables SHALL be omitted

### Requirement: Feature gate behavior (exp14_submit_finding)

The `exp14_submit_finding` feature in `crb-agents/src/templates.rs` SHALL be checked at runtime instead of compile time.
(Previously: `cfg!(feature = "exp14_submit_finding")`)

#### Scenario: Submit finding enabled
- GIVEN `RuntimeConfig::global().exp14_submit_finding == true`
- WHEN template variables are rendered
- THEN the submit-finding context SHALL be wired into the template

#### Scenario: Submit finding disabled
- GIVEN `RuntimeConfig::global().exp14_submit_finding == false`
- WHEN template variables are rendered
- THEN the submit-finding context SHALL be `false`

### Requirement: Feature gate behavior (exp16_adaptive_agents)

The `exp16_adaptive_agents` feature SHALL be checked at runtime instead of compile time.
(Previously: compile-time gate with `#[allow(unused_variables)]` conditioning)

#### Scenario: Adaptive agents enabled
- GIVEN `RuntimeConfig::global().exp16_adaptive_agents == true`
- WHEN `evaluate_pr_with_postprocessing()` runs
- THEN it SHALL apply adaptive agent dispatch logic (single GEN agent for small PRs)

#### Scenario: Adaptive agents disabled
- GIVEN `RuntimeConfig::global().exp16_adaptive_agents == false`
- WHEN `evaluate_pr_with_postprocessing()` runs
- THEN it SHALL skip adaptive dispatch and use user-selected roles
