//! Glob tool - file pattern matching

use async_trait::async_trait;
use glob::glob;

use opencode_core::{Tool, ToolContext, ToolResult, error::Result};

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
            .ok_or_else(|| opencode_core::OpenCodeError::Tool("Missing 'pattern' argument".into()))?;
        
        let full_pattern = if pattern.starts_with('/') {
            pattern.to_string()
        } else {
            format!("{}/{}", context.cwd.display(), pattern)
        };
        
        let mut matches = Vec::new();
        for entry in glob(&full_pattern)
            .map_err(|e| opencode_core::OpenCodeError::Tool(format!("Glob error: {}", e)))? {
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
