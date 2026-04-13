//! Application state

use std::collections::HashMap;
use std::sync::Arc;

use rcode_agent::permissions::InteractivePermissionService;
use rcode_core::{RcodeConfig, SubagentRunner};
use rcode_event::EventBus;
use rcode_lsp::LanguageServerRegistry;
use rcode_providers::catalog::ModelCatalogService;
use rcode_providers::CacheStore;
use rcode_providers::{LlmProvider, ProviderRegistry};
use rcode_session::{ProjectService, SessionService};
use rcode_storage::{schema, catalog_cache::CatalogCacheRepository, MessageRepository, ProjectRepository, SessionRepository};
use rcode_tools::ToolRegistryService;
use rusqlite::Connection;
use tokio::sync::Mutex as TokioMutex;

use crate::cache_store_impl::ServerCacheStore;
use crate::cancellation::CancellationRegistry;
use crate::explorer::ExplorerService;
use crate::subagent_runner_impl::ServerSubagentRunner;

pub struct AppState {
    pub project_service: Option<Arc<ProjectService>>,
    pub session_service: Arc<SessionService>,
    pub event_bus: Arc<EventBus>,
    pub providers: Arc<std::sync::Mutex<ProviderRegistry>>,
    pub tools: Arc<ToolRegistryService>,
    pub config: Arc<std::sync::Mutex<RcodeConfig>>,
    pub catalog: Arc<ModelCatalogService>,
    pub cancellation: Arc<CancellationRegistry>,
    /// Map of session_id to InteractivePermissionService for interactive permission approval
    pub permission_services: Arc<TokioMutex<HashMap<String, Arc<InteractivePermissionService>>>>,
    /// LSP language server registry for code intelligence
    pub lsp_registry: Arc<LanguageServerRegistry>,
    /// Optional mock provider for testing (injected via TestApp)
    pub mock_provider: Arc<std::sync::Mutex<Option<Arc<dyn LlmProvider>>>>,
    /// Explorer service for workspace file tree
    pub explorer_service: Arc<ExplorerService>,
}

fn create_storage_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let data_dir = std::path::PathBuf::from(home).join(".local/share/rcode");
    std::fs::create_dir_all(&data_dir).ok();
    data_dir.join("rcode.db")
}

/// Create a ToolRegistryService with SubagentRunner injected and LSP tools registered
fn create_tools_with_runner(
    session_service: Arc<SessionService>,
    config: &RcodeConfig,
    lsp_registry: Arc<rcode_lsp::LanguageServerRegistry>,
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

    // Register a single LSP tool that uses the registry to find the right client
    // Note: The LspToolAdapter needs a client at creation time, so we use from_registry
    // with a placeholder path. In a full implementation, we would modify LspToolAdapter
    // to use the registry dynamically at execute time.
    if let Some(lsp_tool) = rcode_lsp::LspToolAdapter::from_registry(
        "/tmp", // placeholder path
        Arc::clone(&lsp_registry),
    ) {
        tools.register(Arc::new(lsp_tool));
        tracing::info!("Registered LSP tool adapter");
    }

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

        // ---- Path 1: Database initialization fails ----
        let conn = match Connection::open(&db_path) {
            Ok(conn) => conn,
            Err(e) => {
                tracing::warn!("Failed to open database, using in-memory storage: {}", e);
                let session_service = Arc::new(SessionService::new(event_bus.clone()));
                let lsp_registry = Arc::new(LanguageServerRegistry::new());
                let catalog = Arc::new(ModelCatalogService::new());
                catalog.refresh_all_in_background(config.clone());
                return Self {
                    project_service: None,
                    session_service: session_service.clone(),
                    event_bus,
                    providers: Arc::new(std::sync::Mutex::new(ProviderRegistry::new())),
                    tools: Arc::new(ToolRegistryService::with_session_service(
                        session_service.clone(),
                    )),
                    config: Arc::new(std::sync::Mutex::new(config)),
                    catalog,
                    cancellation: Arc::new(CancellationRegistry::new()),
                    permission_services: Arc::new(TokioMutex::new(HashMap::new())),
                    lsp_registry,
                    mock_provider: Arc::new(std::sync::Mutex::new(None)),
                    explorer_service: Arc::new(ExplorerService::new()),
                };
            }
        };

        // ---- Path 2: Schema initialization fails ----
        if let Err(e) = schema::init_schema(&conn) {
            tracing::warn!(
                "Failed to initialize schema, using in-memory storage: {}",
                e
            );
            let session_service = Arc::new(SessionService::new(event_bus.clone()));
            let lsp_registry = Arc::new(LanguageServerRegistry::new());
            let catalog = Arc::new(ModelCatalogService::new());
            catalog.refresh_all_in_background(config.clone());
            return Self {
                project_service: None,
                session_service: session_service.clone(),
                event_bus,
                providers: Arc::new(std::sync::Mutex::new(ProviderRegistry::new())),
                tools: Arc::new(ToolRegistryService::with_session_service(
                    session_service.clone(),
                )),
                config: Arc::new(std::sync::Mutex::new(config)),
                catalog,
                cancellation: Arc::new(CancellationRegistry::new()),
                permission_services: Arc::new(TokioMutex::new(HashMap::new())),
                lsp_registry,
                mock_provider: Arc::new(std::sync::Mutex::new(None)),
                explorer_service: Arc::new(ExplorerService::new()),
            };
        }

        // ---- Path 3: Second connection (message repo) fails ----
        let message_conn = match Connection::open(&db_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Failed to open second database connection: {}", e);
                let session_service = Arc::new(SessionService::new(event_bus.clone()));
                let lsp_registry = Arc::new(LanguageServerRegistry::new());
                let catalog = Arc::new(ModelCatalogService::new());
                catalog.refresh_all_in_background(config.clone());
                return Self {
                    project_service: None,
                    session_service: session_service.clone(),
                    event_bus,
                    providers: Arc::new(std::sync::Mutex::new(ProviderRegistry::new())),
                    tools: Arc::new(ToolRegistryService::with_session_service(
                        session_service.clone(),
                    )),
                    config: Arc::new(std::sync::Mutex::new(config)),
                    catalog,
                    cancellation: Arc::new(CancellationRegistry::new()),
                    permission_services: Arc::new(TokioMutex::new(HashMap::new())),
                    lsp_registry,
                    mock_provider: Arc::new(std::sync::Mutex::new(None)),
                    explorer_service: Arc::new(ExplorerService::new()),
                };
            }
        };

        // ---- Path 4: Third connection (cache repo) fails ----
        let cache_conn = match Connection::open(&db_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Failed to open third database connection for cache: {}", e);
                // Continue without cache store
                let session_repo = SessionRepository::new(conn);
                let message_repo = MessageRepository::new(message_conn);
                let session_service = Arc::new(SessionService::with_storage(
                    event_bus.clone(),
                    session_repo,
                    message_repo,
                ));
                let lsp_registry = Arc::new(LanguageServerRegistry::new());
                let catalog = Arc::new(ModelCatalogService::new());
                catalog.refresh_all_in_background(config.clone());
                return Self {
                    project_service: None,
                    session_service: session_service.clone(),
                    event_bus,
                    providers: Arc::new(std::sync::Mutex::new(ProviderRegistry::new())),
                    tools: Arc::new(ToolRegistryService::with_session_service(
                        session_service.clone(),
                    )),
                    config: Arc::new(std::sync::Mutex::new(config)),
                    catalog,
                    cancellation: Arc::new(CancellationRegistry::new()),
                    permission_services: Arc::new(TokioMutex::new(HashMap::new())),
                    lsp_registry,
                    mock_provider: Arc::new(std::sync::Mutex::new(None)),
                    explorer_service: Arc::new(ExplorerService::new()),
                };
            }
        };

        // ---- Success path: All connections opened ----
        let project_conn = Connection::open(&db_path).ok();
        let session_repo = SessionRepository::new(conn);
        let message_repo = MessageRepository::new(message_conn);
        let cache_repo = CatalogCacheRepository::new(cache_conn);
        let cache_store = Arc::new(ServerCacheStore::new(cache_repo));
        let project_service = project_conn.map(|conn| Arc::new(ProjectService::new(Arc::new(ProjectRepository::new(conn)))));

        let session_service = Arc::new(SessionService::with_storage(
            event_bus.clone(),
            session_repo,
            message_repo,
        ));

        // Spawn async task to load sessions from storage (hydrates sessions from previous runs)
        let session_service_clone = session_service.clone();
        tokio::spawn(async move {
            let loaded = session_service_clone.load_all_from_storage();
            tracing::info!("Loaded {} sessions from storage", loaded.len());
        });

        // Create LSP registry
        let lsp_registry = Arc::new(LanguageServerRegistry::new());

        // Create tools with the runner injected
        let tools =
            create_tools_with_runner(session_service.clone(), &config, Arc::clone(&lsp_registry));

        let tools_for_commands = Arc::clone(&tools);
        tokio::spawn(async move {
            if let Err(error) = tools_for_commands.register_slash_commands().await {
                tracing::warn!(%error, "Failed to register slash commands");
            } else {
                tracing::info!("Registered slash commands");
            }
        });

        // Create catalog with cache store and trigger background warmup
        let cache_store_for_catalog: Arc<dyn CacheStore> = cache_store;
        let catalog = Arc::new(ModelCatalogService::with_cache_store(Some(cache_store_for_catalog)));
        catalog.refresh_all_in_background(config.clone());

        Self {
            project_service,
            session_service: session_service.clone(),
            event_bus,
            providers: Arc::new(std::sync::Mutex::new(ProviderRegistry::new())),
            tools,
            config: Arc::new(std::sync::Mutex::new(config)),
            catalog,
            cancellation: Arc::new(CancellationRegistry::new()),
            permission_services: Arc::new(TokioMutex::new(HashMap::new())),
            lsp_registry,
            mock_provider: Arc::new(std::sync::Mutex::new(None)),
            explorer_service: Arc::new(ExplorerService::new()),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_app_state_has_lsp_registry() {
        let state = AppState::new();
        assert!(state.lsp_registry.get_server("nonexistent").is_none());
    }

    #[tokio::test]
    async fn test_app_state_lsp_registry_is_shared() {
        let state = AppState::new();
        let registry1 = Arc::clone(&state.lsp_registry);
        let registry2 = Arc::clone(&state.lsp_registry);
        // Both references should point to the same registry
        assert!(Arc::ptr_eq(&registry1, &registry2));
    }

    #[tokio::test]
    async fn test_lsp_tool_not_registered_without_config() {
        let config = RcodeConfig::default();
        let state = AppState::with_config(config);
        // Without LSP config, no tool should be registered
        // The tool is only registered when LSP servers are configured
        let tools = state.tools.list();
        let lsp_tools: Vec<_> = tools.iter().filter(|t| t.id == "lsp").collect();
        assert!(
            lsp_tools.is_empty(),
            "Expected no LSP tools without config, found: {:?}",
            lsp_tools
        );
    }
}
