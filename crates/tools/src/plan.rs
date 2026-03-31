//! Plan tool - plan file operations before executing them

use async_trait::async_trait;

use opencode_core::{Tool, ToolContext, ToolResult, error::{Result, OpenCodeError}};

pub struct PlanTool;

impl PlanTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PlanTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for PlanTool {
    fn id(&self) -> &str { "plan" }
    fn name(&self) -> &str { "Plan File Operations" }
    fn description(&self) -> &str { "Plan file operations before executing them" }
    
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operations": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "List of operations to plan"
                }
            },
            "required": ["operations"]
        })
    }
    
    async fn execute(&self, args: serde_json::Value, _context: &ToolContext) -> Result<ToolResult> {
        let operations = args["operations"]
            .as_array()
            .ok_or_else(|| OpenCodeError::Validation {
                field: "operations".to_string(),
                message: "Expected an array of operations".to_string(),
            })?;
        
        let operations: Vec<String> = operations
            .iter()
            .map(|v| v.as_str().unwrap_or("").to_string())
            .collect();
        
        let plan = operations.iter()
            .enumerate()
            .map(|(i, op)| format!("{}. {}", i + 1, op))
            .collect::<Vec<_>>()
            .join("\n");
        
        let total = operations.len();
        
        Ok(ToolResult {
            title: "Execution Plan".to_string(),
            content: format!("Planned Operations:\n{}\n\nTotal: {} operations", plan, total),
            metadata: None,
            attachments: vec![],
        })
    }
}