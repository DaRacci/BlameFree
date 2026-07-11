use std::{collections::HashMap, fmt::Display};

use serde::Deserialize;

use crate::error::ConfigError;

/// A single linter definition from the TOML config file.
#[derive(Debug, Clone, Deserialize)]
pub struct LinterConfig {
    /// Display name for the linter.
    pub name: String,

    /// The command and arguments to invoke the linter.
    pub cmd: Vec<String>,

    /// Environment variables to set for the linter.
    pub environment: Option<HashMap<String, String>>,

    /// Per-invocation timeout in seconds.
    pub timeout_secs: Option<u64>,

    /// Output format.
    pub output_format: OutputFormat,

    /// If true, a missing binary is non-fatal.
    pub optional: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub enum OutputFormat {
    Json,
    Text,
}

impl Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputFormat::Json => write!(f, "json"),
            OutputFormat::Text => write!(f, "text"),
        }
    }
}

/// Top-level config file structure.
#[derive(Debug, Deserialize)]
pub struct LinterConfigFile {
    pub linters: HashMap<String, LinterConfig>,
}

/// Load and validate linter configuration from a TOML file.
///
/// Returns a `HashMap` keyed by linter identifier.
pub fn load_linter_config(path: &str) -> Result<HashMap<String, LinterConfig>, ConfigError> {
    let content = std::fs::read_to_string(path).map_err(ConfigError::IoError)?;

    let config: LinterConfigFile =
        toml::from_str(&content).map_err(|e| ConfigError::ParseError(e.to_string()))?;

    for (key, lc) in &config.linters {
        if lc.cmd.is_empty() {
            return Err(ConfigError::ValidationError(format!(
                "linter '{key}' has empty cmd"
            )));
        }

        if lc.name.is_empty() {
            return Err(ConfigError::ValidationError(format!(
                "linter '{key}' has empty name"
            )));
        }
    }

    Ok(config.linters)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_temp_config(filename: &str, content: &str) -> Result<HashMap<String, LinterConfig>, ConfigError> {
        let dir = std::env::temp_dir();
        let path = dir.join(filename);
        std::fs::write(&path, content).expect("failed to write temp config");
        let result = load_linter_config(path.to_str().unwrap());
        std::fs::remove_file(&path).ok();
        result
    }

    #[test]
    fn test_load_linter_config_file_not_found() {
        let result = load_linter_config("/nonexistent/path/linters.toml");
        assert!(result.is_err());
        match result.unwrap_err() {
            ConfigError::IoError(_) => {}
            other => panic!("expected IoError, got {other:?}"),
        }
    }

    #[test]
    fn test_load_linter_config_invalid_toml() {
        let result = load_temp_config("test_invalid_linters.toml", "not toml = [[[");
        assert!(result.is_err());
        match result.unwrap_err() {
            ConfigError::ParseError(_) => {}
            other => panic!("expected ParseError, got {other:?}"),
        }
    }

    #[test]
    fn test_load_linter_config_validation_empty_cmd() {
        let toml_content = r#"
[linters.test]
name = "test"
cmd = []
timeout_secs = 60
output_format = "json"
optional = false
"#;
        let result = load_temp_config("test_empty_cmd_linters.toml", toml_content);
        assert!(result.is_err());
        match result.unwrap_err() {
            ConfigError::ValidationError(msg) => {
                assert!(msg.contains("empty cmd"));
            }
            other => panic!("expected ValidationError, got {other:?}"),
        }
    }

    #[test]
    fn test_load_linter_config_validation_bad_format() {
        let toml_content = r#"
[linters.test]
name = "test"
cmd = ["test"]
timeout_secs = 60
output_format = "yaml"
optional = false
"#;
        let result = load_temp_config("test_bad_format_linters.toml", toml_content);
        assert!(result.is_err());
        match result.unwrap_err() {
            ConfigError::ValidationError(msg) => {
                assert!(msg.contains("invalid output_format"));
            }
            other => panic!("expected ValidationError, got {other:?}"),
        }
    }

    #[test]
    fn test_load_linter_config_valid() {
        let toml_content = r#"
[linters.ruff]
name = "ruff"
cmd = ["ruff", "check"]
timeout_secs = 60
output_format = "json"
optional = false

[linters.eslint]
name = "eslint"
cmd = ["npx", "eslint", "--format", "json"]
timeout_secs = 90
output_format = "json"
optional = true
"#;
        let result = load_temp_config("test_valid_linters.toml", toml_content);
        assert!(result.is_ok());
        let configs = result.unwrap();
        assert_eq!(configs.len(), 2);
        assert!(configs.contains_key("ruff"));
        assert!(configs.contains_key("eslint"));

        let ruff = &configs["ruff"];
        assert_eq!(ruff.name, "ruff");
        assert_eq!(ruff.cmd, vec!["ruff", "check"]);
        assert_eq!(ruff.timeout_secs, Some(60));
        assert_eq!(ruff.optional, Some(false));
    }
}
