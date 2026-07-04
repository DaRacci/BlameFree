//! Web UI configuration with Linux standard search path.
//!
//! Search order (first match wins):
//! 1. `--config` CLI flag
//! 2. `CRB_WEBUI_CONFIG` environment variable
//! 3. `./webui.toml` (current working directory)
//! 4. `$XDG_CONFIG_HOME/crb-webui/config.toml` (or `~/.config/crb-webui/config.toml`)
//! 5. `/etc/crb-webui/config.toml`
//! 6. Built-in defaults (OAuth disabled)

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Top-level web UI configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebUiConfig {
    #[serde(default)]
    pub server: ServerConfig,
    /// OAuth is disabled by default. Set to `Some(...)` to enable.
    #[serde(default)]
    pub oauth: Option<OAuthConfig>,
}

/// Server binding configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

/// OAuth authentication configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthConfig {
    /// Provider name: "github", "google", or "gitlab".
    pub provider: String,
    /// OAuth App client ID.
    pub client_id: String,
    /// OAuth App client secret.
    pub client_secret: String,
    /// Redirect URL (must match the provider's registered redirect URI).
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

impl Default for WebUiConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            oauth: None,
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
        }
    }
}

/// Load configuration using Linux standard search path.
///
/// `cli_config_path` is the optional `--config` CLI flag value.
pub fn load_config(cli_config_path: Option<&Path>) -> WebUiConfig {
    // 1. --config CLI flag
    if let Some(path) = cli_config_path {
        if path.exists() {
            tracing::info!("Loading config from --config flag: {}", path.display());
            return load_from_file(path).unwrap_or_default();
        }
        tracing::warn!(
            "Config file specified via --config not found: {}",
            path.display()
        );
    }

    // 2. CRB_WEBUI_CONFIG env var
    if let Ok(env_path) = std::env::var("CRB_WEBUI_CONFIG") {
        let path = Path::new(&env_path);
        if path.exists() {
            tracing::info!("Loading config from CRB_WEBUI_CONFIG: {}", path.display());
            return load_from_file(path).unwrap_or_default();
        }
    }

    // 3. ./webui.toml (current working directory)
    let cwd_path = Path::new("webui.toml");
    if cwd_path.exists() {
        tracing::info!("Loading config from ./webui.toml");
        return load_from_file(cwd_path).unwrap_or_default();
    }

    // 4. $XDG_CONFIG_HOME/crb-webui/config.toml
    if let Ok(xdg_home) = std::env::var("XDG_CONFIG_HOME") {
        let xdg_path = Path::new(&xdg_home).join("crb-webui/config.toml");
        if xdg_path.exists() {
            tracing::info!("Loading config from XDG config: {}", xdg_path.display());
            return load_from_file(&xdg_path).unwrap_or_default();
        }
    } else if let Ok(home) = std::env::var("HOME") {
        let fallback_path = Path::new(&home).join(".config/crb-webui/config.toml");
        if fallback_path.exists() {
            tracing::info!("Loading config from ~/.config: {}", fallback_path.display());
            return load_from_file(&fallback_path).unwrap_or_default();
        }
    }

    // 5. /etc/crb-webui/config.toml
    let etc_path = Path::new("/etc/crb-webui/config.toml");
    if etc_path.exists() {
        tracing::info!("Loading config from /etc: {}", etc_path.display());
        return load_from_file(etc_path).unwrap_or_default();
    }

    // 6. Built-in defaults
    tracing::info!("No config file found; using built-in defaults");
    WebUiConfig::default()
}

fn load_from_file(path: &Path) -> Option<WebUiConfig> {
    let content = std::fs::read_to_string(path).ok()?;
    match toml::from_str::<WebUiConfig>(&content) {
        Ok(cfg) => {
            tracing::debug!("Parsed config from {}", path.display());
            Some(cfg)
        }
        Err(e) => {
            tracing::warn!("Failed to parse config file {}: {}", path.display(), e);
            None
        }
    }
}
