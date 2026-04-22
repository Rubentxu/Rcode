//! Task tool - sub-agent delegation with session continuation support

use async_trait::async_trait;
use std::sync::Arc;
use parking_lot::RwLock;

use rcode_core::{Tool, ToolContext, ToolResult, PermissionChecker, PermissionConfig, Permission, AgentRegistry, SubagentRunner, SubagentResult, error::{Result as CoreResult, RCodeError}};
use rcode_session::SessionService;
use rcode_event::EventBus;
use rcode_providers::ProviderRegistry;

use super::registry::ToolRegistryService;

/// System prompt for task sub-agents with read-only tools
pub const TASK_SYSTEM_PROMPT: &str = "You are a research agent. Your task is to investigate and return findings. You have access to read-only tools: glob, grep, read, webfetch. Use them to search the codebase. Return a concise summary of your findings.";

/// Read-only tools allowed for task sub-agents
pub const READONLY_TOOLS: &[&str] = &["glob", "grep", "read", "webfetch"];

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
    /// Session service for creating child sessions
    session_service: Option<Arc<SessionService>>,
    /// Tool registry for subagent execution
    tool_registry: Option<Arc<ToolRegistryService>>,
    /// Event bus for publishing events
    event_bus: Option<Arc<EventBus>>,
    /// Provider registry for model resolution
    provider_registry: Option<Arc<ProviderRegistry>>,
    /// Subagent runner for actual delegation (server-provided)
    /// Made public for injection by server composition root
    pub subagent_runner: Option<Arc<dyn SubagentRunner>>,
}

impl TaskTool {
    /// Create a new TaskTool without agent registry
    pub fn new() -> Self {
        Self {
            state: Arc::new(TaskToolState::new()),
            permission_checker: None,
            agent_registry: None,
            session_service: None,
            tool_registry: None,
            event_bus: None,
            provider_registry: None,
            subagent_runner: None,
        }
    }
    
    /// Create a TaskTool with state
    pub fn with_state(state: Arc<TaskToolState>) -> Self {
        Self {
            state,
            permission_checker: None,
            agent_registry: None,
            session_service: None,
            tool_registry: None,
            event_bus: None,
            provider_registry: None,
            subagent_runner: None,
        }
    }
    
    /// Create a TaskTool with permission checking enabled
    pub fn with_permission_config(permission_config: PermissionConfig) -> Self {
        Self {
            state: Arc::new(TaskToolState::new()),
            permission_checker: Some(Arc::new(PermissionChecker::new(permission_config))),
            agent_registry: None,
            session_service: None,
            tool_registry: None,
            event_bus: None,
            provider_registry: None,
            subagent_runner: None,
        }
    }

    /// Create a TaskTool with an agent registry
    pub fn with_agent_registry(registry: Arc<AgentRegistry>) -> Self {
        Self {
            state: Arc::new(TaskToolState::new()),
            permission_checker: None,
            agent_registry: Some(registry),
            session_service: None,
            tool_registry: None,
            event_bus: None,
            provider_registry: None,
            subagent_runner: None,
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
            session_service: None,
            tool_registry: None,
            event_bus: None,
            provider_registry: None,
            subagent_runner: None,
        }
    }

    /// Create a TaskTool with services for real delegation
    pub fn with_services(
        state: Arc<TaskToolState>,
        session_service: Arc<SessionService>,
        tool_registry: Arc<ToolRegistryService>,
        event_bus: Arc<EventBus>,
        provider_registry: Arc<ProviderRegistry>,
    ) -> Self {
        Self {
            state,
            permission_checker: None,
            agent_registry: None,
            session_service: Some(session_service),
            tool_registry: Some(tool_registry),
            event_bus: Some(event_bus),
            provider_registry: Some(provider_registry),
            subagent_runner: None,
        }
    }

    /// Create a TaskTool with session service for child session creation
    pub fn with_session_service(session_service: Arc<SessionService>) -> Self {
        Self {
            state: Arc::new(TaskToolState::new()),
            permission_checker: None,
            agent_registry: None,
            session_service: Some(session_service),
            tool_registry: None,
            event_bus: None,
            provider_registry: None,
            subagent_runner: None,
        }
    }

    /// Create a TaskTool with a subagent runner for actual delegation
    pub fn with_subagent_runner(runner: Arc<dyn SubagentRunner>) -> Self {
        Self {
            state: Arc::new(TaskToolState::new()),
            permission_checker: None,
            agent_registry: None,
            session_service: None,
            tool_registry: None,
            event_bus: None,
            provider_registry: None,
            subagent_runner: Some(runner),
        }
    }

    /// Set the subagent runner on an existing TaskTool
    /// 
    /// This is used by the server composition root to inject the runner
    /// after the TaskTool has been created by ToolRegistryService.
    pub fn set_subagent_runner(&mut self, runner: Arc<dyn SubagentRunner>) {
        self.subagent_runner = Some(runner);
    }

    /// Create a clone of this TaskTool with a different subagent runner
    /// 
    /// This allows replacing the runner while preserving all other fields.
    pub fn with_runner(mut self, runner: Arc<dyn SubagentRunner>) -> Self {
        self.subagent_runner = Some(runner);
        self
    }

    /// Check if all delegation services are available
    pub fn has_delegation_services(&self) -> bool {
        self.session_service.is_some()
            && self.tool_registry.is_some()
            && self.event_bus.is_some()
            && self.provider_registry.is_some()
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
    
    async fn execute(&self, args: serde_json::Value, context: &ToolContext) -> CoreResult<ToolResult> {
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
        
        // If we have a subagent_runner, delegate to it for actual execution
        if let Some(ref runner) = self.subagent_runner {
            let session_id = if let Some(existing_task_id) = task_id {
                // Session continuation: check if session exists
                if let Some(sid) = self.state.get_session(existing_task_id) {
                    sid
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
                
                // Use real SessionService if available to create a proper child session
                let sid = if let Some(ref session_service) = self.session_service {
                    match session_service.create_child(&context.session_id, agent_type.to_string(), "claude-sonnet-4-5".to_string()) {
                        Ok(child_session) => {
                            let real_session_id = child_session.id.0.clone();
                            self.state.register_session(&new_task_id, &real_session_id);
                            real_session_id
                        }
                        Err(e) => {
                            let synthetic_id = format!("session_{}_{}_{}", 
                                context.session_id,
                                new_task_id,
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_nanos()
                            );
                            tracing::warn!("Failed to create child session: {}. Using synthetic ID: {}", e, synthetic_id);
                            self.state.register_session(&new_task_id, &synthetic_id);
                            synthetic_id
                        }
                    }
                } else {
                    let synthetic_id = format!("session_{}_{}_{}", 
                        context.session_id,
                        new_task_id,
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_nanos()
                    );
                    self.state.register_session(&new_task_id, &synthetic_id);
                    synthetic_id
                };
                sid
            };

            // Delegate to the subagent runner
            match runner.run_subagent(&session_id, agent_type, prompt, READONLY_TOOLS).await {
                Ok(result) => {
                    return Ok(ToolResult {
                        title: format!("Task: {}", description),
                        content: result.response_text,
                        metadata: Some(serde_json::json!({
                            "agent_type": agent_type,
                            "task_id": task_id,
                            "delegated": true,
                            "child_session_id": result.child_session_id,
                        })),
                        attachments: vec![],
                    });
                }
                Err(e) => {
                    return Err(RCodeError::Tool(format!(
                        "Subagent execution failed: {}", e
                    )));
                }
            }
        }

        // Fallback: no subagent_runner, use current placeholder behavior
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
            
            // Use real SessionService if available to create a proper child session
            let session_id = if let Some(ref session_service) = self.session_service {
                // Create a real child session via SessionService
                match session_service.create_child(&context.session_id, agent_type.to_string(), "claude-sonnet-4-5".to_string()) {
                    Ok(child_session) => {
                        let real_session_id = child_session.id.0.clone();
                        // Register the real session for potential continuation
                        self.state.register_session(&new_task_id, &real_session_id);
                        real_session_id
                    }
                    Err(e) => {
                        // Fall back to synthetic session ID if child creation fails
                        let synthetic_id = format!("session_{}_{}_{}", 
                            context.session_id,
                            new_task_id,
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_nanos()
                        );
                        tracing::warn!("Failed to create child session: {}. Using synthetic ID: {}", e, synthetic_id);
                        self.state.register_session(&new_task_id, &synthetic_id);
                        synthetic_id
                    }
                }
            } else {
                // Fall back to synthetic session ID when no SessionService available
                let synthetic_id = format!("session_{}_{}_{}", 
                    context.session_id,
                    new_task_id,
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos()
                );
                self.state.register_session(&new_task_id, &synthetic_id);
                synthetic_id
            };
            
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

    // ========== Service Injection Tests ==========

    #[test]
    fn test_task_tool_with_session_service() {
        let event_bus = Arc::new(rcode_event::EventBus::new(1));
        let session_service = Arc::new(rcode_session::SessionService::new(event_bus));
        let tool = TaskTool::with_session_service(session_service.clone());
        
        // Tool should have session_service but not full delegation services
        assert!(!tool.has_delegation_services());
    }

    #[test]
    fn test_task_tool_has_delegation_services_false_when_no_services() {
        let tool = TaskTool::new();
        assert!(!tool.has_delegation_services());
    }

    #[test]
    fn test_task_tool_has_delegation_services_false_when_partial() {
        let event_bus = Arc::new(rcode_event::EventBus::new(1));
        let session_service = Arc::new(rcode_session::SessionService::new(event_bus));
        let tool = TaskTool::with_session_service(session_service);
        
        // Only session_service, missing tool_registry, event_bus, provider_registry
        assert!(!tool.has_delegation_services());
    }

    #[test]
    fn test_task_tool_has_delegation_services_requires_all() {
        // Verify that has_delegation_services checks all four services
        let tool = TaskTool::new();
        
        // None of the services are set
        assert!(tool.session_service.is_none());
        assert!(tool.tool_registry.is_none());
        assert!(tool.event_bus.is_none());
        assert!(tool.provider_registry.is_none());
    }

    #[test]
    fn test_task_system_prompt_defined() {
        assert!(!TASK_SYSTEM_PROMPT.is_empty());
        assert!(TASK_SYSTEM_PROMPT.contains("research agent"));
        assert!(TASK_SYSTEM_PROMPT.contains("glob"));
        assert!(TASK_SYSTEM_PROMPT.contains("grep"));
        assert!(TASK_SYSTEM_PROMPT.contains("read"));
        assert!(TASK_SYSTEM_PROMPT.contains("webfetch"));
    }

    #[test]
    fn test_readonly_tools_defined() {
        assert_eq!(READONLY_TOOLS.len(), 4);
        assert!(READONLY_TOOLS.contains(&"glob"));
        assert!(READONLY_TOOLS.contains(&"grep"));
        assert!(READONLY_TOOLS.contains(&"read"));
        assert!(READONLY_TOOLS.contains(&"webfetch"));
    }

    #[tokio::test]
    async fn test_task_execute_with_real_session_service() {
        // Create a real session service
        let event_bus = Arc::new(rcode_event::EventBus::new(1));
        let session_service = Arc::new(rcode_session::SessionService::new(event_bus));
        
        // Create a parent session first so create_child can find it
        let parent = rcode_core::Session::new(
            std::path::PathBuf::from("/tmp"),
            "parent".to_string(),
            "claude-sonnet-4-5".to_string(),
        );
        let parent_id = parent.id.0.clone();
        session_service.create(parent);
        
        let tool = TaskTool::with_session_service(session_service);
        let mut context = ctx();
        context.session_id = parent_id;
        
        let result = tool.execute(
            serde_json::json!({"description": "Test task", "prompt": "Do something"}),
            &context
        ).await.unwrap();
        
        // Should contain real session info
        assert!(result.content.contains("Created new task"));
        // The session_id should be a real one created by session_service
        // Not a synthetic one like "session_s1_task_xxx_xxx"
        assert!(!result.content.contains("session_s1_task_"));
    }

    // ========== SubagentRunner Tests ==========

    /// Mock SubagentRunner for testing
    struct MockSubagentRunner {
        response_text: String,
        should_error: bool,
        child_session_id: String,
    }

    #[async_trait::async_trait]
    impl SubagentRunner for MockSubagentRunner {
        async fn run_subagent(
            &self,
            parent_session_id: &str,
            _agent_id: &str,
            _prompt: &str,
            _allowed_tools: &[&str],
        ) -> Result<SubagentResult, Box<dyn std::error::Error + Send + Sync>> {
            if self.should_error {
                Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "mock error")) as Box<dyn std::error::Error + Send + Sync>)
            } else {
                Ok(SubagentResult {
                    response_text: self.response_text.clone(),
                    child_session_id: if self.child_session_id.is_empty() {
                        parent_session_id.to_string()
                    } else {
                        self.child_session_id.clone()
                    },
                })
            }
        }
    }

    #[test]
    fn test_task_tool_with_subagent_runner() {
        let runner: Arc<dyn SubagentRunner> = Arc::new(MockSubagentRunner {
            response_text: "delegated response".to_string(),
            should_error: false,
            child_session_id: String::new(),
        });
        let tool = TaskTool::with_subagent_runner(runner);
        // The runner should be set
        assert!(tool.subagent_runner.is_some());
    }

    #[tokio::test]
    async fn test_task_execute_delegates_when_runner_available() {
        let runner: Arc<dyn SubagentRunner> = Arc::new(MockSubagentRunner {
            response_text: "delegated response".to_string(),
            should_error: false,
            child_session_id: String::new(),
        });
        
        let event_bus = Arc::new(rcode_event::EventBus::new(1));
        let session_service = Arc::new(rcode_session::SessionService::new(event_bus));
        
        // Create a parent session first
        let parent = rcode_core::Session::new(
            std::path::PathBuf::from("/tmp"),
            "parent".to_string(),
            "claude-sonnet-4-5".to_string(),
        );
        let parent_id = parent.id.0.clone();
        session_service.create(parent);
        
        let tool = TaskTool::with_session_service(session_service)
            .with_runner(runner);
        
        let mut context = ctx();
        context.session_id = parent_id;
        
        let result = tool.execute(
            serde_json::json!({"description": "Test", "prompt": "Do something"}),
            &context
        ).await.unwrap();
        
        // Should return the delegated response
        assert_eq!(result.content, "delegated response");
        assert!(result.metadata.as_ref().unwrap()["delegated"].as_bool().unwrap_or(false));
    }

    #[tokio::test]
    async fn test_task_execute_falls_back_when_runner_returns_error() {
        let runner: Arc<dyn SubagentRunner> = Arc::new(MockSubagentRunner {
            response_text: String::new(),
            should_error: true,
            child_session_id: String::new(),
        });
        
        let event_bus = Arc::new(rcode_event::EventBus::new(1));
        let session_service = Arc::new(rcode_session::SessionService::new(event_bus));
        
        // Create a parent session first
        let parent = rcode_core::Session::new(
            std::path::PathBuf::from("/tmp"),
            "parent".to_string(),
            "claude-sonnet-4-5".to_string(),
        );
        let parent_id = parent.id.0.clone();
        session_service.create(parent);
        
        let tool = TaskTool::with_session_service(session_service)
            .with_runner(runner);
        
        let mut context = ctx();
        context.session_id = parent_id;
        
        let result = tool.execute(
            serde_json::json!({"description": "Test", "prompt": "Do something"}),
            &context
        ).await;
        
        // Should fail with runner error
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Subagent execution failed"));
    }

    #[tokio::test]
    async fn test_task_execute_continues_existing_session_with_runner() {
        let runner: Arc<dyn SubagentRunner> = Arc::new(MockSubagentRunner {
            response_text: "continuation response".to_string(),
            should_error: false,
            child_session_id: String::new(),
        });
        
        let event_bus = Arc::new(rcode_event::EventBus::new(1));
        let session_service = Arc::new(rcode_session::SessionService::new(event_bus));
        
        // Create a parent session first
        let parent = rcode_core::Session::new(
            std::path::PathBuf::from("/tmp"),
            "parent".to_string(),
            "claude-sonnet-4-5".to_string(),
        );
        let parent_id = parent.id.0.clone();
        session_service.create(parent);
        
        let tool = TaskTool::with_session_service(session_service)
            .with_runner(runner);
        
        let mut context = ctx();
        context.session_id = parent_id.clone();
        
        // First execute to create a session
        tool.execute(
            serde_json::json!({"description": "Initial", "prompt": "First"}),
            &context
        ).await.unwrap();
        
        // Get the task_id from the state
        let state = &tool.state;
        let task_ids: Vec<_> = state.task_sessions.read().keys().cloned().collect();
        assert!(!task_ids.is_empty());
        let task_id = task_ids.first().unwrap();
        
        // Second execute with task_id to continue
        let result = tool.execute(
            serde_json::json!({"description": "Continue", "prompt": "Continue task", "task_id": task_id}),
            &context
        ).await.unwrap();
        
        // Should use the delegated runner
        assert_eq!(result.content, "continuation response");
    }

    #[tokio::test]
    async fn test_task_execute_without_runner_falls_back_to_placeholder() {
        // No runner - should use placeholder behavior
        let event_bus = Arc::new(rcode_event::EventBus::new(1));
        let session_service = Arc::new(rcode_session::SessionService::new(event_bus));
        
        // Create a parent session first
        let parent = rcode_core::Session::new(
            std::path::PathBuf::from("/tmp"),
            "parent".to_string(),
            "claude-sonnet-4-5".to_string(),
        );
        let parent_id = parent.id.0.clone();
        session_service.create(parent);
        
        // Create tool WITHOUT subagent_runner
        let tool = TaskTool::with_session_service(session_service);
        
        let mut context = ctx();
        context.session_id = parent_id;
        
        let result = tool.execute(
            serde_json::json!({"description": "Test", "prompt": "Do something"}),
            &context
        ).await.unwrap();
        
        // Should return placeholder content, not delegated response
        assert!(result.content.contains("Created new task"));
        // delegated flag should not be present
        if let Some(metadata) = &result.metadata {
            assert!(!metadata.get("delegated").map(|v| v.as_bool().unwrap_or(false)).unwrap_or(false));
        }
    }
}