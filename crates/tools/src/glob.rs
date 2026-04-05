//! Glob tool - file pattern matching

use async_trait::async_trait;
use glob::glob;

use rcode_core::{Tool, ToolContext, ToolResult, error::Result};

pub struct GlobTool;

impl GlobTool {
    pub fn new() -> Self { Self }
}

impl Default for GlobTool {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl Tool for GlobTool {
    fn id(&self) -> &str { "glob" }
    fn name(&self) -> &str { "Glob" }
    fn description(&self) -> &str { "Find files matching a glob pattern" }
    
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern to match"
                }
            },
            "required": ["pattern"]
        })
    }
    
    async fn execute(&self, args: serde_json::Value, context: &ToolContext) -> Result<ToolResult> {
        let pattern = args["pattern"]
            .as_str()
            .ok_or_else(|| rcode_core::RCodeError::Tool("Missing 'pattern' argument".into()))?;
        
        let full_pattern = if pattern.starts_with('/') {
            pattern.to_string()
        } else {
            format!("{}/{}", context.cwd.display(), pattern)
        };
        
        let mut matches = Vec::new();
        for entry in glob(&full_pattern)
            .map_err(|e| rcode_core::RCodeError::Tool(format!("Glob error: {}", e)))? {
            if let Ok(path) = entry {
                matches.push(path.display().to_string());
            }
        }
        
        let content = if matches.is_empty() {
            "No files matched".to_string()
        } else {
            matches.join("\n")
        };
        
        Ok(ToolResult {
            title: format!("Glob: {}", pattern),
            content,
            metadata: Some(serde_json::json!({ "count": matches.len() })),
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
    async fn test_glob_finds_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "").unwrap();
        std::fs::write(dir.path().join("b.rs"), "").unwrap();
        let tool = GlobTool::new();
        let result = tool.execute(serde_json::json!({"pattern": "*.rs"}), &ctx(dir.path().to_str().unwrap())).await.unwrap();
        assert!(result.content.contains("a.rs"));
        assert!(result.content.contains("b.rs"));
        assert_eq!(result.metadata.unwrap()["count"], 2);
    }

    #[tokio::test]
    async fn test_glob_no_matches() {
        let dir = tempfile::tempdir().unwrap();
        let tool = GlobTool::new();
        let result = tool.execute(serde_json::json!({"pattern": "*.xyz"}), &ctx(dir.path().to_str().unwrap())).await.unwrap();
        assert_eq!(result.content, "No files matched");
    }

    #[tokio::test]
    async fn test_glob_missing_pattern() {
        let tool = GlobTool::new();
        let result = tool.execute(serde_json::json!({}), &ctx("/tmp")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_glob_absolute_path() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "").unwrap();
        let tool = GlobTool::new();
        let pattern = format!("{}/*.txt", dir.path().display());
        let result = tool.execute(serde_json::json!({"pattern": &pattern}), &ctx("/tmp")).await.unwrap();
        assert!(result.content.contains("test.txt"));
    }
}
