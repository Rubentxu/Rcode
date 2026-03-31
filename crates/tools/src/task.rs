//! Task tool - sub-agent delegation with session continuation support

use async_trait::async_trait;
use std::sync::Arc;
use parking_lot::RwLock;

use opencode_core::{Tool, ToolContext, ToolResult, PermissionChecker, PermissionConfig, Permission, error::{Result, OpenCodeError}};

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

pub struct TaskTool {
    /// Available agent types that can be spawned
    pub agent_types: Vec<String>,
    /// Shared state for task sessions
    pub state: Arc<TaskToolState>,
    /// Permission checker for validating subagent invocations
    permission_checker: Option<Arc<PermissionChecker>>,
}

impl TaskTool {
    pub fn new() -> Self {
        Self {
            agent_types: vec![
                "general".to_string(),
                "explore".to_string(),
                "refactor".to_string(),
                "debug".to_string(),
            ],
            state: Arc::new(TaskToolState::new()),
            permission_checker: None,
        }
    }
    
    pub fn with_state(state: Arc<TaskToolState>) -> Self {
        Self {
            agent_types: vec![
                "general".to_string(),
                "explore".to_string(),
                "refactor".to_string(),
                "debug".to_string(),
            ],
            state,
            permission_checker: None,
        }
    }
    
    /// Create a TaskTool with permission checking enabled
    pub fn with_permission_config(permission_config: PermissionConfig) -> Self {
        Self {
            agent_types: vec![
                "general".to_string(),
                "explore".to_string(),
                "refactor".to_string(),
                "debug".to_string(),
            ],
            state: Arc::new(TaskToolState::new()),
            permission_checker: Some(Arc::new(PermissionChecker::new(permission_config))),
        }
    }
    
    /// Get available agent types
    pub fn available_agent_types(&self) -> &[String] {
        &self.agent_types
    }
    
    /// Check if an agent type is allowed
    pub fn is_agent_type_allowed(&self, agent_type: &str) -> bool {
        self.agent_types.iter().any(|t| t == agent_type)
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
                    "enum": ["general", "explore", "refactor", "debug"]
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
            .ok_or_else(|| OpenCodeError::Validation {
                field: "description".to_string(),
                message: "Description is required".to_string(),
            })?;
        
        let prompt = args["prompt"].as_str()
            .ok_or_else(|| OpenCodeError::Validation {
                field: "prompt".to_string(),
                message: "Prompt is required".to_string(),
            })?;
        
        let agent_type = args["agent_type"].as_str().unwrap_or("general");
        let task_id = args["task_id"].as_str();
        
        // Validate agent type is allowed
        if !self.is_agent_type_allowed(agent_type) {
            return Err(OpenCodeError::Permission(format!(
                "Agent type '{}' is not allowed. Available types: {:?}",
                agent_type, self.agent_types
            )));
        }
        
        // Check permission before launching subagent
        if let Some(ref checker) = self.permission_checker {
            let check_result = checker.check(&context.agent, "task", Some(agent_type));
            
            match check_result.permission {
                Permission::Allow => { /* proceed */ }
                Permission::Deny => {
                    return Err(OpenCodeError::Permission(format!(
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
                return Err(OpenCodeError::Session(format!(
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
            
            format!(
                "Created new task '{}' (session: {}) with {} agent:\n\nDescription: {}\n\nPrompt:\n{}",
                new_task_id, session_id, agent_type, description, prompt
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
