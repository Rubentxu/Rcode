//! ACP Session Manager - maps ACP session IDs to internal sessions

use std::collections::HashMap;
use std::sync::Arc;
use anyhow::Result;
use tokio::sync::RwLock;
use uuid::Uuid;
use rcode_core::{Message, Session, SessionId};

pub struct ACPSessionManager {
    pub(crate) sessions: Arc<RwLock<HashMap<String, SessionHandle>>>,
    session_service: Option<Arc<rcode_session::SessionService>>,
    cancellation_tokens: Arc<RwLock<HashMap<String, rcode_agent::CancellationToken>>>,
}

struct SessionHandle {
    _id: String,
}

impl ACPSessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            session_service: None,
            cancellation_tokens: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn with_session_service(mut self, service: Arc<rcode_session::SessionService>) -> Self {
        self.session_service = Some(service);
        self
    }

    pub async fn create_session(&self) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let handle = SessionHandle { _id: id.clone() };

        let mut sessions = self.sessions.write().await;
        sessions.insert(id.clone(), handle);

        self.cancellation_tokens.write().await.insert(
            id.clone(),
            rcode_agent::CancellationToken::new(),
        );

        if let Some(svc) = &self.session_service {
            let session = Session::new(
                std::env::current_dir().unwrap_or_default(),
                "acp".to_string(),
                "claude-sonnet-4-5".to_string(),
            );
            let mut session = session;
            session.id = SessionId(id.clone());
            svc.create(session);
        }

        tracing::info!("Created ACP session: {}", id);
        Ok(id)
    }

    pub async fn destroy_session(&self, session_id: &str) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        if sessions.remove(session_id).is_some() {
            self.cancellation_tokens.write().await.remove(session_id);
            if let Some(svc) = &self.session_service {
                svc.delete(session_id);
            }
            tracing::info!("Destroyed ACP session: {}", session_id);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Session not found: {}", session_id))
        }
    }

    pub async fn load_session(&self, id: &str) -> Result<Session> {
        if let Some(svc) = &self.session_service {
            svc.get(&SessionId(id.to_string()))
                .map(|s| (*s).clone())
                .ok_or_else(|| anyhow::anyhow!("Session not found: {}", id))
        } else {
            Err(anyhow::anyhow!("Session service not configured").into())
        }
    }

    pub async fn get_messages(&self, session_id: &str) -> Vec<Message> {
        if let Some(svc) = &self.session_service {
            svc.get_messages(session_id)
        } else {
            Vec::new()
        }
    }

    pub async fn add_message(&self, session_id: &str, message: Message) {
        if let Some(svc) = &self.session_service {
            svc.add_message(session_id, message);
        }
    }

    pub fn get_cancellation_token(&self, session_id: &str) -> Option<rcode_agent::CancellationToken> {
        let guard = self.cancellation_tokens.try_read().ok()?;
        guard.get(session_id).cloned()
    }

    pub async fn get_cancellation_token_async(&self, session_id: &str) -> Option<rcode_agent::CancellationToken> {
        self.cancellation_tokens.read().await.get(session_id).cloned()
    }

    pub async fn has_session(&self, session_id: &str) -> bool {
        self.sessions.read().await.contains_key(session_id)
    }

    pub async fn execute(&self, session_id: &str, prompt: &str) -> Result<serde_json::Value> {
        if !self.has_session(session_id).await {
            return Err(anyhow::anyhow!("Session not found: {}", session_id));
        }

        tracing::debug!("Execute in session {}: {}", session_id, prompt);

        Ok(serde_json::json!({
            "status": "ok",
            "message": format!("Executed: {}", prompt)
        }))
    }

    pub async fn list_sessions(&self) -> Vec<String> {
        let sessions = self.sessions.read().await;
        sessions.keys().cloned().collect()
    }
}

impl Default for ACPSessionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_session() {
        let mgr = ACPSessionManager::new();
        let id = mgr.create_session().await.unwrap();
        assert!(!id.is_empty());
        assert!(mgr.has_session(&id).await);
    }

    #[tokio::test]
    async fn test_create_unique_sessions() {
        let mgr = ACPSessionManager::new();
        let id1 = mgr.create_session().await.unwrap();
        let id2 = mgr.create_session().await.unwrap();
        assert_ne!(id1, id2);
    }

    #[tokio::test]
    async fn test_destroy_session() {
        let mgr = ACPSessionManager::new();
        let id = mgr.create_session().await.unwrap();
        assert!(mgr.destroy_session(&id).await.is_ok());
        assert!(!mgr.has_session(&id).await);
    }

    #[tokio::test]
    async fn test_destroy_nonexistent_session() {
        let mgr = ACPSessionManager::new();
        let result = mgr.destroy_session("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_sessions() {
        let mgr = ACPSessionManager::new();
        let id1 = mgr.create_session().await.unwrap();
        let id2 = mgr.create_session().await.unwrap();
        let list = mgr.list_sessions().await;
        assert_eq!(list.len(), 2);
        assert!(list.contains(&id1));
        assert!(list.contains(&id2));
    }

    #[tokio::test]
    async fn test_has_session_false() {
        let mgr = ACPSessionManager::new();
        assert!(!mgr.has_session("nonexistent").await);
    }

    #[tokio::test]
    async fn test_execute_nonexistent_session() {
        let mgr = ACPSessionManager::new();
        let result = mgr.execute("nonexistent", "hello").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_existing_session() {
        let mgr = ACPSessionManager::new();
        let id = mgr.create_session().await.unwrap();
        let result = mgr.execute(&id, "hello world").await.unwrap();
        assert_eq!(result["status"], "ok");
    }

    #[tokio::test]
    async fn test_cancellation_token_created() {
        let mgr = ACPSessionManager::new();
        let id = mgr.create_session().await.unwrap();
        let token = mgr.get_cancellation_token(&id).unwrap();
        assert!(!token.is_cancelled());
    }

    #[tokio::test]
    async fn test_cancellation_token_not_found() {
        let mgr = ACPSessionManager::new();
        assert!(mgr.get_cancellation_token("nonexistent").is_none());
    }

    #[tokio::test]
    async fn test_cancellation_token_async() {
        let mgr = ACPSessionManager::new();
        let id = mgr.create_session().await.unwrap();
        let token = mgr.get_cancellation_token_async(&id).await.unwrap();
        assert!(!token.is_cancelled());
    }

    #[tokio::test]
    async fn test_destroy_removes_cancellation_token() {
        let mgr = ACPSessionManager::new();
        let id = mgr.create_session().await.unwrap();
        mgr.destroy_session(&id).await.unwrap();
        assert!(mgr.get_cancellation_token(&id).is_none());
    }

    #[tokio::test]
    async fn test_load_session_without_service() {
        let mgr = ACPSessionManager::new();
        let result = mgr.load_session("any").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_messages_without_service() {
        let mgr = ACPSessionManager::new();
        let msgs = mgr.get_messages("any").await;
        assert!(msgs.is_empty());
    }

    #[tokio::test]
    async fn test_add_message_without_service() {
        let mgr = ACPSessionManager::new();
        let msg = rcode_core::Message::user("session1".into(), vec![rcode_core::Part::Text { content: "hello".into() }]);
        mgr.add_message("any", msg).await;
    }

    #[tokio::test]
    async fn test_with_session_service() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let session_service = Arc::new(rcode_session::SessionService::new(event_bus));
        let mgr = ACPSessionManager::new().with_session_service(session_service);
        let id = mgr.create_session().await.unwrap();
        let msgs = mgr.get_messages(&id).await;
        assert!(msgs.is_empty());
    }
}
