use serde::Deserialize;
use std::path::Path;

/// A single MCP server configuration from a TOML config file.
#[derive(Debug, Clone, Deserialize)]
pub struct McpServerConfig {
    /// Human-readable name for this server.
    pub name: String,

    /// Base URL for the MCP server endpoint.
    pub url: String,

    /// Transport protocol (HTTP or stdio).
    #[serde(default)]
    pub transport: McpTransportType,

    /// Whether this server is enabled at startup.
    #[serde(default)]
    pub enabled: bool,
}

/// Transport protocol for connecting to an MCP server.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum McpTransportType {
    /// HTTP (or HTTPS) transport — the default.
    #[default]
    Http,

    /// Stdio transport — spawns a subprocess and communicates via stdin/stdout.
    Stdio,
}

/// Top-level MCP configuration from a TOML config file.
#[derive(Debug, Clone, Deserialize)]
pub struct McpConfig {
    /// List of MCP server configurations.
    pub servers: Vec<McpServerConfig>,
}

impl McpConfig {
    /// Load MCP configuration from a TOML file.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_transport_type_default() {
        let transport = McpTransportType::default();
        assert!(matches!(transport, McpTransportType::Http));
    }

    #[test]
    fn test_mcp_transport_type_serde() {
        let http: McpTransportType =
            serde::Deserialize::deserialize(serde::de::value::StrDeserializer::<
                serde::de::value::Error,
            >::new("http"))
            .unwrap();
        assert!(matches!(http, McpTransportType::Http));

        let stdio: McpTransportType =
            serde::Deserialize::deserialize(serde::de::value::StrDeserializer::<
                serde::de::value::Error,
            >::new("stdio"))
            .unwrap();
        assert!(matches!(stdio, McpTransportType::Stdio));
    }

    #[test]
    fn test_mcp_server_config_default_enabled() {
        let config: McpServerConfig = toml::from_str(
            r#"
name = "test"
url = "http://localhost:9999"
"#,
        )
        .unwrap();
        assert_eq!(config.name, "test");
        assert_eq!(config.url, "http://localhost:9999");
        assert!(matches!(config.transport, McpTransportType::Http));
        // enabled should default to false
        assert!(!config.enabled);
    }

    #[test]
    fn test_mcp_config_load_from_str() {
        let toml_str = r#"
[[servers]]
name = "context7"
url = "https://mcp.context7.com/mcp"
transport = "http"
enabled = true
"#;
        let config: McpConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.servers.len(), 1);
        assert_eq!(config.servers[0].name, "context7");
        assert_eq!(config.servers[0].url, "https://mcp.context7.com/mcp");
        assert!(matches!(
            config.servers[0].transport,
            McpTransportType::Http
        ));
        assert!(config.servers[0].enabled);
    }

    #[test]
    fn test_mcp_config_stdio_transport() {
        let toml_str = r#"
[[servers]]
name = "local"
url = "http://localhost:8080"
transport = "stdio"
enabled = true
"#;
        let config: McpConfig = toml::from_str(toml_str).unwrap();
        assert!(matches!(
            config.servers[0].transport,
            McpTransportType::Stdio
        ));
    }

    #[test]
    fn test_mcp_config_load_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("mcp_test.toml");
        std::fs::write(
            &config_path,
            r#"
[[servers]]
name = "file-test"
url = "https://example.com/mcp"
transport = "http"
enabled = true
"#,
        )
        .unwrap();

        let config = McpConfig::load(&config_path).unwrap();
        assert_eq!(config.servers.len(), 1);
        assert_eq!(config.servers[0].name, "file-test");
    }

    #[test]
    fn test_mcp_config_load_nonexistent_file() {
        let result = McpConfig::load(Path::new("/tmp/nonexistent_config_file.toml"));
        assert!(result.is_err());
    }
}
