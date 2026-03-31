//! Session service with L1 cache and write-through SQLite persistence

use chrono::Utc;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use opencode_core::{
    Message, PaginatedMessages, PaginationParams, Session, SessionId, SessionStatus,
};
use opencode_event::EventBus;
use opencode_storage::{MessageRepository, SessionRepository};

use crate::compaction::{CompactionConfig, CompactionResult, CompactionStrategy};
use crate::summarizer::Summarizer;

pub struct SessionService {
    sessions: RwLock<HashMap<String, Arc<Session>>>,
    messages: RwLock<HashMap<String, Vec<Message>>>,
    event_bus: Arc<EventBus>,
    session_repo: Option<Arc<SessionRepository>>,
    message_repo: Option<Arc<MessageRepository>>,
    compaction_config: CompactionConfig,
    summarizer: Option<Arc<Summarizer>>,
    compaction_strategy: CompactionStrategy,
}

impl SessionService {
    pub fn new(event_bus: Arc<EventBus>) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            messages: RwLock::new(HashMap::new()),
            event_bus,
            session_repo: None,
            message_repo: None,
            compaction_config: CompactionConfig::default(),
            summarizer: None,
            compaction_strategy: CompactionStrategy::default(),
        }
    }

    pub fn with_storage(
        event_bus: Arc<EventBus>,
        session_repo: SessionRepository,
        message_repo: MessageRepository,
    ) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            messages: RwLock::new(HashMap::new()),
            event_bus,
            session_repo: Some(Arc::new(session_repo)),
            message_repo: Some(Arc::new(message_repo)),
            compaction_config: CompactionConfig::default(),
            summarizer: None,
            compaction_strategy: CompactionStrategy::default(),
        }
    }

    /// Configure the service with a summarizer for compaction
    pub fn with_summarizer(
        mut self,
        summarizer: Arc<Summarizer>,
        config: CompactionConfig,
        strategy: CompactionStrategy,
    ) -> Self {
        self.summarizer = Some(summarizer);
        self.compaction_config = config;
        self.compaction_strategy = strategy;
        self
    }

    /// Update compaction configuration
    pub fn set_compaction_config(&mut self, config: CompactionConfig) {
        self.compaction_config = config;
    }

    /// Update compaction strategy
    pub fn set_compaction_strategy(&mut self, strategy: CompactionStrategy) {
        self.compaction_strategy = strategy;
    }

    /// Set the summarizer for compaction
    pub fn set_summarizer(&mut self, summarizer: Arc<Summarizer>) {
        self.summarizer = Some(summarizer);
    }

    pub fn create(&self, session: Session) -> Arc<Session> {
        let session = Arc::new(session);

        // Persist to SQLite if storage is configured
        if let Some(repo) = &self.session_repo {
            if let Err(e) = repo.save(&session) {
                tracing::error!("Failed to persist session: {}", e);
            }
        }

        self.sessions
            .write()
            .insert(session.id.0.clone(), session.clone());
        self.messages
            .write()
            .insert(session.id.0.clone(), Vec::new());
        self.event_bus
            .publish(opencode_event::Event::SessionCreated {
                session_id: session.id.0.clone(),
            });
        session
    }

    pub fn get(&self, id: &SessionId) -> Option<Arc<Session>> {
        self.sessions.read().get(&id.0).cloned()
    }

    pub fn list_all(&self) -> Vec<Arc<Session>> {
        self.sessions.read().values().cloned().collect()
    }

    /// List sessions sorted by most recently updated
    pub fn list_sessions(&self) -> Vec<Arc<Session>> {
        let mut sessions: Vec<_> = self.list_all();
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        sessions
    }

    /// Get recent sessions with optional limit
    pub fn get_recent_sessions(&self, limit: usize) -> Vec<Arc<Session>> {
        let mut sessions = self.list_sessions();
        sessions.truncate(limit);
        sessions
    }

    /// Get sessions by status
    pub fn get_sessions_by_status(&self, status: SessionStatus) -> Vec<Arc<Session>> {
        self.sessions
            .read()
            .values()
            .filter(|s| s.status == status)
            .cloned()
            .collect()
    }

    pub fn add_message(&self, session_id: &str, message: Message) {
        // Persist to SQLite if storage is configured
        if let Some(repo) = &self.message_repo {
            if let Err(e) = repo.save_message(session_id, &message) {
                tracing::error!("Failed to persist message: {}", e);
            }
        }

        self.messages
            .write()
            .get_mut(session_id)
            .map(|msgs| msgs.push(message.clone()));

        if let Some(session) = self.sessions.write().get_mut(session_id) {
            let session_mut = Arc::make_mut(session);
            session_mut.updated_at = Utc::now();

            // Persist updated session timestamp
            if let Some(repo) = &self.session_repo {
                if let Err(e) = repo.save(session) {
                    tracing::error!("Failed to persist session update: {}", e);
                }
            }
        }

        self.event_bus.publish(opencode_event::Event::MessageAdded {
            session_id: session_id.to_string(),
            message_id: message.id.0.clone(),
        });
    }

    pub fn get_messages(&self, session_id: &str) -> Vec<Message> {
        self.messages
            .read()
            .get(session_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn get_messages_paginated(
        &self,
        session_id: &str,
        pagination: &PaginationParams,
    ) -> Result<PaginatedMessages, String> {
        match &self.message_repo {
            Some(repo) => repo
                .get_messages_paginated(session_id, pagination)
                .map_err(|e| format!("Failed to load paginated messages: {}", e)),
            None => {
                // Fallback to in-memory messages
                let all_messages = self.get_messages(session_id);
                let total = all_messages.len();
                let messages = all_messages
                    .into_iter()
                    .skip(pagination.offset)
                    .take(pagination.limit)
                    .collect();
                Ok(PaginatedMessages {
                    messages,
                    total,
                    offset: pagination.offset,
                    limit: pagination.limit,
                })
            }
        }
    }

    pub fn update_status(&self, session_id: &str, status: SessionStatus) -> bool {
        if let Some(session) = self.sessions.write().get_mut(session_id) {
            let session_mut = Arc::make_mut(session);

            // Validate status transition
            if !session_mut.status.can_transition_to(status) {
                tracing::warn!(
                    "Invalid status transition from {:?} to {:?}",
                    session_mut.status,
                    status
                );
                return false;
            }

            session_mut.status = status;
            session_mut.updated_at = Utc::now();

            // Persist to SQLite if storage is configured
            if let Some(repo) = &self.session_repo {
                if let Err(e) = repo.save(session) {
                    tracing::error!("Failed to persist session status update: {}", e);
                }
            }

            self.event_bus
                .publish(opencode_event::Event::SessionUpdated {
                    session_id: session_id.to_string(),
                });
            true
        } else {
            false
        }
    }

    pub fn delete(&self, session_id: &str) -> bool {
        if self.sessions.write().remove(session_id).is_some() {
            self.messages.write().remove(session_id);

            // Delete from SQLite if storage is configured
            if let Some(repo) = &self.message_repo {
                if let Err(e) = repo.delete_messages_for_session(session_id) {
                    tracing::error!("Failed to delete messages from storage: {}", e);
                }
            }
            if let Some(repo) = &self.session_repo {
                if let Err(e) = repo.delete(&SessionId(session_id.to_string())) {
                    tracing::error!("Failed to delete session from storage: {}", e);
                }
            }

            self.event_bus
                .publish(opencode_event::Event::SessionDeleted {
                    session_id: session_id.to_string(),
                });
            true
        } else {
            false
        }
    }

    /// Load session and its messages from storage into cache
    pub fn load_from_storage(&self, session_id: &str) -> Option<Arc<Session>> {
        let session_repo = self.session_repo.as_ref()?;
        let message_repo = self.message_repo.as_ref()?;

        let session = session_repo
            .load(&SessionId(session_id.to_string()))
            .ok()??;
        let messages = message_repo.load_messages(session_id).unwrap_or_default();

        let session = Arc::new(session);
        self.sessions
            .write()
            .insert(session_id.to_string(), session.clone());
        self.messages
            .write()
            .insert(session_id.to_string(), messages);

        Some(session)
    }

    /// Load all sessions from storage into cache
    pub fn load_all_from_storage(&self) -> Vec<Arc<Session>> {
        let session_repo = match &self.session_repo {
            Some(repo) => repo,
            None => return Vec::new(),
        };
        let message_repo = match &self.message_repo {
            Some(repo) => repo,
            None => return Vec::new(),
        };

        let sessions = session_repo.list().unwrap_or_default();
        sessions
            .into_iter()
            .map(|session| {
                let session_id = session.id.0.clone();
                let messages = message_repo.load_messages(&session_id).unwrap_or_default();
                let session = Arc::new(session);
                self.sessions
                    .write()
                    .insert(session_id.clone(), session.clone());
                self.messages.write().insert(session_id, messages);
                session
            })
            .collect()
    }

    /// Check if compaction is needed and perform it if so
    /// Returns Some(CompactionResult) if compaction was performed, None otherwise
    pub async fn maybe_compact(&self, session_id: &str) -> Option<CompactionResult> {
        let summarizer = self.summarizer.as_ref()?;
        
        let messages = self.messages.read().get(session_id)?.clone();
        
        // Check if compaction is needed
        if !crate::compaction::needs_compaction_by_count(&messages, &self.compaction_config)
            && !crate::compaction::needs_compaction_by_tokens(&messages, &self.compaction_config)
        {
            return None;
        }

        tracing::info!(
            "Compacting session {}: {} messages",
            session_id,
            messages.len()
        );

        // Perform compaction based on strategy
        let result = match self.compaction_strategy {
            CompactionStrategy::SummarizeOlder { preserved_recent: _ } => {
                self.compact_summarize_older(&messages, session_id, summarizer).await
            }
            CompactionStrategy::TruncateMiddle { preserved_recent: _ } => {
                self.compact_truncate_middle(&messages)
            }
            CompactionStrategy::Hybrid { preserved_recent: _, max_total: _ } => {
                // Try summarize older first, fall back to truncate middle
                match self.compact_summarize_older(&messages, session_id, summarizer).await {
                    Ok(result) => Ok(result),
                    Err(_) => self.compact_truncate_middle(&messages),
                }
            }
            // Other strategies don't need special handling here
            _ => self.compact_truncate_middle(&messages),
        };

        match result {
            Ok(compaction_result) => {
                // Update stored messages
                let new_messages = self.build_compacted_messages(&messages, &compaction_result.summary_message);
                
                // Persist the new messages
                if let Some(repo) = &self.message_repo {
                    // Delete old messages and save new ones
                    if let Err(e) = repo.delete_messages_for_session(session_id) {
                        tracing::error!("Failed to delete old messages during compaction: {}", e);
                    }
                    for msg in &new_messages {
                        if let Err(e) = repo.save_message(session_id, msg) {
                            tracing::error!("Failed to persist compacted message: {}", e);
                        }
                    }
                }
                
                // Update in-memory messages
                *self.messages.write().get_mut(session_id)? = new_messages;

                // Publish compaction event
                self.event_bus.publish(opencode_event::Event::CompactionPerformed {
                    session_id: session_id.to_string(),
                    original_count: compaction_result.original_count,
                    new_count: compaction_result.new_count,
                    tokens_saved: compaction_result.tokens_saved,
                });

                tracing::info!(
                    "Compaction complete for session {}: {} -> {} messages, {} tokens saved",
                    session_id,
                    compaction_result.original_count,
                    compaction_result.new_count,
                    compaction_result.tokens_saved
                );

                Some(compaction_result)
            }
            Err(e) => {
                tracing::error!("Compaction failed for session {}: {}", session_id, e);
                None
            }
        }
    }

    /// Summarize older messages strategy
    async fn compact_summarize_older(
        &self,
        messages: &[Message],
        session_id: &str,
        summarizer: &Arc<Summarizer>,
    ) -> Result<CompactionResult, opencode_core::OpenCodeError> {
        let max_messages = self.compaction_config.max_messages;
        let max_tokens = self.compaction_config.max_tokens;

        summarizer
            .compact_messages(messages, max_messages, max_tokens, session_id)
            .await
            .map_err(|e| opencode_core::OpenCodeError::Session(e.to_string()))
    }

    /// Truncate middle messages strategy
    fn compact_truncate_middle(&self, messages: &[Message]) -> Result<CompactionResult, opencode_core::OpenCodeError> {
        let max_messages = self.compaction_config.max_messages;
        
        if messages.len() <= max_messages {
            return Err(opencode_core::OpenCodeError::Session(
                "Not enough messages to truncate".to_string()
            ));
        }

        // Keep first 2 (system + initial) and last (max_messages - 2) messages
        let keep_recent = max_messages.saturating_sub(2);
        let preserve_count = 2 + keep_recent;
        let original_count = messages.len();

        if original_count <= preserve_count {
            return Err(opencode_core::OpenCodeError::Session(
                "Not enough messages to truncate".to_string()
            ));
        }

        // Create truncated message list
        let mut new_messages = Vec::with_capacity(preserve_count + 1);
        new_messages.push(messages[0].clone());
        new_messages.push(messages[1].clone());
        
        // Add placeholder for truncated content
        let truncated_count = original_count - preserve_count;
        let placeholder = Message::assistant(
            messages[1].session_id.clone(),
            vec![opencode_core::Part::Text {
                content: format!(
                    "[{} messages were truncated due to length]",
                    truncated_count
                ),
            }],
        );
        new_messages.push(placeholder.clone());
        new_messages.extend_from_slice(&messages[original_count - keep_recent..]);

        let original_tokens = crate::compaction::estimate_message_tokens(messages);
        let new_tokens = crate::compaction::estimate_message_tokens(&new_messages);

        Ok(CompactionResult::new(
            original_count,
            new_messages.len(),
            placeholder,
            original_tokens.saturating_sub(new_tokens),
        ))
    }

    /// Build the new message list after compaction
    fn build_compacted_messages(&self, original: &[Message], summary: &Message) -> Vec<Message> {
        let max_messages = self.compaction_config.max_messages;
        let keep_recent = max_messages.saturating_sub(3); // Account for system, context, and summary
        
        let mut result = Vec::with_capacity(3 + keep_recent);
        
        // Keep system message
        if !original.is_empty() {
            result.push(original[0].clone());
        }
        
        // Add summary
        result.push(summary.clone());
        
        // Keep recent messages
        let recent_start = original.len().saturating_sub(keep_recent);
        for msg in original.iter().skip(recent_start) {
            result.push(msg.clone());
        }
        
        result
    }

    /// Get current compaction configuration
    pub fn get_compaction_config(&self) -> CompactionConfig {
        self.compaction_config.clone()
    }

    /// Get current compaction strategy
    pub fn get_compaction_strategy(&self) -> CompactionStrategy {
        self.compaction_strategy
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencode_core::Part;
    use opencode_storage::schema;
    use rusqlite::Connection;
    use tempfile::tempdir;

    fn create_test_service() -> (SessionService, tempfile::TempDir) {
        let dir = tempdir().unwrap();

        let event_bus = Arc::new(opencode_event::EventBus::new(100));

        // Use a single database file for both repos since messages reference sessions via FK
        let db_path = dir.path().join("test.db");
        let session_conn = Connection::open(&db_path).unwrap();
        schema::init_schema(&session_conn).unwrap();
        let session_repo = SessionRepository::new(session_conn);

        // Open a second connection to the same database file
        let message_conn = Connection::open(&db_path).unwrap();
        let message_repo = MessageRepository::new(message_conn);

        (
            SessionService::with_storage(event_bus, session_repo, message_repo),
            dir,
        )
    }

    #[test]
    fn test_create_persists_session() {
        let (service, _dir) = create_test_service();

        let session = Session::new(
            std::path::PathBuf::from("/test"),
            "agent".to_string(),
            "model".to_string(),
        );
        let session_id = session.id.0.clone();

        service.create(session);

        // Verify session is in storage
        let loaded = service
            .session_repo
            .as_ref()
            .unwrap()
            .load(&SessionId(session_id.clone()))
            .unwrap();
        assert!(loaded.is_some());
    }

    #[test]
    fn test_add_message_persists() {
        let (service, _dir) = create_test_service();

        let session = Session::new(
            std::path::PathBuf::from("/test"),
            "agent".to_string(),
            "model".to_string(),
        );
        let session_id = session.id.0.clone();
        service.create(session);

        let message = Message::user(
            session_id.clone(),
            vec![Part::Text {
                content: "Hello".to_string(),
            }],
        );
        let message_id = message.id.0.clone();
        service.add_message(&session_id, message);

        // Verify message is persisted
        let messages = service
            .message_repo
            .as_ref()
            .unwrap()
            .load_messages(&session_id)
            .unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id.0, message_id);
    }

    #[test]
    fn test_delete_removes_from_storage() {
        let (service, _dir) = create_test_service();

        let session = Session::new(
            std::path::PathBuf::from("/test"),
            "agent".to_string(),
            "model".to_string(),
        );
        let session_id = session.id.0.clone();
        service.create(session);

        let message = Message::user(
            session_id.clone(),
            vec![Part::Text {
                content: "Hello".to_string(),
            }],
        );
        service.add_message(&session_id, message);

        service.delete(&session_id);

        // Verify session is removed from storage
        let loaded = service
            .session_repo
            .as_ref()
            .unwrap()
            .load(&SessionId(session_id))
            .unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_load_from_storage() {
        let (service, _dir) = create_test_service();

        // Create and persist a session with messages
        let session = Session::new(
            std::path::PathBuf::from("/test"),
            "agent".to_string(),
            "model".to_string(),
        );
        let session_id = session.id.0.clone();
        service.create(session);

        let message = Message::user(
            session_id.clone(),
            vec![Part::Text {
                content: "Hello".to_string(),
            }],
        );
        service.add_message(&session_id, message);

        // Clear in-memory state
        service.sessions.write().clear();
        service.messages.write().clear();

        // Load from storage
        let loaded = service.load_from_storage(&session_id);
        assert!(loaded.is_some());
        assert_eq!(service.get_messages(&session_id).len(), 1);
    }

    #[test]
    fn test_paginated_messages() {
        let (service, _dir) = create_test_service();

        let session = Session::new(
            std::path::PathBuf::from("/test"),
            "agent".to_string(),
            "model".to_string(),
        );
        let session_id = session.id.0.clone();
        service.create(session);

        // Create 5 messages
        for i in 0..5 {
            let message = Message::user(
                session_id.clone(),
                vec![Part::Text {
                    content: format!("Message {}", i),
                }],
            );
            service.add_message(&session_id, message);
        }

        // Get paginated messages
        let pagination = PaginationParams::new(0, 2);
        let result = service
            .get_messages_paginated(&session_id, &pagination)
            .unwrap();

        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.total, 5);
        assert_eq!(result.offset, 0);
        assert_eq!(result.limit, 2);
    }
}
