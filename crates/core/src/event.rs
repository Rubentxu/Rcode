//! Event types for the event bus

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    SessionCreated {
        session_id: String,
    },
    SessionUpdated {
        session_id: String,
    },
    SessionDeleted {
        session_id: String,
    },
    MessageAdded {
        session_id: String,
        message_id: String,
    },
    ToolExecuted {
        session_id: String,
        tool_id: String,
        success: bool,
    },
    AgentStarted {
        session_id: String,
        agent_id: String,
    },
    AgentFinished {
        session_id: String,
        agent_id: String,
    },
    ConfigChanged {
        key: String,
    },
    ProviderAuthChanged {
        provider: String,
    },
}

pub struct EventBus {
    sender: broadcast::Sender<Event>,
}

impl EventBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1024);
        Self { sender }
    }
    
    pub fn subscribe(&self) -> EventSubscriber {
        EventSubscriber {
            receiver: self.sender.subscribe(),
        }
    }
    
    pub fn publish(&self, event: Event) {
        let _ = self.sender.send(event);
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

pub struct EventSubscriber {
    receiver: broadcast::Receiver<Event>,
}

impl EventSubscriber {
    pub async fn recv(&mut self) -> Result<Event, RecvError> {
        self.receiver.recv().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_bus_new() {
        let bus = EventBus::new();
        // Should be able to create and use it
        let _sub = bus.subscribe();
    }

    #[test]
    fn test_event_bus_default() {
        let bus = EventBus::default();
        let _sub = bus.subscribe();
    }

    #[test]
    fn test_event_bus_subscribe_returns_subscriber() {
        let bus = EventBus::new();
        let mut sub = bus.subscribe();
        // Subscribe should return a valid subscriber
        assert!(sub.receiver.len() == 0);
    }

    #[tokio::test]
    async fn test_event_bus_publish_and_subscribe() {
        let bus = EventBus::new();
        let mut sub = bus.subscribe();
        
        bus.publish(Event::SessionCreated {
            session_id: "test-123".to_string(),
        });
        
        let event = sub.recv().await.unwrap();
        assert!(matches!(event, Event::SessionCreated { .. }));
    }

    #[tokio::test]
    async fn test_event_bus_publish_no_subscriber_drops() {
        let bus = EventBus::new();
        // Publishing without a subscriber should not panic
        bus.publish(Event::SessionCreated {
            session_id: "test-123".to_string(),
        });
    }

    #[tokio::test]
    async fn test_event_subscriber_recv() {
        let bus = EventBus::new();
        let mut sub = bus.subscribe();
        
        bus.publish(Event::MessageAdded {
            session_id: "s1".to_string(),
            message_id: "m1".to_string(),
        });
        
        let result = sub.recv().await;
        assert!(result.is_ok());
    }
}
