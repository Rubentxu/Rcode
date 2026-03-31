//! Grep tool - content search

use async_trait::async_trait;
use regex::Regex;
use tokio::fs;

use opencode_core::{Tool, ToolContext, ToolResult, error::Result};

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
            .ok_or_else(|| opencode_core::OpenCodeError::Tool("Missing 'pattern' argument".into()))?;
        let path = args["path"]
            .as_str()
            .unwrap_or(".");
        
        let re = Regex::new(pattern)
            .map_err(|e| opencode_core::OpenCodeError::Tool(format!("Invalid regex: {}", e)))?;
        
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
