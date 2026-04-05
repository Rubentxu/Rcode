//! Application state

use rcode_core::RcodeConfig;
use rcode_event::EventBus;
use rcode_providers::catalog::ModelCatalogService;
use rcode_providers::ProviderRegistry;
use rcode_session::SessionService;
use rcode_storage::{schema, MessageRepository, SessionRepository};
use rcode_tools::ToolRegistryService;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

use crate::cancellation::CancellationRegistry;

pub struct AppState {
    pub session_service: Arc<SessionService>,
    pub event_bus: Arc<EventBus>,
    pub providers: Arc<Mutex<ProviderRegistry>>,
    pub tools: Arc<ToolRegistryService>,
    pub config: Arc<std::sync::Mutex<RcodeConfig>>,
    pub catalog: Arc<ModelCatalogService>,
    pub cancellation: Arc<CancellationRegistry>,
}

fn create_storage_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let data_dir = std::path::PathBuf::from(home).join(".local/share/rcode");
    std::fs::create_dir_all(&data_dir).ok();
    data_dir.join("rcode.db")
}

impl AppState {
    pub fn new() -> Self {
        Self::with_storage_impl(RcodeConfig::default())
    }

    pub fn with_config(config: RcodeConfig) -> Self {
        Self::with_storage_impl(config)
    }

    fn with_storage_impl(config: RcodeConfig) -> Self {
        let event_bus = Arc::new(EventBus::new(1024));

        let db_path = create_storage_path();
        tracing::info!("Using database at: {:?}", db_path);

        let conn = match Connection::open(&db_path) {
            Ok(conn) => conn,
            Err(e) => {
                tracing::warn!("Failed to open database, using in-memory storage: {}", e);
                let session_service = Arc::new(SessionService::new(event_bus.clone()));
                return Self {
                    session_service: session_service.clone(),
                    event_bus,
                    providers: Arc::new(Mutex::new(ProviderRegistry::new())),
                    tools: Arc::new(ToolRegistryService::with_session_service(session_service)),
                    config: Arc::new(Mutex::new(config)),
                    catalog: Arc::new(ModelCatalogService::new()),
                    cancellation: Arc::new(CancellationRegistry::new()),
                };
            }
        };

        if let Err(e) = schema::init_schema(&conn) {
            tracing::warn!(
                "Failed to initialize schema, using in-memory storage: {}",
                e
            );
            let session_service = Arc::new(SessionService::new(event_bus.clone()));
            return Self {
                session_service: session_service.clone(),
                event_bus,
                providers: Arc::new(Mutex::new(ProviderRegistry::new())),
                tools: Arc::new(ToolRegistryService::with_session_service(session_service)),
                config: Arc::new(Mutex::new(config)),
                catalog: Arc::new(ModelCatalogService::new()),
                cancellation: Arc::new(CancellationRegistry::new()),
            };
        }

        // Open a second connection for MessageRepository since Connection doesn't implement Clone
        let message_conn = match Connection::open(&db_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Failed to open second database connection: {}", e);
                let session_service = Arc::new(SessionService::new(event_bus.clone()));
                return Self {
                    session_service: session_service.clone(),
                    event_bus,
                    providers: Arc::new(Mutex::new(ProviderRegistry::new())),
                    tools: Arc::new(ToolRegistryService::with_session_service(session_service)),
                    config: Arc::new(Mutex::new(config)),
                    catalog: Arc::new(ModelCatalogService::new()),
                    cancellation: Arc::new(CancellationRegistry::new()),
                };
            }
        };

        let session_repo = SessionRepository::new(conn);
        let message_repo = MessageRepository::new(message_conn);

        let session_service = Arc::new(SessionService::with_storage(
            event_bus.clone(),
            session_repo,
            message_repo,
        ));

        Self {
            session_service: session_service.clone(),
            event_bus,
            providers: Arc::new(Mutex::new(ProviderRegistry::new())),
            tools: Arc::new(ToolRegistryService::with_session_service(session_service)),
            config: Arc::new(Mutex::new(config)),
            catalog: Arc::new(ModelCatalogService::new()),
            cancellation: Arc::new(CancellationRegistry::new()),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
