//! TUI Application state and models

use rcode_core::{Message, Session, SessionId};
use std::sync::Arc;

/// Application mode / screen
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    SessionList,
    Chat,
    Settings,
    ModelPicker,
}

/// Main TUI application state
#[derive(Debug)]
pub struct RcodeTui {
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
    /// Current streaming delta being accumulated (session_id -> text)
    pub streaming_deltas: std::collections::HashMap<String, String>,
    /// Model list for picker: (model_id, provider, enabled)
    pub model_list: Vec<(String, String, bool)>,
    /// Current index in model list
    pub model_picker_index: usize,
    /// Current provider filter index
    pub model_picker_provider_index: usize,
}

#[derive(Debug, Clone)]
pub enum ToolStatus {
    Running,
    Completed,
    Failed(String),
}

impl RcodeTui {
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
            streaming_deltas: std::collections::HashMap::new(),
            model_list: Vec::new(),
            model_picker_index: 0,
            model_picker_provider_index: 0,
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

    /// Append streaming delta text for a session
    pub fn append_streaming_delta(&mut self, session_id: &str, delta: &str) {
        self.streaming_deltas
            .entry(session_id.to_string())
            .or_insert_with(String::new)
            .push_str(delta);
    }

    /// Get the current streaming text for a session
    pub fn get_streaming_text(&self, session_id: &str) -> Option<String> {
        self.streaming_deltas.get(session_id).cloned()
    }

    /// Clear streaming delta for a session
    pub fn clear_streaming_delta(&mut self, session_id: &str) {
        self.streaming_deltas.remove(session_id);
    }

    /// Add a completed message to the display
    pub fn add_message_to_display(&mut self, session_id: &str, message: Message) {
        // Only add if this message is for the current session
        if let Some(current) = &self.current_session {
            if current.0 == session_id {
                self.messages.push(message);
                // Clear any streaming delta for this session since message is now complete
                self.clear_streaming_delta(session_id);
            }
        }
    }

    /// Set running state
    pub fn set_running(&mut self, running: bool) {
        self.is_running = running;
    }
}

impl Default for RcodeTui {
    fn default() -> Self {
        Self::new()
    }
}
