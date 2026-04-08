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
    PermissionRequested {
        request_id: Uuid,
        session_id: String,
        tool_name: String,
        tool_input: serde_json::Value,
    },
    PermissionResolved {
        request_id: Uuid,
        session_id: String,
        granted: bool,
        reason: Option<String>,
    },
    // Phase 3 streaming events - additive alongside legacy StreamingProgress
    StreamTextDelta { session_id: String, delta: String },
    StreamReasoningDelta { session_id: String, delta: String },
    StreamToolCallStart { session_id: String, tool_call_id: String, name: String },
    StreamToolCallArg { session_id: String, tool_call_id: String, value: String },
    StreamToolCallEnd { session_id: String, tool_call_id: String },
    StreamToolResult { session_id: String, tool_call_id: String, content: String, is_error: bool },
    StreamAssistantCommitted { session_id: String },
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
            Event::PermissionRequested { .. } => "permission_requested",
            Event::PermissionResolved { .. } => "permission_resolved",
            Event::StreamTextDelta { .. } => "stream_text_delta",
            Event::StreamReasoningDelta { .. } => "stream_reasoning_delta",
            Event::StreamToolCallStart { .. } => "stream_tool_call_start",
            Event::StreamToolCallArg { .. } => "stream_tool_call_args_delta",
            Event::StreamToolCallEnd { .. } => "stream_tool_call_end",
            Event::StreamToolResult { .. } => "stream_tool_result",
            Event::StreamAssistantCommitted { .. } => "stream_assistant_committed",
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
            Event::PermissionRequested { session_id, .. } => Some(session_id),
            Event::PermissionResolved { session_id, .. } => Some(session_id),
            Event::StreamTextDelta { session_id, .. } => Some(session_id),
            Event::StreamReasoningDelta { session_id, .. } => Some(session_id),
            Event::StreamToolCallStart { session_id, .. } => Some(session_id),
            Event::StreamToolCallArg { session_id, .. } => Some(session_id),
            Event::StreamToolCallEnd { session_id, .. } => Some(session_id),
            Event::StreamToolResult { session_id, .. } => Some(session_id),
            Event::StreamAssistantCommitted { session_id, .. } => Some(session_id),
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
        let event_type = event.event_type();
        let session_id = event.session_id().map(str::to_string);
        match self.sender.send(event) {
            Ok(receiver_count) => {
                tracing::debug!(event_type, session_id = ?session_id, receiver_count, "published event to bus");
            }
            Err(error) => {
                tracing::warn!(event_type, session_id = ?session_id, error = %error, "failed to publish event to bus");
            }
        }
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
            
            if let Some(ref filter) = self.session_filter
                && event.session_id() != Some(filter.as_str())
            {
                continue;
            }
            
            return Ok(event);
        }
    }
    
    pub fn try_recv(&mut self) -> Result<Event, TryRecvError> {
        loop {
            let event = self.receiver.try_recv()?;
            
            if let Some(ref filter) = self.session_filter
                && event.session_id() != Some(filter.as_str())
            {
                continue;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_event_type_names() {
        assert_eq!(Event::SessionCreated { session_id: "s1".into() }.event_type(), "session_created");
        assert_eq!(Event::MessageAdded { session_id: "s1".into(), message_id: "m1".into() }.event_type(), "message_added");
        assert_eq!(Event::ToolExecuted { session_id: "s1".into(), tool_id: "t1".into() }.event_type(), "tool_executed");
        assert_eq!(Event::AgentStarted { session_id: "s1".into() }.event_type(), "agent_started");
        assert_eq!(Event::AgentFinished { session_id: "s1".into() }.event_type(), "agent_finished");
        assert_eq!(Event::ConfigChanged { key: "k".into() }.event_type(), "config_changed");
        assert_eq!(Event::AppStarted { version: "1.0".into() }.event_type(), "app_started");
        assert_eq!(Event::AppShutdown { reason: "done".into() }.event_type(), "app_shutdown");
        assert_eq!(Event::AgentError { session_id: "s1".into(), agent_id: "a1".into(), error: "e".into() }.event_type(), "agent_error");
        assert_eq!(Event::ToolError { session_id: "s1".into(), tool_id: "t1".into(), error: "e".into(), duration_ms: 100 }.event_type(), "tool_error");
        assert_eq!(Event::ProviderConnected { provider_id: "p1".into(), provider_type: "anthropic".into() }.event_type(), "provider_connected");
        assert_eq!(Event::ProviderDisconnected { provider_id: "p1".into(), reason: "r".into() }.event_type(), "provider_disconnected");
        assert_eq!(Event::ProviderError { provider_id: "p1".into(), error: "e".into() }.event_type(), "provider_error");
        assert_eq!(Event::PluginInstalled { plugin_id: "pl".into(), version: "1.0".into() }.event_type(), "plugin_installed");
        assert_eq!(Event::PluginActivated { plugin_id: "pl".into() }.event_type(), "plugin_activated");
        assert_eq!(Event::PluginDeactivated { plugin_id: "pl".into(), reason: "r".into() }.event_type(), "plugin_deactivated");
    }

    #[test]
    fn test_event_session_id() {
        assert_eq!(Event::SessionCreated { session_id: "s1".into() }.session_id(), Some("s1"));
        assert_eq!(Event::MessageAdded { session_id: "s1".into(), message_id: "m1".into() }.session_id(), Some("s1"));
        assert_eq!(Event::ConfigChanged { key: "k".into() }.session_id(), None);
        assert_eq!(Event::AppStarted { version: "1.0".into() }.session_id(), None);
        assert_eq!(Event::AppShutdown { reason: "done".into() }.session_id(), None);
        assert_eq!(Event::ProviderConnected { provider_id: "p".into(), provider_type: "t".into() }.session_id(), None);
        assert_eq!(Event::PluginInstalled { plugin_id: "p".into(), version: "v".into() }.session_id(), None);
    }

    #[test]
    fn test_event_serialization() {
        let event = Event::SessionCreated { session_id: "s1".into() };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "session_created");
        assert_eq!(json["session_id"], "s1");
    }

    #[test]
    fn test_compaction_event_serialization() {
        let event = Event::CompactionPerformed {
            session_id: "s1".into(),
            original_count: 100,
            new_count: 10,
            tokens_saved: 9000,
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["original_count"], 100);
        assert_eq!(json["new_count"], 10);
    }

    #[test]
    fn test_sse_event_from_event() {
        let event = Event::StreamingProgress {
            session_id: "s1".into(),
            accumulated_text: "hello".into(),
            accumulated_reasoning: "".into(),
        };
        let sse: SseEvent = event.into();
        assert_eq!(sse.session_id, Some("s1".to_string()));
        assert_eq!(sse.event_type, "streaming_progress");
        assert!(!sse.id.is_empty());
    }

    #[test]
    fn test_sse_event_no_session() {
        let event = Event::ConfigChanged { key: "model".into() };
        let sse: SseEvent = event.into();
        assert_eq!(sse.session_id, None);
        assert_eq!(sse.event_type, "config_changed");
    }

    #[tokio::test]
    async fn test_publish_and_subscribe() {
        let bus = EventBus::new(10);
        let mut sub = bus.subscribe();
        bus.publish(Event::SessionCreated { session_id: "s1".into() });
        let event = sub.recv().await.unwrap();
        assert_eq!(event.event_type(), "session_created");
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let bus = EventBus::new(10);
        let mut sub1 = bus.subscribe();
        let mut sub2 = bus.subscribe();
        bus.publish(Event::AgentStarted { session_id: "s1".into() });
        let e1 = sub1.recv().await.unwrap();
        let e2 = sub2.recv().await.unwrap();
        assert_eq!(e1.event_type(), "agent_started");
        assert_eq!(e2.event_type(), "agent_started");
    }

    #[tokio::test]
    async fn test_session_filtered_subscriber() {
        let bus = EventBus::new(10);
        let mut sub = bus.subscribe_for_session("s1");
        bus.publish(Event::SessionCreated { session_id: "s1".into() });
        bus.publish(Event::SessionCreated { session_id: "s2".into() });
        let e1 = sub.recv().await.unwrap();
        assert_eq!(e1.event_type(), "session_created");
        if let Ok(Ok(e2)) = tokio::time::timeout(Duration::from_millis(50), sub.recv()).await {
            assert_eq!(e2.event_type(), "session_created");
            assert_eq!(e2.session_id(), Some("s1"));
        }
    }

    #[tokio::test]
    async fn test_session_filter_blocks_other_sessions() {
        let bus = EventBus::new(10);
        let mut sub = bus.subscribe_for_session("s_target");
        bus.publish(Event::MessageAdded { session_id: "s_other".into(), message_id: "m1".into() });
        bus.publish(Event::MessageAdded { session_id: "s_target".into(), message_id: "m2".into() });
        let event = sub.recv().await.unwrap();
        assert_eq!(event.session_id(), Some("s_target"));
    }

    #[tokio::test]
    async fn test_filtered_subscriber_by_type() {
        let bus = EventBus::new(10);
        let mut sub = bus.subscribe_to(vec![EventType::ToolExecuted]);
        bus.publish(Event::SessionCreated { session_id: "s1".into() });
        bus.publish(Event::ToolExecuted { session_id: "s1".into(), tool_id: "t1".into() });
        bus.publish(Event::AgentStarted { session_id: "s1".into() });
        let event = sub.recv().await.unwrap();
        assert_eq!(event.event_type(), "tool_executed");
    }

    #[tokio::test]
    async fn test_filtered_subscriber_multiple_types() {
        let bus = EventBus::new(10);
        let mut sub = bus.subscribe_to(vec![EventType::AgentStarted, EventType::AgentFinished]);
        bus.publish(Event::SessionCreated { session_id: "s1".into() });
        bus.publish(Event::AgentStarted { session_id: "s1".into() });
        bus.publish(Event::ToolExecuted { session_id: "s1".into(), tool_id: "t1".into() });
        let e1 = sub.recv().await.unwrap();
        assert_eq!(e1.event_type(), "agent_started");
    }

    #[tokio::test]
    async fn test_send_returns_error_when_no_subscribers() {
        let bus = EventBus::new(1);
        let result = bus.send(Event::AppStarted { version: "1.0".into() });
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_send_succeeds_with_subscriber() {
        let bus = EventBus::new(10);
        let _sub = bus.subscribe();
        let result = bus.send(Event::AppStarted { version: "1.0".into() });
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_small_buffer_drops_old_events() {
        let bus = EventBus::new(2);
        let mut sub = bus.subscribe();
        bus.publish(Event::SessionCreated { session_id: "s1".into() });
        bus.publish(Event::SessionCreated { session_id: "s2".into() });
        bus.publish(Event::SessionCreated { session_id: "s3".into() });
        let _skipped = sub.recv().await;
        let e2 = sub.recv().await.unwrap();
        let e3 = sub.recv().await.unwrap();
        assert_eq!(e2.session_id(), Some("s2"));
        assert_eq!(e3.session_id(), Some("s3"));
    }

    #[test]
    fn test_event_type_equality() {
        assert_eq!(EventType::AgentStarted, EventType::AgentStarted);
        assert_ne!(EventType::AgentStarted, EventType::AgentFinished);
    }

    #[tokio::test]
    async fn test_try_recv_on_subscriber() {
        let bus = EventBus::new(10);
        let mut sub = bus.subscribe();
        bus.publish(Event::SessionCreated { session_id: "s1".into() });
        
        let event = sub.try_recv().unwrap();
        assert_eq!(event.session_id(), Some("s1"));
    }

    #[tokio::test]
    async fn test_try_recv_returns_error_when_empty() {
        let bus = EventBus::new(10);
        let mut sub = bus.subscribe();
        
        let result = sub.try_recv();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_filtered_subscriber_recv_filters_correctly() {
        let bus = EventBus::new(10);
        let mut sub = bus.subscribe_to(vec![EventType::SessionCreated, EventType::SessionDeleted]);
        
        bus.publish(Event::AgentStarted { session_id: "s1".into() });
        bus.publish(Event::SessionCreated { session_id: "s2".into() });
        bus.publish(Event::AgentFinished { session_id: "s1".into() });
        
        let event = sub.recv().await.unwrap();
        assert_eq!(event.event_type(), "session_created");
    }

    #[tokio::test]
    async fn test_event_bus_min_capacity() {
        // Capacity 1 is the minimum valid capacity
        let bus = EventBus::new(1);
        let _sub = bus.subscribe();
        let result = bus.send(Event::AppStarted { version: "1.0".into() });
        assert!(result.is_ok());
    }

    #[test]
    fn test_sse_event_serialization_roundtrip() {
        let event = Event::SessionCreated { session_id: "s1".into() };
        let sse: SseEvent = event.clone().into();
        
        let json = serde_json::to_value(&sse).unwrap();
        assert_eq!(json["event_type"], "session_created");
        assert_eq!(json["session_id"], "s1");
    }

    #[test]
    fn test_compaction_event_session_id() {
        let event = Event::CompactionPerformed {
            session_id: "s1".into(),
            original_count: 100,
            new_count: 10,
            tokens_saved: 9000,
        };
        assert_eq!(event.session_id(), Some("s1"));
    }

    #[tokio::test]
    async fn test_publish_while_subscribers_exist() {
        let bus = EventBus::new(10);
        let mut sub1 = bus.subscribe();
        let mut sub2 = bus.subscribe_for_session("s1");
        
        bus.publish(Event::MessageAdded { session_id: "s1".into(), message_id: "m1".into() });
        
        // Both should receive
        let e1 = sub1.recv().await.unwrap();
        assert_eq!(e1.event_type(), "message_added");
        
        let e2 = sub2.recv().await.unwrap();
        assert_eq!(e2.session_id(), Some("s1"));
    }

    #[test]
    fn test_event_session_id_for_all_types() {
        // Test all event types that have session_id
        assert_eq!(Event::SessionCreated { session_id: "s".into() }.session_id(), Some("s"));
        assert_eq!(Event::SessionUpdated { session_id: "s".into() }.session_id(), Some("s"));
        assert_eq!(Event::SessionDeleted { session_id: "s".into() }.session_id(), Some("s"));
        assert_eq!(Event::MessageAdded { session_id: "s".into(), message_id: "m".into() }.session_id(), Some("s"));
        assert_eq!(Event::ToolExecuted { session_id: "s".into(), tool_id: "t".into() }.session_id(), Some("s"));
        assert_eq!(Event::AgentStarted { session_id: "s".into() }.session_id(), Some("s"));
        assert_eq!(Event::AgentFinished { session_id: "s".into() }.session_id(), Some("s"));
        assert_eq!(Event::CompactionPerformed { session_id: "s".into(), original_count: 1, new_count: 1, tokens_saved: 1 }.session_id(), Some("s"));
        assert_eq!(Event::StreamingProgress { session_id: "s".into(), accumulated_text: "".into(), accumulated_reasoning: "".into() }.session_id(), Some("s"));
        assert_eq!(Event::AgentError { session_id: "s".into(), agent_id: "a".into(), error: "e".into() }.session_id(), Some("s"));
        assert_eq!(Event::ToolError { session_id: "s".into(), tool_id: "t".into(), error: "e".into(), duration_ms: 1 }.session_id(), Some("s"));
        
        // Test types without session_id
        assert_eq!(Event::ConfigChanged { key: "k".into() }.session_id(), None);
        assert_eq!(Event::AppStarted { version: "v".into() }.session_id(), None);
        assert_eq!(Event::AppShutdown { reason: "r".into() }.session_id(), None);
        assert_eq!(Event::ProviderConnected { provider_id: "p".into(), provider_type: "t".into() }.session_id(), None);
        assert_eq!(Event::ProviderDisconnected { provider_id: "p".into(), reason: "r".into() }.session_id(), None);
        assert_eq!(Event::ProviderError { provider_id: "p".into(), error: "e".into() }.session_id(), None);
        assert_eq!(Event::PluginInstalled { plugin_id: "p".into(), version: "v".into() }.session_id(), None);
        assert_eq!(Event::PluginActivated { plugin_id: "p".into() }.session_id(), None);
        assert_eq!(Event::PluginDeactivated { plugin_id: "p".into(), reason: "r".into() }.session_id(), None);
    }

    #[test]
    fn test_event_type_for_all_variants() {
        // Test event_type() for all variants
        assert_eq!(Event::SessionCreated { session_id: "s".into() }.event_type(), "session_created");
        assert_eq!(Event::SessionUpdated { session_id: "s".into() }.event_type(), "session_updated");
        assert_eq!(Event::SessionDeleted { session_id: "s".into() }.event_type(), "session_deleted");
        assert_eq!(Event::CompactionPerformed { session_id: "s".into(), original_count: 1, new_count: 1, tokens_saved: 1 }.event_type(), "compaction_performed");
        assert_eq!(Event::StreamingProgress { session_id: "s".into(), accumulated_text: "t".into(), accumulated_reasoning: "r".into() }.event_type(), "streaming_progress");
    }

    #[test]
    fn test_event_bus_new_capacity_1() {
        // Capacity 1 should work
        let bus = EventBus::new(1);
        let _sub = bus.subscribe();
        let result = bus.send(Event::AppStarted { version: "1.0".into() });
        // Should succeed with subscriber
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_filtered_subscriber_with_matching_event() {
        let bus = EventBus::new(10);
        let mut sub = bus.subscribe_to(vec![EventType::AgentStarted]);
        
        bus.publish(Event::AgentStarted { session_id: "s1".into() });
        bus.publish(Event::SessionCreated { session_id: "s2".into() });
        
        // Should receive AgentStarted events
        let result = sub.recv().await;
        assert!(result.is_ok());
        let event = result.unwrap();
        assert_eq!(event.event_type(), "agent_started");
    }

    #[test]
    fn test_event_type_equality_ordered() {
        // Verify EventType variants can be checked for equality
        assert_eq!(EventType::AppStarted, EventType::AppStarted);
        assert_ne!(EventType::AppStarted, EventType::AppShutdown);
    }

    #[test]
    fn test_sse_event_timestamp() {
        let event = Event::SessionCreated { session_id: "s1".into() };
        let sse: SseEvent = event.into();
        // Timestamp should be set to current time (within reason)
        let now = chrono::Utc::now();
        let diff = (now - sse.timestamp).num_seconds();
        assert!(diff.abs() <= 1); // Within 1 second
    }

    #[test]
    fn test_sse_event_id_unique() {
        let event1 = Event::SessionCreated { session_id: "s1".into() };
        let event2 = Event::SessionCreated { session_id: "s2".into() };
        let sse1: SseEvent = event1.into();
        let sse2: SseEvent = event2.into();
        assert_ne!(sse1.id, sse2.id);
    }

    #[test]
    fn test_event_type_all_variants() {
        // Test all EventType variants exist and can be used
        let _ = EventType::AppStarted;
        let _ = EventType::AppShutdown;
        let _ = EventType::SessionCreated;
        let _ = EventType::SessionUpdated;
        let _ = EventType::SessionDeleted;
        let _ = EventType::MessageAdded;
        let _ = EventType::ToolExecuted;
        let _ = EventType::ToolError;
        let _ = EventType::AgentStarted;
        let _ = EventType::AgentFinished;
        let _ = EventType::AgentError;
        let _ = EventType::ProviderConnected;
        let _ = EventType::ProviderDisconnected;
        let _ = EventType::ProviderError;
        let _ = EventType::PluginInstalled;
        let _ = EventType::PluginActivated;
        let _ = EventType::PluginDeactivated;
    }

    #[tokio::test]
    async fn test_filtered_subscriber_multiple_event_types() {
        let bus = EventBus::new(10);
        let mut sub = bus.subscribe_to(vec![
            EventType::SessionCreated,
            EventType::SessionDeleted,
            EventType::ToolExecuted,
        ]);
        
        bus.publish(Event::AgentStarted { session_id: "s1".into() });
        bus.publish(Event::SessionDeleted { session_id: "s1".into() });
        bus.publish(Event::ToolExecuted { session_id: "s1".into(), tool_id: "bash".into() });
        
        let event = sub.recv().await.unwrap();
        assert_eq!(event.event_type(), "session_deleted");
    }

    #[tokio::test]
    async fn test_filtered_subscriber_skips_non_matching() {
        let bus = EventBus::new(10);
        let mut sub = bus.subscribe_to(vec![EventType::ToolError]);
        
        bus.publish(Event::SessionCreated { session_id: "s1".into() });
        bus.publish(Event::AgentStarted { session_id: "s1".into() });
        bus.publish(Event::ToolError { 
            session_id: "s1".into(), 
            tool_id: "bash".into(), 
            error: "failed".into(),
            duration_ms: 100,
        });
        
        let event = sub.recv().await.unwrap();
        assert_eq!(event.event_type(), "tool_error");
    }

    #[test]
    fn test_event_type_debug() {
        let et = EventType::AgentStarted;
        let debug_str = format!("{:?}", et);
        assert!(debug_str.contains("AgentStarted"));
    }

    #[test]
    fn test_event_type_clone() {
        let et = EventType::ProviderConnected;
        let cloned = et;
        assert_eq!(et, cloned);
    }

    #[test]
    fn test_event_type_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(EventType::AppStarted);
        set.insert(EventType::AppShutdown);
        set.insert(EventType::SessionCreated);
        assert!(set.contains(&EventType::AppStarted));
        assert!(set.len() == 3);
    }
}
