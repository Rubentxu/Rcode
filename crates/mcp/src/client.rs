//! MCP client implementation

use serde_json::Value;

use super::error::{McpError, Result};
use super::transport::{HttpTransport, McpTransport, StdioTransport};
use super::types::{
    JsonRpcRequest, JsonRpcResponse, McpCapabilities,
    McpServerInfo, McpTool, McpToolResult,
};

/// MCP client for connecting to MCP servers
#[allow(dead_code)]
pub struct McpClient {
    transport: Box<dyn McpTransport>,
    protocol_version: String,
    server_info: Option<McpServerInfo>,
    capabilities: McpCapabilities,
}

impl McpClient {
    /// Connect to an MCP server via HTTP
    pub async fn connect_http(url: &str) -> Result<Self> {
        let transport = HttpTransport::new(url);
        let mut client = Self {
            transport: Box::new(transport),
            protocol_version: "2024-11-05".to_string(),
            server_info: None,
            capabilities: McpCapabilities::default(),
        };

        client.initialize().await?;
        Ok(client)
    }

    /// Connect to an MCP server via stdio (process)
    pub async fn connect_stdio(command: &str, args: &[&str]) -> Result<Self> {
        let transport = StdioTransport::spawn(command, args).await?;
        let mut client = Self {
            transport: Box::new(transport),
            protocol_version: "2024-11-05".to_string(),
            server_info: None,
            capabilities: McpCapabilities::default(),
        };

        client.initialize().await?;
        Ok(client)
    }

    /// Initialize the connection with the MCP server
    async fn initialize(&mut self) -> Result<()> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "initialize".to_string(),
            params: Some(serde_json::json!({
                "protocolVersion": self.protocol_version,
                "capabilities": {},
                "clientInfo": {
                    "name": "rcode",
                    "version": "0.1.0"
                }
            })),
            id: Some(Value::Number(0.into())),
        };

        self.send_json_rpc(request).await?;

        // Send initialized notification
        let notification = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "notifications/initialized".to_string(),
            params: None,
            id: None,
        };
        self.send_json_rpc(notification).await?;

        Ok(())
    }

    /// Send a JSON-RPC request and get the response
    async fn send_json_rpc(&mut self, request: JsonRpcRequest) -> Result<JsonRpcResponse> {
        self.transport.send(request).await?;
        let response = self.transport.receive().await?;

        if let Some(error) = response.error {
            return Err(McpError::JsonRpc(format!(
                "Code {}: {}",
                error.code, error.message
            )));
        }

        Ok(response)
    }

    /// List available tools from the MCP server
    pub async fn list_tools(&mut self) -> Result<Vec<McpTool>> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "tools/list".to_string(),
            params: None,
            id: Some(Value::Number(1.into())),
        };

        let response = self.send_json_rpc(request).await?;

        let result = response.result.ok_or_else(|| {
            McpError::InvalidResponse("No result in response".to_string())
        })?;

        // Parse the tools list from the result
        let tools: Vec<McpTool> = if let Some(tools_array) = result.get("tools").and_then(|t| t.as_array()) {
            tools_array
                .iter()
                .filter_map(|t| serde_json::from_value(t.clone()).ok())
                .collect()
        } else {
            Vec::new()
        };

        Ok(tools)
    }

    /// Call a tool on the MCP server
    pub async fn call_tool(
        &mut self,
        name: &str,
        arguments: Value,
    ) -> Result<McpToolResult> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({
                "name": name,
                "arguments": arguments
            })),
            id: Some(Value::Number(2.into())),
        };

        let response = self.send_json_rpc(request).await?;

        let result = response.result.ok_or_else(|| {
            McpError::InvalidResponse("No result in response".to_string())
        })?;

        let tool_result: McpToolResult = serde_json::from_value(result)?;
        Ok(tool_result)
    }

    /// Get the server info
    pub fn server_info(&self) -> Option<&McpServerInfo> {
        self.server_info.as_ref()
    }

    /// Get the protocol version
    pub fn protocol_version(&self) -> &str {
        &self.protocol_version
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_json_rpc_request_serialization() {
        let request = JsonRpcRequest::new("tools/list");
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"method\":\"tools/list\""));
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
    }

    #[tokio::test]
    async fn test_mcp_tool_deserialization() {
        let json = r#"{
            "name": "test_tool",
            "description": "A test tool",
            "inputSchema": {"type": "object"}
        }"#;
        let tool: McpTool = serde_json::from_str(json).unwrap();
        assert_eq!(tool.name, "test_tool");
    }
}
