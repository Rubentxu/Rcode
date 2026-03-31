//! MCP tool adapter - exposes MCP tools as regular tools

use std::sync::Arc;
use async_trait::async_trait;
use serde_json::Value;

use opencode_core::{Tool, ToolContext, ToolResult, error::Result};

use opencode_mcp::McpServerRegistry;

/// Adapter that wraps MCP tools as regular tools
pub struct McpToolAdapter {
    registry: Arc<McpServerRegistry>,
}

impl McpToolAdapter {
    /// Create a new MCP tool adapter
    pub fn new(registry: Arc<McpServerRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Tool for McpToolAdapter {
    fn id(&self) -> &str {
        "mcp"
    }

    fn name(&self) -> &str {
        "MCP Tools"
    }

    fn description(&self) -> &str {
        "Execute tools from MCP (Model Context Protocol) servers"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "server": {
                    "type": "string",
                    "description": "Name of the MCP server"
                },
                "tool": {
                    "type": "string",
                    "description": "Name of the tool to execute"
                },
                "params": {
                    "type": "object",
                    "description": "Parameters to pass to the tool",
                    "additionalProperties": true
                }
            },
            "required": ["server", "tool"]
        })
    }

    async fn execute(&self, args: Value, _context: &ToolContext) -> Result<ToolResult> {
        let server = args["server"]
            .as_str()
            .ok_or_else(|| opencode_core::OpenCodeError::Tool("Missing 'server' argument".into()))?;
        
        let tool = args["tool"]
            .as_str()
            .ok_or_else(|| opencode_core::OpenCodeError::Tool("Missing 'tool' argument".into()))?;
        
        let params = args["params"].clone();

        // Get the MCP client for this server
        let client = self.registry.get_server(server)
            .await
            .ok_or_else(|| opencode_core::OpenCodeError::Tool(format!("MCP server '{}' not found", server)))?;

        let mut client = client.write().await;

        // Call the tool on the MCP server
        let result = client.call_tool(tool, params).await
            .map_err(|e| opencode_core::OpenCodeError::Tool(format!("MCP tool call failed: {}", e)))?;

        let content = result.content_to_string();

        Ok(ToolResult {
            title: tool.to_string(),
            content,
            metadata: Some(serde_json::json!({
                "server": server,
                "tool": tool,
                "is_error": result.is_error,
            })),
            attachments: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use opencode_core::ToolContext;

    fn test_context() -> ToolContext {
        ToolContext {
            session_id: "test".to_string(),
            project_path: std::path::PathBuf::from("/tmp"),
            cwd: std::path::PathBuf::from("/tmp"),
            user_id: None,
            agent: "test-agent".to_string(),
        }
    }

    #[tokio::test]
    async fn test_mcp_tool_adapter_id() {
        let registry = Arc::new(McpServerRegistry::new());
        let adapter = McpToolAdapter::new(registry);
        
        assert_eq!(adapter.id(), "mcp");
        assert_eq!(adapter.name(), "MCP Tools");
    }

    #[tokio::test]
    async fn test_mcp_tool_adapter_missing_server() {
        let registry = Arc::new(McpServerRegistry::new());
        let adapter = McpToolAdapter::new(registry);
        
        let args = serde_json::json!({
            "tool": "some_tool",
            "params": {}
        });

        let result = adapter.execute(args, &test_context()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mcp_tool_adapter_missing_tool() {
        let registry = Arc::new(McpServerRegistry::new());
        let adapter = McpToolAdapter::new(registry);
        
        let args = serde_json::json!({
            "server": "test-server",
            "params": {}
        });

        let result = adapter.execute(args, &test_context()).await;
        assert!(result.is_err());
    }
}
