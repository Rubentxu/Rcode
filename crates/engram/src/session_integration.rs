//! Session integration for Engram
//!
//! This module provides integration between the session service and Engram
//! persistent memory, allowing automatic saving of session context.

use crate::client::EngramClient;
use crate::types::{Observation, ObservationType, Scope};
use rcode_session::SessionService;
use std::path::Path;
use std::sync::Arc;

/// Integration between session service and Engram memory
pub struct EngramSessionIntegration {
    engram: Arc<EngramClient>,
    session_service: Arc<SessionService>,
}

impl EngramSessionIntegration {
    /// Create a new session integration
    pub fn new(engram: Arc<EngramClient>, session_service: Arc<SessionService>) -> Self {
        Self {
            engram,
            session_service,
        }
    }

    /// Called when a session ends - saves relevant context to Engram
    pub async fn on_session_end(&self, session_id: &str) -> anyhow::Result<()> {
        let session = match self.session_service.get(&rcode_core::SessionId(session_id.to_string())) {
            Some(s) => s,
            None => {
                tracing::debug!("Session {} not found, skipping Engram integration", session_id);
                return Ok(());
            }
        };

        let project_path = session.project_path.to_string_lossy().to_string();
        let agent_id = &session.agent_id;
        let message_count = self.session_service.get_messages(session_id).len();

        // Create a summary observation for the session
        let title = format!("Session with {} ({} messages)", agent_id, message_count);
        let content = format!(
            "Session ended with agent '{}'. Project: {}. Total messages: {}.",
            agent_id,
            project_path,
            message_count
        );

        let obs = Observation::with_all(
            title,
            content,
            ObservationType::Learning,
            Scope::Project,
            Some("session/summary".to_string()),
            Some(project_path),
            Some(session_id.to_string()),
        );

        self.engram.save(obs).await?;

        tracing::info!(
            "Saved session {} summary to Engram: {} messages",
            session_id,
            message_count
        );

        Ok(())
    }

    /// Get relevant context for the current project
    pub async fn get_relevant_context(&self, current_project: &Path) -> anyhow::Result<String> {
        let project_str = current_project.to_string_lossy().to_string();

        // Get recent observations for this project
        let observations = self.engram.get_project(&project_str).await?;

        if observations.is_empty() {
            return Ok(String::new());
        }

        // Format as context summary
        let context = observations
            .iter()
            .take(10)
            .map(|obs| {
                format!(
                    "- [{}] {}: {}",
                    obs.obs_type,
                    obs.title,
                    obs.content.chars().take(100).collect::<String>()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        Ok(format!(
            "## Relevant context from project {}:\n\n{}\n",
            project_str, context
        ))
    }

    /// Save a decision discovered during the session
    pub async fn save_decision(
        &self,
        session_id: &str,
        project_path: &Path,
        decision: &str,
        rationale: &str,
    ) -> anyhow::Result<i64> {
        let obs = Observation::with_all(
            format!("Decision: {}", decision),
            rationale.to_string(),
            ObservationType::Decision,
            Scope::Project,
            Some("session/decision".to_string()),
            Some(project_path.to_string_lossy().to_string()),
            Some(session_id.to_string()),
        );

        let id = self.engram.save(obs).await?;
        tracing::debug!("Saved decision to Engram: {} (id={})", decision, id);
        Ok(id)
    }

    /// Save a discovery made during the session
    pub async fn save_discovery(
        &self,
        session_id: &str,
        project_path: &Path,
        title: &str,
        discovery: &str,
    ) -> anyhow::Result<i64> {
        let obs = Observation::with_all(
            title.to_string(),
            discovery.to_string(),
            ObservationType::Discovery,
            Scope::Project,
            Some("session/discovery".to_string()),
            Some(project_path.to_string_lossy().to_string()),
            Some(session_id.to_string()),
        );

        let id = self.engram.save(obs).await?;
        tracing::debug!("Saved discovery to Engram: {} (id={})", title, id);
        Ok(id)
    }

    /// Save a bugfix discovered during the session
    pub async fn save_bugfix(
        &self,
        session_id: &str,
        project_path: &Path,
        bug: &str,
        fix: &str,
    ) -> anyhow::Result<i64> {
        let obs = Observation::with_all(
            format!("Bugfix: {}", bug),
            format!("Problem: {}\n\nSolution: {}", bug, fix),
            ObservationType::Bugfix,
            Scope::Project,
            Some("session/bugfix".to_string()),
            Some(project_path.to_string_lossy().to_string()),
            Some(session_id.to_string()),
        );

        let id = self.engram.save(obs).await?;
        tracing::debug!("Saved bugfix to Engram: {} (id={})", bug, id);
        Ok(id)
    }

    /// Search for relevant observations across all projects
    pub async fn search_relevant(&self, query: &str, limit: usize) -> anyhow::Result<Vec<Observation>> {
        let results = self.engram.search(query, limit).await?;
        Ok(results)
    }

    /// Get the Engram client
    pub fn engram_client(&self) -> Arc<EngramClient> {
        self.engram.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::Session;
    use tempfile::TempDir;

    fn create_test_integration() -> (EngramSessionIntegration, TempDir) {
        let temp = TempDir::new().unwrap();
        let engram = Arc::new(
            EngramClient::new(&temp.path().join("engram.db")).unwrap()
        );

        let event_bus = Arc::new(rcode_event::EventBus::new(100));
        let session_service = Arc::new(SessionService::new(event_bus));

        (
            EngramSessionIntegration::new(engram, session_service),
            temp,
        )
    }

    #[tokio::test]
    async fn test_get_relevant_context_empty() {
        let (integration, _dir) = create_test_integration();
        let context = integration
            .get_relevant_context(std::path::Path::new("/nonexistent"))
            .await
            .unwrap();
        assert!(context.is_empty());
    }

    #[tokio::test]
    async fn test_save_decision() {
        let (integration, _dir) = create_test_integration();

        // Create a session first
        let session = Session::new(
            std::path::PathBuf::from("/test/project"),
            "test-agent".to_string(),
            "test-model".to_string(),
        );
        let session_id = session.id.0.clone();
        integration.session_service.create(session);

        let id = integration
            .save_decision(
                &session_id,
                std::path::Path::new("/test/project"),
                "Use SQLite for storage",
                "SQLite is embedded and doesn't require a separate server",
            )
            .await
            .unwrap();

        assert!(id > 0);

        // Verify it was saved
        let obs = integration.engram.get(id).await.unwrap();
        assert!(obs.is_some());
        let obs = obs.unwrap();
        assert!(obs.title.contains("Use SQLite"));
    }

    #[tokio::test]
    async fn test_save_discovery() {
        let (integration, _dir) = create_test_integration();

        let session = Session::new(
            std::path::PathBuf::from("/test/project"),
            "test-agent".to_string(),
            "test-model".to_string(),
        );
        let session_id = session.id.0.clone();
        integration.session_service.create(session);

        let id = integration
            .save_discovery(
                &session_id,
                std::path::Path::new("/test/project"),
                "FTS5 is available",
                "Full-text search is supported via FTS5 virtual tables",
            )
            .await
            .unwrap();

        assert!(id > 0);
    }

    #[tokio::test]
    async fn test_save_bugfix() {
        let (integration, _dir) = create_test_integration();

        let session = Session::new(
            std::path::PathBuf::from("/test/project"),
            "test-agent".to_string(),
            "test-model".to_string(),
        );
        let session_id = session.id.0.clone();
        integration.session_service.create(session);

        let id = integration
            .save_bugfix(
                &session_id,
                std::path::Path::new("/test/project"),
                "Connection leak in storage",
                "Fixed by using Mutex<Connection> instead of raw Connection",
            )
            .await
            .unwrap();

        assert!(id > 0);
    }

    #[tokio::test]
    async fn test_on_session_end_not_found() {
        let (integration, _dir) = create_test_integration();
        
        // Should not error when session not found
        let result = integration.on_session_end("nonexistent-session").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_on_session_end_success() {
        let (integration, _dir) = create_test_integration();

        let session = Session::new(
            std::path::PathBuf::from("/test/project"),
            "test-agent".to_string(),
            "test-model".to_string(),
        );
        let session_id = session.id.0.clone();
        integration.session_service.create(session);

        // Add a message to the session
        let msg = rcode_core::Message::user(
            session_id.clone(),
            vec![rcode_core::Part::Text { content: "Hello".to_string() }],
        );
        integration.session_service.add_message(&session_id, msg);

        let result = integration.on_session_end(&session_id).await;
        assert!(result.is_ok());

        // Verify session summary was saved
        let context = integration.get_relevant_context(std::path::Path::new("/test/project")).await.unwrap();
        assert!(!context.is_empty());
    }

    #[tokio::test]
    async fn test_search_relevant() {
        let (integration, _dir) = create_test_integration();

        let session = Session::new(
            std::path::PathBuf::from("/test/project"),
            "test-agent".to_string(),
            "test-model".to_string(),
        );
        let session_id = session.id.0.clone();
        integration.session_service.create(session);

        // Save a decision
        integration
            .save_decision(
                &session_id,
                std::path::Path::new("/test/project"),
                "Use SQLite",
                "Because it's embedded",
            )
            .await
            .unwrap();

        // Search for it
        let results = integration.search_relevant("SQLite", 10).await.unwrap();
        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn test_engram_client_getter() {
        let (integration, _dir) = create_test_integration();
        let client = integration.engram_client();
        assert!(Arc::strong_count(&client) >= 1);
    }

    #[tokio::test]
    async fn test_get_relevant_context_with_data() {
        let (integration, _dir) = create_test_integration();

        let session = Session::new(
            std::path::PathBuf::from("/test/project"),
            "test-agent".to_string(),
            "test-model".to_string(),
        );
        let session_id = session.id.0.clone();
        integration.session_service.create(session);

        // Save something
        integration
            .save_decision(
                &session_id,
                std::path::Path::new("/test/project"),
                "Test decision",
                "Test rationale",
            )
            .await
            .unwrap();

        let context = integration.get_relevant_context(std::path::Path::new("/test/project")).await.unwrap();
        assert!(context.contains("Test decision"));
    }
}
