//! Write tool - file creation/overwrite

use async_trait::async_trait;
use tokio::fs;

use rcode_core::{Tool, ToolContext, ToolResult, error::Result};

pub struct WriteTool;

impl WriteTool {
    pub fn new() -> Self { Self }
}

impl Default for WriteTool {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl Tool for WriteTool {
    fn id(&self) -> &str { "write" }
    fn name(&self) -> &str { "Write" }
    fn description(&self) -> &str { "Create or overwrite a file" }
    
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File path to write"
                },
                "content": {
                    "type": "string",
                    "description": "File content"
                }
            },
            "required": ["path", "content"]
        })
    }
    
    async fn execute(&self, args: serde_json::Value, context: &ToolContext) -> Result<ToolResult> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| rcode_core::OpenCodeError::Tool("Missing 'path' argument".into()))?;
        let content = args["content"]
            .as_str()
            .ok_or_else(|| rcode_core::OpenCodeError::Tool("Missing 'content' argument".into()))?;
        
        let full_path = context.cwd.join(path);
        
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).await
                .map_err(|e| rcode_core::OpenCodeError::Tool(format!("Failed to create directory: {}", e)))?;
        }
        
        fs::write(&full_path, content).await
            .map_err(|e| rcode_core::OpenCodeError::Tool(format!("Failed to write {}: {}", path, e)))?;
        
        Ok(ToolResult {
            title: format!("Written: {}", path),
            content: format!("Successfully wrote {} bytes to {}", content.len(), path),
            metadata: None,
            attachments: vec![],
        })
    }
}
