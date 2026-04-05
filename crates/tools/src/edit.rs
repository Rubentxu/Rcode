//! Edit tool - in-place file editing

use async_trait::async_trait;
use tokio::fs;

use rcode_core::{Tool, ToolContext, ToolResult, error::Result};

pub struct EditTool;

impl EditTool {
    pub fn new() -> Self { Self }
}

impl Default for EditTool {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl Tool for EditTool {
    fn id(&self) -> &str { "edit" }
    fn name(&self) -> &str { "Edit" }
    fn description(&self) -> &str { "Edit a file by replacing text" }
    
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File path to edit"
                },
                "old_text": {
                    "type": "string",
                    "description": "Text to replace"
                },
                "new_text": {
                    "type": "string",
                    "description": "Replacement text"
                }
            },
            "required": ["path", "old_text", "new_text"]
        })
    }
    
    async fn execute(&self, args: serde_json::Value, context: &ToolContext) -> Result<ToolResult> {
        let path = args["path"].as_str().unwrap();
        let old_text = args["old_text"].as_str().unwrap();
        let new_text = args["new_text"].as_str().unwrap();
        
        let full_path = context.cwd.join(path);
        let content = fs::read_to_string(&full_path).await?;
        
        if !content.contains(old_text) {
            return Err(rcode_core::RCodeError::Tool(
                format!("Text not found in file: {}", path)
            ));
        }
        
        let new_content = content.replace(old_text, new_text);
        fs::write(&full_path, &new_content).await?;
        
        Ok(ToolResult {
            title: format!("Edited: {}", path),
            content: format!("Replaced {} with {} in {}", old_text.len(), new_text.len(), path),
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
    async fn test_edit_replace() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "hello world").unwrap();
        let tool = EditTool::new();
        let result = tool.execute(serde_json::json!({"path": "test.txt", "old_text": "hello", "new_text": "goodbye"}), &ctx(dir.path().to_str().unwrap())).await.unwrap();
        assert_eq!(std::fs::read_to_string(dir.path().join("test.txt")).unwrap(), "goodbye world");
        assert!(result.content.contains("Replaced"));
    }

    #[tokio::test]
    async fn test_edit_text_not_found() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "hello world").unwrap();
        let tool = EditTool::new();
        let result = tool.execute(serde_json::json!({"path": "test.txt", "old_text": "missing", "new_text": "x"}), &ctx(dir.path().to_str().unwrap())).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_edit_nonexistent_file() {
        let tool = EditTool::new();
        let result = tool.execute(serde_json::json!({"path": "nope.txt", "old_text": "a", "new_text": "b"}), &ctx("/tmp")).await;
        assert!(result.is_err());
    }
}
