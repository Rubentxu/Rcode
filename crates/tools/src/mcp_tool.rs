//! MCP tool adapter - exposes MCP tools as regular tools
//! 
//! This module provides two MCP tool implementations:
//! 1. [`McpToolAdapter`] - Legacy single adapter using "mcp" tool ID (deprecated)
//! 2. [`McpToolBridge`] - Individual first-class tools with ID format `mcp/{server}/{tool}`
//! 
//! Use [`McpToolBridge::register_tools_for_server`] to dynamically register MCP server tools
//! and [`McpToolBridge::unregister_tools_for_server`] to unregister them.

use std::sync::Arc;
use std::time::Duration;
use async_trait::async_trait;
use serde_json::Value;

use rcode_core::{Tool, ToolContext, ToolResult, error::Result};

use rcode_mcp::McpServerRegistry;

/// Default timeout for MCP tool execution (60 seconds)
const DEFAULT_MCP_TOOL_TIMEOUT_SECS: u64 = 60;

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

    async fn execute(&self, args: Value, context: &ToolContext) -> Result<ToolResult> {
        let server = args["server"]
            .as_str()
            .ok_or_else(|| rcode_core::RCodeError::Tool("Missing 'server' argument".into()))?;
        
        let tool = args["tool"]
            .as_str()
            .ok_or_else(|| rcode_core::RCodeError::Tool("Missing 'tool' argument".into()))?;
        
        let params = args["params"].clone();

        // Get the MCP client for this server
        let client = self.registry.get_server(server)
            .await
            .ok_or_else(|| rcode_core::RCodeError::Tool(format!("MCP server '{}' not found", server)))?;

        let mut client = client.write().await;

        // Call the tool on the MCP server, passing session context
        let result = client.call_tool(tool, params, Some(context.session_id.clone())).await
            .map_err(|e| rcode_core::RCodeError::Tool(format!("MCP tool call failed: {}", e)))?;

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

// ============================================================================
// McpToolBridge - Individual MCP tool as first-class Tool
// ============================================================================

/// Bridge that wraps a single MCP tool as a first-class Tool.
///
/// Each MCP tool from each connected server is exposed as its own tool with
/// ID format `mcp/{server_name}/{tool_name}`. This allows the LLM to discover
/// and use MCP tools naturally without knowing server names.
///
/// # Example
///
/// ```ignore
/// let bridge = McpToolBridge::new(
///     "exa".to_string(),
///     "search".to_string(),
///     "Search the web".to_string(),
///     input_schema,
///     mcp_registry,
/// );
/// // bridge.id() == "mcp/exa/search"
/// ```
pub struct McpToolBridge {
    /// Pre-computed tool ID: `mcp/{server_name}/{tool_name}`
    tool_id: String,
    /// Name of the MCP server (e.g., "exa", "filesystem")
    server_name: String,
    /// Name of the tool on the MCP server (e.g., "search", "read")
    tool_name: String,
    /// Human-readable description of the tool
    description: String,
    /// JSON Schema for the tool's input parameters
    input_schema: Value,
    /// Reference to the MCP server registry for executing tools
    mcp_registry: Arc<McpServerRegistry>,
    /// Timeout for tool execution
    timeout: Duration,
}

impl McpToolBridge {
    /// Create a new MCP tool bridge with default timeout (60 seconds).
    ///
    /// # Arguments
    /// * `server_name` - Name of the MCP server
    /// * `tool_name` - Name of the tool on the MCP server
    /// * `description` - Human-readable description
    /// * `input_schema` - JSON Schema for tool parameters
    /// * `mcp_registry` - Reference to the MCP server registry
    pub fn new(
        server_name: String,
        tool_name: String,
        description: String,
        input_schema: Value,
        mcp_registry: Arc<McpServerRegistry>,
    ) -> Self {
        let tool_id = format!("mcp/{}/{}", server_name, tool_name);
        Self {
            tool_id,
            server_name,
            tool_name,
            description,
            input_schema,
            mcp_registry,
            timeout: Duration::from_secs(DEFAULT_MCP_TOOL_TIMEOUT_SECS),
        }
    }

    /// Create a new MCP tool bridge with custom timeout.
    ///
    /// # Arguments
    /// * `server_name` - Name of the MCP server
    /// * `tool_name` - Name of the tool on the MCP server
    /// * `description` - Human-readable description
    /// * `input_schema` - JSON Schema for tool parameters
    /// * `mcp_registry` - Reference to the MCP server registry
    /// * `timeout` - Custom timeout duration for tool execution
    pub fn with_timeout(
        server_name: String,
        tool_name: String,
        description: String,
        input_schema: Value,
        mcp_registry: Arc<McpServerRegistry>,
        timeout: Duration,
    ) -> Self {
        let tool_id = format!("mcp/{}/{}", server_name, tool_name);
        Self {
            tool_id,
            server_name,
            tool_name,
            description,
            input_schema,
            mcp_registry,
            timeout,
        }
    }

    /// Get the server name for this MCP tool.
    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    /// Get the tool name for this MCP tool.
    pub fn tool_name(&self) -> &str {
        &self.tool_name
    }

    /// Get the timeout duration for this tool.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Generate the tool ID prefix for a given server name.
    ///
    /// Returns `mcp/{server_name}/`.
    pub fn tool_id_prefix(server_name: &str) -> String {
        format!("mcp/{}/", server_name)
    }

    /// Check if a tool ID belongs to an MCP individual tool.
    ///
    /// Returns true if the ID starts with `mcp/` and has at least one `/`
    /// after the prefix (i.e., `mcp/{server}/{tool}`).
    pub fn is_mcp_tool_id(tool_id: &str) -> bool {
        tool_id.starts_with("mcp/") && tool_id.matches('/').count() >= 2
    }

    /// Extract the server name from an MCP tool ID.
    ///
    /// # Example
    /// 
    /// ```
    /// // For "mcp/exa/search", returns Some("exa")
    /// // For "bash", returns None
    /// ```
    pub fn extract_server_name(tool_id: &str) -> Option<String> {
        if !tool_id.starts_with("mcp/") {
            return None;
        }
        let remainder = &tool_id[4..]; // Remove "mcp/" prefix
        let parts: Vec<&str> = remainder.split('/').collect();
        if parts.is_empty() {
            None
        } else {
            Some(parts[0].to_string())
        }
    }

    /// Register all tools from an MCP server into the tool registry.
    ///
    /// This method enumerates all tools from the specified MCP server using
    /// `list_server_tools()` and registers each as an `McpToolBridge` in
    /// the `ToolRegistryService`.
    ///
    /// # Arguments
    /// * `server_name` - Name of the MCP server
    /// * `tool_registry` - Reference to the ToolRegistryService
    ///
    /// # Returns
    /// Number of tools registered, or error if enumeration failed
    ///
    /// # Notes
    /// - Tools are registered with ID format `mcp/{server_name}/{tool_name}`
    /// - Existing tools with the same ID are replaced
    /// - This is called when an MCP server connects
    pub async fn register_tools_for_server(
        server_name: &str,
        tool_registry: &crate::ToolRegistryService,
    ) -> rcode_core::error::Result<usize> {
        let mcp_registry = tool_registry.get_mcp_registry()
            .ok_or_else(|| rcode_core::RCodeError::Tool("MCP registry not configured".into()))?;
        
        let tools = mcp_registry.list_server_tools(server_name).await
            .map_err(|e| rcode_core::RCodeError::Tool(format!("Failed to list MCP tools: {}", e)))?;
        
        let count = tools.len();
        for tool in tools {
            let bridge = McpToolBridge::new(
                server_name.to_string(),
                tool.name.clone(),
                tool.description,
                tool.input_schema,
                Arc::clone(&mcp_registry),
            );
            tool_registry.register(Arc::new(bridge));
        }
        
        Ok(count)
    }

    /// Unregister all tools for an MCP server from the tool registry.
    ///
    /// Removes all tools with IDs starting with `mcp/{server_name}/`.
    ///
    /// # Arguments
    /// * `server_name` - Name of the MCP server
    /// * `tool_registry` - Reference to the ToolRegistryService
    ///
    /// # Returns
    /// Number of tools unregistered
    ///
    /// # Notes
    /// - This is called when an MCP server disconnects
    pub fn unregister_tools_for_server(
        server_name: &str,
        tool_registry: &crate::ToolRegistryService,
    ) -> usize {
        let prefix = McpToolBridge::tool_id_prefix(server_name);
        tool_registry.unregister_by_prefix(&prefix)
    }
}

#[async_trait]
impl Tool for McpToolBridge {
    fn id(&self) -> &str {
        &self.tool_id
    }

    fn name(&self) -> &str {
        &self.tool_name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> Value {
        self.input_schema.clone()
    }

    async fn execute(&self, args: Value, context: &ToolContext) -> Result<ToolResult> {
        let result = tokio::time::timeout(
            self.timeout,
            self.mcp_registry.call_tool(
                &self.server_name,
                &self.tool_name,
                args,
                Some(context.session_id.clone()),
            ),
        ).await;
        
        match result {
            Ok(Ok(mcp_result)) => {
                let content = mcp_result.content_to_string();
                Ok(ToolResult {
                    title: self.tool_name.clone(),
                    content,
                    metadata: Some(serde_json::json!({
                        "server": self.server_name,
                        "tool": self.tool_name,
                        "is_error": mcp_result.is_error,
                    })),
                    attachments: vec![],
                })
            }
            Ok(Err(e)) => Err(rcode_core::RCodeError::Tool(format!(
                "MCP tool '{}' on server '{}' failed: {}",
                self.tool_name, self.server_name, e
            ))),
            Err(_) => Err(rcode_core::RCodeError::Timeout {
                duration: self.timeout.as_secs(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::ToolContext;

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

    #[tokio::test]
    async fn test_mcp_tool_adapter_server_not_found() {
        let registry = Arc::new(McpServerRegistry::new());
        let adapter = McpToolAdapter::new(registry);
        
        let args = serde_json::json!({
            "server": "nonexistent-server",
            "tool": "some_tool",
            "params": {}
        });

        let result = adapter.execute(args, &test_context()).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("not found") || err.to_string().contains("nonexistent-server"));
    }

    #[tokio::test]
    async fn test_mcp_tool_adapter_parameters_schema() {
        let registry = Arc::new(McpServerRegistry::new());
        let adapter = McpToolAdapter::new(registry);
        
        let schema = adapter.parameters();
        assert!(schema.is_object());
        let obj = schema.as_object().unwrap();
        assert!(obj.contains_key("properties"));
        
        let props = obj.get("properties").unwrap().as_object().unwrap();
        assert!(props.contains_key("server"));
        assert!(props.contains_key("tool"));
        assert!(props.contains_key("params"));
        
        let required = obj.get("required").unwrap().as_array().unwrap();
        assert!(required.iter().any(|r| r == "server"));
        assert!(required.iter().any(|r| r == "tool"));
    }

    #[tokio::test]
    async fn test_mcp_tool_adapter_new() {
        let registry = Arc::new(McpServerRegistry::new());
        let adapter = McpToolAdapter::new(registry.clone());
        // Should create successfully
        assert_eq!(adapter.id(), "mcp");
        assert_eq!(adapter.name(), "MCP Tools");
        assert_eq!(adapter.description(), "Execute tools from MCP (Model Context Protocol) servers");
    }
}

// ============================================================================
// McpToolBridge tests - Individual MCP tools as first-class tools
// ============================================================================

#[cfg(test)]
mod mcp_tool_bridge_tests {
    use super::*;

    fn test_context() -> ToolContext {
        ToolContext {
            session_id: "test".to_string(),
            project_path: std::path::PathBuf::from("/tmp"),
            cwd: std::path::PathBuf::from("/tmp"),
            user_id: None,
            agent: "test-agent".to_string(),
        }
    }

    #[test]
    fn test_mcp_tool_bridge_id_format() {
        let registry = Arc::new(McpServerRegistry::new());
        let bridge = McpToolBridge::new(
            "exa".to_string(),
            "search".to_string(),
            "Search the web".to_string(),
            serde_json::json!({"type": "object"}),
            Arc::clone(&registry),
        );
        
        assert_eq!(bridge.id(), "mcp/exa/search");
    }

    #[test]
    fn test_mcp_tool_bridge_name() {
        let registry = Arc::new(McpServerRegistry::new());
        let bridge = McpToolBridge::new(
            "exa".to_string(),
            "search".to_string(),
            "Search the web".to_string(),
            serde_json::json!({"type": "object"}),
            Arc::clone(&registry),
        );
        
        assert_eq!(bridge.name(), "search");
    }

    #[test]
    fn test_mcp_tool_bridge_description() {
        let registry = Arc::new(McpServerRegistry::new());
        let bridge = McpToolBridge::new(
            "exa".to_string(),
            "search".to_string(),
            "Search the web for information".to_string(),
            serde_json::json!({"type": "object"}),
            Arc::clone(&registry),
        );
        
        assert_eq!(bridge.description(), "Search the web for information");
    }

    #[test]
    fn test_mcp_tool_bridge_parameters() {
        let registry = Arc::new(McpServerRegistry::new());
        let input_schema = serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query"
                }
            },
            "required": ["query"]
        });
        let bridge = McpToolBridge::new(
            "exa".to_string(),
            "search".to_string(),
            "Search the web".to_string(),
            input_schema.clone(),
            Arc::clone(&registry),
        );
        
        let params = bridge.parameters();
        assert!(params.is_object());
        let obj = params.as_object().unwrap();
        assert!(obj.contains_key("properties"));
    }

    #[test]
    fn test_mcp_tool_bridge_server_name() {
        let registry = Arc::new(McpServerRegistry::new());
        let bridge = McpToolBridge::new(
            "my_server".to_string(),
            "tool_name".to_string(),
            "A tool description".to_string(),
            serde_json::json!({"type": "object"}),
            Arc::clone(&registry),
        );
        
        assert_eq!(bridge.server_name(), "my_server");
    }

    #[test]
    fn test_mcp_tool_bridge_tool_name() {
        let registry = Arc::new(McpServerRegistry::new());
        let bridge = McpToolBridge::new(
            "server".to_string(),
            "my_tool".to_string(),
            "A tool description".to_string(),
            serde_json::json!({"type": "object"}),
            Arc::clone(&registry),
        );
        
        assert_eq!(bridge.tool_name(), "my_tool");
    }

    #[tokio::test]
    async fn test_mcp_tool_bridge_execute_server_not_found() {
        let registry = Arc::new(McpServerRegistry::new());
        let bridge = McpToolBridge::new(
            "nonexistent".to_string(),
            "search".to_string(),
            "Search".to_string(),
            serde_json::json!({"type": "object"}),
            Arc::clone(&registry),
        );
        
        let args = serde_json::json!({"query": "test"});
        let result = bridge.execute(args, &test_context()).await;
        
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("nonexistent") || err.to_string().contains("not found"));
    }

    #[test]
    fn test_mcp_tool_bridge_new_with_defaults() {
        let registry = Arc::new(McpServerRegistry::new());
        let bridge = McpToolBridge::new(
            "exa".to_string(),
            "search".to_string(),
            "Search the web".to_string(),
            serde_json::json!({"type": "object"}),
            Arc::clone(&registry),
        );
        
        // Default timeout should be 60 seconds
        assert_eq!(bridge.timeout(), std::time::Duration::from_secs(60));
    }

    #[test]
    fn test_mcp_tool_bridge_new_with_custom_timeout() {
        let registry = Arc::new(McpServerRegistry::new());
        let bridge = McpToolBridge::with_timeout(
            "exa".to_string(),
            "search".to_string(),
            "Search the web".to_string(),
            serde_json::json!({"type": "object"}),
            Arc::clone(&registry),
            std::time::Duration::from_secs(120),
        );
        
        assert_eq!(bridge.timeout(), std::time::Duration::from_secs(120));
    }
}

// ============================================================================
// Dynamic registration tests
// ============================================================================

#[cfg(test)]
mod dynamic_registration_tests {
    use super::*;

    #[test]
    fn test_tool_id_prefix_for_server() {
        let prefix = McpToolBridge::tool_id_prefix("exa");
        assert_eq!(prefix, "mcp/exa/");
    }

    #[test]
    fn test_is_mcp_tool_id_valid_prefix() {
        assert!(McpToolBridge::is_mcp_tool_id("mcp/exa/search"));
        assert!(McpToolBridge::is_mcp_tool_id("mcp/server/tool"));
    }

    #[test]
    fn test_is_mcp_tool_id_invalid_prefix() {
        assert!(!McpToolBridge::is_mcp_tool_id("bash"));
        assert!(!McpToolBridge::is_mcp_tool_id("mcp"));  // old adapter
        assert!(!McpToolBridge::is_mcp_tool_id("read"));
        assert!(!McpToolBridge::is_mcp_tool_id("mcp_tool_bridge_test"));
    }

    #[test]
    fn test_extract_server_name_from_tool_id() {
        assert_eq!(McpToolBridge::extract_server_name("mcp/exa/search"), Some("exa".to_string()));
        assert_eq!(McpToolBridge::extract_server_name("mcp/server/tool"), Some("server".to_string()));
        assert_eq!(McpToolBridge::extract_server_name("bash"), None);
    }
}
