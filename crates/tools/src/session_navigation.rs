//! Session navigation tool - list, switch, attach sessions

use async_trait::async_trait;
use std::sync::Arc;

use rcode_core::{Tool, ToolContext, ToolResult, SessionId, Session, error::Result};
use rcode_session::SessionService;

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
                    .ok_or_else(|| rcode_core::RCodeError::Tool("session_id required for switch/attach".into()))?;
                
                if self.session_service.get(&SessionId(session_id.to_string())).is_none() {
                    return Err(rcode_core::RCodeError::Tool(format!("Session {} not found", session_id)));
                }
                
                Ok(ToolResult {
                    title: format!("Session {}", if action == "switch" { "Switched" } else { "Attached" }),
                    content: format!("Now in session: {}", session_id),
                    metadata: None,
                    attachments: vec![],
                })
            }
            _ => Err(rcode_core::RCodeError::Tool(format!("Unknown action: {}", action)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ctx() -> ToolContext {
        ToolContext {
            session_id: "s1".into(),
            project_path: PathBuf::from("/tmp"),
            cwd: PathBuf::from("/tmp"),
            user_id: None,
            agent: "test".into(),
        }
    }

    fn make_service() -> Arc<SessionService> {
        Arc::new(SessionService::new(Arc::new(rcode_event::EventBus::new(10))))
    }

    #[tokio::test]
    async fn test_list_sessions_empty() {
        let tool = SessionNavigationTool::new(make_service());
        let result = tool.execute(serde_json::json!({"action": "list"}), &ctx()).await.unwrap();
        assert_eq!(result.content, "No sessions");
    }

    #[tokio::test]
    async fn test_list_sessions_with_data() {
        let service = make_service();
        let session = Session::new(PathBuf::from("/tmp"), "agent".into(), "model".into());
        let sid = session.id.0.clone();
        service.create(session);
        let tool = SessionNavigationTool::new(service);
        let result = tool.execute(serde_json::json!({"action": "list"}), &ctx()).await.unwrap();
        assert!(result.content.contains(&sid));
    }

    #[tokio::test]
    async fn test_current_session() {
        let tool = SessionNavigationTool::new(make_service());
        let result = tool.execute(serde_json::json!({"action": "current"}), &ctx()).await.unwrap();
        assert_eq!(result.content, "s1");
    }

    #[tokio::test]
    async fn test_switch_missing_session_id() {
        let tool = SessionNavigationTool::new(make_service());
        let result = tool.execute(serde_json::json!({"action": "switch"}), &ctx()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_switch_nonexistent_session() {
        let tool = SessionNavigationTool::new(make_service());
        let result = tool.execute(serde_json::json!({"action": "switch", "session_id": "nope"}), &ctx()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_switch_valid_session() {
        let service = make_service();
        let session = Session::new(PathBuf::from("/tmp"), "a".into(), "m".into());
        let sid = session.id.0.clone();
        service.create(session);
        let tool = SessionNavigationTool::new(service);
        let result = tool.execute(serde_json::json!({"action": "switch", "session_id": &sid}), &ctx()).await.unwrap();
        assert!(result.content.contains(&sid));
        assert!(result.title.contains("Switched"));
    }

    #[tokio::test]
    async fn test_attach_valid_session() {
        let service = make_service();
        let session = Session::new(PathBuf::from("/tmp"), "a".into(), "m".into());
        let sid = session.id.0.clone();
        service.create(session);
        let tool = SessionNavigationTool::new(service);
        let result = tool.execute(serde_json::json!({"action": "attach", "session_id": &sid}), &ctx()).await.unwrap();
        assert!(result.title.contains("Attached"));
    }

    #[tokio::test]
    async fn test_unknown_action() {
        let tool = SessionNavigationTool::new(make_service());
        let result = tool.execute(serde_json::json!({"action": "delete"}), &ctx()).await;
        assert!(result.is_err());
    }
}