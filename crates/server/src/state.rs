//! Application state

use std::collections::HashMap;
use std::sync::Arc;

use rcode_agent::hooks::HookRegistry;
use rcode_agent::permissions::InteractivePermissionService;
use rcode_core::{AgentRegistry, RcodeConfig, SubagentRunner};
use rcode_event::EventBus;
use rcode_privacy::service::PrivacyService;
use rcode_providers::catalog::ModelCatalogService;
use rcode_providers::CacheStore;
use rcode_providers::{LlmProvider, ProviderFactory, ProviderRegistry};
use rcode_session::{ProjectService, SessionService};
use rcode_storage::{schema, catalog_cache::CatalogCacheRepository, MessageRepository, ProjectRepository, SessionRepository};
use rcode_tools::ToolRegistryService;
use rusqlite::Connection;
use tokio::sync::Mutex as TokioMutex;

use crate::cache_store_impl::ServerCacheStore;
use crate::cancellation::CancellationRegistry;
use crate::explorer::ExplorerService;
use crate::project_health::ProjectHealthRegistry;
use crate::subagent_runner_impl::ServerSubagentRunner;

/// Adapter to wrap rcode_providers::LlmProvider and expose it as rcode_core::LlmProvider.
/// This allows using production providers (via ProviderFactory) with LlmDetector
/// which expects rcode_core::LlmProvider.
struct PrivacyProviderAdapter {
    inner: Arc<dyn rcode_providers::LlmProvider>,
}

impl PrivacyProviderAdapter {
    fn new(inner: Arc<dyn rcode_providers::LlmProvider>) -> Self {
        Self { inner }
    }
}

#[async_trait::async_trait]
impl rcode_core::LlmProvider for PrivacyProviderAdapter {
    async fn complete(&self, req: rcode_core::CompletionRequest) -> rcode_core::error::Result<rcode_core::CompletionResponse> {
        self.inner.complete(req).await
    }

    async fn stream(&self, req: rcode_core::CompletionRequest) -> rcode_core::error::Result<rcode_core::StreamingResponse> {
        self.inner.stream(req).await
    }

    fn model_info(&self, model_id: &str) -> Option<rcode_core::ModelInfo> {
        self.inner.model_info(model_id)
    }
}

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
    /// Optional mock provider for testing (injected via TestApp)
    pub mock_provider: Arc<std::sync::Mutex<Option<Arc<dyn LlmProvider>>>>,
    /// Explorer service for workspace file tree
    pub explorer_service: Arc<ExplorerService>,
    /// Privacy service for sensitive data sanitization
    pub privacy: PrivacyService,
    /// Project health registry (cargo check results)
    pub project_health: Arc<ProjectHealthRegistry>,
    /// CogniCode service handle — kept alive so Drop fires on shutdown (aborting background tasks)
    pub cognicode_service: Arc<std::sync::Mutex<Option<rcode_cognicode::service::CogniCodeService>>>,
    /// Hook registry for extending agent behavior at trigger points
    pub hooks: Arc<HookRegistry>,
    /// Agent registry for worker agents (explore, implement, test, verify, research)
    pub agent_registry: Arc<AgentRegistry>,
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
        rcode_tools::task::TaskTool::with_session_service(session_service).with_runner(Arc::clone(&runner));
    tools.set_task_tool(task_tool);

    // Inject the same runner into DelegateTool for real async agent spawning
    tools.set_delegate_tool(runner);

    tools
}

/// Build a [`PrivacyService`] from the config.
///
/// Privacy is disabled by default (passthrough) to preserve existing behavior.
fn build_privacy_service(config: &RcodeConfig, _event_bus: &Arc<EventBus>) -> PrivacyService {
    // Try to extract privacy config from RcodeConfig.extra
    // If not present, use defaults (privacy disabled)
    let privacy_config = extract_privacy_config(config);

    let mut builder = rcode_privacy::service::PrivacyService::builder()
        .with_config(privacy_config.clone())
        .with_emitter(rcode_privacy::events::EventEmitter::noop());

    // Wire LLM detector based on detector_model
    match &privacy_config.detector_model {
        rcode_privacy::detector::DetectorModel::LocalOllama { .. } => {
            let detector_config = rcode_privacy::detector::LlmDetectorConfig {
                model: privacy_config.detector_model.clone(),
                trigger_mode: privacy_config.trigger_mode,
                auto_tokenize_threshold: privacy_config.auto_tokenize_threshold as f32,
                proposal_threshold: privacy_config.proposal_threshold as f32,
                max_proposals_per_session: privacy_config.max_proposals_per_session,
            };
            builder = builder.with_llm_detector(Arc::new(rcode_privacy::detector::LlmDetector::new(detector_config)));
        }
        rcode_privacy::detector::DetectorModel::ConfiguredProvider { model } => {
            if let Ok((provider, _)) = ProviderFactory::build(model, Some(config)) {
                let detector_config = rcode_privacy::detector::LlmDetectorConfig {
                    model: privacy_config.detector_model.clone(),
                    trigger_mode: privacy_config.trigger_mode,
                    auto_tokenize_threshold: privacy_config.auto_tokenize_threshold as f32,
                    proposal_threshold: privacy_config.proposal_threshold as f32,
                    max_proposals_per_session: privacy_config.max_proposals_per_session,
                };
                let adapter = Arc::new(PrivacyProviderAdapter::new(provider)) as Arc<dyn rcode_core::LlmProvider>;
                builder = builder.with_llm_detector(Arc::new(rcode_privacy::detector::LlmDetector::with_provider(detector_config, adapter)));
            }
        }
        rcode_privacy::detector::DetectorModel::Disabled => {}
    }

    builder.build()
}

/// Extract [`rcode_privacy::service::PrivacyConfig`] from the RCode overlay config.
///
/// Uses a Kustomize-like configuration model:
/// - **Base**: standard opencode config (`~/.config/opencode/opencode.json`) — never modified
/// - **Overlay**: RCode-specific config (`~/.config/rcode/config.json`) — extends/overrides base
///
/// The overlay file contains only RCode-specific extensions like `privacy`:
/// ```json
/// {
///   "privacy": { "enabled": true, "security_level": "Strict" }
/// }
/// ```
///
/// Standard opencode fields (model, providers, lsp, etc.) can also be overridden
/// in the overlay, keeping the base opencode config untouched.
fn extract_privacy_config(_config: &RcodeConfig) -> rcode_privacy::service::PrivacyConfig {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let rcode_config_path = std::path::PathBuf::from(home)
        .join(".config")
        .join("rcode")
        .join("config.json");

    if !rcode_config_path.exists() {
        return rcode_privacy::service::PrivacyConfig::default();
    }

    let content = match std::fs::read_to_string(&rcode_config_path) {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!(path = %rcode_config_path.display(), error = %e, "rcode overlay config not readable");
            return rcode_privacy::service::PrivacyConfig::default();
        }
    };

    let full: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(path = %rcode_config_path.display(), error = %e, "failed to parse rcode overlay config");
            return rcode_privacy::service::PrivacyConfig::default();
        }
    };

    let privacy_json = match full.get("privacy") {
        Some(v) => v,
        None => return rcode_privacy::service::PrivacyConfig::default(),
    };

    match serde_json::from_value(privacy_json.clone()) {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::warn!(error = %e, "failed to parse privacy config from rcode overlay, using defaults");
            rcode_privacy::service::PrivacyConfig::default()
        }
    }
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

        // Build privacy service with config (passthrough by default)
        let privacy = build_privacy_service(&config, &event_bus);

        // Initialize agent registry with worker agents
        let agent_registry = Arc::new(AgentRegistry::new());

        let db_path = create_storage_path();
        tracing::info!("Using database at: {:?}", db_path);

        // ---- Path 1: Database initialization fails ----
        let conn = match Connection::open(&db_path) {
            Ok(conn) => conn,
            Err(e) => {
                tracing::warn!("Failed to open database, using in-memory storage: {}", e);
                let session_service = Arc::new(SessionService::new(event_bus.clone()));
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
                    mock_provider: Arc::new(std::sync::Mutex::new(None)),
                    explorer_service: Arc::new(ExplorerService::new()),
                    privacy,
                    project_health: Arc::new(ProjectHealthRegistry::new()),
                    cognicode_service: Arc::new(std::sync::Mutex::new(None)),
                    hooks: Arc::new(HookRegistry::new()),
                    agent_registry,
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
                mock_provider: Arc::new(std::sync::Mutex::new(None)),
                explorer_service: Arc::new(ExplorerService::new()),
                privacy,
                project_health: Arc::new(ProjectHealthRegistry::new()),
                cognicode_service: Arc::new(std::sync::Mutex::new(None)),
                hooks: Arc::new(HookRegistry::new()),
                agent_registry,
            };
        }

        // ---- Path 3: Second connection (message repo) fails ----
        let message_conn = match Connection::open(&db_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Failed to open second database connection: {}", e);
                let session_service = Arc::new(SessionService::new(event_bus.clone()));
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
                    mock_provider: Arc::new(std::sync::Mutex::new(None)),
                    explorer_service: Arc::new(ExplorerService::new()),
                    privacy,
                    project_health: Arc::new(ProjectHealthRegistry::new()),
                    cognicode_service: Arc::new(std::sync::Mutex::new(None)),
                    hooks: Arc::new(HookRegistry::new()),
                    agent_registry,
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
                    mock_provider: Arc::new(std::sync::Mutex::new(None)),
                    explorer_service: Arc::new(ExplorerService::new()),
                    privacy,
                    project_health: Arc::new(ProjectHealthRegistry::new()),
                    cognicode_service: Arc::new(std::sync::Mutex::new(None)),
                    hooks: Arc::new(HookRegistry::new()),
                    agent_registry,
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

        // Spawn async task to load worker agents from disk (~/.config/rcode/agents/, etc.)
        let agent_registry_clone = agent_registry.clone();
        tokio::spawn(async move {
            match agent_registry_clone.load_all().await {
                Ok(()) => tracing::info!("Worker agents loaded from disk"),
                Err(e) => tracing::warn!("Failed to load worker agents: {}", e),
            }
        });

        // Create tools with the runner injected
        let tools =
            create_tools_with_runner(session_service.clone(), &config);

        // Register CogniCode intelligence tools (graceful: skip if unavailable)
        // Spawn in background to avoid blocking the sync constructor.
        let cognicode_service: Arc<std::sync::Mutex<Option<rcode_cognicode::service::CogniCodeService>>> =
            Arc::new(std::sync::Mutex::new(None));
        let tools_clone = Arc::clone(&tools);
        let service_holder = cognicode_service.clone();
        tokio::spawn(async move {
            let cwd = std::env::current_dir().unwrap_or_default();
            match rcode_cognicode::service::CogniCodeService::spawn(&cwd).await {
                Ok(service) => {
                    // Register tools
                    let session = service.session().clone();
                    rcode_cognicode::tools::register_all(&tools_clone, session);
                    tracing::info!("CogniCode service spawned and tools registered");
                    
                    // Store service so it stays alive — Drop will fire when AppState drops,
                    // which sends shutdown signal and aborts background tasks.
                    *service_holder.lock().unwrap() = Some(service);
                }
                Err(e) => {
                    tracing::warn!("CogniCode service unavailable (agent will work normally): {}", e);
                }
            }
        });

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
            mock_provider: Arc::new(std::sync::Mutex::new(None)),
            explorer_service: Arc::new(ExplorerService::new()),
            privacy,
            project_health: Arc::new(ProjectHealthRegistry::new()),
            cognicode_service,
            hooks: Arc::new(HookRegistry::new()),
            agent_registry,
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
    #[allow(unused_imports)]
    use super::*;

    // LSP-related tests removed as rcode-lsp dependency was eliminated
}
