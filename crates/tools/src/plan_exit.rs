//! Plan exit tool - signals intent to exit plan mode

use async_trait::async_trait;

use rcode_core::{Tool, ToolContext, ToolResult, error::Result};

pub struct PlanExitTool;

impl PlanExitTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PlanExitTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for PlanExitTool {
    fn id(&self) -> &str { "plan_exit" }
    fn name(&self) -> &str { "Plan Exit" }
    fn description(&self) -> &str { "Exit plan mode and return to build agent" }
    
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }
    
    async fn execute(&self, _args: serde_json::Value, _context: &ToolContext) -> Result<ToolResult> {
        Ok(ToolResult {
            title: "Plan mode exited".to_string(),
            content: "Returning control to build agent".to_string(),
            metadata: Some(serde_json::json!({
                "action": "exit_plan_mode"
            })),
            attachments: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::ToolContext;

    fn test_context() -> ToolContext {
        ToolContext {
            session_id: "test".to_string(),
            project_path: std::path::PathBuf::from("/tmp"),
            cwd: std::path::PathBuf::from("/tmp"),
            user_id: None,
            agent: "test-agent".to_string(),
        }
    }

    #[tokio::test]
    async fn test_plan_exit_tool_execute() {
        let tool = PlanExitTool::new();
        let result = tool.execute(serde_json::json!({}), &test_context()).await.unwrap();
        
        assert_eq!(result.title, "Plan mode exited");
        assert_eq!(result.content, "Returning control to build agent");
        assert!(result.metadata.is_some());
    }

    #[tokio::test]
    async fn test_plan_exit_tool_id() {
        let tool = PlanExitTool::new();
        assert_eq!(tool.id(), "plan_exit");
    }

    #[tokio::test]
    async fn test_plan_exit_tool_name() {
        let tool = PlanExitTool::new();
        assert_eq!(tool.name(), "Plan Exit");
    }

    #[tokio::test]
    async fn test_plan_exit_tool_description() {
        let tool = PlanExitTool::new();
        assert_eq!(tool.description(), "Exit plan mode and return to build agent");
    }

    #[tokio::test]
    async fn test_plan_exit_tool_parameters() {
        let tool = PlanExitTool::new();
        let params = tool.parameters();
        assert_eq!(params["type"], "object");
        assert!(params["properties"].as_object().unwrap().is_empty());
        assert!(params["required"].as_array().unwrap().is_empty());
    }
}
