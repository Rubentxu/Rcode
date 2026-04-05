//! Session service with L1 cache and write-through SQLite persistence

use chrono::Utc;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use rcode_core::{
    Message, PaginatedMessages, PaginationParams, Part, Role, Session, SessionId, SessionStatus,
};
use rcode_event::EventBus;
use rcode_storage::{MessageRepository, SessionRepository};

use crate::compaction::{CompactionConfig, CompactionResult, CompactionStrategy};
use crate::summarizer::Summarizer;

/// Generate a title from the first text part content
fn generate_title(text: &str) -> String {
    let trimmed = text.trim();
    let max_len = 50;
    if trimmed.chars().count() <= max_len {
        return trimmed.to_string();
    }
    // Find a clean boundary within max_len chars using char_indices
    let boundary = trimmed.char_indices()
        .take(max_len)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(max_len.min(trimmed.len()));
    format!("{}...", &trimmed[..boundary])
}

pub struct SessionService {
    sessions: RwLock<HashMap<String, Arc<Session>>>,
    messages: RwLock<HashMap<String, Vec<Message>>>,
    event_bus: Arc<EventBus>,
    session_repo: Option<Arc<SessionRepository>>,
    message_repo: Option<Arc<MessageRepository>>,
    compaction_config: CompactionConfig,
    summarizer: Option<Arc<Summarizer>>,
    compaction_strategy: CompactionStrategy,
    /// Undo stacks: session_id -> stack of removed message sets (each set is 2 messages: user + assistant)
    undo_stacks: RwLock<HashMap<String, Vec<Vec<Message>>>>,
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
            undo_stacks: RwLock::new(HashMap::new()),
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
            undo_stacks: RwLock::new(HashMap::new()),
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

    /// Get a reference to the session repository (G4)
    pub fn session_repo(&self) -> Option<&Arc<SessionRepository>> {
        self.session_repo.as_ref()
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
            .publish(rcode_event::Event::SessionCreated {
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

    /// Internal add_message that can optionally clear the redo stack
    fn add_message_internal(&self, session_id: &str, message: Message, clear_redo: bool) {
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

        // D8: Only clear redo stack when explicitly requested (for new user messages)
        // Redo-restored messages should NOT clear the redo stack
        if clear_redo {
            let mut stacks = self.undo_stacks.write();
            stacks.remove(session_id);
        }

        if let Some(session) = self.sessions.write().get_mut(session_id) {
            let session_mut = Arc::make_mut(session);
            session_mut.updated_at = Utc::now();

            // Generate title from first user message if session has no title
            if session_mut.title.is_none() && message.role == Role::User {
                if let Some(first_text) = message.parts.iter().find_map(|p| {
                    if let Part::Text { content } = p {
                        Some(content.clone())
                    } else {
                        None
                    }
                }) {
                    session_mut.title = Some(generate_title(&first_text));
                }
            }

            // Persist updated session timestamp
            if let Some(repo) = &self.session_repo {
                if let Err(e) = repo.save(session) {
                    tracing::error!("Failed to persist session update: {}", e);
                }
            }
        }

        self.event_bus.publish(rcode_event::Event::MessageAdded {
            session_id: session_id.to_string(),
            message_id: message.id.0.clone(),
        });

        // D9: Check if compaction thresholds are met and trigger async compaction
        // Note: Since SessionService is not Clone, we cannot directly spawn a task 
        // that calls maybe_compact (which is async). This is a limitation - the actual
        // compaction should be triggered by the caller via maybe_compact() after
        // adding messages, or via a background task that has access to the service.
        // Here we just log a debug message if thresholds appear to be met.
        let messages = self.messages.read().get(session_id).cloned();
        if let Some(ref msgs) = messages {
            let needs_by_count = crate::compaction::needs_compaction_by_count(msgs, &self.compaction_config);
            let needs_by_tokens = crate::compaction::needs_compaction_by_tokens(msgs, &self.compaction_config);
            
            if needs_by_count || needs_by_tokens {
                tracing::debug!("Compaction thresholds met for session {}, strategy: {:?}. Call maybe_compact() to perform compaction.",
                    session_id, self.compaction_strategy);
            }
        }
    }

    /// Add a new message to the session (clears redo stack)
    pub fn add_message(&self, session_id: &str, message: Message) {
        self.add_message_internal(session_id, message, true);
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
                .publish(rcode_event::Event::SessionUpdated {
                    session_id: session_id.to_string(),
                });
            true
        } else {
            false
        }
    }

    /// Update the model for a session
    pub fn update_model(&self, session_id: &str, model_id: String) -> Result<(), String> {
        {
            let mut sessions = self.sessions.write();
            let session = sessions.get_mut(session_id)
                .ok_or_else(|| format!("Session not found: {}", session_id))?;

            let session_mut = Arc::make_mut(session);
            session_mut.set_model(model_id.clone());
            session_mut.updated_at = Utc::now();
        } // Lock released before persist

        // Persist to SQLite if storage is configured
        if let Some(repo) = &self.session_repo {
            repo.update_model(session_id, &model_id)
                .map_err(|e| format!("Failed to persist model update: {}", e))?;
        }

        self.event_bus
            .publish(rcode_event::Event::SessionUpdated {
                session_id: session_id.to_string(),
            });
        Ok(())
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
                .publish(rcode_event::Event::SessionDeleted {
                    session_id: session_id.to_string(),
                });
            // Also clear undo stack for this session
            self.undo_stacks.write().remove(session_id);
            true
        } else {
            false
        }
    }

    /// Remove the last exchange (user message + assistant response) from the session
    ///
    /// This removes messages from the last user message boundary to the end, properly
    /// handling tool-driven conversations and odd-length histories.
    /// The removed messages are stored on an undo stack for later redo.
    pub fn undo_last_exchange(&self, session_id: &str) -> Result<(), String> {
        let removed: Vec<Message> = {
            let mut messages = self.messages.write();
            let messages_mut = messages.get_mut(session_id)
                .ok_or_else(|| format!("Session not found: {}", session_id))?;
            
            if messages_mut.len() < 2 {
                return Err("Not enough messages to undo".to_string());
            }
            
            // Find the last user message boundary (scan backwards for Role::User)
            let last_user_idx = messages_mut.iter().rposition(|m| matches!(m.role, Role::User))
                .ok_or("No user message found to undo".to_string())?;
            
            // D7: Validate exchange structure after the last User message
            let messages_after = &messages_mut[last_user_idx..];
            
            // Determine what to undo based on exchange structure:
            // - If only a User with no response yet, undo just that User message
            // - If there's an exchange (User + Assistant/tool results), undo the whole thing
            let undo_end_idx = if messages_after.len() == 1 {
                // Only User message, no response yet - undo just this message
                messages_mut.len()
            } else {
                // Has at least User + something else - check if there's an Assistant response
                // Look for the next User message to find the boundary of this exchange
                let exchange_end = messages_after[1..].iter()
                    .position(|m| matches!(m.role, Role::User))
                    .map(|pos| pos + 1) // Include the User that starts next exchange
                    .unwrap_or(messages_after.len()); // Or go to end of this exchange if no next User
                last_user_idx + exchange_end
            };
            
            // Remove from last_user_idx to undo_end_idx
            messages_mut.drain(last_user_idx..undo_end_idx).collect()
        }; // Lock released

        // Persist deletions
        if let Some(msg_repo) = &self.message_repo {
            for msg in &removed {
                if let Err(e) = msg_repo.delete_message(&msg.id.0) {
                    tracing::error!("Failed to delete undone message from storage: {}", e);
                }
            }
        }
        
        // Push to undo stack
        {
            let mut stacks = self.undo_stacks.write();
            stacks.entry(session_id.to_string())
                .or_insert_with(Vec::new)
                .push(removed.clone());
        }

        // Update session timestamp
        if let Some(session) = self.sessions.write().get_mut(session_id) {
            let session_mut = Arc::make_mut(session);
            session_mut.updated_at = Utc::now();

            if let Some(repo) = &self.session_repo {
                if let Err(e) = repo.save(session) {
                    tracing::error!("Failed to persist session after undo: {}", e);
                }
            }
        }

        self.event_bus.publish(rcode_event::Event::MessageAdded {
            session_id: session_id.to_string(),
            message_id: "undo".to_string(),
        });

        Ok(())
    }

    /// Re-add messages that were undone (redo) — pops from the undo stack
    /// D8: Uses add_message_internal to NOT clear the redo stack when restoring
    pub fn redo_last_exchange(&self, session_id: &str) -> Result<(), String> {
        let messages_to_redo = {
            let mut undo_stacks = self.undo_stacks.write();
            undo_stacks
                .get_mut(session_id)
                .and_then(|stack| stack.pop())
                .ok_or_else(|| "Nothing to redo".to_string())?
                .clone()
        }; // Lock released here

        // Re-add the messages using add_message_internal with clear_redo=false
        // This ensures the redo stack is NOT cleared when restoring messages
        for msg in &messages_to_redo {
            self.add_message_internal(session_id, msg.clone(), false);
        }

        // Update session timestamp
        if let Some(session) = self.sessions.write().get_mut(session_id) {
            let session_mut = Arc::make_mut(session);
            session_mut.updated_at = Utc::now();

            if let Some(repo) = &self.session_repo {
                if let Err(e) = repo.save(session) {
                    tracing::error!("Failed to persist session after redo: {}", e);
                }
            }
        }

        self.event_bus.publish(rcode_event::Event::MessageAdded {
            session_id: session_id.to_string(),
            message_id: "redo".to_string(),
        });

        Ok(())
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
                self.event_bus.publish(rcode_event::Event::CompactionPerformed {
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
    ) -> Result<CompactionResult, rcode_core::RCodeError> {
        let max_messages = self.compaction_config.max_messages;
        let max_tokens = self.compaction_config.max_tokens;

        summarizer
            .compact_messages(messages, max_messages, max_tokens, session_id)
            .await
            .map_err(|e| rcode_core::RCodeError::Session(e.to_string()))
    }

    /// Truncate middle messages strategy
    fn compact_truncate_middle(&self, messages: &[Message]) -> Result<CompactionResult, rcode_core::RCodeError> {
        let max_messages = self.compaction_config.max_messages;
        
        if messages.len() <= max_messages {
            return Err(rcode_core::RCodeError::Session(
                "Not enough messages to truncate".to_string()
            ));
        }

        // Keep first 2 (system + initial) and last (max_messages - 2) messages
        let keep_recent = max_messages.saturating_sub(2);
        let preserve_count = 2 + keep_recent;
        let original_count = messages.len();

        if original_count <= preserve_count {
            return Err(rcode_core::RCodeError::Session(
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
            vec![rcode_core::Part::Text {
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

    /// Create a child session inheriting project_path from parent
    pub fn create_child(&self, parent_id: &str, agent_id: String, model_id: String) -> Result<Arc<Session>, String> {
        // Get parent session to inherit project_path
        let parent = self.get(&SessionId(parent_id.to_string()))
            .ok_or_else(|| format!("Parent session not found: {}", parent_id))?;
        
        let mut session = Session::new(parent.project_path.clone(), agent_id, model_id);
        session = session.with_parent(parent_id.to_string());
        Ok(self.create(session))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::{Part, Role};
    use rcode_storage::schema;
    use rusqlite::Connection;
    use tempfile::tempdir;
    use crate::create_default_session_service;

    fn create_test_service() -> (SessionService, tempfile::TempDir) {
        let dir = tempdir().unwrap();

        let event_bus = Arc::new(rcode_event::EventBus::new(100));

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

    #[tokio::test]
    async fn test_create_session_publishes_event() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let service = SessionService::new(event_bus.clone());

        let mut sub = event_bus.subscribe();

        let session = Session::new(
            std::path::PathBuf::from("/test"),
            "agent".to_string(),
            "model".to_string(),
        );
        let session_id = session.id.0.clone();
        service.create(session);

        let event = sub.recv().await.unwrap();
        assert_eq!(event.event_type(), "session_created");
        assert_eq!(event.session_id(), Some(session_id.as_str()));
    }

    #[tokio::test]
    async fn test_add_message_publishes_event() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let service = SessionService::new(event_bus.clone());

        let session = Session::new(
            std::path::PathBuf::from("/test"),
            "agent".to_string(),
            "model".to_string(),
        );
        let session_id = session.id.0.clone();
        service.create(session);

        let mut sub = event_bus.subscribe();

        let msg = Message::user(session_id.clone(), vec![Part::Text { content: "hi".to_string() }]);
        service.add_message(&session_id, msg);

        let event = sub.recv().await.unwrap();
        assert_eq!(event.event_type(), "message_added");
    }

    #[tokio::test]
    async fn test_delete_session_publishes_event() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let service = SessionService::new(event_bus.clone());

        let session = Session::new(
            std::path::PathBuf::from("/test"),
            "agent".to_string(),
            "model".to_string(),
        );
        let session_id = session.id.0.clone();
        service.create(session);

        let msg = Message::user(session_id.clone(), vec![Part::Text { content: "hi".to_string() }]);
        service.add_message(&session_id, msg);

        let mut sub = event_bus.subscribe();

        service.delete(&session_id);

        let event = sub.recv().await.unwrap();
        assert_eq!(event.event_type(), "session_deleted");
    }

    #[test]
    fn test_delete_nonexistent_session() {
        let (service, _dir) = create_test_service();
        assert!(!service.delete("nonexistent"));
    }

    #[test]
    fn test_get_nonexistent_session() {
        let (service, _dir) = create_test_service();
        assert!(service.get(&SessionId("nonexistent".into())).is_none());
    }

    #[test]
    fn test_get_messages_empty_session() {
        let (service, _dir) = create_test_service();
        let msgs = service.get_messages("nonexistent");
        assert!(msgs.is_empty());
    }

    #[test]
    fn test_multiple_sessions_isolated() {
        let (service, _dir) = create_test_service();

        let s1 = Session::new(std::path::PathBuf::from("/test1"), "a1".into(), "m1".into());
        let s2 = Session::new(std::path::PathBuf::from("/test2"), "a2".into(), "m2".into());
        let id1 = s1.id.0.clone();
        let id2 = s2.id.0.clone();

        service.create(s1);
        service.create(s2);

        service.add_message(&id1, Message::user(id1.clone(), vec![Part::Text { content: "msg1".to_string() }]));
        service.add_message(&id2, Message::user(id2.clone(), vec![Part::Text { content: "msg2".to_string() }]));

        assert_eq!(service.get_messages(&id1).len(), 1);
        assert_eq!(service.get_messages(&id2).len(), 1);
    }

    #[test]
    fn test_list_all_sessions() {
        let (service, _dir) = create_test_service();
        let s1 = Session::new(std::path::PathBuf::from("/t1"), "a".into(), "m".into());
        let s2 = Session::new(std::path::PathBuf::from("/t2"), "a".into(), "m".into());
        service.create(s1);
        service.create(s2);
        assert_eq!(service.list_all().len(), 2);
    }

    #[test]
    fn test_list_sessions_sorted() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let service = SessionService::new(event_bus);
        let s1 = Session::new(std::path::PathBuf::from("/t1"), "a".into(), "m".into());
        let id1 = s1.id.0.clone();
        service.create(s1);
        // Small sleep to ensure different updated_at
        std::thread::sleep(std::time::Duration::from_millis(10));
        let s2 = Session::new(std::path::PathBuf::from("/t2"), "a".into(), "m".into());
        let s2_id = s2.id.0.clone();
        service.create(s2);
        let sorted = service.list_sessions();
        assert_eq!(sorted.len(), 2);
        // s2 should be first (more recent)
        assert_eq!(sorted[0].id.0, s2_id);
    }

    #[test]
    fn test_get_recent_sessions_with_limit() {
        let (service, _dir) = create_test_service();
        for i in 0..5 {
            let s = Session::new(std::path::PathBuf::from(&format!("/t{}", i)), "a".into(), "m".into());
            service.create(s);
        }
        let recent = service.get_recent_sessions(3);
        assert_eq!(recent.len(), 3);
    }

    #[test]
    fn test_get_sessions_by_status() {
        let (service, _dir) = create_test_service();
        let s1 = Session::new(std::path::PathBuf::from("/t1"), "a".into(), "m".into());
        let id1 = s1.id.0.clone();
        service.create(s1);
        let s2 = Session::new(std::path::PathBuf::from("/t2"), "a".into(), "m".into());
        let id2 = s2.id.0.clone();
        service.create(s2);
        service.update_status(&id2, SessionStatus::Running);
        let running = service.get_sessions_by_status(SessionStatus::Running);
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].id.0, id2);
        let idle = service.get_sessions_by_status(SessionStatus::Idle);
        assert_eq!(idle.len(), 1);
        assert_eq!(idle[0].id.0, id1);
    }

    #[test]
    fn test_update_status_valid_transition() {
        let (service, _dir) = create_test_service();
        let s = Session::new(std::path::PathBuf::from("/t1"), "a".into(), "m".into());
        let id = s.id.0.clone();
        service.create(s);
        assert!(service.update_status(&id, SessionStatus::Running));
        let session = service.get(&SessionId(id)).unwrap();
        assert_eq!(session.status, SessionStatus::Running);
    }

    #[test]
    fn test_update_status_nonexistent() {
        let (service, _dir) = create_test_service();
        assert!(!service.update_status("nope", SessionStatus::Running));
    }

    #[test]
    fn test_update_status_invalid_transition() {
        let (service, _dir) = create_test_service();
        let s = Session::new(std::path::PathBuf::from("/t1"), "a".into(), "m".into());
        let id = s.id.0.clone();
        service.create(s);
        // Idle -> Complete should be invalid
        assert!(!service.update_status(&id, SessionStatus::Completed));
    }

    #[test]
    fn test_add_message_updates_timestamp() {
        let (service, _dir) = create_test_service();
        let s = Session::new(std::path::PathBuf::from("/t1"), "a".into(), "m".into());
        let id = s.id.0.clone();
        let original_updated = s.updated_at;
        service.create(s);
        std::thread::sleep(std::time::Duration::from_millis(10));
        service.add_message(&id, Message::user(id.clone(), vec![Part::Text { content: "hi".into() }]));
        let session = service.get(&SessionId(id)).unwrap();
        assert!(session.updated_at > original_updated);
    }

    #[test]
    fn test_add_message_nonexistent_session() {
        let (service, _dir) = create_test_service();
        service.add_message("nope", Message::user("nope".into(), vec![Part::Text { content: "x".into() }]));
        // Should not panic, message just gets dropped
    }

    #[test]
    fn test_paginated_messages_second_page() {
        let (service, _dir) = create_test_service();
        let s = Session::new(std::path::PathBuf::from("/t"), "a".into(), "m".into());
        let id = s.id.0.clone();
        service.create(s);
        for i in 0..5 {
            service.add_message(&id, Message::user(id.clone(), vec![Part::Text { content: format!("M{}", i) }]));
        }
        let page2 = service.get_messages_paginated(&id, &PaginationParams::new(2, 2)).unwrap();
        assert_eq!(page2.messages.len(), 2);
        assert_eq!(page2.offset, 2);
    }

    #[test]
    fn test_compact_truncate_middle() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let mut service = SessionService::new(event_bus);
        service.set_compaction_config(CompactionConfig::new(5, 1000, 0.9, 3));
        let s = Session::new(std::path::PathBuf::from("/t"), "a".into(), "m".into());
        let id = s.id.0.clone();
        service.create(s);
        for i in 0..10 {
            service.add_message(&id, Message::user(id.clone(), vec![Part::Text { content: format!("M{}", i) }]));
        }
        let msgs = service.get_messages(&id);
        assert!(msgs.len() >= 10);
        let result = service.compact_truncate_middle(&msgs).unwrap();
        assert!(result.original_count > result.new_count);
        assert!(result.tokens_saved >= 0);
    }

    #[test]
    fn test_compact_truncate_under_limit() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let service = SessionService::new(event_bus);
        let msgs = vec![
            Message::user("s1".into(), vec![Part::Text { content: "short".into() }]),
        ];
        let result = service.compact_truncate_middle(&msgs);
        assert!(result.is_err());
    }

    #[test]
    fn test_build_compacted_messages() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let mut service = SessionService::new(event_bus);
        service.set_compaction_config(CompactionConfig::new(5, 1000, 0.9, 3));
        let msgs: Vec<Message> = (0..10).map(|i| Message::user("s1".into(), vec![Part::Text { content: format!("M{}", i) }])).collect();
        assert!(msgs.len() >= 10);
        let summary = Message::assistant("s1".into(), vec![Part::Text { content: "Summary".into() }]);
        let result = service.build_compacted_messages(&msgs, &summary);
        assert!(result.len() < msgs.len());
        assert!(matches!(&result[1].parts[0], Part::Text { content } if content == "Summary"));
    }

    #[test]
    fn test_build_compacted_messages_empty() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let service = SessionService::new(event_bus);
        let summary = Message::assistant("s1".into(), vec![Part::Text { content: "Summary".into() }]);
        let result = service.build_compacted_messages(&[], &summary);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_get_compaction_config() {
        let service = SessionService::new(Arc::new(rcode_event::EventBus::new(10)));
        let config = service.get_compaction_config();
        assert_eq!(config.max_messages, 50);
    }

    #[test]
    fn test_get_compaction_strategy() {
        let service = SessionService::new(Arc::new(rcode_event::EventBus::new(10)));
        let strategy = service.get_compaction_strategy();
        assert!(matches!(strategy, CompactionStrategy::Hybrid { .. }));
    }

    #[test]
    fn test_set_compaction_config() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let mut service = SessionService::new(event_bus);
        let config = CompactionConfig::new(10, 1000, 0.9, 5);
        service.set_compaction_config(config.clone());
        assert_eq!(service.get_compaction_config().max_messages, 10);
    }

    #[test]
    fn test_set_compaction_strategy() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let mut service = SessionService::new(event_bus);
        service.set_compaction_strategy(CompactionStrategy::PreserveRecent { count: 5 });
        assert_eq!(service.get_compaction_strategy(), CompactionStrategy::PreserveRecent { count: 5 });
    }

    #[test]
    fn test_load_all_from_storage() {
        let (service, _dir) = create_test_service();
        let s1 = Session::new(std::path::PathBuf::from("/t1"), "a".into(), "m".into());
        let id1 = s1.id.0.clone();
        service.create(s1);
        service.add_message(&id1, Message::user(id1.clone(), vec![Part::Text { content: "msg1".into() }]));
        let s2 = Session::new(std::path::PathBuf::from("/t2"), "a".into(), "m".into());
        let id2 = s2.id.0.clone();
        service.create(s2);
        service.add_message(&id2, Message::user(id2.clone(), vec![Part::Text { content: "msg2".into() }]));

        // Clear memory
        service.sessions.write().clear();
        service.messages.write().clear();

        let loaded = service.load_all_from_storage();
        assert_eq!(loaded.len(), 2);
        assert_eq!(service.get_messages(&id1).len(), 1);
        assert_eq!(service.get_messages(&id2).len(), 1);
    }

    #[test]
    fn test_load_all_from_storage_no_storage() {
        let service = SessionService::new(Arc::new(rcode_event::EventBus::new(10)));
        let loaded = service.load_all_from_storage();
        assert!(loaded.is_empty());
    }

    #[test]
    fn test_load_from_storage_no_storage() {
        let service = SessionService::new(Arc::new(rcode_event::EventBus::new(10)));
        assert!(service.load_from_storage("any").is_none());
    }

    #[tokio::test]
    async fn test_maybe_compact_no_summarizer() {
        let service = SessionService::new(Arc::new(rcode_event::EventBus::new(10)));
        let s = Session::new(std::path::PathBuf::from("/t"), "a".into(), "m".into());
        let id = s.id.0.clone();
        service.create(s);
        // No summarizer configured → should return None
        let result = service.maybe_compact(&id).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_maybe_compact_not_needed() {
        let summarizer = Arc::new(Summarizer::new(
            Arc::new(TestProvider), "m".into()
        ));
        let service = SessionService::new(Arc::new(rcode_event::EventBus::new(10)))
            .with_summarizer(summarizer, CompactionConfig::default(), CompactionStrategy::default());
        let s = Session::new(std::path::PathBuf::from("/t"), "a".into(), "m".into());
        let id = s.id.0.clone();
        service.create(s);
        service.add_message(&id, Message::user(id.clone(), vec![Part::Text { content: "short".into() }]));
        let result = service.maybe_compact(&id).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_maybe_compact_needs_compaction_truncate() {
        let summarizer = Arc::new(Summarizer::new(
            Arc::new(TestProvider), "m".into()
        ));
        let config = CompactionConfig::new(5, 100, 0.8, 3);
        let service = SessionService::new(Arc::new(rcode_event::EventBus::new(10)))
            .with_summarizer(summarizer, config, CompactionStrategy::TruncateMiddle { preserved_recent: 3 });
        let s = Session::new(std::path::PathBuf::from("/t"), "a".into(), "m".into());
        let id = s.id.0.clone();
        service.create(s);
        for i in 0..10 {
            service.add_message(&id, Message::user(id.clone(), vec![Part::Text { content: format!("M{}", i) }]));
        }
        let result = service.maybe_compact(&id).await;
        assert!(result.is_some());
        let compacted = result.unwrap();
        assert!(compacted.original_count > compacted.new_count);
    }

    #[test]
    fn test_compact_truncate_middle_preserves_recent_messages() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let mut service = SessionService::new(event_bus);
        service.set_compaction_config(CompactionConfig::new(5, 1000, 0.9, 3));
        
        // Create 10 messages
        let msgs: Vec<Message> = (0..10).map(|i| Message::user("s1".into(), vec![Part::Text { content: format!("Message {}", i) }])).collect();
        
        let result = service.compact_truncate_middle(&msgs).unwrap();
        
        // Original has 10, new should be less
        assert!(result.original_count > result.new_count);
        assert!(result.new_count <= 6); // preserve_count + 1 (placeholder)
    }

    #[test]
    fn test_compact_truncate_middle_edge_case_exact_preserve() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let mut service = SessionService::new(event_bus);
        service.set_compaction_config(CompactionConfig::new(5, 1000, 0.9, 3));
        
        // 5 messages with max_messages=5 - exactly at preserve limit
        let msgs: Vec<Message> = (0..5).map(|i| Message::user("s1".into(), vec![Part::Text { content: format!("M{}", i) }])).collect();
        
        let result = service.compact_truncate_middle(&msgs);
        assert!(result.is_err()); // Should error because not enough to truncate
    }

    #[test]
    fn test_compact_truncate_middle_with_one_message() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let service = SessionService::new(event_bus);
        
        let msgs = vec![Message::user("s1".into(), vec![Part::Text { content: "only one".into() }])];
        let result = service.compact_truncate_middle(&msgs);
        assert!(result.is_err());
    }

    #[test]
    fn test_build_compacted_messages_preserves_system_message() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let mut service = SessionService::new(event_bus);
        service.set_compaction_config(CompactionConfig::new(5, 1000, 0.9, 3));
        
        // Create 10 messages, first is system - construct manually since Message::system doesn't exist
        let system_msg = Message {
            id: rcode_core::MessageId::new(),
            session_id: "s1".into(),
            role: Role::System,
            parts: vec![Part::Text { content: "system".into() }],
            created_at: chrono::Utc::now(),
        };
        let mut msgs: Vec<Message> = vec![system_msg];
        msgs.extend((1..10).map(|i| Message::user("s1".into(), vec![Part::Text { content: format!("M{}", i) }])));
        
        let summary = Message::assistant("s1".into(), vec![Part::Text { content: "Summary".into() }]);
        let result = service.build_compacted_messages(&msgs, &summary);
        
        // System message should be first
        assert!(matches!(&result[0].parts[0], Part::Text { content } if content == "system"));
    }

    #[test]
    fn test_create_without_storage() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let service = SessionService::new(event_bus);
        
        let session = Session::new(
            std::path::PathBuf::from("/test"),
            "agent".to_string(),
            "model".to_string(),
        );
        let session_id = session.id.0.clone();
        
        let created = service.create(session);
        assert_eq!(created.id.0, session_id);
        
        // Should still be retrievable from memory even without storage
        assert!(service.get(&SessionId(session_id)).is_some());
    }

    #[test]
    fn test_add_message_updates_timestamp_without_storage() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let service = SessionService::new(event_bus);
        
        let session = Session::new(
            std::path::PathBuf::from("/test"),
            "agent".to_string(),
            "model".to_string(),
        );
        let id = session.id.0.clone();
        service.create(session);
        
        let original_session = service.get(&SessionId(id.clone())).unwrap();
        let original_time = original_session.updated_at;
        
        std::thread::sleep(std::time::Duration::from_millis(10));
        service.add_message(&id, Message::user(id.clone(), vec![Part::Text { content: "msg".into() }]));
        
        let updated_session = service.get(&SessionId(id)).unwrap();
        assert!(updated_session.updated_at > original_time);
    }

    #[test]
    fn test_update_status_invalid_transition_does_not_persist() {
        let (service, _dir) = create_test_service();
        let s = Session::new(std::path::PathBuf::from("/t1"), "a".into(), "m".into());
        let id = s.id.0.clone();
        service.create(s);
        
        // Idle -> Completed is invalid
        assert!(!service.update_status(&id, SessionStatus::Completed));
        
        // Status should still be Idle
        let session = service.get(&SessionId(id)).unwrap();
        assert_eq!(session.status, SessionStatus::Idle);
    }

    #[tokio::test]
    async fn test_set_summarizer() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let mut service = SessionService::new(event_bus);
        
        let summarizer = Arc::new(Summarizer::new(
            Arc::new(TestProvider), "m".into()
        ));
        service.set_summarizer(summarizer);
        
        // Should now be able to compact
        let s = Session::new(std::path::PathBuf::from("/t"), "a".into(), "m".into());
        let id = s.id.0.clone();
        service.create(s);
        
        // maybe_compact should return None because no messages need compaction
        let result = service.maybe_compact(&id).await;
        assert!(result.is_none());
    }

    #[test]
    fn test_compaction_config_clone() {
        let config = CompactionConfig::new(10, 2000, 0.8, 5);
        let cloned = config.clone();
        assert_eq!(cloned.max_messages, 10);
        assert_eq!(cloned.max_tokens, 2000);
    }

    #[test]
    fn test_compaction_strategy_clone() {
        let strategy = CompactionStrategy::TruncateMiddle { preserved_recent: 5 };
        let cloned = strategy.clone();
        assert!(matches!(cloned, CompactionStrategy::TruncateMiddle { preserved_recent: 5 }));
    }

    #[tokio::test]
    async fn test_maybe_compact_no_session() {
        let summarizer = Arc::new(Summarizer::new(
            Arc::new(TestProvider), "m".into()
        ));
        let config = CompactionConfig::new(5, 100, 0.8, 3);
        let service = SessionService::new(Arc::new(rcode_event::EventBus::new(10)))
            .with_summarizer(summarizer, config, CompactionStrategy::TruncateMiddle { preserved_recent: 3 });
        
        // Session doesn't exist
        let result = service.maybe_compact("nonexistent").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_messages_paginated_fallback_to_memory() {
        // Test the in-memory fallback when no storage is configured
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let service = SessionService::new(event_bus);
        
        let session = Session::new(std::path::PathBuf::from("/test"), "a".into(), "m".into());
        let id = session.id.0.clone();
        service.create(session);
        
        // Add some messages
        for i in 0..5 {
            service.add_message(&id, Message::user(id.clone(), vec![Part::Text { content: format!("M{}", i) }]));
        }
        
        // Get paginated - without storage, should use in-memory fallback
        let pagination = PaginationParams::new(2, 2);
        let result = service.get_messages_paginated(&id, &pagination).unwrap();
        
        assert_eq!(result.total, 5);
        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.offset, 2);
    }

    #[tokio::test]
    async fn test_maybe_compact_hybrid_strategy_summarize_fallback() {
        // Test Hybrid strategy that falls back to truncate_middle on summarizer error
        let summarizer = Arc::new(Summarizer::new(
            Arc::new(TestProvider), "m".into()
        ));
        let config = CompactionConfig::new(5, 100, 0.8, 3);
        let service = SessionService::new(Arc::new(rcode_event::EventBus::new(10)))
            .with_summarizer(summarizer, config, CompactionStrategy::Hybrid { preserved_recent: 3, max_total: 10 });
        
        let s = Session::new(std::path::PathBuf::from("/t"), "a".into(), "m".into());
        let id = s.id.0.clone();
        service.create(s);
        
        // Add enough messages to trigger compaction
        for i in 0..15 {
            service.add_message(&id, Message::user(id.clone(), vec![Part::Text { content: format!("M{}", i) }]));
        }
        
        // Hybrid strategy: with summarizer that always succeeds, should summarize
        let result = service.maybe_compact(&id).await;
        assert!(result.is_some());
    }

    #[test]
    fn test_get_messages_paginated_offset_beyond_total() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let service = SessionService::new(event_bus);
        
        let session = Session::new(std::path::PathBuf::from("/test"), "a".into(), "m".into());
        let id = session.id.0.clone();
        service.create(session);
        
        service.add_message(&id, Message::user(id.clone(), vec![Part::Text { content: "M1".into() }]));
        
        // offset beyond total
        let pagination = PaginationParams::new(100, 10);
        let result = service.get_messages_paginated(&id, &pagination).unwrap();
        
        assert_eq!(result.messages.len(), 0);
        assert_eq!(result.total, 1);
    }

    #[test]
    fn test_update_status_does_not_persist_on_invalid_transition() {
        let (service, _dir) = create_test_service();
        let s = Session::new(std::path::PathBuf::from("/t1"), "a".into(), "m".into());
        let id = s.id.0.clone();
        service.create(s);
        
        // Try invalid transition
        let result = service.update_status(&id, SessionStatus::Completed);
        assert!(!result);
        
        // Verify status is still Idle
        let session = service.get(&SessionId(id)).unwrap();
        assert_eq!(session.status, SessionStatus::Idle);
    }

    #[test]
    fn test_create_publishes_session_created_event() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let service = SessionService::new(event_bus.clone());
        
        let session = Session::new(std::path::PathBuf::from("/test"), "a".into(), "m".into());
        service.create(session);
        
        // Event is published (we can't easily verify without subscriber, but we test it doesn't panic)
    }

    #[test]
    fn test_delete_nonexistent_does_not_panic() {
        let (service, _dir) = create_test_service();
        // Should return false, not panic
        let result = service.delete("nonexistent-session");
        assert!(!result);
    }

    // ============ Undo/Redo Tests ============

    #[test]
    fn test_undo_last_exchange_removes_two_messages() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let service = SessionService::new(event_bus);
        
        let session = Session::new(std::path::PathBuf::from("/test"), "a".into(), "m".into());
        let id = session.id.0.clone();
        service.create(session);
        
        // Add user message
        service.add_message(&id, Message::user(id.clone(), vec![Part::Text { content: "Hello".into() }]));
        // Add assistant response
        service.add_message(&id, Message::assistant(id.clone(), vec![Part::Text { content: "Hi there".into() }]));
        
        assert_eq!(service.get_messages(&id).len(), 2);
        
        // Undo should remove both messages
        let result = service.undo_last_exchange(&id);
        assert!(result.is_ok());
        assert_eq!(service.get_messages(&id).len(), 0);
    }

    #[test]
    fn test_undo_last_exchange_not_enough_messages() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let service = SessionService::new(event_bus);
        
        let session = Session::new(std::path::PathBuf::from("/test"), "a".into(), "m".into());
        let id = session.id.0.clone();
        service.create(session);
        
        // Add only one message
        service.add_message(&id, Message::user(id.clone(), vec![Part::Text { content: "Hello".into() }]));
        
        // Undo should fail
        let result = service.undo_last_exchange(&id);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Not enough messages to undo");
    }

    #[test]
    fn test_undo_last_exchange_nonexistent_session() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let service = SessionService::new(event_bus);
        
        let result = service.undo_last_exchange("nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Session not found"));
    }

    #[test]
    fn test_redo_last_exchange_restores_messages() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let service = SessionService::new(event_bus);
        
        let session = Session::new(std::path::PathBuf::from("/test"), "a".into(), "m".into());
        let id = session.id.0.clone();
        service.create(session);
        
        // Add user message and assistant response
        service.add_message(&id, Message::user(id.clone(), vec![Part::Text { content: "Hello".into() }]));
        service.add_message(&id, Message::assistant(id.clone(), vec![Part::Text { content: "Hi there".into() }]));
        
        assert_eq!(service.get_messages(&id).len(), 2);
        
        // Undo
        let result = service.undo_last_exchange(&id);
        assert!(result.is_ok());
        assert_eq!(service.get_messages(&id).len(), 0);
        
        // Redo
        let result = service.redo_last_exchange(&id);
        assert!(result.is_ok());
        assert_eq!(service.get_messages(&id).len(), 2);
        
        // Verify message content
        let messages = service.get_messages(&id);
        assert!(matches!(messages[0].role, Role::User));
        assert!(matches!(messages[1].role, Role::Assistant));
    }

    #[test]
    fn test_redo_without_undo_returns_error() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let service = SessionService::new(event_bus);
        
        let session = Session::new(std::path::PathBuf::from("/test"), "a".into(), "m".into());
        let id = session.id.0.clone();
        service.create(session);
        
        // Add messages but don't undo
        service.add_message(&id, Message::user(id.clone(), vec![Part::Text { content: "Hello".into() }]));
        
        // Redo should fail
        let result = service.redo_last_exchange(&id);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Nothing to redo");
    }

    #[test]
    fn test_multiple_undo_and_redo() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let service = SessionService::new(event_bus);
        
        let session = Session::new(std::path::PathBuf::from("/test"), "a".into(), "m".into());
        let id = session.id.0.clone();
        service.create(session);
        
        // Add first exchange
        service.add_message(&id, Message::user(id.clone(), vec![Part::Text { content: "First".into() }]));
        service.add_message(&id, Message::assistant(id.clone(), vec![Part::Text { content: "Response 1".into() }]));
        
        // Add second exchange
        service.add_message(&id, Message::user(id.clone(), vec![Part::Text { content: "Second".into() }]));
        service.add_message(&id, Message::assistant(id.clone(), vec![Part::Text { content: "Response 2".into() }]));
        
        assert_eq!(service.get_messages(&id).len(), 4);
        
        // Undo second exchange
        service.undo_last_exchange(&id).unwrap();
        assert_eq!(service.get_messages(&id).len(), 2);
        
        // Undo first exchange
        service.undo_last_exchange(&id).unwrap();
        assert_eq!(service.get_messages(&id).len(), 0);
        
        // Redo first exchange
        service.redo_last_exchange(&id).unwrap();
        assert_eq!(service.get_messages(&id).len(), 2);
        
        // Redo second exchange
        service.redo_last_exchange(&id).unwrap();
        assert_eq!(service.get_messages(&id).len(), 4);
    }

    #[test]
    fn test_generate_title_short_text() {
        let short_text = "Hello world";
        let title = generate_title(short_text);
        assert_eq!(title, "Hello world");
    }

    #[test]
    fn test_generate_title_exactly_50_chars() {
        let text = "This is exactly fifty characters long text!!"; // 43 chars
        let title = generate_title(text);
        assert_eq!(title, text);
    }

    #[test]
    fn test_generate_title_truncates_unicode() {
        // Unicode characters should be handled properly - use a text over 50 chars
        let text = "日本語の文字列也是很长的需要被截断处理的文本内容，确保超过五十个字符限制才能测试截断功能是否正常工作并验证结果";
        let title = generate_title(text);
        assert!(title.ends_with("..."), "Title should end with ... but got: {}", title);
        // Should not panic and should be valid UTF-8
        assert!(!title.is_empty());
        // Verify it's valid UTF-8
        assert!(std::str::from_utf8(title.as_bytes()).is_ok());
    }

    #[test]
    fn test_generate_title_handles_empty_string() {
        let title = generate_title("");
        assert_eq!(title, "");
    }

    #[test]
    fn test_generate_title_trims_whitespace() {
        let title = generate_title("   Hello world   ");
        assert_eq!(title, "Hello world");
    }

    #[test]
    fn test_generate_title_emoji() {
        let text = "Hello 👋🎉 This is a very long message that needs truncation";
        let title = generate_title(text);
        assert!(title.ends_with("...") || title.len() <= 53); // 50 + "..."
    }

    #[test]
    fn test_undo_last_exchange_with_odd_message_count() {
        let (service, _dir) = create_test_service();
        let session = Session::new(std::path::PathBuf::from("/test"), "agent".into(), "model".into());
        let id = session.id.0.clone();
        service.create(session);
        
        // Add 3 messages (odd number): User, Assistant, User
        service.add_message(&id, Message::user(id.clone(), vec![Part::Text { content: "1".into() }]));
        service.add_message(&id, Message::assistant(id.clone(), vec![Part::Text { content: "2".into() }]));
        service.add_message(&id, Message::user(id.clone(), vec![Part::Text { content: "3".into() }]));
        
        assert_eq!(service.get_messages(&id).len(), 3);
        
        // Undo should remove from last user message (msg 3) to end
        // This leaves the first exchange (User + Assistant)
        service.undo_last_exchange(&id).unwrap();
        assert_eq!(service.get_messages(&id).len(), 2); // Messages 1 and 2 remain
    }

    #[test]
    fn test_undo_last_exchange_with_tool_conversation() {
        let (service, _dir) = create_test_service();
        let session = Session::new(std::path::PathBuf::from("/test"), "agent".into(), "model".into());
        let id = session.id.0.clone();
        service.create(session);
        
        // User message, assistant tool call, tool result
        service.add_message(&id, Message::user(id.clone(), vec![Part::Text { content: "Read file".into() }]));
        service.add_message(&id, Message::assistant(id.clone(), vec![
            Part::ToolCall {
                id: "tool1".into(),
                name: "read_file".into(),
                arguments: Box::new(serde_json::json!({"path": "/test.txt"})),
            },
        ]));
        service.add_message(&id, Message::user(id.clone(), vec![
            Part::ToolResult {
                tool_call_id: "tool1".into(),
                content: "file content".into(),
                is_error: false,
            },
        ]));
        service.add_message(&id, Message::assistant(id.clone(), vec![Part::Text { content: "The file contains...".into() }]));
        
        assert_eq!(service.get_messages(&id).len(), 4);
        
        // Undo should remove the last exchange (user tool result + assistant response)
        service.undo_last_exchange(&id).unwrap();
        let messages = service.get_messages(&id);
        assert_eq!(messages.len(), 2);
        
        // Verify the messages are user msg and assistant tool call
        assert!(matches!(messages[0].role, Role::User));
        assert!(matches!(messages[1].role, Role::Assistant));
    }

    #[test]
    fn test_add_message_clears_redo_stack() {
        let (service, _dir) = create_test_service();
        let session = Session::new(std::path::PathBuf::from("/test"), "agent".into(), "model".into());
        let id = session.id.0.clone();
        service.create(session);
        
        // Add messages and undo
        service.add_message(&id, Message::user(id.clone(), vec![Part::Text { content: "1".into() }]));
        service.add_message(&id, Message::assistant(id.clone(), vec![Part::Text { content: "2".into() }]));
        service.undo_last_exchange(&id).unwrap();
        assert_eq!(service.get_messages(&id).len(), 0);
        
        // Add new message after undo - this should clear the redo stack
        service.add_message(&id, Message::user(id.clone(), vec![Part::Text { content: "3".into() }]));
        
        // Redo should fail because the stack was cleared
        let result = service.redo_last_exchange(&id);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Nothing to redo");
    }

    #[test]
    fn test_create_default_session_service() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        
        // This should create a session service with SQLite storage
        let service = create_default_session_service(event_bus.clone());
        
        // Create a session and verify it works
        let session = Session::new(std::path::PathBuf::from("/test"), "agent".into(), "model".into());
        let created = service.create(session);
        assert_eq!(created.agent_id, "agent");
        
        // Verify session is persisted - get it again
        let retrieved = service.get(&created.id);
        assert!(retrieved.is_some());
    }
}

/// Minimal test provider for compaction tests
struct TestProvider;
#[async_trait::async_trait]
impl rcode_core::LlmProvider for TestProvider {
    async fn complete(&self, _req: rcode_core::CompletionRequest) -> rcode_core::error::Result<rcode_core::CompletionResponse> {
        Ok(rcode_core::CompletionResponse {
            content: "summary".into(), reasoning: None, tool_calls: vec![],
            usage: rcode_core::TokenUsage { input_tokens: 0, output_tokens: 0, total_tokens: Some(0) },
            stop_reason: rcode_core::provider::StopReason::EndTurn,
        })
    }
    async fn stream(&self, _req: rcode_core::CompletionRequest) -> rcode_core::error::Result<rcode_core::StreamingResponse> { unimplemented!() }
    fn model_info(&self, _model_id: &str) -> Option<rcode_core::ModelInfo> { None }
}
