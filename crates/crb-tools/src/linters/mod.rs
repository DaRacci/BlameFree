use std::time::Duration;

use crb_shared::finding::Finding;

use crate::{
    error::LinterError,
    linters::{
        config::{LinterConfig, OutputFormat},
        eslint::parse_eslint_output,
        govet::parse_govet_output,
        ruff::parse_ruff_output,
        tool::LinterTool,
    },
};

pub mod config;
pub mod eslint;
pub mod govet;
pub mod ruff;
pub mod tool;

/// Internal helper to create a [`LinterTool`] from a [`LinterConfig`] and a parser function.
fn create_linter_tool_inner(
    config: &LinterConfig,
    parser: fn(&str) -> Result<Vec<Finding>, LinterError>,
) -> LinterTool {
    LinterTool {
        name: config.name.clone(),
        cmd: config.cmd.clone(),
        parser,
        timeout: Duration::from_secs(config.timeout_secs.unwrap_or(60)),
    }
}

/// Create a [`LinterTool`] for ruff from its configuration.
pub fn create_ruff_tool(config: &LinterConfig) -> LinterTool {
    create_linter_tool_inner(config, parse_ruff_output)
}

/// Create a [`LinterTool`] for ESLint from its configuration.
pub fn create_eslint_tool(config: &LinterConfig) -> LinterTool {
    create_linter_tool_inner(config, parse_eslint_output)
}

/// Create a [`LinterTool`] for `go vet` from its configuration.
pub fn create_govet_tool(config: &LinterConfig) -> LinterTool {
    create_linter_tool_inner(config, parse_govet_output)
}

/// Create a [`LinterTool`] from configuration, selecting the parser based on
/// the `output_format` field.
pub fn create_linter_tool(config: &LinterConfig) -> LinterTool {
    let parser: fn(&str) -> Result<Vec<Finding>, LinterError> = match config.output_format {
        OutputFormat::Json => parse_ruff_output,
        OutputFormat::Text => parse_govet_output,
    };
    create_linter_tool_inner(config, parser)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_ruff_tool() {
        let config = LinterConfig {
            name: "ruff".to_string(),
            cmd: vec!["ruff".to_string(), "check".to_string()],
            timeout_secs: Some(60),
            output_format: OutputFormat::Json,
            environment: None,
            optional: Some(false),
        };
        let tool = create_ruff_tool(&config);
        assert_eq!(tool.name, "ruff");
        assert_eq!(tool.cmd, vec!["ruff", "check"]);
        assert_eq!(tool.timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_create_eslint_tool() {
        let config = LinterConfig {
            name: "eslint".to_string(),
            cmd: vec!["npx".to_string(), "eslint".to_string()],
            timeout_secs: Some(90),
            output_format: OutputFormat::Json,
            environment: None,
            optional: Some(true),
        };
        let tool = create_eslint_tool(&config);
        assert_eq!(tool.name, "eslint");
        assert_eq!(tool.timeout, Duration::from_secs(90));
    }

    #[test]
    fn test_create_govet_tool() {
        let config = LinterConfig {
            name: "go vet".to_string(),
            cmd: vec!["go".to_string(), "vet".to_string(), "./...".to_string()],
            timeout_secs: None,
            output_format: OutputFormat::Text,
            environment: None,
            optional: Some(true),
        };
        let tool = create_govet_tool(&config);
        assert_eq!(tool.name, "go vet");
        // Default timeout when None
        assert_eq!(tool.timeout, Duration::from_secs(60));
    }
}
