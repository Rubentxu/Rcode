//! Event bus using tokio broadcast channel

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::{RecvError, SendError, TryRecvError};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    SessionCreated { session_id: String },
    SessionUpdated { session_id: String },
    SessionDeleted { session_id: String },
    MessageAdded { session_id: String, message_id: String },
    ToolExecuted { session_id: String, tool_id: String },
    AgentStarted { session_id: String },
    AgentFinished { session_id: String },
    ConfigChanged { key: String },
    CompactionPerformed {
        session_id: String,
        original_count: usize,
        new_count: usize,
        tokens_saved: usize,
    },
    StreamingProgress {
        session_id: String,
        accumulated_text: String,
        accumulated_reasoning: String,
    },
    AppStarted { version: String },
    AppShutdown { reason: String },
    AgentError { session_id: String, agent_id: String, error: String },
    ToolError { session_id: String, tool_id: String, error: String, duration_ms: u64 },
    ProviderConnected { provider_id: String, provider_type: String },
    ProviderDisconnected { provider_id: String, reason: String },
    ProviderError { provider_id: String, error: String },
    PluginInstalled { plugin_id: String, version: String },
    PluginActivated { plugin_id: String },
    PluginDeactivated { plugin_id: String, reason: String },
}

impl Event {
    /// Returns the event type name for SSE
    pub fn event_type(&self) -> &'static str {
        match self {
            Event::SessionCreated { .. } => "session_created",
            Event::SessionUpdated { .. } => "session_updated",
            Event::SessionDeleted { .. } => "session_deleted",
            Event::MessageAdded { .. } => "message_added",
            Event::ToolExecuted { .. } => "tool_executed",
            Event::AgentStarted { .. } => "agent_started",
            Event::AgentFinished { .. } => "agent_finished",
            Event::ConfigChanged { .. } => "config_changed",
            Event::CompactionPerformed { .. } => "compaction_performed",
            Event::StreamingProgress { .. } => "streaming_progress",
            Event::AppStarted { .. } => "app_started",
            Event::AppShutdown { .. } => "app_shutdown",
            Event::AgentError { .. } => "agent_error",
            Event::ToolError { .. } => "tool_error",
            Event::ProviderConnected { .. } => "provider_connected",
            Event::ProviderDisconnected { .. } => "provider_disconnected",
            Event::ProviderError { .. } => "provider_error",
            Event::PluginInstalled { .. } => "plugin_installed",
            Event::PluginActivated { .. } => "plugin_activated",
            Event::PluginDeactivated { .. } => "plugin_deactivated",
        }
    }
    
    /// Returns the session_id associated with this event (if any)
    pub fn session_id(&self) -> Option<&str> {
        match self {
            Event::SessionCreated { session_id } => Some(session_id),
            Event::SessionUpdated { session_id } => Some(session_id),
            Event::SessionDeleted { session_id } => Some(session_id),
            Event::MessageAdded { session_id, .. } => Some(session_id),
            Event::ToolExecuted { session_id, .. } => Some(session_id),
            Event::AgentStarted { session_id } => Some(session_id),
            Event::AgentFinished { session_id } => Some(session_id),
            Event::ConfigChanged { .. } => None,
            Event::CompactionPerformed { session_id, .. } => Some(session_id),
            Event::StreamingProgress { session_id, .. } => Some(session_id),
            Event::AppStarted { .. } => None,
            Event::AppShutdown { .. } => None,
            Event::AgentError { session_id, .. } => Some(session_id),
            Event::ToolError { session_id, .. } => Some(session_id),
            Event::ProviderConnected { .. } => None,
            Event::ProviderDisconnected { .. } => None,
            Event::ProviderError { .. } => None,
            Event::PluginInstalled { .. } => None,
            Event::PluginActivated { .. } => None,
            Event::PluginDeactivated { .. } => None,
        }
    }
}

/// SSE event wrapper for external API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SseEvent {
    pub id: String,
    pub session_id: Option<String>,
    pub event_type: String,
    pub timestamp: DateTime<Utc>,
    pub data: Event,
}

impl From<Event> for SseEvent {
    fn from(event: Event) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session_id: event.session_id().map(|s| s.to_string()),
            event_type: event.event_type().to_string(),
            timestamp: Utc::now(),
            data: event,
        }
    }
}

#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<Event>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }
    
    pub fn subscribe(&self) -> EventSubscriber {
        EventSubscriber {
            receiver: self.sender.subscribe(),
            session_filter: None,
        }
    }
    
    pub fn subscribe_for_session(&self, session_id: &str) -> EventSubscriber {
        EventSubscriber {
            receiver: self.sender.subscribe(),
            session_filter: Some(session_id.to_string()),
        }
    }
    
    pub fn publish(&self, event: Event) {
        let _ = self.sender.send(event);
    }
    
    pub fn send(&self, event: Event) -> Result<(), SendError<Event>> {
        self.sender.send(event).map(|_| ())
    }
}

pub struct EventSubscriber {
    receiver: broadcast::Receiver<Event>,
    session_filter: Option<String>,
}

impl EventSubscriber {
    pub async fn recv(&mut self) -> Result<Event, RecvError> {
        loop {
            let event = self.receiver.recv().await?;
            
            if let Some(ref filter) = self.session_filter {
                if event.session_id() != Some(filter.as_str()) {
                    continue;
                }
            }
            
            return Ok(event);
        }
    }
    
    pub fn try_recv(&mut self) -> Result<Event, TryRecvError> {
        loop {
            let event = self.receiver.try_recv()?;
            
            if let Some(ref filter) = self.session_filter {
                if event.session_id() != Some(filter.as_str()) {
                    continue;
                }
            }
            
            return Ok(event);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventType {
    AppStarted,
    AppShutdown,
    SessionCreated,
    SessionUpdated,
    SessionDeleted,
    MessageAdded,
    ToolExecuted,
    ToolError,
    AgentStarted,
    AgentFinished,
    AgentError,
    ProviderConnected,
    ProviderDisconnected,
    ProviderError,
    PluginInstalled,
    PluginActivated,
    PluginDeactivated,
}

impl EventBus {
    pub fn subscribe_to(&self, types: Vec<EventType>) -> FilteredSubscriber {
        FilteredSubscriber {
            receiver: self.sender.subscribe(),
            type_filter: types,
        }
    }
}

pub struct FilteredSubscriber {
    receiver: broadcast::Receiver<Event>,
    type_filter: Vec<EventType>,
}

impl FilteredSubscriber {
    pub async fn recv(&mut self) -> Result<Event, RecvError> {
        loop {
            let event = self.receiver.recv().await?;
            let event_type = event.event_type();
            
            if self.type_filter.iter().any(|t| {
                let t_name = match t {
                    EventType::AppStarted => "app_started",
                    EventType::AppShutdown => "app_shutdown",
                    EventType::SessionCreated => "session_created",
                    EventType::SessionUpdated => "session_updated",
                    EventType::SessionDeleted => "session_deleted",
                    EventType::MessageAdded => "message_added",
                    EventType::ToolExecuted => "tool_executed",
                    EventType::ToolError => "tool_error",
                    EventType::AgentStarted => "agent_started",
                    EventType::AgentFinished => "agent_finished",
                    EventType::AgentError => "agent_error",
                    EventType::ProviderConnected => "provider_connected",
                    EventType::ProviderDisconnected => "provider_disconnected",
                    EventType::ProviderError => "provider_error",
                    EventType::PluginInstalled => "plugin_installed",
                    EventType::PluginActivated => "plugin_activated",
                    EventType::PluginDeactivated => "plugin_deactivated",
                };
                event_type == t_name
            }) {
                return Ok(event);
            }
        }
    }
}
