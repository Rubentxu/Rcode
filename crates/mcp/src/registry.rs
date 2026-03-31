//! MCP server registry for managing multiple MCP server connections

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::client::McpClient;
use super::error::Result;
use super::types::McpTool;

/// Registry for managing multiple MCP server connections
pub struct McpServerRegistry {
    servers: RwLock<HashMap<String, Arc<RwLock<McpClient>>>>,
}

impl McpServerRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            servers: RwLock::new(HashMap::new()),
        }
    }

    /// Add a server to the registry
    pub async fn add_server(&self, name: String, url: String) -> Result<()> {
        let client = McpClient::connect_http(&url).await?;
        self.servers.write().await.insert(name, Arc::new(RwLock::new(client)));
        Ok(())
    }

    /// Add a server via stdio
    pub async fn add_stdio_server(&self, name: String, command: &str, args: &[&str]) -> Result<()> {
        let client = McpClient::connect_stdio(command, args).await?;
        self.servers.write().await.insert(name, Arc::new(RwLock::new(client)));
        Ok(())
    }

    /// Get a server by name
    pub async fn get_server(&self, name: &str) -> Option<Arc<RwLock<McpClient>>> {
        self.servers.read().await.get(name).cloned()
    }

    /// List all server names
    pub async fn list_servers(&self) -> Vec<String> {
        self.servers.read().await.keys().cloned().collect()
    }

    /// Remove a server from the registry
    pub async fn remove_server(&self, name: &str) -> Option<Arc<RwLock<McpClient>>> {
        self.servers.write().await.remove(name)
    }

    /// Get all tools from a specific server
    pub async fn list_server_tools(&self, server_name: &str) -> Result<Vec<McpTool>> {
        let client = self
            .get_server(server_name)
            .await
            .ok_or_else(|| super::error::McpError::Connection(format!("Server not found: {}", server_name)))?;

        let mut client = client.write().await;
        client.list_tools().await
    }

    /// Call a tool on a specific server
    pub async fn call_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<super::types::McpToolResult> {
        let client = self
            .get_server(server_name)
            .await
            .ok_or_else(|| super::error::McpError::Connection(format!("Server not found: {}", server_name)))?;

        let mut client = client.write().await;
        client.call_tool(tool_name, arguments).await
    }
}

impl Default for McpServerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_registry_creation() {
        let registry = McpServerRegistry::new();
        assert!(registry.list_servers().await.is_empty());
    }
}
