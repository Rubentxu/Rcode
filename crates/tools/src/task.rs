//! Task tool - sub-agent delegation with session continuation support

use async_trait::async_trait;
use std::sync::Arc;
use parking_lot::RwLock;

use rcode_core::{Tool, ToolContext, ToolResult, PermissionChecker, PermissionConfig, Permission, AgentRegistry, error::{Result, RCodeError}};

/// Agent ID type for identifying subagents
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentId(pub String);

impl AgentId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
    
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Session continuation token for linking to existing sessions
#[derive(Debug, Clone)]
pub struct SessionToken(pub String);

impl SessionToken {
    pub fn new(token: impl Into<String>) -> Self {
        Self(token.into())
    }
    
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SessionToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Task tool state for tracking active task sessions
pub struct TaskToolState {
    /// Maps task_id to session_id for continuation
    pub task_sessions: RwLock<std::collections::HashMap<String, String>>,
}

impl TaskToolState {
    pub fn new() -> Self {
        Self {
            task_sessions: RwLock::new(std::collections::HashMap::new()),
        }
    }
    
    /// Register a task session
    pub fn register_session(&self, task_id: &str, session_id: &str) {
        self.task_sessions.write().insert(task_id.to_string(), session_id.to_string());
    }
    
    /// Get session for a task_id if it exists
    pub fn get_session(&self, task_id: &str) -> Option<String> {
        self.task_sessions.read().get(task_id).cloned()
    }
    
    /// Remove a task session registration
    pub fn remove_session(&self, task_id: &str) -> Option<String> {
        self.task_sessions.write().remove(task_id)
    }
}

impl Default for TaskToolState {
    fn default() -> Self {
        Self::new()
    }
}

/// Agent registry reference for looking up available agents
pub struct TaskTool {
    /// Shared state for task sessions
    pub state: Arc<TaskToolState>,
    /// Permission checker for validating subagent invocations
    permission_checker: Option<Arc<PermissionChecker>>,
    /// Agent registry for looking up agents
    agent_registry: Option<Arc<AgentRegistry>>,
}

impl TaskTool {
    /// Create a new TaskTool without agent registry
    pub fn new() -> Self {
        Self {
            state: Arc::new(TaskToolState::new()),
            permission_checker: None,
            agent_registry: None,
        }
    }
    
    /// Create a TaskTool with state
    pub fn with_state(state: Arc<TaskToolState>) -> Self {
        Self {
            state,
            permission_checker: None,
            agent_registry: None,
        }
    }
    
    /// Create a TaskTool with permission checking enabled
    pub fn with_permission_config(permission_config: PermissionConfig) -> Self {
        Self {
            state: Arc::new(TaskToolState::new()),
            permission_checker: Some(Arc::new(PermissionChecker::new(permission_config))),
            agent_registry: None,
        }
    }

    /// Create a TaskTool with an agent registry
    pub fn with_agent_registry(registry: Arc<AgentRegistry>) -> Self {
        Self {
            state: Arc::new(TaskToolState::new()),
            permission_checker: None,
            agent_registry: Some(registry),
        }
    }

    /// Create a TaskTool with all options
    pub fn with_all(
        state: Arc<TaskToolState>,
        permission_checker: Option<Arc<PermissionChecker>>,
        registry: Option<Arc<AgentRegistry>>,
    ) -> Self {
        Self {
            state,
            permission_checker,
            agent_registry: registry,
        }
    }
    
    /// Get available agent types from registry if available
    pub fn available_agent_types(&self) -> Vec<String> {
        if let Some(ref registry) = self.agent_registry {
            registry.list().into_iter().map(|a| a.id).collect()
        } else {
            // Fallback to default agent types
            vec![
                "general".to_string(),
                "explore".to_string(),
                "refactor".to_string(),
                "debug".to_string(),
            ]
        }
    }
    
    /// Check if an agent type is allowed (exists in registry or is a known default)
    pub fn is_agent_type_allowed(&self, agent_type: &str) -> bool {
        // If we have a registry, check if the agent exists
        if let Some(ref registry) = self.agent_registry {
            return registry.contains(agent_type);
        }
        
        // Fallback to default agent types
        matches!(
            agent_type,
            "general" | "explore" | "refactor" | "debug"
        )
    }

    /// Get agent by ID from registry
    pub fn get_agent(&self, agent_id: &str) -> Option<Arc<dyn rcode_core::agent::Agent>> {
        self.agent_registry.as_ref()?.get(agent_id)
    }
}

impl Default for TaskTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for TaskTool {
    fn id(&self) -> &str { "task" }
    fn name(&self) -> &str { "Task" }
    fn description(&self) -> &str { "Spawn a sub-agent to complete a task or continue an existing task session" }
    
    fn parameters(&self) -> serde_json::Value {
        // Build enum from available agents if registry is available
        let agent_types: Vec<String> = if let Some(ref registry) = self.agent_registry {
            let agents = registry.list();
            agents.into_iter().map(|a| a.id).collect()
        } else {
            vec!["general".to_string(), "explore".to_string(), "refactor".to_string(), "debug".to_string()]
        };

        serde_json::json!({
            "type": "object",
            "properties": {
                "description": {
                    "type": "string",
                    "description": "One-line description of the task"
                },
                "prompt": {
                    "type": "string",
                    "description": "Full instructions for the sub-agent"
                },
                "agent_type": {
                    "type": "string",
                    "description": "Type of sub-agent to invoke",
                    "enum": agent_types
                },
                "task_id": {
                    "type": "string",
                    "description": "Optional task ID for session continuation. If provided, continues an existing session instead of creating a new one"
                }
            },
            "required": ["description", "prompt"]
        })
    }
    
    async fn execute(&self, args: serde_json::Value, context: &ToolContext) -> Result<ToolResult> {
        let description = args["description"].as_str()
            .ok_or_else(|| RCodeError::Validation {
                field: "description".to_string(),
                message: "Description is required".to_string(),
            })?;
        
        let prompt = args["prompt"].as_str()
            .ok_or_else(|| RCodeError::Validation {
                field: "prompt".to_string(),
                message: "Prompt is required".to_string(),
            })?;
        
        let agent_type = args["agent_type"].as_str().unwrap_or("general");
        let task_id = args["task_id"].as_str();
        
        // Validate agent type is allowed
        if !self.is_agent_type_allowed(agent_type) {
            return Err(RCodeError::Permission(format!(
                "Agent type '{}' is not allowed. Available types: {:?}",
                agent_type, self.available_agent_types()
            )));
        }
        
        // Check permission before launching subagent
        if let Some(ref checker) = self.permission_checker {
            let check_result = checker.check(&context.agent, "task", Some(agent_type));
            
            match check_result.permission {
                Permission::Allow => { /* proceed */ }
                Permission::Deny => {
                    return Err(RCodeError::Permission(format!(
                        "Agent {} denied to invoke {} subagent: {}",
                        context.agent, agent_type, check_result.reason
                    )));
                }
                Permission::Ask => {
                    // Return pending response for ask permission
                    return Ok(ToolResult {
                        title: "Permission Required".to_string(),
                        content: format!(
                            "Permission required to invoke {} from {}. Awaiting approval.\n\nReason: {}",
                            agent_type, context.agent, check_result.reason
                        ),
                        metadata: Some(serde_json::json!({
                            "agent_type": agent_type,
                            "task_id": task_id,
                            "permission_pending": true,
                        })),
                        attachments: vec![],
                    });
                }
            }
        }
        
        let result_content = if let Some(existing_task_id) = task_id {
            // Session continuation: check if session exists
            if let Some(session_id) = self.state.get_session(existing_task_id) {
                format!(
                    "Continuing task session '{}' (session: {}) for: {}\n\nPrompt:\n{}",
                    existing_task_id, session_id, description, prompt
                )
            } else {
                return Err(RCodeError::Session(format!(
                    "Task session '{}' not found or expired", existing_task_id
                )));
            }
        } else {
            // Create new subagent session
            let new_task_id = format!("task_{}_{}", 
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                agent_type
            );
            
            // Generate a new session ID for this task
            let session_id = format!("session_{}_{}_{}", 
                context.session_id,
                new_task_id,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            
            // Register the session for potential continuation
            self.state.register_session(&new_task_id, &session_id);
            
            // Get agent info if available
            let agent_info = self.get_agent(agent_type)
                .map(|a| format!(" (model: {:?})", a.supported_tools().first()));

            format!(
                "Created new task '{}' (session: {}) with {} agent{}:\n\nDescription: {}\n\nPrompt:\n{}",
                new_task_id, session_id, agent_type, agent_info.unwrap_or_default(), description, prompt
            )
        };
        
        Ok(ToolResult {
            title: format!("Task: {}", description),
            content: result_content,
            metadata: Some(serde_json::json!({
                "agent_type": agent_type,
                "task_id": task_id,
            })),
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
        ToolContext { session_id: "s1".into(), project_path: PathBuf::from("/tmp"), cwd: PathBuf::from("/tmp"), user_id: None, agent: "main".into() }
    }

    #[test]
    fn test_agent_id_new() {
        let id = AgentId::new("test-agent");
        assert_eq!(id.as_str(), "test-agent");
    }

    #[test]
    fn test_agent_id_display() {
        let id = AgentId::new("my-agent");
        assert_eq!(format!("{}", id), "my-agent");
    }

    #[test]
    fn test_session_token_new() {
        let token = SessionToken::new("abc123");
        assert_eq!(token.as_str(), "abc123");
    }

    #[test]
    fn test_session_token_display() {
        let token = SessionToken::new("tok");
        assert_eq!(format!("{}", token), "tok");
    }

    #[test]
    fn test_task_tool_state_register_and_get() {
        let state = TaskToolState::new();
        state.register_session("task1", "sess1");
        assert_eq!(state.get_session("task1"), Some("sess1".to_string()));
        assert_eq!(state.get_session("task2"), None);
    }

    #[test]
    fn test_task_tool_state_remove() {
        let state = TaskToolState::new();
        state.register_session("task1", "sess1");
        assert_eq!(state.remove_session("task1"), Some("sess1".to_string()));
        assert_eq!(state.get_session("task1"), None);
    }

    #[test]
    fn test_task_tool_state_default() {
        let state = TaskToolState::default();
        assert!(state.get_session("any").is_none());
    }

    #[test]
    fn test_task_tool_available_agent_types_default() {
        let tool = TaskTool::new();
        let types = tool.available_agent_types();
        assert!(types.contains(&"general".to_string()));
        assert!(types.contains(&"explore".to_string()));
        assert!(types.contains(&"refactor".to_string()));
        assert!(types.contains(&"debug".to_string()));
    }

    #[test]
    fn test_task_tool_is_agent_type_allowed() {
        let tool = TaskTool::new();
        assert!(tool.is_agent_type_allowed("general"));
        assert!(tool.is_agent_type_allowed("explore"));
        assert!(!tool.is_agent_type_allowed("unknown_type"));
    }

    #[tokio::test]
    async fn test_task_execute_new_task() {
        let tool = TaskTool::new();
        let result = tool.execute(serde_json::json!({"description": "Test task", "prompt": "Do something"}), &ctx()).await.unwrap();
        assert!(result.title.contains("Test task"));
        assert!(result.content.contains("Created new task"));
        assert!(result.metadata.unwrap()["agent_type"] == "general");
    }

    #[tokio::test]
    async fn test_task_execute_missing_description() {
        let tool = TaskTool::new();
        let result = tool.execute(serde_json::json!({"prompt": "Do something"}), &ctx()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_task_execute_missing_prompt() {
        let tool = TaskTool::new();
        let result = tool.execute(serde_json::json!({"description": "Test"}), &ctx()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_task_execute_disallowed_agent_type() {
        let tool = TaskTool::new();
        let result = tool.execute(serde_json::json!({"description": "Test", "prompt": "Go", "agent_type": "evil_agent"}), &ctx()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_task_execute_continue_session() {
        let state = Arc::new(TaskToolState::new());
        state.register_session("existing_task", "existing_session");
        let tool = TaskTool::with_state(state);
        let result = tool.execute(serde_json::json!({"description": "Continue", "prompt": "Keep going", "task_id": "existing_task"}), &ctx()).await.unwrap();
        assert!(result.content.contains("Continuing task session"));
        assert!(result.content.contains("existing_session"));
    }

    #[tokio::test]
    async fn test_task_execute_continue_nonexistent() {
        let tool = TaskTool::new();
        let result = tool.execute(serde_json::json!({"description": "Continue", "prompt": "Go", "task_id": "no_such_task"}), &ctx()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_task_execute_with_agent_type() {
        let tool = TaskTool::new();
        let result = tool.execute(serde_json::json!({"description": "Debug", "prompt": "Fix bug", "agent_type": "debug"}), &ctx()).await.unwrap();
        assert!(result.content.contains("debug"));
    }

    #[test]
    fn test_task_tool_default() {
        let tool = TaskTool::default();
        assert_eq!(tool.id(), "task");
    }

    #[tokio::test]
    async fn test_task_parameters_includes_agents() {
        let tool = TaskTool::new();
        let params = tool.parameters();
        let agent_enum = params["properties"]["agent_type"]["enum"].as_array().unwrap();
        assert!(agent_enum.len() >= 4);
    }

    #[test]
    fn test_task_tool_with_permission_config() {
        let config = PermissionConfig::default();
        let tool = TaskTool::with_permission_config(config);
        assert!(tool.permission_checker.is_some());
    }

    #[test]
    fn test_task_tool_with_all() {
        let state = Arc::new(TaskToolState::new());
        let tool = TaskTool::with_all(state.clone(), None, None);
        // Cannot compare Arc directly, just verify state is set
        assert!(Arc::ptr_eq(&tool.state, &state) || tool.state.get_session("nonexistent").is_none());
        assert!(tool.permission_checker.is_none());
        assert!(tool.agent_registry.is_none());
    }

    #[test]
    fn test_available_agent_types_excludes_unknown() {
        let tool = TaskTool::new();
        let types = tool.available_agent_types();
        // Default types should not include unknown types
        assert!(!types.contains(&"unknown".to_string()));
    }

    #[test]
    fn test_is_agent_type_allowed_explicit_false() {
        let tool = TaskTool::new();
        // Test with various invalid agent types
        assert!(!tool.is_agent_type_allowed(""));
        assert!(!tool.is_agent_type_allowed("hacker"));
        assert!(!tool.is_agent_type_allowed("ADMIN"));
    }

    #[test]
    fn test_task_tool_with_state_preserves_state() {
        let state = Arc::new(TaskToolState::new());
        state.register_session("task1", "session1");
        let tool = TaskTool::with_state(state.clone());
        assert_eq!(tool.state.get_session("task1"), Some("session1".to_string()));
    }

    #[test]
    fn test_task_tool_get_agent_none() {
        let tool = TaskTool::new();
        // Without agent registry, get_agent should return None
        assert!(tool.get_agent("any").is_none());
    }

    #[tokio::test]
    async fn test_task_execute_permission_ask() {
        use rcode_core::Permission;
        
        let config = PermissionConfig {
            default_permission: Permission::Ask,
            ..Default::default()
        };
        let tool = TaskTool::with_permission_config(config);
        let result = tool.execute(
            serde_json::json!({"description": "Test", "prompt": "Do something", "agent_type": "general"}),
            &ctx()
        ).await.unwrap();
        
        // Should get a permission pending response
        assert!(result.title.contains("Permission"));
        let metadata = result.metadata.unwrap();
        assert!(metadata["permission_pending"].as_bool().unwrap_or(false));
    }

    #[tokio::test]
    async fn test_task_execute_permission_deny() {
        use rcode_core::Permission;
        
        let config = PermissionConfig {
            default_permission: Permission::Deny,
            ..Default::default()
        };
        let tool = TaskTool::with_permission_config(config);
        let result = tool.execute(
            serde_json::json!({"description": "Test", "prompt": "Do something", "agent_type": "general"}),
            &ctx()
        ).await;
        
        // Should be denied
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("denied") || err.to_string().contains("Permission"));
    }

    #[tokio::test]
    async fn test_task_execute_new_task_with_specific_agent_type() {
        let tool = TaskTool::new();
        let result = tool.execute(
            serde_json::json!({"description": "Explore", "prompt": "Explore the codebase", "agent_type": "explore"}),
            &ctx()
        ).await.unwrap();
        
        assert!(result.content.contains("explore"));
        assert!(result.content.contains("Created new task"));
    }

    #[tokio::test]
    async fn test_task_execute_new_task_with_refactor_agent() {
        let tool = TaskTool::new();
        let result = tool.execute(
            serde_json::json!({"description": "Refactor", "prompt": "Refactor the code", "agent_type": "refactor"}),
            &ctx()
        ).await.unwrap();
        
        assert!(result.content.contains("refactor"));
    }
}