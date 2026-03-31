//! Application state

use opencode_core::OpencodeConfig;
use opencode_event::EventBus;
use opencode_providers::ProviderRegistry;
use opencode_session::SessionService;
use opencode_tools::ToolRegistryService;
use std::sync::{Arc, Mutex};

pub struct AppState {
    pub session_service: Arc<SessionService>,
    pub event_bus: Arc<EventBus>,
    pub providers: Arc<Mutex<ProviderRegistry>>,
    pub tools: Arc<ToolRegistryService>,
    pub config: OpencodeConfig,
}

impl AppState {
    pub fn new() -> Self {
        let event_bus = Arc::new(EventBus::new(1024));
        Self {
            session_service: Arc::new(SessionService::new(event_bus.clone())),
            event_bus,
            providers: Arc::new(Mutex::new(ProviderRegistry::new())),
            tools: Arc::new(ToolRegistryService::new()),
            config: OpencodeConfig::default(),
        }
    }

    pub fn with_config(config: OpencodeConfig) -> Self {
        let event_bus = Arc::new(EventBus::new(1024));
        Self {
            session_service: Arc::new(SessionService::new(event_bus.clone())),
            event_bus,
            providers: Arc::new(Mutex::new(ProviderRegistry::new())),
            tools: Arc::new(ToolRegistryService::new()),
            config,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
