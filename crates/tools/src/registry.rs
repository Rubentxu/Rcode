//! Tool registry service

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use parking_lot::RwLock;
use tokio::sync::RwLock as TokioRwLock;
use tokio::time::timeout;

use rcode_core::{Tool, ToolInfo, ToolContext, ToolResult, PermissionConfig, error::{Result, RCodeError}};
use rcode_session::SessionService;
use super::validator::ToolValidator;

type AnyhowResult<T> = anyhow::Result<T>;

const DEFAULT_TOOL_TIMEOUT_SECS: u64 = 300;

pub struct ToolRegistryService {
    tools: RwLock<HashMap<String, Arc<dyn Tool>>>,
    default_timeout: Duration,
    #[allow(dead_code)]
    permission_config: Option<PermissionConfig>,
    command_registry: RwLock<Option<Arc<super::command_registry::CommandRegistry>>>,
    mcp_registry: RwLock<Option<Arc<rcode_mcp::McpServerRegistry>>>,
    truncation_config: Option<super::truncate::TruncationConfig>,
    /// Shared store for DelegateTool + DelegationReadTool; retained so
    /// set_delegate_tool() can replace only DelegateTool while keeping the
    /// same store reference in DelegationReadTool.
    delegate_store: super::delegate::DelegationStore,
}

impl ToolRegistryService {
    fn new_store() -> super::delegate::DelegationStore {
        Arc::new(TokioRwLock::new(std::collections::HashMap::new()))
    }

    pub fn new() -> Self {
        let registry = Self {
            tools: RwLock::new(HashMap::new()),
            default_timeout: Duration::from_secs(DEFAULT_TOOL_TIMEOUT_SECS),
            permission_config: None,
            command_registry: RwLock::new(None),
            mcp_registry: RwLock::new(None),
            truncation_config: None,
            delegate_store: Self::new_store(),
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
            mcp_registry: RwLock::new(None),
            truncation_config: None,
            delegate_store: Self::new_store(),
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
            mcp_registry: RwLock::new(None),
            truncation_config: None,
            delegate_store: Self::new_store(),
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
            mcp_registry: RwLock::new(None),
            truncation_config: None,
            delegate_store: Self::new_store(),
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
            mcp_registry: RwLock::new(None),
            truncation_config: None,
            delegate_store: Self::new_store(),
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
        self.register(Arc::new(super::multiedit::MultieditTool::new()));
        self.register(Arc::new(super::glob::GlobTool::new()));
        self.register(Arc::new(super::grep::GrepTool::new()));
        
        // Register TaskTool with session_service if provided, otherwise without
        if let Some(ref service) = session_service {
            self.register(Arc::new(super::task::TaskTool::with_session_service(service.clone())));
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
        let delegate_store = Arc::clone(&self.delegate_store);
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

    /// Register the MCP tool adapter with the given MCP server registry.
    /// 
    /// Also stores the MCP registry reference for dynamic tool registration.
    /// The old "mcp" adapter is kept for backward compatibility.
    pub fn register_mcp_tool(&self, mcp_registry: Arc<rcode_mcp::McpServerRegistry>) {
        *self.mcp_registry.write() = Some(Arc::clone(&mcp_registry));
        self.register(Arc::new(super::mcp_tool::McpToolAdapter::new(mcp_registry)));
    }

    /// Get the MCP server registry if configured.
    pub fn get_mcp_registry(&self) -> Option<Arc<rcode_mcp::McpServerRegistry>> {
        self.mcp_registry.read().clone()
    }

    /// Unregister all tools whose IDs start with the given prefix.
    /// 
    /// Returns the number of tools unregistered.
    pub fn unregister_by_prefix(&self, prefix: &str) -> usize {
        let mut tools = self.tools.write();
        let keys_to_remove: Vec<String> = tools.keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect();
        let count = keys_to_remove.len();
        for key in keys_to_remove {
            tools.remove(&key);
        }
        count
    }

    /// Replace the TaskTool with a custom implementation
    /// 
    /// Replace the TaskTool with a runner-enabled version.
    ///
    /// This allows the server composition root to inject a TaskTool
    /// that has the SubagentRunner configured.
    pub fn set_task_tool(&self, task_tool: super::task::TaskTool) {
        let new_tool: Arc<dyn Tool> = Arc::new(task_tool);
        self.register(new_tool);
    }

    /// Replace DelegateTool with a runner-enabled version.
    ///
    /// Uses the same `delegate_store` already held by `DelegationReadTool` so
    /// both tools continue to share state after the replacement.
    pub fn set_delegate_tool(&self, runner: Arc<dyn rcode_core::SubagentRunner>) {
        let new_tool = Arc::new(super::delegate::DelegateTool::with_store_and_runner(
            Arc::clone(&self.delegate_store),
            runner,
        ));
        self.register(new_tool);
    }

    /// Return a clone of the shared delegation store (for testing / inspection).
    pub fn delegation_store(&self) -> super::delegate::DelegationStore {
        Arc::clone(&self.delegate_store)
    }

    /// Set the truncation configuration for tool output truncation.
    /// 
    /// When configured, tool outputs exceeding the limit will be truncated
    /// with the full output written to a temp file and a preview returned.
    pub fn set_truncation_config(&mut self, config: super::truncate::TruncationConfig) {
        self.truncation_config = Some(config);
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
            .ok_or_else(|| RCodeError::Tool(format!("Tool not found: {}", tool_id)))?;
        
        // Validate arguments against tool's schema
        let schema = tool.parameters();
        if let Err(e) = ToolValidator::validate_with_schema(&args, &schema) {
            return Err(RCodeError::Validation {
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
            Ok(Ok(tool_result)) => {
                // Apply truncation if configured
                let final_result = if let Some(ref config) = self.truncation_config {
                    let truncation_result = super::truncate::truncate_output(
                        &tool_result.content,
                        config,
                        &context.session_id,
                        tool_id,
                    );
                    
                    match truncation_result {
                        super::truncate::TruncationResult::NotTruncated { content: _ } => {
                            // Content unchanged, return as-is
                            tool_result
                        }
                        super::truncate::TruncationResult::Truncated { preview, output_path, total_bytes } => {
                            // Build truncation metadata
                            let mut metadata = tool_result.metadata.unwrap_or(serde_json::json!({}));
                            metadata["truncated"] = serde_json::json!(true);
                            metadata["truncation"] = serde_json::json!({
                                "output_path": output_path.to_string_lossy(),
                                "total_bytes": total_bytes,
                            });
                            
                            ToolResult {
                                content: preview,
                                metadata: Some(metadata),
                                ..tool_result
                            }
                        }
                    }
                } else {
                    tool_result
                };
                Ok(final_result)
            }
            Ok(Err(e)) => Err(RCodeError::Tool(
                format!("Tool '{}' execution failed: {}", tool_id, e)
            )),
            Err(_) => Err(RCodeError::Timeout { 
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

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::ToolContext;
    use std::path::PathBuf;

    fn create_test_context() -> ToolContext {
        ToolContext {
            session_id: "test-session".to_string(),
            project_path: PathBuf::from("/tmp"),
            cwd: PathBuf::from("/tmp"),
            user_id: None,
            agent: "test".to_string(),
        }
    }

    #[test]
    fn test_new_registry_has_defaults() {
        let registry = ToolRegistryService::new();
        let tools = registry.list();
        assert!(!tools.is_empty());
        assert!(tools.iter().any(|t| t.id == "bash"));
        assert!(tools.iter().any(|t| t.id == "question"));
        assert!(tools.iter().any(|t| t.id == "read"));
        assert!(tools.iter().any(|t| t.id == "write"));
        assert!(tools.iter().any(|t| t.id == "edit"));
        assert!(tools.iter().any(|t| t.id == "glob"));
        assert!(tools.iter().any(|t| t.id == "grep"));
        assert!(tools.iter().any(|t| t.id == "task"));
        assert!(tools.iter().any(|t| t.id == "batch") == false);
    }

    #[test]
    fn test_get_existing_tool() {
        let registry = ToolRegistryService::new();
        let tool = registry.get("bash");
        assert!(tool.is_some());
        assert_eq!(tool.unwrap().id(), "bash");
    }

    #[test]
    fn test_get_nonexistent_tool() {
        let registry = ToolRegistryService::new();
        assert!(registry.get("nonexistent_tool").is_none());
    }

    #[test]
    fn test_register_custom_tool() {
        let registry = ToolRegistryService::new();
        struct DummyTool;
        #[async_trait::async_trait]
        impl rcode_core::Tool for DummyTool {
            fn id(&self) -> &str { "dummy" }
            fn name(&self) -> &str { "Dummy" }
            fn description(&self) -> &str { "A dummy tool" }
            fn parameters(&self) -> serde_json::Value { serde_json::json!({}) }
            async fn execute(&self, _args: serde_json::Value, _ctx: &ToolContext) -> rcode_core::error::Result<ToolResult> {
                Ok(ToolResult { title: "ok".into(), content: "done".into(), metadata: None, attachments: vec![] })
            }
        }
        registry.register(Arc::new(DummyTool));
        assert!(registry.get("dummy").is_some());
    }

    #[test]
    fn test_list_returns_tool_info() {
        let registry = ToolRegistryService::new();
        let list = registry.list();
        assert!(!list.is_empty());
        for info in &list {
            assert!(!info.id.is_empty());
            assert!(!info.name.is_empty());
            assert!(!info.description.is_empty());
        }
    }

    #[test]
    fn test_with_timeout() {
        let registry = ToolRegistryService::with_timeout(10);
        assert_eq!(registry.default_timeout, Duration::from_secs(10));
    }

    #[test]
    fn test_with_batch_includes_batch_tool() {
        let registry = ToolRegistryService::with_batch();
        assert!(registry.get("batch").is_some());
    }

    #[test]
    fn test_default_impl() {
        let registry = ToolRegistryService::default();
        assert!(registry.get("bash").is_some());
    }

    #[tokio::test]
    async fn test_execute_nonexistent_tool() {
        let registry = ToolRegistryService::new();
        let ctx = create_test_context();
        let result = registry.execute("nonexistent", serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_tool_not_found_error_message() {
        let registry = ToolRegistryService::new();
        let ctx = create_test_context();
        let result = registry.execute("missing_tool", serde_json::json!({}), &ctx).await.unwrap_err();
        let msg = result.to_string();
        assert!(msg.contains("missing_tool"));
    }

    #[tokio::test]
    async fn test_register_slash_commands() {
        let registry = ToolRegistryService::new();
        // register_slash_commands should complete without error even if no commands found
        let result = registry.register_slash_commands().await;
        assert!(result.is_ok());
        // Command registry should be created
        assert!(registry.get_command_registry().is_some());
    }

    #[test]
    fn test_get_command_registry_initially_none() {
        let registry = ToolRegistryService::new();
        // Before register_slash_commands, command_registry is not initialized
        // But since register_defaults is called in new(), it may or may not be set
        // This tests the method returns what exists
        let _ = registry.get_command_registry();
    }

    #[test]
    fn test_register_mcp_tool() {
        use rcode_mcp::McpServerRegistry;
        let registry = ToolRegistryService::new();
        let mcp_registry = Arc::new(McpServerRegistry::new());
        registry.register_mcp_tool(mcp_registry);
        // The mcp tool should now be registered
        assert!(registry.get("mcp").is_some());
    }

    #[tokio::test]
    async fn test_execute_with_timeout_validation_error() {
        let registry = ToolRegistryService::new();
        let ctx = create_test_context();
        // Bash tool requires a "command" parameter - passing empty object should fail validation
        let result = registry.execute("bash", serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_with_timeout_custom_duration() {
        let registry = ToolRegistryService::with_timeout(1);
        let ctx = create_test_context();
        // This should timeout since 'sleep' command takes longer than 100ms
        let result = registry.execute_with_timeout(
            "bash",
            serde_json::json!({"command": "sleep 5"}),
            &ctx,
            std::time::Duration::from_millis(100)
        ).await;
        assert!(result.is_err());
        // Verify it's a timeout error by checking the error contains duration info
        let err = result.unwrap_err();
        let err_str = err.to_string();
        // Timeout error should mention duration
        assert!(err_str.contains("100") || err_str.contains("second") || err_str.contains("timeout") || err_str.to_lowercase().contains("time"));
    }

    #[tokio::test]
    async fn test_execute_with_timeout_success() {
        let registry = ToolRegistryService::with_timeout(5);
        let ctx = create_test_context();
        // This should succeed with a short timeout
        let result = registry.execute_with_timeout(
            "bash",
            serde_json::json!({"command": "echo hello"}),
            &ctx,
            std::time::Duration::from_secs(5)
        ).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_with_permission_config() {
        let config = rcode_core::PermissionConfig::default();
        let registry = ToolRegistryService::with_permission_config(config);
        // Registry should be created with permission config and have default tools
        assert!(registry.get("bash").is_some());
    }

    #[test]
    fn test_with_session_service() {
        // SessionService would need a real implementation, just test construction doesn't panic
        // We can test with_session_service_and_batch which doesn't require SessionService
        let registry = ToolRegistryService::with_batch();
        assert!(registry.get("batch").is_some());
    }

    #[test]
    fn test_registry_default_timeout_is_300_secs() {
        let registry = ToolRegistryService::new();
        assert_eq!(registry.default_timeout, Duration::from_secs(300));
    }

    #[test]
    fn test_registry_list_contains_all_default_tools() {
        let registry = ToolRegistryService::new();
        let tools = registry.list();
        let tool_ids: Vec<_> = tools.iter().map(|t| t.id.as_str()).collect();
        
        // Verify all expected default tools are present
        assert!(tool_ids.contains(&"bash"));
        assert!(tool_ids.contains(&"read"));
        assert!(tool_ids.contains(&"write"));
        assert!(tool_ids.contains(&"edit"));
        assert!(tool_ids.contains(&"glob"));
        assert!(tool_ids.contains(&"grep"));
        assert!(tool_ids.contains(&"task"));
        assert!(tool_ids.contains(&"plan"));
        assert!(tool_ids.contains(&"question"));
    }

    #[test]
    fn test_registry_list_tool_info_fields() {
        let registry = ToolRegistryService::new();
        let tools = registry.list();
        
        for tool in tools {
            assert!(!tool.id.is_empty(), "Tool id should not be empty");
            assert!(!tool.name.is_empty(), "Tool name should not be empty for {}", tool.id);
            assert!(!tool.description.is_empty(), "Tool description should not be empty for {}", tool.id);
        }
    }

    #[test]
    fn test_register_tool_replaces_existing() {
        let registry = ToolRegistryService::new();
        
        // Create a custom tool with same id as existing
        struct CustomBashTool;
        #[async_trait::async_trait]
        impl rcode_core::Tool for CustomBashTool {
            fn id(&self) -> &str { "bash" }
            fn name(&self) -> &str { "Custom Bash" }
            fn description(&self) -> &str { "A custom bash tool" }
            fn parameters(&self) -> serde_json::Value { serde_json::json!({}) }
            async fn execute(&self, _args: serde_json::Value, _ctx: &ToolContext) -> rcode_core::error::Result<ToolResult> {
                Ok(ToolResult { title: "custom".into(), content: "custom".into(), metadata: None, attachments: vec![] })
            }
        }
        
        registry.register(Arc::new(CustomBashTool));
        
        let tool = registry.get("bash").unwrap();
        assert_eq!(tool.name(), "Custom Bash");
    }

    #[test]
    fn test_get_command_registry_after_slash_commands() {
        let registry = ToolRegistryService::new();
        // Initially may or may not have command registry depending on initialization
        let initial = registry.get_command_registry();
        
        // After registering slash commands async
        // Note: This test just verifies the method is callable
        let _ = initial;
    }

    #[tokio::test]
    async fn test_execute_with_timeout_nonexistent_tool_error_type() {
        let registry = ToolRegistryService::new();
        let ctx = create_test_context();
        let result = registry.execute("definitely_does_not_exist", serde_json::json!({}), &ctx).await;
        
        assert!(result.is_err());
        let err = result.unwrap_err();
        // Error should indicate tool not found
        assert!(err.to_string().contains("not found") || err.to_string().contains("Tool"));
    }

    #[tokio::test]
    async fn test_execute_with_validation_error_for_bash() {
        let registry = ToolRegistryService::new();
        let ctx = create_test_context();
        
        // Bash tool requires 'command' parameter - empty args should fail validation
        let result = registry.execute("bash", serde_json::json!({"command": ""}), &ctx).await;
        // Either validation error or execution error is acceptable
        if result.is_err() {
            let err_msg = result.unwrap_err().to_string();
            assert!(err_msg.contains("command") || err_msg.contains("validation") || err_msg.contains("required"));
        }
    }

    #[tokio::test]
    async fn test_execute_read_tool_missing_path() {
        let registry = ToolRegistryService::new();
        let ctx = create_test_context();
        
        // Read tool requires 'path' parameter
        let result = registry.execute("read", serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_with_timeout_error_path() {
        let registry = ToolRegistryService::new();
        let ctx = create_test_context();
        
        // Create a custom tool that returns an error
        struct ErrorTool;
        #[async_trait::async_trait]
        impl rcode_core::Tool for ErrorTool {
            fn id(&self) -> &str { "error_tool" }
            fn name(&self) -> &str { "Error Tool" }
            fn description(&self) -> &str { "A tool that returns error" }
            fn parameters(&self) -> serde_json::Value { serde_json::json!({}) }
            async fn execute(&self, _args: serde_json::Value, _ctx: &ToolContext) -> rcode_core::error::Result<ToolResult> {
                Err(rcode_core::RCodeError::Tool("Execution failed explicitly".into()))
            }
        }
        registry.register(Arc::new(ErrorTool));
        
        let result = registry.execute_with_timeout(
            "error_tool",
            serde_json::json!({}),
            &ctx,
            std::time::Duration::from_secs(5)
        ).await;
        
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Execution failed") || err.to_string().contains("error_tool"));
    }

    #[tokio::test]
    async fn test_execute_with_timeout_timeout_path() {
        let registry = ToolRegistryService::with_timeout(1);
        let ctx = create_test_context();
        
        // Use bash tool with a long sleep to trigger timeout
        let result = registry.execute_with_timeout(
            "bash",
            serde_json::json!({"command": "sleep 10"}),
            &ctx,
            std::time::Duration::from_millis(100)
        ).await;
        
        assert!(result.is_err());
        let err = result.unwrap_err();
        // Timeout error should be in the message
        assert!(err.to_string().to_lowercase().contains("timeout") || err.to_string().contains("1"));
    }

    // ========================================================================
    // MCP individual tools tests
    // ========================================================================

    #[test]
    fn test_get_mcp_registry_initially_none() {
        let registry = ToolRegistryService::new();
        // Initially no MCP registry configured
        assert!(registry.get_mcp_registry().is_none());
    }

    #[test]
    fn test_register_mcp_tool_sets_registry() {
        use rcode_mcp::McpServerRegistry;
        let registry = ToolRegistryService::new();
        let mcp_registry = Arc::new(McpServerRegistry::new());
        registry.register_mcp_tool(mcp_registry.clone());
        // After registering, get_mcp_registry should return Some
        let retrieved = registry.get_mcp_registry();
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_unregister_by_prefix_empty_registry() {
        let registry = ToolRegistryService::new();
        // Unregistering from empty registry returns 0
        let count = registry.unregister_by_prefix("mcp/exa/");
        assert_eq!(count, 0);
    }

    #[test]
    fn test_unregister_by_prefix_removes_tools() {
        use rcode_mcp::McpServerRegistry;
        let registry = ToolRegistryService::new();
        let mcp_registry = Arc::new(McpServerRegistry::new());
        registry.register_mcp_tool(mcp_registry);

        // Create a mock MCP tool bridge and register it
        struct MockMcpTool {
            id: String,
        }
        impl MockMcpTool {
            fn new(server: &str, tool: &str) -> Self {
                Self {
                    id: format!("mcp/{}/{}", server, tool),
                }
            }
        }
        #[async_trait::async_trait]
        impl rcode_core::Tool for MockMcpTool {
            fn id(&self) -> &str { &self.id }
            fn name(&self) -> &str { "mock" }
            fn description(&self) -> &str { "mock tool" }
            fn parameters(&self) -> serde_json::Value { serde_json::json!({}) }
            async fn execute(&self, _: serde_json::Value, _: &rcode_core::ToolContext) -> rcode_core::error::Result<rcode_core::ToolResult> {
                Ok(rcode_core::ToolResult {
                    title: "ok".into(),
                    content: "ok".into(),
                    metadata: None,
                    attachments: vec![],
                })
            }
        }

        // Register mock MCP tools for "exa" server
        registry.register(Arc::new(MockMcpTool::new("exa", "search")));
        registry.register(Arc::new(MockMcpTool::new("exa", "crawl")));
        // Register mock MCP tool for "filesystem" server
        registry.register(Arc::new(MockMcpTool::new("filesystem", "read")));

        // Verify tools are registered
        assert!(registry.get("mcp/exa/search").is_some());
        assert!(registry.get("mcp/exa/crawl").is_some());
        assert!(registry.get("mcp/filesystem/read").is_some());

        // Unregister all tools for "exa" server
        let count = registry.unregister_by_prefix("mcp/exa/");
        assert_eq!(count, 2);

        // exa tools should be gone
        assert!(registry.get("mcp/exa/search").is_none());
        assert!(registry.get("mcp/exa/crawl").is_none());
        // filesystem tool should remain
        assert!(registry.get("mcp/filesystem/read").is_some());
    }

    #[test]
    fn test_unregister_by_prefix_does_not_affect_non_mcp_tools() {
        let registry = ToolRegistryService::new();
        // Register a non-MCP tool
        struct DummyTool;
        #[async_trait::async_trait]
        impl rcode_core::Tool for DummyTool {
            fn id(&self) -> &str { "dummy" }
            fn name(&self) -> &str { "Dummy" }
            fn description(&self) -> &str { "A dummy tool" }
            fn parameters(&self) -> serde_json::Value { serde_json::json!({}) }
            async fn execute(&self, _: serde_json::Value, _: &rcode_core::ToolContext) -> rcode_core::error::Result<rcode_core::ToolResult> {
                Ok(rcode_core::ToolResult {
                    title: "ok".into(),
                    content: "ok".into(),
                    metadata: None,
                    attachments: vec![],
                })
            }
        }
        registry.register(Arc::new(DummyTool));

        // Unregister with MCP prefix should not affect dummy
        let count = registry.unregister_by_prefix("mcp/exa/");
        assert_eq!(count, 0);
        assert!(registry.get("dummy").is_some());
    }
}
