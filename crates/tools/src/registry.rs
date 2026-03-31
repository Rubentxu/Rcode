//! Tool registry service

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use parking_lot::RwLock;
use tokio::time::timeout;

use opencode_core::{Tool, ToolInfo, ToolContext, ToolResult, error::{Result, OpenCodeError}};
use super::validator::ToolValidator;

const DEFAULT_TOOL_TIMEOUT_SECS: u64 = 300;

pub struct ToolRegistryService {
    tools: RwLock<HashMap<String, Arc<dyn Tool>>>,
    default_timeout: Duration,
}

impl ToolRegistryService {
    pub fn new() -> Self {
        let registry = Self {
            tools: RwLock::new(HashMap::new()),
            default_timeout: Duration::from_secs(DEFAULT_TOOL_TIMEOUT_SECS),
        };
        registry.register_defaults();
        registry
    }

    pub fn with_timeout(timeout_secs: u64) -> Self {
        let registry = Self {
            tools: RwLock::new(HashMap::new()),
            default_timeout: Duration::from_secs(timeout_secs),
        };
        registry.register_defaults();
        registry
    }
    
    fn register_defaults(&self) {
        self.register(Arc::new(super::bash::BashTool::new()));
        self.register(Arc::new(super::read::ReadTool::new()));
        self.register(Arc::new(super::write::WriteTool::new()));
        self.register(Arc::new(super::edit::EditTool::new()));
        self.register(Arc::new(super::glob::GlobTool::new()));
        self.register(Arc::new(super::grep::GrepTool::new()));
        self.register(Arc::new(super::task::TaskTool::new()));
    }
    
    pub fn register(&self, tool: Arc<dyn Tool>) {
        self.tools.write().insert(tool.id().to_string(), tool);
    }
    
    pub fn get(&self, id: &str) -> Option<Arc<dyn Tool>> {
        self.tools.read().get(id).cloned()
    }
    
    pub fn list(&self) -> Vec<ToolInfo> {
        self.tools.read()
            .values()
            .map(|t| ToolInfo {
                id: t.id().to_string(),
                name: t.name().to_string(),
                description: t.description().to_string(),
            })
            .collect()
    }
    
    pub async fn execute(
        &self,
        tool_id: &str,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        self.execute_with_timeout(tool_id, args, context, self.default_timeout).await
    }

    pub async fn execute_with_timeout(
        &self,
        tool_id: &str,
        args: serde_json::Value,
        context: &ToolContext,
        timeout_duration: Duration,
    ) -> Result<ToolResult> {
        let tool = self.get(tool_id)
            .ok_or_else(|| OpenCodeError::Tool(format!("Tool not found: {}", tool_id)))?;
        
        // Validate arguments against tool's schema
        let schema = tool.parameters();
        if let Err(e) = ToolValidator::validate_with_schema(&args, &schema) {
            return Err(OpenCodeError::Validation {
                field: String::new(),
                message: format!("Tool '{}': {}", tool_id, e),
            });
        }
        
        // Execute with timeout
        let result = timeout(
            timeout_duration,
            tool.execute(args, context)
        ).await;
        
        match result {
            Ok(Ok(tool_result)) => Ok(tool_result),
            Ok(Err(e)) => Err(OpenCodeError::Tool(
                format!("Tool '{}' execution failed: {}", tool_id, e)
            )),
            Err(_) => Err(OpenCodeError::Timeout { 
                duration: timeout_duration.as_secs() 
            }),
        }
    }
}

impl Default for ToolRegistryService {
    fn default() -> Self {
        Self::new()
    }
}
