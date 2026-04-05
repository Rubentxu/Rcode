//! Grep tool - content search

use async_trait::async_trait;
use regex::Regex;
use tokio::fs;

use rcode_core::{Tool, ToolContext, ToolResult, error::Result};

pub struct GrepTool;

impl GrepTool {
    pub fn new() -> Self { Self }
}

impl Default for GrepTool {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl Tool for GrepTool {
    fn id(&self) -> &str { "grep" }
    fn name(&self) -> &str { "Grep" }
    fn description(&self) -> &str { "Search file contents using regex" }
    
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Regex pattern to search"
                },
                "path": {
                    "type": "string",
                    "description": "File or directory to search"
                }
            },
            "required": ["pattern"]
        })
    }
    
    async fn execute(&self, args: serde_json::Value, context: &ToolContext) -> Result<ToolResult> {
        let pattern = args["pattern"]
            .as_str()
            .ok_or_else(|| rcode_core::RCodeError::Tool("Missing 'pattern' argument".into()))?;
        let path = args["path"]
            .as_str()
            .unwrap_or(".");
        
        let re = Regex::new(pattern)
            .map_err(|e| rcode_core::RCodeError::Tool(format!("Invalid regex: {}", e)))?;
        
        let full_path = context.cwd.join(path);
        let mut results = Vec::new();
        
        if full_path.is_file() {
            if let Ok(content) = fs::read_to_string(&full_path).await {
                for (i, line) in content.lines().enumerate() {
                    if re.is_match(line) {
                        results.push(format!("{}:{}:{}", path, i + 1, line));
                    }
                }
            }
        }
        
        let content = if results.is_empty() {
            "No matches found".to_string()
        } else {
            results.join("\n")
        };
        
        Ok(ToolResult {
            title: format!("Grep: {}", pattern),
            content,
            metadata: Some(serde_json::json!({ "count": results.len() })),
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
    async fn test_grep_finds_pattern() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "line1 hello\nline2 world\nline3 hello").unwrap();
        let tool = GrepTool::new();
        let result = tool.execute(serde_json::json!({"pattern": "hello", "path": "test.txt"}), &ctx(dir.path().to_str().unwrap())).await.unwrap();
        assert_eq!(result.metadata.unwrap()["count"], 2);
    }

    #[tokio::test]
    async fn test_grep_no_matches() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "no match here").unwrap();
        let tool = GrepTool::new();
        let result = tool.execute(serde_json::json!({"pattern": "xyz", "path": "test.txt"}), &ctx(dir.path().to_str().unwrap())).await.unwrap();
        assert_eq!(result.content, "No matches found");
    }

    #[tokio::test]
    async fn test_grep_invalid_regex() {
        let tool = GrepTool::new();
        let result = tool.execute(serde_json::json!({"pattern": "(invalid", "path": "f.txt"}), &ctx("/tmp")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_grep_missing_pattern() {
        let tool = GrepTool::new();
        let result = tool.execute(serde_json::json!({}), &ctx("/tmp")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_grep_nonexistent_file() {
        let tool = GrepTool::new();
        let result = tool.execute(serde_json::json!({"pattern": "test", "path": "nope.txt"}), &ctx("/tmp")).await.unwrap();
        assert_eq!(result.content, "No matches found");
    }
}
