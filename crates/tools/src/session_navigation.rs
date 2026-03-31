//! Session navigation tool - list, switch, attach sessions

use async_trait::async_trait;
use std::sync::Arc;

use opencode_core::{Tool, ToolContext, ToolResult, SessionId, error::Result};
use opencode_session::SessionService;

pub struct SessionNavigationTool {
    session_service: Arc<SessionService>,
}

impl SessionNavigationTool {
    pub fn new(session_service: Arc<SessionService>) -> Self {
        Self { session_service }
    }
}

#[async_trait]
impl Tool for SessionNavigationTool {
    fn id(&self) -> &str { "session" }
    fn name(&self) -> &str { "Session Navigation" }
    fn description(&self) -> &str { "Navigate between sessions: list, switch, attach" }
    
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "switch", "attach", "current"],
                    "description": "Action to perform"
                },
                "session_id": {
                    "type": "string",
                    "description": "Session ID to switch to or attach to"
                }
            },
            "required": ["action"]
        })
    }
    
    async fn execute(&self, args: serde_json::Value, context: &ToolContext) -> Result<ToolResult> {
        let action = args["action"]
            .as_str()
            .unwrap_or("list");
        
        match action {
            "list" => {
                let sessions = self.session_service.list_all();
                let formatted = sessions.iter()
                    .map(|s| format!("{} - {:?} ({})", s.id.0, s.status, s.updated_at))
                    .collect::<Vec<_>>()
                    .join("\n");
                Ok(ToolResult {
                    title: "Sessions".to_string(),
                    content: if formatted.is_empty() { "No sessions".to_string() } else { formatted },
                    metadata: None,
                    attachments: vec![],
                })
            }
            "current" => {
                Ok(ToolResult {
                    title: "Current Session".to_string(),
                    content: context.session_id.clone(),
                    metadata: None,
                    attachments: vec![],
                })
            }
            "switch" | "attach" => {
                let session_id = args["session_id"]
                    .as_str()
                    .ok_or_else(|| opencode_core::OpenCodeError::Tool("session_id required for switch/attach".into()))?;
                
                if self.session_service.get(&SessionId(session_id.to_string())).is_none() {
                    return Err(opencode_core::OpenCodeError::Tool(format!("Session {} not found", session_id)));
                }
                
                Ok(ToolResult {
                    title: format!("Session {}", if action == "switch" { "Switched" } else { "Attached" }),
                    content: format!("Now in session: {}", session_id),
                    metadata: None,
                    attachments: vec![],
                })
            }
            _ => Err(opencode_core::OpenCodeError::Tool(format!("Unknown action: {}", action)))
        }
    }
}