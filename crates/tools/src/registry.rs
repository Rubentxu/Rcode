//! Tool registry service

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use parking_lot::RwLock;
use tokio::sync::RwLock as TokioRwLock;
use tokio::time::timeout;

use rcode_core::{Tool, ToolInfo, ToolContext, ToolResult, PermissionConfig, error::{Result, OpenCodeError}};
use rcode_session::SessionService;
use super::validator::ToolValidator;

type AnyhowResult<T> = anyhow::Result<T>;

const DEFAULT_TOOL_TIMEOUT_SECS: u64 = 300;

pub struct ToolRegistryService {
    tools: RwLock<HashMap<String, Arc<dyn Tool>>>,
    default_timeout: Duration,
    permission_config: Option<PermissionConfig>,
    command_registry: RwLock<Option<Arc<super::command_registry::CommandRegistry>>>,
}

impl ToolRegistryService {
    pub fn new() -> Self {
        let registry = Self {
            tools: RwLock::new(HashMap::new()),
            default_timeout: Duration::from_secs(DEFAULT_TOOL_TIMEOUT_SECS),
            permission_config: None,
            command_registry: RwLock::new(None),
        };
        registry.register_defaults(None);
        registry
    }

    /// Create a registry with session service for session navigation tool
    pub fn with_session_service(session_service: Arc<SessionService>) -> Self {
        let registry = Self {
            tools: RwLock::new(HashMap::new()),
            default_timeout: Duration::from_secs(DEFAULT_TOOL_TIMEOUT_SECS),
            permission_config: None,
            command_registry: RwLock::new(None),
        };
        registry.register_defaults(Some(session_service));
        registry
    }

    pub fn with_timeout(timeout_secs: u64) -> Self {
        let registry = Self {
            tools: RwLock::new(HashMap::new()),
            default_timeout: Duration::from_secs(timeout_secs),
            permission_config: None,
            command_registry: RwLock::new(None),
        };
        registry.register_defaults(None);
        registry
    }
    
    /// Create a registry with permission configuration for TaskTool
    pub fn with_permission_config(permission_config: PermissionConfig) -> Self {
        let registry = Self {
            tools: RwLock::new(HashMap::new()),
            default_timeout: Duration::from_secs(DEFAULT_TOOL_TIMEOUT_SECS),
            permission_config: Some(permission_config),
            command_registry: RwLock::new(None),
        };
        registry.register_defaults(None);
        registry
    }

    /// Create a registry with the batch tool registered
    pub fn with_batch() -> Arc<Self> {
        let registry = Arc::new(Self::new());
        registry.register(Arc::new(super::batch::BatchTool::new(Arc::clone(&registry))));
        registry
    }

    /// Create a registry with session service and batch tool
    pub fn with_session_service_and_batch(session_service: Arc<SessionService>) -> Arc<Self> {
        let registry = Arc::new(Self {
            tools: RwLock::new(HashMap::new()),
            default_timeout: Duration::from_secs(DEFAULT_TOOL_TIMEOUT_SECS),
            permission_config: None,
            command_registry: RwLock::new(None),
        });
        registry.register_defaults(Some(session_service));
        registry.register(Arc::new(super::batch::BatchTool::new(Arc::clone(&registry))));
        registry
    }
    
    fn register_defaults(&self, session_service: Option<Arc<SessionService>>) {
        self.register(Arc::new(super::bash::BashTool::new()));
        self.register(Arc::new(super::question::QuestionTool::new()));
        self.register(Arc::new(super::read::ReadTool::new()));
        self.register(Arc::new(super::write::WriteTool::new()));
        self.register(Arc::new(super::edit::EditTool::new()));
        self.register(Arc::new(super::glob::GlobTool::new()));
        self.register(Arc::new(super::grep::GrepTool::new()));
        
        // Register TaskTool with permission config if provided
        if let Some(ref config) = self.permission_config {
            self.register(Arc::new(super::task::TaskTool::with_permission_config(config.clone())));
        } else {
            self.register(Arc::new(super::task::TaskTool::new()));
        }
        
        self.register(Arc::new(super::plan::PlanTool::new()));
        self.register(Arc::new(super::plan_exit::PlanExitTool::new()));
        self.register(Arc::new(super::todowrite::TodowriteTool::new()));
        
        // Register skill_tool with proper discovery and registry
        let skill_discovery = Arc::new(super::skill_discovery::SkillDiscovery::new());
        let skill_registry = Arc::new(super::skill_registry::SkillRegistry::new(skill_discovery));
        self.register(Arc::new(super::skill_tool::SkillTool::new(skill_registry)));
        
        self.register(Arc::new(super::webfetch::WebfetchTool::new()));
        self.register(Arc::new(super::websearch::WebsearchTool::new()));
        self.register(Arc::new(super::codesearch::CodesearchTool::new()));
        self.register(Arc::new(super::applypatch::ApplypatchTool::new()));

        // Register session navigation tool if session service is provided
        if let Some(service) = session_service {
            self.register(Arc::new(super::session_navigation::SessionNavigationTool::new(service)));
        }

        // Register delegate tools with shared store
        let delegate_store = Arc::new(TokioRwLock::new(std::collections::HashMap::new()));
        self.register(Arc::new(super::delegate::DelegateTool::with_store(delegate_store.clone())));
        self.register(Arc::new(super::delegate::DelegationReadTool::new(delegate_store)));
    }
    
    pub fn register(&self, tool: Arc<dyn Tool>) {
        self.tools.write().insert(tool.id().to_string(), tool);
    }
    
    pub fn get(&self, id: &str) -> Option<Arc<dyn Tool>> {
        self.tools.read().get(id).cloned()
    }
    
    pub fn list(&self) -> Vec<ToolInfo> {
        self.tools.read()
            .values()
            .map(|t| ToolInfo {
                id: t.id().to_string(),
                name: t.name().to_string(),
                description: t.description().to_string(),
            })
            .collect()
    }

    /// Initialize and register the slash command tool with discovered commands
    pub async fn register_slash_commands(&self) -> AnyhowResult<()> {
        // Create command registry if not already created
        let registry = {
            let mut guard = self.command_registry.write();
            if guard.is_none() {
                *guard = Some(Arc::new(super::command_registry::CommandRegistry::new()));
            }
            guard.as_ref().unwrap().clone()
        };

        // Discover commands
        let discovery = super::command_discovery::CommandDiscovery::new();
        let commands = discovery.discover_commands().await?;

        // Register discovered commands
        for cmd in commands {
            registry.register(cmd);
        }

        // Create and register the slash command tool
        let tool = Arc::new(super::slash_command_tool::SlashCommandTool::new(registry));
        self.register(tool);

        Ok(())
    }

    /// Get a clone of the command registry if it exists
    pub fn get_command_registry(&self) -> Option<Arc<super::command_registry::CommandRegistry>> {
        self.command_registry.read().clone()
    }

    /// Register the MCP tool adapter with the given MCP server registry
    pub fn register_mcp_tool(&self, mcp_registry: Arc<rcode_mcp::McpServerRegistry>) {
        self.register(Arc::new(super::mcp_tool::McpToolAdapter::new(mcp_registry)));
    }
    
    pub async fn execute(
        &self,
        tool_id: &str,
        args: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        self.execute_with_timeout(tool_id, args, context, self.default_timeout).await
    }

    pub async fn execute_with_timeout(
        &self,
        tool_id: &str,
        args: serde_json::Value,
        context: &ToolContext,
        timeout_duration: Duration,
    ) -> Result<ToolResult> {
        let tool = self.get(tool_id)
            .ok_or_else(|| OpenCodeError::Tool(format!("Tool not found: {}", tool_id)))?;
        
        // Validate arguments against tool's schema
        let schema = tool.parameters();
        if let Err(e) = ToolValidator::validate_with_schema(&args, &schema) {
            return Err(OpenCodeError::Validation {
                field: String::new(),
                message: format!("Tool '{}': {}", tool_id, e),
            });
        }
        
        // Execute with timeout
        let result = timeout(
            timeout_duration,
            tool.execute(args, context)
        ).await;
        
        match result {
            Ok(Ok(tool_result)) => Ok(tool_result),
            Ok(Err(e)) => Err(OpenCodeError::Tool(
                format!("Tool '{}' execution failed: {}", tool_id, e)
            )),
            Err(_) => Err(OpenCodeError::Timeout { 
                duration: timeout_duration.as_secs() 
            }),
        }
    }
}

impl Default for ToolRegistryService {
    fn default() -> Self {
        Self::new()
    }
}
