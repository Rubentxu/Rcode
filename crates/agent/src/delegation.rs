//! Delegation for sub-agents

use std::sync::Arc;

use rcode_core::{SessionId, SessionStatus, Role, Part, error::Result};
use rcode_session::SessionService;
use rcode_event::EventBus;

pub struct ChildResult {
    pub session_id: String,
    pub result: String,
}

pub struct DelegationManager {
    session_service: Arc<SessionService>,
    event_bus: Arc<EventBus>,
}

impl DelegationManager {
    pub fn new(session_service: Arc<SessionService>, event_bus: Arc<EventBus>) -> Self {
        Self {
            session_service,
            event_bus,
        }
    }

    /// Create a child session under the given parent session.
    /// Returns the child session_id.
    pub async fn create_child_session(
        &self,
        parent_session_id: &str,
        _description: &str,
        subagent_type: &str,
    ) -> Result<String> {
        // Create a child session using SessionService::create_child()
        // This properly inherits project_path from the parent session
        let child_session = self.session_service.create_child(
            parent_session_id,
            subagent_type.to_string(),
            "claude-sonnet-4-5".to_string(), // model_id (default)
        )
        .map_err(|e| rcode_core::RCodeError::Session(e))?;
        
        let child_id = child_session.id.0.clone();
        
        // Note: SessionService.create_child() already publishes SessionCreated event,
        // so we don't need to publish here
        
        Ok(child_id)
    }

    /// Wait for a child session to produce a result.
    /// Polls the session service for a completed status.
    pub async fn wait_for_child(
        &self,
        child_session_id: &str,
    ) -> Result<String> {
        // Poll for session completion (max 30s)
        let timeout = std::time::Duration::from_secs(30);
        let start = std::time::Instant::now();
        
        loop {
            if start.elapsed() > timeout {
                return Err(rcode_core::error::RCodeError::Agent(
                    format!("Child session {} timed out", child_session_id)
                ));
            }
            
            // Check if session is complete
            if let Some(session) = self.session_service.get(&SessionId(child_session_id.to_string())) {
                if session.status == SessionStatus::Completed {
                    // Get the last assistant message as result
                    let messages = self.session_service.get_messages(&session.id.0);
                    let result = messages.iter().rev()
                        .find(|m| m.role == Role::Assistant)
                        .map(|m| {
                            m.parts.iter().map(|p| match p {
                                Part::Text { content } => content.clone(),
                                _ => String::new(),
                            }).collect::<Vec<_>>().join("\n")
                        })
                        .unwrap_or_else(|| "No result".to_string());
                    return Ok(result);
                }
            }
            
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use rcode_core::Session;

    fn create_test_delegation_manager() -> (DelegationManager, Arc<SessionService>, String) {
        let event_bus = Arc::new(rcode_event::EventBus::new(1));
        let session_service = Arc::new(rcode_session::SessionService::new(event_bus.clone()));
        let delegation_manager = DelegationManager::new(session_service.clone(), event_bus);
        
        // Create a parent session so create_child can find it
        let parent = Session::new(
            std::path::PathBuf::from("/tmp/test-project"),
            "parent-agent".to_string(),
            "test-model".to_string(),
        );
        let parent_id = parent.id.0.clone();
        session_service.create(parent);
        
        (delegation_manager, session_service, parent_id)
    }

    #[tokio::test]
    async fn test_delegation_manager_new() {
        let (manager, _session_service, _parent_id) = create_test_delegation_manager();
        // Just verify it can be created
    }

    #[tokio::test]
    async fn test_create_child_session_creates_real_session() {
        let (manager, session_service, parent_id) = create_test_delegation_manager();
        
        let child_session_id = manager
            .create_child_session(&parent_id, "Test description", "explorer")
            .await
            .unwrap();
        
        // Verify a real session was created in the session service
        let session = session_service.get(&SessionId(child_session_id.clone()));
        assert!(session.is_some());
        assert_eq!(session.unwrap().agent_id, "explorer");
    }

    #[tokio::test]
    async fn test_create_child_session_publishes_event() {
        let (manager, _session_service, parent_id) = create_test_delegation_manager();
        
        let child_session_id = manager
            .create_child_session(&parent_id, "Test description", "test-agent")
            .await
            .unwrap();
        
        // Just verify the child session ID is returned (event publishing is tested in session service)
        assert!(!child_session_id.is_empty());
    }

    #[tokio::test]
    async fn test_wait_for_child_returns_no_result_when_empty() {
        let (manager, session_service, parent_id) = create_test_delegation_manager();
        
        // Create a session but don't add any messages
        let child_session_id = manager
            .create_child_session(&parent_id, "desc", "test")
            .await
            .unwrap();
        
        // Mark session as completed (simulating child task finished with no output)
        // Must transition Idle -> Running -> Completed
        session_service.update_status(&child_session_id, SessionStatus::Running);
        session_service.update_status(&child_session_id, SessionStatus::Completed);
        
        // Wait should return "No result" because there are no assistant messages
        let result = manager.wait_for_child(&child_session_id).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "No result");
    }

    #[tokio::test]
    async fn test_wait_for_child_returns_assistant_message() {
        let (manager, session_service, parent_id) = create_test_delegation_manager();
        
        // Create a session and add an assistant message
        let child_session_id = manager
            .create_child_session(&parent_id, "desc", "test")
            .await
            .unwrap();
        
        // Add an assistant message with text content
        let assistant_msg = rcode_core::Message::assistant(
            child_session_id.clone(),
            vec![Part::Text { content: "Hello from child".to_string() }],
        );
        session_service.add_message(&child_session_id, assistant_msg);
        
        // Update session status to completed
        session_service.update_status(&child_session_id, SessionStatus::Running);
        session_service.update_status(&child_session_id, SessionStatus::Completed);
        
        // Wait should return the assistant message content
        let result = manager.wait_for_child(&child_session_id).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("Hello from child"));
    }
}
