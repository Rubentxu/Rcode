//! Batch tool implementation - executes multiple tool calls concurrently

use std::sync::Arc;
use async_trait::async_trait;
use tokio::task::JoinSet;

use rcode_core::{Tool, ToolContext, ToolResult, error::{Result, RCodeError}};

use super::registry::ToolRegistryService;

pub struct BatchTool {
    tool_registry: Arc<ToolRegistryService>,
}

impl BatchTool {
    pub fn new(tool_registry: Arc<ToolRegistryService>) -> Self {
        Self { tool_registry }
    }
}

#[derive(Debug, serde::Deserialize)]
struct BatchCall {
    tool: String,
    args: serde_json::Value,
}

#[derive(Debug, serde::Serialize)]
struct BatchResultEntry {
    tool: String,
    success: bool,
    result: ToolResult,
}

#[async_trait]
impl Tool for BatchTool {
    fn id(&self) -> &str { "batch" }
    fn name(&self) -> &str { "Batch" }
    fn description(&self) -> &str { "Execute multiple tool calls concurrently" }
    
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "calls": {
                    "type": "array",
                    "description": "Array of tool calls to execute",
                    "items": {
                        "type": "object",
                        "properties": {
                            "tool": {
                                "type": "string",
                                "description": "The tool ID to call"
                            },
                            "args": {
                                "type": "object",
                                "description": "Arguments for the tool"
                            }
                        },
                        "required": ["tool", "args"]
                    }
                }
            },
            "required": ["calls"]
        })
    }
    
    async fn execute(
        &self,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        let calls: Vec<BatchCall> = serde_json::from_value(
            args.get("calls")
                .ok_or_else(|| RCodeError::Tool("Missing 'calls' argument".into()))?
                .to_owned()
        ).map_err(|e| RCodeError::Tool(format!("Invalid calls format: {}", e)))?;

        if calls.is_empty() {
            return Ok(ToolResult {
                title: "Batch: No calls".to_string(),
                content: "No tool calls provided".to_string(),
                metadata: Some(serde_json::json!({
                    "count": 0
                })),
                attachments: vec![],
            });
        }

        let registry = self.tool_registry.clone();
        
        let results = run_all_calls(calls, registry, context).await;

        let total = results.len();
        let successes = results.iter().filter(|(_, r)| r.is_ok()).count();
        let failures = total - successes;

        let mut entries: Vec<BatchResultEntry> = Vec::with_capacity(total);
        for (tool_name, result) in results {
            match result {
                Ok(r) => entries.push(BatchResultEntry {
                    tool: tool_name,
                    success: true,
                    result: r,
                }),
                Err(e) => entries.push(BatchResultEntry {
                    tool: tool_name.clone(),
                    success: false,
                    result: ToolResult {
                        title: format!("Error: {}", tool_name),
                        content: e.to_string(),
                        metadata: Some(serde_json::json!({
                            "is_error": true
                        })),
                        attachments: vec![],
                    },
                }),
            }
        }

        let content = serde_json::to_string_pretty(&entries)
            .unwrap_or_else(|_| "Failed to serialize results".to_string());

        Ok(ToolResult {
            title: format!("Batch: {}/{} succeeded", successes, total),
            content,
            metadata: Some(serde_json::json!({
                "total": total,
                "successes": successes,
                "failures": failures
            })),
            attachments: vec![],
        })
    }
}

async fn run_all_calls(
    calls: Vec<BatchCall>,
    registry: Arc<ToolRegistryService>,
    context: &ToolContext,
) -> Vec<(String, Result<ToolResult>)> {
    let mut join_set = JoinSet::new();
    let call_count = calls.len();
    
    let base_context = Arc::new(ToolContext {
        session_id: context.session_id.clone(),
        project_path: context.project_path.clone(),
        cwd: context.cwd.clone(),
        user_id: context.user_id.clone(),
        agent: context.agent.clone(),
    });
    
    for call in calls {
        let registry = registry.clone();
        let tool_name = call.tool.clone();
        let call_args = call.args.clone();
        let ctx = base_context.clone();
        join_set.spawn(async move {
            let result = registry.execute(&call.tool, call_args, &ctx).await;
            (tool_name, result)
        });
    }

    let mut results = Vec::with_capacity(call_count);
    while let Some(result) = join_set.join_next().await {
        match result {
            Ok((name, res)) => results.push((name, res)),
            Err(e) => results.push((String::new(), Err(RCodeError::Tool(format!("Join error: {:?}", e))))),
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::ToolContext;
    use std::path::PathBuf;

    fn create_test_context() -> ToolContext {
        ToolContext {
            session_id: "test-session".to_string(),
            project_path: PathBuf::from("/tmp"),
            cwd: PathBuf::from("/tmp"),
            user_id: None,
            agent: "test".to_string(),
        }
    }

    fn create_registry() -> Arc<ToolRegistryService> {
        Arc::new(ToolRegistryService::new())
    }

    #[tokio::test]
    async fn test_batch_empty_calls() {
        let registry = create_registry();
        let tool = BatchTool::new(registry);
        
        let args = serde_json::json!({
            "calls": []
        });
        
        let result = tool.execute(args, &create_test_context()).await.unwrap();
        assert_eq!(result.title, "Batch: No calls");
        assert_eq!(result.content, "No tool calls provided");
    }

    #[tokio::test]
    async fn test_batch_single_call() {
        let registry = create_registry();
        let tool = BatchTool::new(registry);
        
        let args = serde_json::json!({
            "calls": [
                {
                    "tool": "question",
                    "args": {"question": "test"}
                }
            ]
        });
        
        let result = tool.execute(args, &create_test_context()).await.unwrap();
        assert!(result.content.contains("\"success\": true"));
    }

    #[tokio::test]
    async fn test_batch_multiple_calls() {
        let registry = create_registry();
        let tool = BatchTool::new(registry);
        
        let args = serde_json::json!({
            "calls": [
                {"tool": "question", "args": {"question": "test1"}},
                {"tool": "question", "args": {"question": "test2"}}
            ]
        });
        
        let result = tool.execute(args, &create_test_context()).await.unwrap();
        assert!(result.content.contains("\"success\": true"));
    }

    #[tokio::test]
    async fn test_batch_invalid_tool() {
        let registry = create_registry();
        let tool = BatchTool::new(registry);
        
        let args = serde_json::json!({
            "calls": [
                {"tool": "nonexistent", "args": {}}
            ]
        });
        
        let result = tool.execute(args, &create_test_context()).await.unwrap();
        assert!(result.content.contains("\"success\": false"));
    }

    #[tokio::test]
    async fn test_batch_missing_calls_arg() {
        let registry = create_registry();
        let tool = BatchTool::new(registry);
        
        let args = serde_json::json!({});
        
        let result = tool.execute(args, &create_test_context()).await;
        assert!(result.is_err());
    }
}
