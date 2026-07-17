//! MCP (Model Context Protocol) tool integration.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use rig_compose::{registry::KernelError, tool::ToolSchema};
use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{info, warn};

use crate::{
    error::McpError,
    mcp::config::{McpConfig, McpServerConfig, McpTransportType},
};

/// Arguments for a rig-core MCP tool invocation.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct McpArgs {
    /// JSON-encoded arguments for the MCP tool.
    pub arguments: String,
}

/// Response from an MCP tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResponse {
    /// Whether the call succeeded.
    pub success: bool,

    /// Result content (JSON string).
    pub result: String,

    /// Error message if not successful.
    pub error: Option<String>,
}

/// An HTTP MCP transport that talks to a remote MCP server via JSON-RPC 2.0
/// over HTTP POST.
///
/// Implements [`rig_mcp::transport::McpTransport`] so it can be used with
/// the rig-mcp tool discovery and invocation machinery.
pub struct HttpTransport {
    endpoint: String,
    client: reqwest::Client,
    timeout: Duration,
}

impl HttpTransport {
    /// Create a new HTTP transport for the given MCP server URL.
    pub fn new(url: &str, timeout: Duration) -> Self {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_default();

        Self {
            endpoint: url.to_string(),
            client,
            timeout,
        }
    }

    /// Check a JSON-RPC response body for an error field,
    /// returning [`KernelError::ToolFailed`] if one is present.
    fn check_json_rpc_error(body: &Value) -> Result<(), KernelError> {
        if let Some(error) = body.get("error") {
            let msg = error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown MCP error");
            return Err(KernelError::ToolFailed(msg.to_string()));
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl rig_mcp::transport::McpTransport for HttpTransport {
    fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Discover tools from the MCP server via `list_tools`.
    async fn list_tools(&self) -> Result<Vec<ToolSchema>, KernelError> {
        let url = format!("{}/list_tools", self.endpoint);

        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "list_tools",
            "id": 1,
        });

        let response = tokio::time::timeout(self.timeout, async {
            self.client.post(&url).json(&payload).send().await
        })
        .await
        .map_err(|_| KernelError::ToolFailed("MCP list_tools timed out".into()))?
        .map_err(|e| KernelError::ToolFailed(format!("MCP list_tools request failed: {e}")))?;

        let body: Value = response.json().await.map_err(|e| {
            KernelError::ToolFailed(format!("MCP list_tools response parse failed: {e}"))
        })?;

        Self::check_json_rpc_error(&body)?;

        // Extract tool schemas from result.tools
        let tools = body
            .get("result")
            .and_then(|r| r.get("tools"))
            .and_then(|t| t.as_array())
            .ok_or_else(|| {
                KernelError::ToolFailed("MCP list_tools: no 'result.tools' in response".into())
            })?;

        let schemas: Result<Vec<_>, _> = tools
            .iter()
            .map(|t| {
                let name = t
                    .get("name")
                    .and_then(|n| n.as_str())
                    .ok_or_else(|| KernelError::ToolFailed("MCP tool missing 'name'".into()))?;
                let description = t
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("")
                    .to_string();
                let input_schema = t
                    .get("input_schema")
                    .cloned()
                    .unwrap_or(serde_json::json!({}));

                Ok::<_, KernelError>(ToolSchema {
                    name: name.to_string(),
                    description,
                    args_schema: input_schema,
                    result_schema: serde_json::json!({}),
                })
            })
            .collect();

        Ok(schemas?)
    }

    /// Call a tool on the MCP server via `call_tool`.
    async fn call_tool(&self, name: &str, args: Value) -> Result<Value, KernelError> {
        let url = format!("{}/call_tool", self.endpoint);

        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "call_tool",
            "params": {
                "name": name,
                "arguments": args,
            },
            "id": 2,
        });

        let response = tokio::time::timeout(self.timeout, async {
            self.client
                .post(&url)
                .headers({
                    let mut h = reqwest::header::HeaderMap::new();
                    h.insert(
                        reqwest::header::CONTENT_TYPE,
                        reqwest::header::HeaderValue::from_static("application/json"),
                    );
                    h
                })
                .json(&payload)
                .send()
                .await
        })
        .await
        .map_err(|_| KernelError::ToolFailed("MCP call_tool timed out".into()))?
        .map_err(|e| KernelError::ToolFailed(format!("MCP call_tool request failed: {e}")))?;

        let body: Value = response.json().await.map_err(|e| {
            KernelError::ToolFailed(format!("MCP call_tool response parse failed: {e}"))
        })?;

        Self::check_json_rpc_error(&body)?;

        // Extract result
        let result = body.get("result").cloned().unwrap_or(Value::Null);

        Ok(result)
    }
}

/// A [`rig_core::tool::Tool`] wrapper around an MCP transport + tool schema.
///
/// Each instance wraps a single discovered tool from a remote MCP server.
pub struct RigCoreMcpTool {
    /// Full display name for this tool (e.g. "context7_web_search").
    tool_name: String,
    /// Tool schema from the MCP server's `list_tools`.
    schema: ToolSchema,
    /// Transport to the MCP server.
    transport: Arc<dyn rig_mcp::transport::McpTransport>,
}

impl RigCoreMcpTool {
    /// Create a new `RigCoreMcpTool` wrapping a transport and schema.
    pub fn new(
        server_name: &str,
        transport: Arc<dyn rig_mcp::transport::McpTransport>,
        schema: ToolSchema,
    ) -> Self {
        let tool_name = format!("{}_{}", server_name, schema.name);
        Self {
            tool_name,
            schema,
            transport,
        }
    }
}

impl Tool for RigCoreMcpTool {
    const NAME: &'static str = "mcp";

    type Error = McpError;
    type Args = McpArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.tool_name.clone(),
            description: self.schema.description.clone(),
            parameters: self.schema.args_schema.clone(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let parsed_args: Value = serde_json::from_str(&args.arguments).map_err(|e| {
            McpError::ToolError(format!(
                "invalid JSON arguments for '{}': {e}",
                self.tool_name
            ))
        })?;

        let result = self
            .transport
            .call_tool(&self.schema.name, parsed_args)
            .await
            .map_err(|e| {
                McpError::ToolError(format!("MCP tool '{}' call failed: {e}", self.schema.name))
            })?;

        let result_str = result.to_string();

        Ok(result_str)
    }
}

/// Load MCP configuration, connect to enabled servers, and return
/// ready-to-use [`RigCoreMcpTool`] instances.
///
/// Each enabled server has its tools discovered (via `list_tools`) and
/// wrapped as individual [`RigCoreMcpTool`] instances. The result is a
/// flat list of tools across all servers.
///
/// This is a synchronous function that blocks the current thread to
/// perform async discovery. Call from an async context via
/// `tokio::task::spawn_blocking` if needed.
#[allow(clippy::cognitive_complexity)]
pub fn load_mcp_tools(config_path: &Path) -> Result<Vec<RigCoreMcpTool>, McpError> {
    let config = McpConfig::load(config_path)
        .map_err(|e| McpError::ConfigError(format!("Failed to load MCP config: {e}")))?;

    let enabled_servers: Vec<&McpServerConfig> =
        config.servers.iter().filter(|s| s.enabled).collect();

    if enabled_servers.is_empty() {
        info!("No enabled MCP servers in config");
        return Ok(Vec::new());
    }

    let rt = tokio::runtime::Handle::try_current().map_err(|_| {
        McpError::ConfigError("No Tokio runtime available; cannot discover MCP tools".into())
    })?;

    let mut all_tools: Vec<RigCoreMcpTool> = Vec::new();

    for server in &enabled_servers {
        info!(
            "Connecting to MCP server '{}' at {}",
            server.name, server.url
        );

        let transport: Arc<dyn rig_mcp::transport::McpTransport> = match server.transport {
            McpTransportType::Http => {
                Arc::new(HttpTransport::new(&server.url, Duration::from_secs(30)))
            }
            McpTransportType::Stdio => {
                warn!(
                    "Stdio transport for MCP server '{}' not yet implemented; skipping",
                    server.name
                );
                continue;
            }
        };

        let schemas = rt.block_on(async { transport.list_tools().await });

        match schemas {
            Ok(schemas) => {
                info!(
                    "Discovered {} tool(s) from MCP server '{}'",
                    schemas.len(),
                    server.name
                );

                for schema in schemas {
                    let tool = RigCoreMcpTool::new(&server.name, Arc::clone(&transport), schema);
                    all_tools.push(tool);
                }
            }
            Err(e) => {
                warn!(
                    "Failed to discover tools from MCP server '{}': {e}",
                    server.name
                );
            }
        }
    }

    Ok(all_tools)
}

/// Check whether an MCP config file exists and has enabled servers.
pub fn has_enabled_servers(config_path: &Path) -> bool {
    if !config_path.exists() {
        return false;
    }
    match McpConfig::load(config_path) {
        Ok(cfg) => cfg.servers.iter().any(|s| s.enabled),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rig_mcp::transport::McpTransport;

    #[test]
    fn test_mcp_error_display() {
        let err = McpError::TransportError("connection refused".into());
        assert!(err.to_string().contains("MCP transport error"));

        let err = McpError::TimeoutElapsed;
        assert_eq!(err.to_string(), "MCP request timed out");

        let err = McpError::ToolError("internal error".into());
        assert!(err.to_string().contains("MCP tool error"));

        let err = McpError::ConfigError("bad config".into());
        assert!(err.to_string().contains("MCP configuration error"));
    }

    #[test]
    fn test_mcp_args_serde() {
        let json = r#"{"arguments": "{\"query\": \"hello\"}"}"#;
        let args: McpArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.arguments, r#"{"query": "hello"}"#);
    }

    #[test]
    fn test_mcp_response_serde() {
        let resp = McpResponse {
            success: true,
            result: r#"{"answer": 42}"#.into(),
            error: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\\\"answer\\\""));
        assert!(json.contains("\"success\":true"));
    }

    #[test]
    fn test_http_transport_endpoint() {
        let transport = HttpTransport::new("http://localhost:8080/mcp", Duration::from_secs(10));
        assert_eq!(transport.endpoint(), "http://localhost:8080/mcp");
    }

    #[test]
    fn test_has_enabled_servers_no_file() {
        assert!(!has_enabled_servers(Path::new(
            "/tmp/nonexistent_mcp_file.toml"
        )));
    }

    #[test]
    fn test_check_json_rpc_error_no_error_field() {
        let body = serde_json::json!({"result": {"tools": []}});
        let result = HttpTransport::check_json_rpc_error(&body);
        insta::assert_debug_snapshot!(result.is_ok());
    }

    #[test]
    fn test_check_json_rpc_error_with_error_field() {
        let body = serde_json::json!({
            "error": {"message": "Method not found", "code": -32601}
        });
        let result = HttpTransport::check_json_rpc_error(&body);
        insta::assert_debug_snapshot!(result.is_err());
        insta::assert_snapshot!(result.unwrap_err().to_string());
    }

    #[test]
    fn test_load_mcp_tools_no_file() {
        let tools = load_mcp_tools(Path::new("/tmp/nonexistent_mcp_file.toml"));
        assert!(tools.is_err());
    }
}
