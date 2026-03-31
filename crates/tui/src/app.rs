//! TUI Application state and models

use opencode_core::{Message, Session, SessionId, SessionStatus};
use std::sync::Arc;

/// Application mode / screen
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    SessionList,
    Chat,
    Settings,
}

/// Main TUI application state
#[derive(Debug)]
pub struct OpencodeTui {
    /// All available sessions
    pub sessions: Vec<Arc<Session>>,
    /// Currently active session ID
    pub current_session: Option<SessionId>,
    /// Messages for the current session
    pub messages: Vec<Message>,
    /// User input buffer
    pub input_buffer: String,
    /// Current application mode
    pub mode: AppMode,
    /// Whether agent is currently running
    pub is_running: bool,
    /// Tool execution status (tool name -> status)
    pub tool_status: std::collections::HashMap<String, ToolStatus>,
    /// Scroll offset for message list
    pub scroll_offset: usize,
    /// Search query for session list
    pub session_search: String,
}

#[derive(Debug, Clone)]
pub enum ToolStatus {
    Running,
    Completed,
    Failed(String),
}

impl OpencodeTui {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
            current_session: None,
            messages: Vec::new(),
            input_buffer: String::new(),
            mode: AppMode::SessionList,
            is_running: false,
            tool_status: std::collections::HashMap::new(),
            scroll_offset: 0,
            session_search: String::new(),
        }
    }

    /// Get the current session if one is selected
    pub fn current_session(&self) -> Option<&Arc<Session>> {
        self.current_session
            .as_ref()
            .and_then(|id| self.sessions.iter().find(|s| s.id == *id))
    }

    /// Switch to a different session
    pub fn select_session(&mut self, session_id: &SessionId) {
        self.current_session = Some(session_id.clone());
        self.messages.clear();
        self.mode = AppMode::Chat;
    }

    /// Create a new session and select it
    pub fn create_session(&mut self, session: Arc<Session>) {
        self.sessions.insert(0, session.clone());
        self.select_session(&session.id);
    }

    /// Update messages from session service
    pub fn update_messages(&mut self, messages: Vec<Message>) {
        self.messages = messages;
    }

    /// Filter sessions by search query
    pub fn filtered_sessions(&self) -> Vec<&Arc<Session>> {
        if self.session_search.is_empty() {
            self.sessions.iter().collect()
        } else {
            let query = self.session_search.to_lowercase();
            self.sessions
                .iter()
                .filter(|s| {
                    s.title
                        .as_ref()
                        .map(|t| t.to_lowercase().contains(&query))
                        .unwrap_or(false)
                        || s.id.0.to_lowercase().contains(&query)
                })
                .collect()
        }
    }
}

impl Default for OpencodeTui {
    fn default() -> Self {
        Self::new()
    }
}
