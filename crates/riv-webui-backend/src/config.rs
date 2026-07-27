//! Web UI configuration with Linux standard search path.
//!
//! Search order (first match wins):
//! 1. `--config` CLI flag
//! 2. `CRB_WEBUI_CONFIG` environment variable
//! 3. `./webui.toml` (current working directory)
//! 4. `$XDG_CONFIG_HOME/riv-webui/config.toml` (or `~/.config/riv-webui/config.toml`)
//! 5. `/etc/riv-webui/config.toml`
//! 6. Built-in defaults (OAuth disabled)

use std::{env, fs, path::Path, path::PathBuf};

use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use crate::auth::OAuthProvider;

const FOLDER: &str = "riv";
const FILENAME: &str = "config.toml";

/// Top-level web UI configuration.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct WebUiConfig {
    #[serde(default)]
    pub server: ServerConfig,

    /// The configured OAuth config.
    ///
    /// If `None`, OAuth authentication is disabled.
    #[serde(default)]
    pub oauth: Option<OAuthConfig>,
}

/// Server binding configuration.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,

    #[serde(default = "default_port")]
    pub port: u16,

    /// Directory containing datasets.
    #[serde(default)]
    pub dataset_dir: PathBuf,

    /// Path to the code-review-benchmark directory.
    #[serde(default)]
    pub benchmark_dir: Option<PathBuf>,

    /// Path for the riv-stor database file.
    ///
    /// Defaults to `riv-stor.db` in the output directory.
    #[serde(default)]
    pub store_dir: Option<PathBuf>,
}

/// OAuth authentication configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthConfig {
    /// Provider
    pub provider: OAuthProvider,

    /// OAuth App client ID.
    pub client_id: String,

    /// OAuth App client secret.
    pub client_secret: String,

    /// Redirect URL for OAuth callback.
    pub redirect_url: String,

    /// OAuth scopes to request.
    #[serde(default = "default_scopes")]
    pub scopes: Vec<String>,
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_scopes() -> Vec<String> {
    vec!["read:user".to_string(), "user:email".to_string()]
}

/// Load configuration using Linux standard search path.
///
/// `cli_config_path` is the optional `--config` CLI flag value.
pub fn load_config(cli_config_path: Option<&Path>) -> WebUiConfig {
    if let Some(path) = cli_config_path {
        if path.exists() {
            info!("Loading config from --config flag: {}", path.display());
            return load_from_file(path).unwrap_or_default();
        }
        error!(
            "Config file specified via --config not found: {}",
            path.display()
        );
    }

    if let Ok(env_path) = env::var("CRB_WEBUI_CONFIG") {
        let path = Path::new(&env_path);
        if path.exists() {
            info!("Loading config from CRB_WEBUI_CONFIG: {}", path.display());
            return load_from_file(path).unwrap_or_default();
        }
    }

    let cwd_path = Path::new(FILENAME);
    if cwd_path.exists() {
        info!("Loading config from ./{FILENAME}");
        return load_from_file(cwd_path).unwrap_or_default();
    }

    const ENV_OPTIONS: &[&str] = &["XDG_CONFIG_HOME", "HOME"];
    for option in ENV_OPTIONS {
        let Ok(env_value) = env::var(option) else {
            continue;
        };

        let config_path = Path::new(&env_value).join(format!("{FOLDER}/{FILENAME}"));
        if config_path.exists() {
            info!("Loading config from {option}: {}", config_path.display());
            return load_from_file(&config_path).unwrap_or_default();
        }
    }

    let etc_path = Path::new("/etc/riv-webui/config.toml");
    if etc_path.exists() {
        info!("Loading config from /etc: {}", etc_path.display());
        return load_from_file(etc_path).unwrap_or_default();
    }

    info!("No config file found; using defaults");
    WebUiConfig::default()
}

fn load_from_file(path: &Path) -> Option<WebUiConfig> {
    let content = fs::read_to_string(path).ok()?;
    match toml::from_str::<WebUiConfig>(&content) {
        Ok(cfg) => {
            debug!("Parsed config from {}", path.display());
            Some(cfg)
        }
        Err(e) => {
            warn!("Failed to parse config file {}: {}", path.display(), e);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_load_valid() {
        let toml_str = r#"
            [server]
            host = "192.168.1.1"
            port = 3000

            [oauth]
            provider = "google"
            client_id = "google-client"
            client_secret = "google-secret"
            redirect_url = "http://localhost:3000/callback"
        "#;
        assert!(
            toml::from_str::<WebUiConfig>(toml_str).is_ok(),
            "valid TOML should parse"
        );
    }

    #[test]
    fn test_config_load_invalid() {
        let toml_str = r#"not valid toml {{"#;
        let result: Result<WebUiConfig, toml::de::Error> = toml::from_str(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_load_from_file_nonexistent() {
        let result = load_from_file(Path::new("/nonexistent/path/config.toml"));
        assert!(result.is_none());
    }
}
