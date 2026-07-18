//! Web UI configuration with Linux standard search path.
//!
//! Search order (first match wins):
//! 1. `--config` CLI flag
//! 2. `CRB_WEBUI_CONFIG` environment variable
//! 3. `./webui.toml` (current working directory)
//! 4. `$XDG_CONFIG_HOME/crb-webui/config.toml` (or `~/.config/crb-webui/config.toml`)
//! 5. `/etc/crb-webui/config.toml`
//! 6. Built-in defaults (OAuth disabled)

use std::{env, fs, path::Path};

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::auth::OAuthProvider;

/// Top-level web UI configuration.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct WebUiConfig {
    #[serde(default)]
    pub server: ServerConfig,

    /// OAuth is disabled by default. Set to `Some(...)` to enable.
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
        warn!(
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

    let cwd_path = Path::new("webui.toml");
    if cwd_path.exists() {
        info!("Loading config from ./webui.toml");
        return load_from_file(cwd_path).unwrap_or_default();
    }

    if let Ok(xdg_home) = env::var("XDG_CONFIG_HOME") {
        let xdg_path = Path::new(&xdg_home).join("crb-webui/config.toml");
        if xdg_path.exists() {
            info!("Loading config from XDG config: {}", xdg_path.display());
            return load_from_file(&xdg_path).unwrap_or_default();
        }
    } else if let Ok(home) = env::var("HOME") {
        let fallback_path = Path::new(&home).join(".config/crb-webui/config.toml");
        if fallback_path.exists() {
            info!("Loading config from ~/.config: {}", fallback_path.display());
            return load_from_file(&fallback_path).unwrap_or_default();
        }
    }

    let etc_path = Path::new("/etc/crb-webui/config.toml");
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
    fn test_config_default() {
        let cfg = WebUiConfig::default();
        insta::assert_debug_snapshot!(cfg.server.host);
        insta::assert_debug_snapshot!(cfg.server.port);
        insta::assert_debug_snapshot!(cfg.oauth.is_none());
    }

    #[test]
    fn test_server_config_default() {
        let cfg = ServerConfig::default();
        insta::assert_debug_snapshot!(cfg.host);
        insta::assert_debug_snapshot!(cfg.port);
    }

    #[test]
    fn test_oauth_config_default_scopes() {
        // Check that the serde default works when scopes are omitted
        let toml_str = r#"
            provider = "github"
            client_id = "id"
            client_secret = "secret"
            redirect_url = "http://localhost:8080/callback"
        "#;
        let deserialized: OAuthConfig =
            toml::from_str(toml_str).expect("should deserialize with scopes default");
        insta::assert_debug_snapshot!(deserialized.scopes);
    }

    #[test]
    fn test_config_serde_roundtrip() {
        let cfg = WebUiConfig {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 9090,
            },
            oauth: Some(OAuthConfig {
                provider: OAuthProvider::GitHub,
                client_id: "my-client".to_string(),
                client_secret: "my-secret".to_string(),
                redirect_url: "http://localhost:9090/callback".to_string(),
                scopes: vec!["repo".to_string()],
            }),
        };
        let toml_str = toml::to_string(&cfg).expect("serialize");
        let deserialized: WebUiConfig = toml::from_str(&toml_str).expect("deserialize");
        insta::assert_debug_snapshot!(deserialized);
    }

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
        let cfg: WebUiConfig = toml::from_str(toml_str).expect("valid TOML should parse");
        insta::assert_debug_snapshot!(cfg);
    }

    #[test]
    fn test_config_load_minimal() {
        // Minimal config — server defaults, no oauth
        let toml_str = r#""#;
        let cfg: WebUiConfig =
            toml::from_str(toml_str).expect("empty TOML should parse with defaults");
        insta::assert_debug_snapshot!(cfg);
    }

    #[test]
    fn test_config_load_invalid() {
        let toml_str = r#"not valid toml {{"#;
        let result: Result<WebUiConfig, toml::de::Error> = toml::from_str(toml_str);
        insta::assert_debug_snapshot!(result.is_err());
    }

    #[test]
    fn test_config_save_and_load() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("config.toml");

        let cfg = WebUiConfig {
            server: ServerConfig {
                host: "10.0.0.1".to_string(),
                port: 8080,
            },
            oauth: Some(OAuthConfig {
                provider: OAuthProvider::GitLab,
                client_id: "gl-id".to_string(),
                client_secret: "gl-secret".to_string(),
                redirect_url: "http://10.0.0.1:8080/callback".to_string(),
                scopes: vec!["api".to_string()],
            }),
        };

        // Write
        let toml_str = toml::to_string(&cfg).expect("serialize");
        std::fs::write(&path, &toml_str).expect("write file");

        // Read back
        let loaded = load_from_file(&path).expect("load from file");
        insta::assert_debug_snapshot!(loaded);
    }

    #[test]
    fn test_config_load_from_file_nonexistent() {
        let result = load_from_file(Path::new("/nonexistent/path/config.toml"));
        insta::assert_debug_snapshot!(result.is_none());
    }

    #[test]
    fn test_default_host() {
        insta::assert_debug_snapshot!(default_host());
    }

    #[test]
    fn test_default_port() {
        insta::assert_debug_snapshot!(default_port());
    }

    #[test]
    fn test_default_scopes() {
        insta::assert_debug_snapshot!(default_scopes());
    }
}
