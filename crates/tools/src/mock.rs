//! Mock Tool for testing

use std::sync::Arc;
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};

use opencode_core::{Tool, ToolContext, ToolResult, error::Result};

/// Invocation record for tracking tool calls
#[derive(Debug, Clone)]
pub struct ToolInvocation {
    pub args: serde_json::Value,
    pub context: ToolContext,
    pub call_count: usize,
}

/// Configuration for mock tool behavior
#[derive(Debug, Clone)]
pub struct MockToolConfig {
    pub result: Option<ToolResult>,
    pub error: Option<String>,
    pub should_succeed: bool,
}

impl Default for MockToolConfig {
    fn default() -> Self {
        Self {
            result: None,
            error: None,
            should_succeed: true,
        }
    }
}

impl MockToolConfig {
    pub fn returning(result: ToolResult) -> Self {
        Self {
            result: Some(result),
            error: None,
            should_succeed: true,
        }
    }

    pub fn failing(error: String) -> Self {
        Self {
            result: None,
            error: Some(error),
            should_succeed: false,
        }
    }
}

/// Mock Tool for testing
pub struct MockTool {
    id: String,
    name: String,
    description: String,
    parameters: serde_json::Value,
    invocation_count: AtomicUsize,
    invocations: std::sync::Mutex<Vec<ToolInvocation>>,
    config: std::sync::Mutex<MockToolConfig>,
    execute_sleep_ms: AtomicUsize,
}

impl MockTool {
    pub fn new(id: &str, name: &str, description: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "input": {
                        "type": "string",
                        "description": "Input to the mock tool"
                    }
                },
                "required": ["input"]
            }),
            invocation_count: AtomicUsize::new(0),
            invocations: std::sync::Mutex::new(Vec::new()),
            config: std::sync::Mutex::new(MockToolConfig::default()),
            execute_sleep_ms: AtomicUsize::new(0),
        }
    }

    /// Configure the mock tool to return a specific result
    pub fn then_return(&self, result: ToolResult) {
        *self.config.lock().unwrap() = MockToolConfig::returning(result);
    }

    /// Configure the mock tool to fail with a specific error
    pub fn then_fail(&self, error: String) {
        *self.config.lock().unwrap() = MockToolConfig::failing(error);
    }

    /// Add a delay to execute (for testing timeouts)
    pub fn with_execute_delay(&self, delay_ms: usize) {
        self.execute_sleep_ms.store(delay_ms, Ordering::SeqCst);
    }

    /// Get the number of times execute() was called
    pub fn invocation_count(&self) -> usize {
        self.invocation_count.load(Ordering::SeqCst)
    }

    /// Get all invocation records
    pub fn get_invocations(&self) -> Vec<ToolInvocation> {
        self.invocations.lock().unwrap().clone()
    }

    /// Clear all invocation history
    pub fn reset(&self) {
        self.invocation_count.store(0, Ordering::SeqCst);
        self.invocations.lock().unwrap().clear();
        *self.config.lock().unwrap() = MockToolConfig::default();
        self.execute_sleep_ms.store(0, Ordering::SeqCst);
    }
}

#[async_trait]
impl Tool for MockTool {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> serde_json::Value {
        self.parameters.clone()
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        // Clone context before the async block to avoid lifetime issues
        let context_clone = context.clone();
        let args_clone = args.clone();
        let name = self.name.clone();
        
        let count = self.invocation_count.fetch_add(1, Ordering::SeqCst) + 1;
        
        // Record invocation
        self.invocations.lock().unwrap().push(ToolInvocation {
            args: args_clone,
            context: context_clone,
            call_count: count,
        });

        // Apply delay if configured
        let delay = self.execute_sleep_ms.load(Ordering::SeqCst);
        if delay > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(delay as u64)).await;
        }

        let config = self.config.lock().unwrap().clone();

        if config.should_succeed {
            if let Some(result) = config.result {
                Ok(result)
            } else {
                Ok(ToolResult {
                    title: format!("Mock: {}", name),
                    content: format!("Executed with args: {}", args),
                    metadata: None,
                    attachments: vec![],
                })
            }
        } else {
            Err(opencode_core::OpenCodeError::Tool(
                config.error.unwrap_or_else(|| "Mock tool error".to_string())
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use opencode_core::ToolContext;

    fn create_test_context() -> ToolContext {
        ToolContext {
            session_id: "test-session".to_string(),
            project_path: std::path::PathBuf::from("/tmp"),
            cwd: std::path::PathBuf::from("/tmp"),
            user_id: None,
            agent: "test-agent".to_string(),
        }
    }

    #[tokio::test]
    async fn test_mock_tool_basic_execution() {
        let tool = MockTool::new("mock", "MockTool", "A mock tool for testing");
        
        let result = tool.execute(
            serde_json::json!({"input": "test"}),
            &create_test_context()
        ).await.unwrap();
        
        assert_eq!(tool.invocation_count(), 1);
        assert!(result.content.contains("test"));
    }

    #[tokio::test]
    async fn test_mock_tool_custom_result() {
        let tool = MockTool::new("mock", "MockTool", "A mock tool for testing");
        
        let custom_result = ToolResult {
            title: "Custom Title".to_string(),
            content: "Custom content".to_string(),
            metadata: None,
            attachments: vec![],
        };
        tool.then_return(custom_result.clone());
        
        let result = tool.execute(
            serde_json::json!({"input": "test"}),
            &create_test_context()
        ).await.unwrap();
        
        assert_eq!(result.title, "Custom Title");
        assert_eq!(result.content, "Custom content");
    }

    #[tokio::test]
    async fn test_mock_tool_failure() {
        let tool = MockTool::new("mock", "MockTool", "A mock tool for testing");
        
        tool.then_fail("Expected error".to_string());
        
        let result = tool.execute(
            serde_json::json!({"input": "test"}),
            &create_test_context()
        ).await;
        
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), opencode_core::OpenCodeError::Tool(_)));
    }

    #[tokio::test]
    async fn test_mock_tool_invocation_tracking() {
        let tool = MockTool::new("mock", "MockTool", "A mock tool for testing");
        
        tool.execute(serde_json::json!({"input": "first"}), &create_test_context()).await.unwrap();
        tool.execute(serde_json::json!({"input": "second"}), &create_test_context()).await.unwrap();
        
        let invocations = tool.get_invocations();
        assert_eq!(invocations.len(), 2);
        assert_eq!(invocations[0].call_count, 1);
        assert_eq!(invocations[1].call_count, 2);
        assert_eq!(invocations[0].args["input"], "first");
        assert_eq!(invocations[1].args["input"], "second");
    }

    #[tokio::test]
    async fn test_mock_tool_reset() {
        let tool = MockTool::new("mock", "MockTool", "A mock tool for testing");
        
        tool.execute(serde_json::json!({"input": "test"}), &create_test_context()).await.unwrap();
        assert_eq!(tool.invocation_count(), 1);
        
        tool.reset();
        assert_eq!(tool.invocation_count(), 0);
        assert!(tool.get_invocations().is_empty());
    }

    #[tokio::test]
    async fn test_mock_tool_delay() {
        let tool = MockTool::new("mock", "MockTool", "A mock tool for testing");
        
        tool.with_execute_delay(50);
        
        let start = std::time::Instant::now();
        tool.execute(serde_json::json!({"input": "test"}), &create_test_context()).await.unwrap();
        let elapsed = start.elapsed();
        
        assert!(elapsed.as_millis() >= 50);
    }

    #[tokio::test]
    async fn test_mock_tool_with_arc() {
        let tool = Arc::new(MockTool::new("mock", "MockTool", "A mock tool for testing"));
        
        let tool_clone = tool.clone();
        tokio::spawn(async move {
            tool_clone.execute(serde_json::json!({"input": "async"}), &create_test_context()).await.unwrap();
        }).await.unwrap();
        
        assert_eq!(tool.invocation_count(), 1);
    }
}
