//! Codesearch tool - search for code patterns in the project

use async_trait::async_trait;
use serde::Deserialize;
use std::path::PathBuf;

use opencode_core::{Tool, ToolContext, ToolResult, error::Result};

pub struct CodesearchTool;

impl CodesearchTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CodesearchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
pub struct CodesearchParams {
    pub query: String,
    pub path: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    20
}

#[async_trait]
impl Tool for CodesearchTool {
    fn id(&self) -> &str { "codesearch" }
    fn name(&self) -> &str { "Code Search" }
    fn description(&self) -> &str { "Search for code patterns in the project" }
    
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Code pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "Path to search in (defaults to current project)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum results",
                    "default": 20
                }
            },
            "required": ["query"]
        })
    }
    
    async fn execute(&self, args: serde_json::Value, context: &ToolContext) -> Result<ToolResult> {
        let params: CodesearchParams = serde_json::from_value(args)
            .map_err(|e| opencode_core::OpenCodeError::Tool(format!("Invalid parameters: {}", e)))?;
        
        let search_path: PathBuf = params.path
            .map(PathBuf::from)
            .unwrap_or_else(|| context.project_path.clone());
        
        let mut results = Vec::new();
        
        let entries = walkdir::WalkDir::new(&search_path)
            .max_depth(10)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .take(1000);
        
        for entry in entries {
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                if content.contains(&params.query) {
                    let lines: Vec<_> = content.lines()
                        .enumerate()
                        .filter(|(_, l)| l.contains(&params.query))
                        .take(3)
                        .map(|(i, l)| format!("{}:{}: {}", entry.path().display(), i+1, l.trim()))
                        .collect();
                    
                    for line in lines {
                        if results.len() < params.limit {
                            results.push(line);
                        }
                    }
                }
            }
        }
        
        let content = if results.is_empty() {
            format!("No code found matching: {}", params.query)
        } else {
            results.join("\n")
        };
        
        Ok(ToolResult {
            title: format!("Code Search: {}", params.query),
            content,
            metadata: Some(serde_json::json!({
                "query": params.query,
                "path": search_path.display().to_string(),
                "result_count": results.len()
            })),
            attachments: vec![],
        })
    }
}