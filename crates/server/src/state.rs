//! Application state

use rcode_core::OpencodeConfig;
use rcode_event::EventBus;
use rcode_providers::ProviderRegistry;
use rcode_session::SessionService;
use rcode_tools::ToolRegistryService;
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
        let session_service = Arc::new(SessionService::new(event_bus.clone()));
        Self {
            session_service: session_service.clone(),
            event_bus,
            providers: Arc::new(Mutex::new(ProviderRegistry::new())),
            tools: Arc::new(ToolRegistryService::with_session_service(session_service)),
            config: OpencodeConfig::default(),
        }
    }

    pub fn with_config(config: OpencodeConfig) -> Self {
        let event_bus = Arc::new(EventBus::new(1024));
        let session_service = Arc::new(SessionService::new(event_bus.clone()));
        Self {
            session_service: session_service.clone(),
            event_bus,
            providers: Arc::new(Mutex::new(ProviderRegistry::new())),
            tools: Arc::new(ToolRegistryService::with_session_service(session_service)),
            config,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
