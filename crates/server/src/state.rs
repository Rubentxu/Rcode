//! Application state

use std::collections::HashMap;
use std::sync::Arc;

use rcode_agent::permissions::InteractivePermissionService;
use rcode_core::{RcodeConfig, SubagentRunner};
use rcode_event::EventBus;
use rcode_providers::catalog::ModelCatalogService;
use rcode_providers::ProviderRegistry;
use rcode_session::SessionService;
use rcode_storage::{schema, MessageRepository, SessionRepository};
use rcode_tools::ToolRegistryService;
use rusqlite::Connection;
use tokio::sync::Mutex as TokioMutex;

use crate::cancellation::CancellationRegistry;
use crate::subagent_runner_impl::ServerSubagentRunner;

pub struct AppState {
    pub session_service: Arc<SessionService>,
    pub event_bus: Arc<EventBus>,
    pub providers: Arc<std::sync::Mutex<ProviderRegistry>>,
    pub tools: Arc<ToolRegistryService>,
    pub config: Arc<std::sync::Mutex<RcodeConfig>>,
    pub catalog: Arc<ModelCatalogService>,
    pub cancellation: Arc<CancellationRegistry>,
    /// Map of session_id to InteractivePermissionService for interactive permission approval
    pub permission_services: Arc<TokioMutex<HashMap<String, Arc<InteractivePermissionService>>>>,
}

fn create_storage_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let data_dir = std::path::PathBuf::from(home).join(".local/share/rcode");
    std::fs::create_dir_all(&data_dir).ok();
    data_dir.join("rcode.db")
}

/// Create a ToolRegistryService with SubagentRunner injected
fn create_tools_with_runner(
    session_service: Arc<SessionService>,
    config: &RcodeConfig,
) -> Arc<ToolRegistryService> {
    let tools = Arc::new(ToolRegistryService::with_session_service(
        session_service.clone(),
    ));

    // Create the subagent runner with the necessary services
    let runner: Arc<dyn SubagentRunner> = Arc::new(ServerSubagentRunner::with_deps(
        session_service.clone(),
        Arc::new(EventBus::new(1024)), // event_bus will be replaced in actual state
        Arc::new(std::sync::Mutex::new(config.clone())),
        Arc::clone(&tools),
    ));

    // Inject the runner into TaskTool
    let task_tool =
        rcode_tools::task::TaskTool::with_session_service(session_service).with_runner(runner);
    tools.set_task_tool(task_tool);

    tools
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
                    providers: Arc::new(std::sync::Mutex::new(ProviderRegistry::new())),
                    tools: Arc::new(ToolRegistryService::with_session_service(
                        session_service.clone(),
                    )),
                    config: Arc::new(std::sync::Mutex::new(config)),
                    catalog: Arc::new(ModelCatalogService::new()),
                    cancellation: Arc::new(CancellationRegistry::new()),
                    permission_services: Arc::new(TokioMutex::new(HashMap::new())),
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
                providers: Arc::new(std::sync::Mutex::new(ProviderRegistry::new())),
                tools: Arc::new(ToolRegistryService::with_session_service(
                    session_service.clone(),
                )),
                config: Arc::new(std::sync::Mutex::new(config)),
                catalog: Arc::new(ModelCatalogService::new()),
                cancellation: Arc::new(CancellationRegistry::new()),
                permission_services: Arc::new(TokioMutex::new(HashMap::new())),
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
                    providers: Arc::new(std::sync::Mutex::new(ProviderRegistry::new())),
                    tools: Arc::new(ToolRegistryService::with_session_service(
                        session_service.clone(),
                    )),
                    config: Arc::new(std::sync::Mutex::new(config)),
                    catalog: Arc::new(ModelCatalogService::new()),
                    cancellation: Arc::new(CancellationRegistry::new()),
                    permission_services: Arc::new(TokioMutex::new(HashMap::new())),
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

        // Create tools with the runner injected
        let tools = create_tools_with_runner(session_service.clone(), &config);

        Self {
            session_service: session_service.clone(),
            event_bus,
            providers: Arc::new(std::sync::Mutex::new(ProviderRegistry::new())),
            tools,
            config: Arc::new(std::sync::Mutex::new(config)),
            catalog: Arc::new(ModelCatalogService::new()),
            cancellation: Arc::new(CancellationRegistry::new()),
            permission_services: Arc::new(TokioMutex::new(HashMap::new())),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
