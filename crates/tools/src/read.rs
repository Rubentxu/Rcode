//! Read tool - file reading

use async_trait::async_trait;
use tokio::fs;

use rcode_core::{Tool, ToolContext, ToolResult, error::Result};

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
            .ok_or_else(|| rcode_core::RCodeError::Tool("Missing 'path' argument".into()))?;
        
        let full_path = context.cwd.join(path);
        let content = fs::read_to_string(&full_path).await
            .map_err(|e| rcode_core::RCodeError::Tool(format!("Failed to read {}: {}", path, e)))?;
        
        Ok(ToolResult {
            title: format!("Read: {}", path),
            content,
            metadata: None,
            attachments: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::ToolContext;
    use std::path::PathBuf;

    fn ctx(cwd: &str) -> ToolContext {
        ToolContext { session_id: "s1".into(), project_path: PathBuf::from(cwd), cwd: PathBuf::from(cwd), user_id: None, agent: "test".into() }
    }

    #[tokio::test]
    async fn test_read_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("hello.txt"), "hello world").unwrap();
        let tool = ReadTool::new();
        let result = tool.execute(serde_json::json!({"path": "hello.txt"}), &ctx(dir.path().to_str().unwrap())).await.unwrap();
        assert_eq!(result.content, "hello world");
    }

    #[tokio::test]
    async fn test_read_missing_path() {
        let tool = ReadTool::new();
        let result = tool.execute(serde_json::json!({}), &ctx("/tmp")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_read_nonexistent_file() {
        let tool = ReadTool::new();
        let result = tool.execute(serde_json::json!({"path": "nonexistent.txt"}), &ctx("/tmp")).await;
        assert!(result.is_err());
    }
}
