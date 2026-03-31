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
