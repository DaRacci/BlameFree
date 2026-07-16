# Delta for Linter Configuration

## ADDED Requirements

### Requirement: Linter Configuration Loading
The system SHALL load linter definitions from a TOML config file at startup.

#### Scenario: Default config generation
- GIVEN no existing `linters.toml` at the config path
- WHEN `load_linter_config()` is called
- THEN the system SHALL generate a default `linters.toml` with standard linter entries

#### Scenario: Custom config path
- GIVEN a custom path to a valid `linters.toml` file
- WHEN `load_linter_config(custom_path)` is called
- THEN the system SHALL parse the TOML file and return the linter definitions

#### Scenario: Validation failure
- GIVEN a malformed `linters.toml` file with missing fields
- WHEN `load_linter_config()` is called
- THEN the system SHALL return a `ConfigError::ParseError`

### Requirement: Schema Validation
The system SHALL validate linter configuration fields at startup after loading.

#### Scenario: Valid config
- GIVEN a `linters.toml` with correct fields (`name`, `cmd`, `output_format`)
- WHEN the config is loaded and validated
- THEN the system SHALL accept the configuration and make linters available
- AND fields SHALL satisfy: `cmd` has at least one element, `output_format` is `"json"` or `"text"`, linter names are non-empty and unique
