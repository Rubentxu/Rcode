//! Todowrite tool - task list management

use async_trait::async_trait;
use std::sync::Arc;
use parking_lot::RwLock;
use std::collections::HashMap;

use opencode_core::{Tool, ToolContext, ToolResult, error::{Result, OpenCodeError}};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TodoItem {
    pub content: String,
    pub status: String,  // "pending" | "in_progress" | "completed"
    pub priority: String, // "high" | "medium" | "low"
}

pub struct TodowriteTool {
    todos: Arc<RwLock<HashMap<String, Vec<TodoItem>>>>,
}

impl TodowriteTool {
    pub fn new() -> Self {
        Self {
            todos: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct TodowriteParams {
    pub action: String,
    #[serde(default)]
    pub todos: Vec<TodoItem>,
    #[serde(default)]
    pub id: Option<String>,
}

#[async_trait]
impl Tool for TodowriteTool {
    fn id(&self) -> &str { "todowrite" }
    fn name(&self) -> &str { "Task List Management" }
    fn description(&self) -> &str { "Manage task lists" }
    
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["create", "update", "list", "complete", "delete"],
                    "description": "Action to perform"
                },
                "todos": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "content": {"type": "string"},
                            "status": {"type": "string"},
                            "priority": {"type": "string"}
                        }
                    },
                    "description": "List of todo items"
                },
                "id": {
                    "type": "string",
                    "description": "Optional ID for the todo list"
                }
            },
            "required": ["action"]
        })
    }
    
    async fn execute(&self, args: serde_json::Value, _context: &ToolContext) -> Result<ToolResult> {
        let params: TodowriteParams = serde_json::from_value(args)
            .map_err(|e| OpenCodeError::Validation {
                field: String::new(),
                message: format!("Invalid parameters: {}", e),
            })?;
        
        let list_id = params.id.unwrap_or_else(|| "default".to_string());
        
        match params.action.as_str() {
            "create" | "update" => {
                let count = params.todos.len();
                let mut todos = self.todos.write();
                todos.insert(list_id.clone(), params.todos);
                Ok(ToolResult {
                    title: "Tasks Updated".to_string(),
                    content: format!("{} tasks saved to list '{}'", count, list_id),
                    metadata: None,
                    attachments: vec![],
                })
            }
            "list" => {
                let todos = self.todos.read();
                if let Some(items) = todos.get(&list_id) {
                    let formatted = items.iter().enumerate()
                        .map(|(i, t)| format!("{}. [{}] {} ({})", i+1, t.status, t.content, t.priority))
                        .collect::<Vec<_>>()
                        .join("\n");
                    Ok(ToolResult {
                        title: format!("Tasks: {}", list_id),
                        content: formatted,
                        metadata: None,
                        attachments: vec![],
                    })
                } else {
                    Ok(ToolResult {
                        title: "No tasks".to_string(),
                        content: format!("No tasks found in list '{}'", list_id),
                        metadata: None,
                        attachments: vec![],
                    })
                }
            }
            "complete" => {
                let mut todos = self.todos.write();
                if let Some(items) = todos.get_mut(&list_id) {
                    if let Some(idx) = params.todos.first().and_then(|_| Some(0)) {
                        if idx < items.len() {
                            items[idx].status = "completed".to_string();
                        }
                    }
                }
                Ok(ToolResult {
                    title: "Task Completed".to_string(),
                    content: "Task marked as completed".to_string(),
                    metadata: None,
                    attachments: vec![],
                })
            }
            "delete" => {
                let mut todos = self.todos.write();
                todos.remove(&list_id);
                Ok(ToolResult {
                    title: "Tasks Deleted".to_string(),
                    content: format!("List '{}' deleted", list_id),
                    metadata: None,
                    attachments: vec![],
                })
            }
            _ => Err(OpenCodeError::Validation {
                field: "action".to_string(),
                message: format!("Unknown action: {}", params.action),
            })
        }
    }
}

impl Default for TodowriteTool {
    fn default() -> Self {
        Self::new()
    }
}
