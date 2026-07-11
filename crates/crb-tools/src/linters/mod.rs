/// TOML-based linter configuration loading and validation.
pub mod config;

/// ESLint JSON output parser.
pub mod eslint;

/// `go vet` text output parser.
pub mod govet;

/// Ruff JSON output parser.
pub mod ruff;

/// Generic [`LinterTool`] implementation wrapping external CLI linters.
pub mod tool;
