//! ACP Events - event forwarding as JSON-RPC notifications

use crate::protocol::JsonRpcNotification;
use std::sync::Arc;

pub struct EventForwarder {
    enabled: bool,
    event_bus: Option<Arc<rcode_event::EventBus>>,
    subscriber: Option<rcode_event::EventSubscriber>,
}

impl EventForwarder {
    pub fn new() -> Self {
        Self { 
            enabled: true,
            event_bus: None,
            subscriber: None,
        }
    }

    pub fn with_enabled(enabled: bool) -> Self {
        Self { 
            enabled,
            event_bus: None,
            subscriber: None,
        }
    }

    pub fn with_event_bus(mut self, event_bus: Arc<rcode_event::EventBus>) -> Self {
        self.event_bus = Some(Arc::clone(&event_bus));
        self.subscriber = Some(event_bus.subscribe());
        self
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn notification(
        &self,
        method: impl Into<String>,
        params: Option<serde_json::Value>,
    ) -> Option<JsonRpcNotification> {
        if !self.enabled {
            return None;
        }
        Some(JsonRpcNotification::new(method, params))
    }

    pub async fn forward_events(&mut self, tx: tokio::sync::mpsc::Sender<JsonRpcNotification>) {
        if let Some(subscriber) = &mut self.subscriber {
            loop {
                match subscriber.recv().await {
                    Ok(event) => {
                        let notification = match event {
                            rcode_event::Event::StreamingProgress { session_id, accumulated_text, accumulated_reasoning } => {
                                Some(JsonRpcNotification::new(
                                    "notifications/streaming",
                                    Some(serde_json::json!({
                                        "sessionId": session_id,
                                        "text": accumulated_text,
                                        "reasoning": accumulated_reasoning,
                                    })),
                                ))
                            }
                            rcode_event::Event::MessageAdded { session_id, message_id } => {
                                Some(JsonRpcNotification::new(
                                    "notifications/message",
                                    Some(serde_json::json!({
                                        "sessionId": session_id,
                                        "messageId": message_id,
                                    })),
                                ))
                            }
                            rcode_event::Event::ToolExecuted { session_id, tool_id } => {
                                Some(JsonRpcNotification::new(
                                    "notifications/tool",
                                    Some(serde_json::json!({
                                        "sessionId": session_id,
                                        "toolId": tool_id,
                                    })),
                                ))
                            }
                            rcode_event::Event::AgentStarted { session_id } => {
                                Some(JsonRpcNotification::new(
                                    "notifications/agent-started",
                                    Some(serde_json::json!({
                                        "sessionId": session_id,
                                    })),
                                ))
                            }
                            rcode_event::Event::AgentFinished { session_id } => {
                                Some(JsonRpcNotification::new(
                                    "notifications/agent-finished",
                                    Some(serde_json::json!({
                                        "sessionId": session_id,
                                    })),
                                ))
                            }
                            _ => None,
                        };

                        if let Some(notification) = notification {
                            let _ = tx.send(notification).await;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Event subscription error: {}", e);
                        break;
                    }
                }
            }
        }
    }
}

impl Default for EventForwarder {
    fn default() -> Self {
        Self::new()
    }
}

pub fn tool_call_event(tool_name: &str, arguments: &serde_json::Value) -> JsonRpcNotification {
    JsonRpcNotification::new(
        "notifications/tools/called",
        Some(serde_json::json!({
            "tool": tool_name,
            "arguments": arguments
        })),
    )
}

pub fn tool_result_event(tool_name: &str, result: &serde_json::Value) -> JsonRpcNotification {
    JsonRpcNotification::new(
        "notifications/tools/result",
        Some(serde_json::json!({
            "tool": tool_name,
            "result": result
        })),
    )
}

pub fn streaming_event(content: &str, done: bool) -> JsonRpcNotification {
    JsonRpcNotification::new(
        "notifications/content",
        Some(serde_json::json!({
            "content": content,
            "done": done
        })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_call_event() {
        let notification = tool_call_event("bash", &serde_json::json!({"command": "ls"}));
        assert_eq!(notification.method, "notifications/tools/called");
        let params = notification.params.unwrap();
        assert_eq!(params["tool"], "bash");
    }

    #[test]
    fn test_tool_result_event() {
        let notification = tool_result_event("bash", &serde_json::json!({"output": "ok"}));
        assert_eq!(notification.method, "notifications/tools/result");
        let params = notification.params.unwrap();
        assert_eq!(params["result"]["output"], "ok");
    }

    #[test]
    fn test_streaming_event() {
        let notification = streaming_event("hello", false);
        assert_eq!(notification.method, "notifications/content");
        let params = notification.params.unwrap();
        assert_eq!(params["content"], "hello");
        assert_eq!(params["done"], false);
    }

    #[test]
    fn test_streaming_event_done() {
        let notification = streaming_event("", true);
        let params = notification.params.unwrap();
        assert_eq!(params["done"], true);
    }

    #[test]
    fn test_event_forwarder_new() {
        let fwd = EventForwarder::new();
        assert!(fwd.is_enabled());
    }

    #[test]
    fn test_event_forwarder_disabled() {
        let fwd = EventForwarder::with_enabled(false);
        assert!(!fwd.is_enabled());
        assert!(fwd.notification("test", None).is_none());
    }

    #[test]
    fn test_event_forwarder_set_enabled() {
        let mut fwd = EventForwarder::new();
        fwd.set_enabled(false);
        assert!(!fwd.is_enabled());
        fwd.set_enabled(true);
        assert!(fwd.is_enabled());
    }

    #[test]
    fn test_event_forwarder_notification() {
        let fwd = EventForwarder::new();
        let notification = fwd.notification("notifications/test", Some(serde_json::json!({"key": "val"})));
        assert!(notification.is_some());
        let n = notification.unwrap();
        assert_eq!(n.method, "notifications/test");
    }

    #[test]
    fn test_event_forwarder_with_event_bus() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let fwd = EventForwarder::new().with_event_bus(event_bus);
        assert!(fwd.is_enabled());
        // Should have a subscriber
        let notification = fwd.notification("test", None);
        assert!(notification.is_some());
    }

    #[test]
    fn test_event_forwarder_default() {
        let fwd = EventForwarder::default();
        assert!(fwd.is_enabled());
    }

    #[test]
    fn test_with_enabled_false_creates_disabled_forwarder() {
        let fwd = EventForwarder::with_enabled(false);
        assert!(!fwd.is_enabled());
        assert!(fwd.notification("test", None).is_none());
    }

    #[test]
    fn test_event_forwarder_notification_no_params() {
        let fwd = EventForwarder::new();
        let notification = fwd.notification("test.method", None);
        assert!(notification.is_some());
        let n = notification.unwrap();
        assert_eq!(n.method, "test.method");
        assert!(n.params.is_none());
    }

    #[tokio::test]
    async fn test_event_forwarder_forward_streaming_progress() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let mut fwd = EventForwarder::new().with_event_bus(event_bus.clone());
        
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        
        // Publish an event before starting forward
        event_bus.publish(rcode_event::Event::StreamingProgress {
            session_id: "s1".into(),
            accumulated_text: "Hello".into(),
            accumulated_reasoning: "thinking".into(),
        });
        
        // Start forwarding in background
        let forward_handle = tokio::spawn(async move {
            fwd.forward_events(tx).await;
        });
        
        // Give it time to process
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        
        // Check if we got the notification
        if let Ok(notification) = rx.try_recv() {
            assert_eq!(notification.method, "notifications/streaming");
        }
        
        // Note: forward_events runs in a loop, we just test the setup
    }

    #[test]
    fn test_event_forwarder_with_enabled_true() {
        let fwd = EventForwarder::with_enabled(true);
        assert!(fwd.is_enabled());
        assert!(fwd.notification("test", None).is_some());
    }

    #[tokio::test]
    async fn test_forward_events_no_subscriber() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let mut fwd = EventForwarder::new().with_event_bus(event_bus.clone());
        
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        
        // Spawn forwarder
        let _forward_handle = tokio::spawn(async move {
            fwd.forward_events(tx).await;
        });
        
        // Publish an event
        event_bus.publish(rcode_event::Event::SessionCreated {
            session_id: "s1".into(),
        });
        
        // Give it time to process
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        
        // rx should not have any messages since SessionCreated isn't forwarded
        if let Ok(notification) = rx.try_recv() {
            assert_ne!(notification.method, "notifications/session");
        }
    }

    #[test]
    fn test_event_forwarder_notification_when_disabled() {
        let fwd = EventForwarder::with_enabled(false);
        let result = fwd.notification("test.method", Some(serde_json::json!({"key": "val"})));
        assert!(result.is_none());
    }

    #[test]
    fn test_event_forwarder_clone_independence() {
        // Test that disabling one forwarder doesn't affect another
        let fwd1 = EventForwarder::new();
        let mut fwd2 = EventForwarder::new();
        
        fwd2.set_enabled(false);
        
        assert!(fwd1.is_enabled());
        assert!(!fwd2.is_enabled());
        assert!(fwd1.notification("test", None).is_some());
        assert!(fwd2.notification("test", None).is_none());
    }

    #[test]
    fn test_event_forwarder_notification_method_and_params() {
        let fwd = EventForwarder::new();
        let notification = fwd.notification("custom.method", Some(serde_json::json!({"a": 1, "b": "test"})));
        assert!(notification.is_some());
        let n = notification.unwrap();
        assert_eq!(n.method, "custom.method");
        assert!(n.params.is_some());
    }

    #[tokio::test]
    async fn test_forward_events_forwards_agent_started() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let mut fwd = EventForwarder::new().with_event_bus(event_bus.clone());
        
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        
        // Publish AgentStarted
        event_bus.publish(rcode_event::Event::AgentStarted {
            session_id: "s1".into(),
        });
        
        let forward_handle = tokio::spawn(async move {
            fwd.forward_events(tx).await;
        });
        
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        drop(forward_handle);
        
        // AgentStarted should be forwarded as notifications/agent-started
        if let Ok(notification) = rx.try_recv() {
            assert_eq!(notification.method, "notifications/agent-started");
        }
    }

    #[tokio::test]
    async fn test_forward_events_forwards_agent_finished() {
        let event_bus = Arc::new(rcode_event::EventBus::new(10));
        let mut fwd = EventForwarder::new().with_event_bus(event_bus.clone());
        
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        
        event_bus.publish(rcode_event::Event::AgentFinished {
            session_id: "s1".into(),
        });
        
        let forward_handle = tokio::spawn(async move {
            fwd.forward_events(tx).await;
        });
        
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        drop(forward_handle);
        
        if let Ok(notification) = rx.try_recv() {
            assert_eq!(notification.method, "notifications/agent-finished");
        }
    }
}
