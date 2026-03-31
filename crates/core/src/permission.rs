//! Permission system for agent and tool access control

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Permission levels for agent/tool access
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    /// Allow the action without prompting
    Allow,
    /// Deny the action unconditionally
    Deny,
    /// Prompt the user for confirmation before proceeding
    Ask,
}

impl Default for Permission {
    fn default() -> Self {
        Permission::Ask
    }
}

/// Configuration for a specific agent type's permissions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPermission {
    /// Type of agent (e.g., "general", "explore", "refactor", "debug")
    pub agent_type: String,
    /// Default permission for this agent type
    pub permission: Permission,
    /// List of tool IDs that are allowed for this agent type
    /// Empty list means all tools are subject to the default permission
    pub tools_allowed: Vec<String>,
    /// List of tool IDs explicitly denied for this agent type
    pub tools_denied: Vec<String>,
}

impl AgentPermission {
    pub fn new(agent_type: impl Into<String>) -> Self {
        Self {
            agent_type: agent_type.into(),
            permission: Permission::Ask,
            tools_allowed: Vec::new(),
            tools_denied: Vec::new(),
        }
    }

    pub fn with_permission(mut self, permission: Permission) -> Self {
        self.permission = permission;
        self
    }

    pub fn allow_tools(mut self, tools: Vec<String>) -> Self {
        self.tools_allowed = tools;
        self
    }

    pub fn deny_tools(mut self, tools: Vec<String>) -> Self {
        self.tools_denied = tools;
        self
    }

    /// Check if a specific tool is allowed for this agent permission
    pub fn is_tool_allowed(&self, tool_id: &str) -> bool {
        // Check if tool is explicitly denied
        if self.tools_denied.iter().any(|t| t == tool_id) {
            return false;
        }

        // If allowed list is empty, all non-denied tools follow default permission
        if self.tools_allowed.is_empty() {
            return self.permission == Permission::Allow;
        }

        // Check if tool is in allowed list
        self.tools_allowed.iter().any(|t| t == tool_id)
    }

    /// Check if this agent type should prompt for a specific tool
    pub fn should_ask_for_tool(&self, tool_id: &str) -> bool {
        // If denied, never ask
        if self.tools_denied.iter().any(|t| t == tool_id) {
            return false;
        }

        // If allowed list is empty, use default permission
        if self.tools_allowed.is_empty() {
            return self.permission == Permission::Ask;
        }

        // If tool is not in allowed list, it's denied
        if !self.tools_allowed.iter().any(|t| t == tool_id) {
            return false;
        }

        self.permission == Permission::Ask
    }
}

/// Global permission configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionConfig {
    /// Default permission for unknown agent types
    pub default_permission: Permission,
    /// Agent-specific permissions
    pub agent_permissions: Vec<AgentPermission>,
    /// Whether to allow task delegation
    pub allow_delegation: bool,
    /// Maximum nesting depth for subagent delegation
    pub max_delegation_depth: usize,
}

impl PermissionConfig {
    pub fn new() -> Self {
        Self {
            default_permission: Permission::Ask,
            agent_permissions: Vec::new(),
            allow_delegation: true,
            max_delegation_depth: 3,
        }
    }

    /// Get permission for a specific agent type
    pub fn get_agent_permission(&self, agent_type: &str) -> Option<&AgentPermission> {
        self.agent_permissions
            .iter()
            .find(|p| p.agent_type == agent_type)
    }

    /// Check if a tool is allowed for an agent type
    pub fn is_tool_allowed(&self, agent_type: &str, tool_id: &str) -> bool {
        self.get_agent_permission(agent_type)
            .map(|p| p.is_tool_allowed(tool_id))
            .unwrap_or(self.default_permission == Permission::Allow)
    }

    /// Check if we should ask for permission for a tool
    pub fn should_ask(&self, agent_type: &str, tool_id: &str) -> bool {
        self.get_agent_permission(agent_type)
            .map(|p| p.should_ask_for_tool(tool_id))
            .unwrap_or(self.default_permission == Permission::Ask)
    }

    /// Check if delegation is allowed
    pub fn is_delegation_allowed(&self) -> bool {
        self.allow_delegation
    }

    /// Check if delegation depth is within limits
    pub fn is_depth_allowed(&self, current_depth: usize) -> bool {
        current_depth < self.max_delegation_depth
    }
}

/// Permission check result with context
#[derive(Debug, Clone)]
pub struct PermissionCheckResult {
    pub permission: Permission,
    pub reason: String,
}

impl PermissionCheckResult {
    pub fn allow(reason: impl Into<String>) -> Self {
        Self {
            permission: Permission::Allow,
            reason: reason.into(),
        }
    }

    pub fn deny(reason: impl Into<String>) -> Self {
        Self {
            permission: Permission::Deny,
            reason: reason.into(),
        }
    }

    pub fn ask(reason: impl Into<String>) -> Self {
        Self {
            permission: Permission::Ask,
            reason: reason.into(),
        }
    }
}

/// Permission checker for evaluating access requests
pub struct PermissionChecker {
    config: PermissionConfig,
}

impl PermissionChecker {
    pub fn new(config: PermissionConfig) -> Self {
        Self { config }
    }

    pub fn with_default() -> Self {
        Self {
            config: PermissionConfig::new(),
        }
    }

    /// Check if an agent type can use a specific tool
    pub fn check_tool(&self, agent_type: &str, tool_id: &str) -> PermissionCheckResult {
        if self.config.is_tool_allowed(agent_type, tool_id) {
            PermissionCheckResult::allow(format!("{} agent can use tool {}", agent_type, tool_id))
        } else if self.config.should_ask(agent_type, tool_id) {
            PermissionCheckResult::ask(format!(
                "{} agent requests tool {} - requires confirmation",
                agent_type, tool_id
            ))
        } else {
            PermissionCheckResult::deny(format!(
                "{} agent is not allowed to use tool {}",
                agent_type, tool_id
            ))
        }
    }

    /// Check if delegation is allowed
    pub fn check_delegation(&self, current_depth: usize) -> PermissionCheckResult {
        if !self.config.is_delegation_allowed() {
            return PermissionCheckResult::deny("Delegation is disabled".to_string());
        }

        if !self.config.is_depth_allowed(current_depth) {
            return PermissionCheckResult::deny(format!(
                "Maximum delegation depth {} exceeded",
                self.config.max_delegation_depth
            ));
        }

        PermissionCheckResult::allow(format!(
            "Delegation allowed at depth {}/{}",
            current_depth, self.config.max_delegation_depth
        ))
    }

    /// Check permission for a resource with optional subagent type
    ///
    /// This is the main entry point for permission checking used by TaskTool.
    ///
    /// - `agent`: The agent ID attempting the action
    /// - `resource`: The resource being accessed (e.g., "task")
    /// - `subagent_type`: Optional subagent type being invoked
    pub fn check(
        &self,
        agent: &str,
        resource: &str,
        subagent_type: Option<&str>,
    ) -> PermissionCheckResult {
        // Use the agent itself as the agent_type for the check
        let agent_type = agent;

        if self.config.is_tool_allowed(agent_type, resource) {
            PermissionCheckResult::allow(format!(
                "Agent {} allowed to access {} with subagent type {:?}",
                agent, resource, subagent_type
            ))
        } else if self.config.should_ask(agent_type, resource) {
            PermissionCheckResult::ask(format!(
                "Agent {} requests {} with subagent type {:?} - requires confirmation",
                agent, resource, subagent_type
            ))
        } else {
            PermissionCheckResult::deny(format!(
                "Agent {} is not allowed to access {} with subagent type {:?}",
                agent, resource, subagent_type
            ))
        }
    }

    /// Get the underlying config
    pub fn config(&self) -> &PermissionConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_default() {
        assert_eq!(Permission::default(), Permission::Ask);
    }

    #[test]
    fn test_agent_permission_default() {
        let perm = AgentPermission::new("general");
        assert_eq!(perm.agent_type, "general");
        assert_eq!(perm.permission, Permission::Ask);
        assert!(perm.tools_allowed.is_empty());
        assert!(perm.tools_denied.is_empty());
    }

    #[test]
    fn test_agent_permission_allowed_tools() {
        let perm = AgentPermission::new("general")
            .with_permission(Permission::Allow)
            .allow_tools(vec!["read".to_string(), "write".to_string()]);

        assert!(perm.is_tool_allowed("read"));
        assert!(perm.is_tool_allowed("write"));
        assert!(!perm.is_tool_allowed("bash")); // Not in allowed list
    }

    #[test]
    fn test_agent_permission_denied_tools() {
        let perm = AgentPermission::new("general")
            .with_permission(Permission::Allow)
            .deny_tools(vec!["bash".to_string()]);

        assert!(perm.is_tool_allowed("read"));
        assert!(!perm.is_tool_allowed("bash")); // Explicitly denied
    }

    #[test]
    fn test_agent_permission_empty_allowed_uses_default() {
        // With empty allowed list and Deny permission, all tools are denied
        let perm = AgentPermission::new("general").with_permission(Permission::Deny);

        assert!(!perm.is_tool_allowed("read"));
        assert!(!perm.is_tool_allowed("bash"));

        // With empty allowed list and Allow permission, all tools are allowed
        let perm = AgentPermission::new("general").with_permission(Permission::Allow);

        assert!(perm.is_tool_allowed("read"));
        assert!(perm.is_tool_allowed("bash"));
    }

    #[test]
    fn test_permission_config_default() {
        let config = PermissionConfig::new();
        assert_eq!(config.default_permission, Permission::Ask);
        assert!(config.agent_permissions.is_empty());
        assert!(config.allow_delegation);
        assert_eq!(config.max_delegation_depth, 3);
    }

    #[test]
    fn test_permission_config_get_agent() {
        let mut config = PermissionConfig::new();
        config
            .agent_permissions
            .push(AgentPermission::new("general").with_permission(Permission::Allow));

        assert!(config.get_agent_permission("general").is_some());
        assert!(config.get_agent_permission("unknown").is_none());
    }

    #[test]
    fn test_permission_checker_allow() {
        let mut config = PermissionConfig::new();
        config.default_permission = Permission::Allow;
        let checker = PermissionChecker::new(config);

        let result = checker.check_tool("general", "read");
        assert_eq!(result.permission, Permission::Allow);
    }

    #[test]
    fn test_permission_checker_delegation_depth() {
        let config = PermissionConfig::new();
        let checker = PermissionChecker::new(config);

        // Depth 0, 1, 2 should be allowed (max is 3)
        assert_eq!(checker.check_delegation(0).permission, Permission::Allow);
        assert_eq!(checker.check_delegation(1).permission, Permission::Allow);
        assert_eq!(checker.check_delegation(2).permission, Permission::Allow);

        // Depth 3 should be denied
        let result = checker.check_delegation(3);
        assert_eq!(result.permission, Permission::Deny);
    }
}
