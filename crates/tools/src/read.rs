//! Read tool - file reading

use async_trait::async_trait;
use tokio::fs;

use opencode_core::{Tool, ToolContext, ToolResult, error::Result};

pub struct ReadTool;

impl ReadTool {
    pub fn new() -> Self { Self }
}

impl Default for ReadTool {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl Tool for ReadTool {
    fn id(&self) -> &str { "read" }
    fn name(&self) -> &str { "Read" }
    fn description(&self) -> &str { "Read file contents" }
    
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File path to read"
                }
            },
            "required": ["path"]
        })
    }
    
    async fn execute(&self, args: serde_json::Value, context: &ToolContext) -> Result<ToolResult> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| opencode_core::OpenCodeError::Tool("Missing 'path' argument".into()))?;
        
        let full_path = context.cwd.join(path);
        let content = fs::read_to_string(&full_path).await
            .map_err(|e| opencode_core::OpenCodeError::Tool(format!("Failed to read {}: {}", path, e)))?;
        
        Ok(ToolResult {
            title: format!("Read: {}", path),
            content,
            metadata: None,
            attachments: vec![],
        })
    }
}
