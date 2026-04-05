//! Plan tool - plan file operations before executing them

use async_trait::async_trait;

use rcode_core::{Tool, ToolContext, ToolResult, error::{Result, RCodeError}};

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
            .ok_or_else(|| RCodeError::Validation {
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

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::ToolContext;
    use std::path::PathBuf;

    fn ctx() -> ToolContext {
        ToolContext { session_id: "s1".into(), project_path: PathBuf::from("/tmp"), cwd: PathBuf::from("/tmp"), user_id: None, agent: "test".into() }
    }

    #[tokio::test]
    async fn test_plan_with_operations() {
        let tool = PlanTool::new();
        let result = tool.execute(serde_json::json!({"operations": ["read file", "edit file", "run tests"]}), &ctx()).await.unwrap();
        assert_eq!(result.title, "Execution Plan");
        assert!(result.content.contains("1. read file"));
        assert!(result.content.contains("3. run tests"));
        assert!(result.content.contains("Total: 3"));
    }

    #[tokio::test]
    async fn test_plan_empty_operations() {
        let tool = PlanTool::new();
        let result = tool.execute(serde_json::json!({"operations": []}), &ctx()).await.unwrap();
        assert!(result.content.contains("Total: 0"));
    }

    #[tokio::test]
    async fn test_plan_missing_operations() {
        let tool = PlanTool::new();
        let result = tool.execute(serde_json::json!({}), &ctx()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_plan_non_string_elements() {
        let tool = PlanTool::new();
        let result = tool.execute(serde_json::json!({"operations": [1, 2, 3]}), &ctx()).await.unwrap();
        assert!(result.content.contains("1. "));
    }
}