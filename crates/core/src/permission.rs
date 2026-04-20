//! Permission system for agent and tool access control

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Permission levels for agent/tool access
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    /// Allow the action without prompting
    Allow,
    /// Deny the action unconditionally
    Deny,
    /// Prompt the user for confirmation before proceeding
    #[default]
    Ask,
}

/// Request for permission to execute a tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRequest {
    /// Name of the tool to execute
    pub tool_name: String,
    /// Tool input arguments
    pub tool_input: serde_json::Value,
    /// Human-readable reason why permission is needed
    pub reason: Option<String>,
}

impl PermissionRequest {
    pub fn new(tool_name: String, tool_input: serde_json::Value) -> Self {
        Self {
            tool_name,
            tool_input,
            reason: None,
        }
    }

    pub fn with_reason(mut self, reason: String) -> Self {
        self.reason = Some(reason);
        self
    }
}

/// Response to a permission request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PermissionResponse {
    /// Allow the tool to execute once
    Allow,
    /// Allow the tool and add it to the always-allow list
    AllowAlways,
    /// Deny the tool once
    Deny,
    /// Deny the tool and add it to the always-deny list
    DenyAlways,
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

// =============================================================================
// Permission Rules for declarative tool access control
// =============================================================================

/// Action to take when a permission rule matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum PermissionRuleAction {
    /// Allow the action without prompting
    #[serde(rename = "allow")]
    Allow,
    /// Prompt the user for confirmation before proceeding
    #[serde(rename = "ask")]
    Ask,
    /// Deny the action unconditionally
    #[default]
    #[serde(rename = "deny")]
    Deny,
}

impl From<PermissionRuleAction> for Permission {
    fn from(action: PermissionRuleAction) -> Self {
        match action {
            PermissionRuleAction::Allow => Permission::Allow,
            PermissionRuleAction::Ask => Permission::Ask,
            PermissionRuleAction::Deny => Permission::Deny,
        }
    }
}

impl From<Permission> for PermissionRuleAction {
    fn from(perm: Permission) -> Self {
        match perm {
            Permission::Allow => PermissionRuleAction::Allow,
            Permission::Ask => PermissionRuleAction::Ask,
            Permission::Deny => PermissionRuleAction::Deny,
        }
    }
}

/// A single permission rule for declarative tool access control.
///
/// Rules are matched using last-matching-wins precedence (like iptables).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PermissionRule {
    /// The tool this rule applies to (e.g., "bash", "write", "edit", "read")
    pub tool: String,
    /// The pattern to match against.
    /// For bash: command pattern (e.g., "git push", "rm -rf")
    /// For write/edit: file path pattern (e.g., "/etc/**", "*.tmp")
    pub pattern: String,
    /// The action to take when this rule matches.
    pub action: PermissionRuleAction,
}

impl PermissionRule {
    /// Creates a new permission rule.
    pub fn new(tool: impl Into<String>, pattern: impl Into<String>, action: PermissionRuleAction) -> Self {
        Self {
            tool: tool.into(),
            pattern: pattern.into(),
            action,
        }
    }

    /// Check if this rule matches the given tool and arguments.
    ///
    /// For bash commands: matches if the command starts with the pattern
    /// For write/edit: matches if the path contains the pattern
    pub fn matches(&self, tool_name: &str, args: &serde_json::Value) -> bool {
        if self.tool != tool_name {
            return false;
        }

        match tool_name {
            "bash" => {
                if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
                    let cmd_lower = cmd.to_lowercase();
                    let pattern_lower = self.pattern.to_lowercase();
                    cmd_lower.starts_with(&pattern_lower)
                } else {
                    false
                }
            }
            "write" | "edit" => {
                if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                    // Simple path matching - pattern can be a substring or glob
                    self.pattern_match_path(path)
                } else {
                    false
                }
            }
            _ => {
                // For other tools, just check if the tool name matches
                true
            }
        }
    }

    /// Simple path pattern matching.
    /// Supports:
    /// - Substring matching: "src" matches "/project/src/main.rs"
    /// - Glob patterns: "*.tmp", "**/*.log"
    fn pattern_match_path(&self, path: &str) -> bool {
        let pattern = &self.pattern;
        
        // Exact substring match (case-insensitive)
        if path.to_lowercase().contains(&pattern.to_lowercase()) {
            return true;
        }
        
        // Simple glob patterns
        if let Some(ext) = pattern.strip_prefix("*.") {
            // *.ext pattern
            return path.to_lowercase().ends_with(&format!(".{}", ext.to_lowercase()));
        }
        
        if pattern.ends_with("/**") {
            // dir/** pattern - matches dir and all subdirectories
            let dir = &pattern[..pattern.len() - 3];
            return path.to_lowercase().starts_with(&dir.to_lowercase());
        }
        
        if pattern.contains("/**/") {
            // /**/中间目录/** pattern
            let parts: Vec<&str> = pattern.split("/**/").collect();
            if parts.len() == 2 {
                let prefix = parts[0];
                let suffix = parts[1];
                return path.to_lowercase().starts_with(&prefix.to_lowercase())
                    && path.to_lowercase().contains(&suffix.to_lowercase());
            }
        }
        
        false
    }
}

/// Permission rules configuration for declarative access control.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct PermissionRulesConfig {
    /// List of permission rules. Rules are evaluated in order, last matching rule wins.
    #[serde(default)]
    pub rules: Vec<PermissionRule>,
}

impl PermissionRulesConfig {
    /// Creates a new empty permission rules config.
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Creates a new permission rules config with the given rules.
    pub fn with_rules(rules: Vec<PermissionRule>) -> Self {
        Self { rules }
    }

    /// Check if the rules list is empty.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

/// Result of evaluating permission rules.
#[derive(Debug, Clone)]
pub enum PermissionRuleResult {
    /// Action is allowed
    Allow,
    /// Action should prompt the user
    Ask { message: String },
    /// Action is denied
    Deny { reason: String },
}

impl PermissionRuleResult {
    /// Returns true if this result allows the action.
    pub fn is_allowed(&self) -> bool {
        matches!(self, PermissionRuleResult::Allow)
    }

    /// Returns true if this result asks for confirmation.
    pub fn is_ask(&self) -> bool {
        matches!(self, PermissionRuleResult::Ask { .. })
    }

    /// Returns true if this result denies the action.
    pub fn is_deny(&self) -> bool {
        matches!(self, PermissionRuleResult::Deny { .. })
    }

    /// Converts to PermissionResult for the permission service.
    pub fn to_permission_result(&self) -> Result<bool, String> {
        match self {
            PermissionRuleResult::Allow => Ok(true),
            PermissionRuleResult::Ask { message } => Err(message.clone()),
            PermissionRuleResult::Deny { reason } => Err(reason.clone()),
        }
    }
}

/// Evaluates permission rules to determine the action for a tool call.
///
/// Rules are evaluated in order, last matching rule wins (iptables-style).
/// Empty rules list means Allow (backward compatible).
///
/// # Arguments
/// * `tool_name` - The name of the tool being called
/// * `args` - The tool arguments (JSON)
/// * `rules` - The list of permission rules to evaluate
///
/// # Returns
/// The action to take based on the first matching rule, or Allow if no rules match.
pub fn evaluate_rules(tool_name: &str, args: &serde_json::Value, rules: &[PermissionRule]) -> PermissionRuleResult {
    if rules.is_empty() {
        // Backward compatible: no rules means allow
        return PermissionRuleResult::Allow;
    }

    let mut result = PermissionRuleResult::Allow;

    for rule in rules {
        if rule.matches(tool_name, args) {
            result = match rule.action {
                PermissionRuleAction::Allow => PermissionRuleResult::Allow,
                PermissionRuleAction::Ask => {
                    let message = build_ask_message(tool_name, args, &rule.pattern);
                    PermissionRuleResult::Ask { message }
                }
                PermissionRuleAction::Deny => {
                    let reason = format!("Blocked by rule: {} {}", rule.tool, rule.pattern);
                    PermissionRuleResult::Deny { reason }
                }
            };
        }
    }

    result
}

/// Builds the confirmation message for Ask actions.
fn build_ask_message(tool_name: &str, args: &serde_json::Value, pattern: &str) -> String {
    match tool_name {
        "bash" => {
            if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
                format!("Command '{}' requires confirmation. Pattern rule: '{}'. Allow?", cmd, pattern)
            } else {
                format!("Tool '{}' requires confirmation. Allow?", tool_name)
            }
        }
        "write" | "edit" => {
            if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                format!("Writing to '{}' requires confirmation. Pattern rule: '{}'. Allow?", path, pattern)
            } else {
                format!("Tool '{}' requires confirmation. Allow?", tool_name)
            }
        }
        _ => format!("Tool '{}' requires confirmation. Allow?", tool_name),
    }
}

#[cfg(test)]
mod permission_rules_tests {
    use super::*;

    #[test]
    fn test_evaluate_rules_empty_is_allow() {
        let result = evaluate_rules("bash", &serde_json::json!({"command": "ls"}), &[]);
        assert!(matches!(result, PermissionRuleResult::Allow));
    }

    #[test]
    fn test_evaluate_rules_first_match_wins() {
        // With last-match-wins, even though git push matches first rule (Allow),
        // it also matches the second rule (Deny), and last match wins
        let rules = vec![
            PermissionRule::new("bash", "git push", PermissionRuleAction::Allow),
            PermissionRule::new("bash", "git", PermissionRuleAction::Deny),
        ];
        
        // git push matches BOTH rules (git push and git), last one wins (Deny)
        let result = evaluate_rules("bash", &serde_json::json!({"command": "git push origin main"}), &rules);
        assert!(matches!(result, PermissionRuleResult::Deny { .. }));
    }

    #[test]
    fn test_evaluate_rules_last_match_wins() {
        let rules = vec![
            PermissionRule::new("bash", "git", PermissionRuleAction::Allow),
            PermissionRule::new("bash", "git push", PermissionRuleAction::Deny),
        ];
        
        // git push matches both rules, but last one wins (Deny)
        let result = evaluate_rules("bash", &serde_json::json!({"command": "git push origin main"}), &rules);
        assert!(matches!(result, PermissionRuleResult::Deny { .. }));
    }

    #[test]
    fn test_evaluate_rules_no_match_is_allow() {
        let rules = vec![
            PermissionRule::new("bash", "git push", PermissionRuleAction::Deny),
        ];
        
        // ls doesn't match any rule, so Allow
        let result = evaluate_rules("bash", &serde_json::json!({"command": "ls -la"}), &rules);
        assert!(matches!(result, PermissionRuleResult::Allow));
    }

    #[test]
    fn test_evaluate_rules_deny_message() {
        let rules = vec![
            PermissionRule::new("bash", "rm -rf", PermissionRuleAction::Deny),
        ];
        
        let result = evaluate_rules("bash", &serde_json::json!({"command": "rm -rf /tmp/build"}), &rules);
        assert!(matches!(result, PermissionRuleResult::Deny { reason } if reason.contains("rm -rf")));
    }

    #[test]
    fn test_evaluate_rules_ask_message() {
        let rules = vec![
            PermissionRule::new("bash", "docker rm", PermissionRuleAction::Ask),
        ];
        
        let result = evaluate_rules("bash", &serde_json::json!({"command": "docker rm container_id"}), &rules);
        match result {
            PermissionRuleResult::Ask { message } => {
                assert!(message.contains("docker rm"));
            }
            _ => panic!("Expected Ask result"),
        }
    }

    #[test]
    fn test_permission_rule_matches_bash() {
        let rule = PermissionRule::new("bash", "git push", PermissionRuleAction::Deny);
        
        assert!(rule.matches("bash", &serde_json::json!({"command": "git push origin main"})));
        assert!(rule.matches("bash", &serde_json::json!({"command": "GIT PUSH --force origin"})));
        assert!(!rule.matches("bash", &serde_json::json!({"command": "git status"})));
        assert!(!rule.matches("read", &serde_json::json!({"path": "/tmp/file"})));
    }

    #[test]
    fn test_permission_rule_matches_write_path() {
        let rule = PermissionRule::new("write", "/etc/**", PermissionRuleAction::Deny);
        
        assert!(rule.matches("write", &serde_json::json!({"path": "/etc/passwd"})));
        assert!(rule.matches("write", &serde_json::json!({"path": "/etc/nginx/nginx.conf"})));
        assert!(!rule.matches("write", &serde_json::json!({"path": "/home/user/file.txt"})));
    }

    #[test]
    fn test_permission_rule_matches_edit_glob() {
        let rule = PermissionRule::new("edit", "*.tmp", PermissionRuleAction::Deny);
        
        assert!(rule.matches("edit", &serde_json::json!({"path": "file.tmp"})));
        assert!(rule.matches("edit", &serde_json::json!({"path": "/tmp/file.tmp"})));
        assert!(!rule.matches("edit", &serde_json::json!({"path": "file.txt"})));
    }

    #[test]
    fn test_permission_rule_action_conversion() {
        assert_eq!(Permission::Allow, PermissionRuleAction::Allow.into());
        assert_eq!(Permission::Ask, PermissionRuleAction::Ask.into());
        assert_eq!(Permission::Deny, PermissionRuleAction::Deny.into());
        
        assert_eq!(PermissionRuleAction::Allow, Permission::Allow.into());
        assert_eq!(PermissionRuleAction::Ask, Permission::Ask.into());
        assert_eq!(PermissionRuleAction::Deny, Permission::Deny.into());
    }

    #[test]
    fn test_permission_rules_config_is_empty() {
        let empty = PermissionRulesConfig::new();
        assert!(empty.is_empty());
        
        let with_rules = PermissionRulesConfig::with_rules(vec![
            PermissionRule::new("bash", "ls", PermissionRuleAction::Allow),
        ]);
        assert!(!with_rules.is_empty());
    }

    #[test]
    fn test_permission_rule_result_helpers() {
        let allow = PermissionRuleResult::Allow;
        assert!(allow.is_allowed());
        assert!(!allow.is_ask());
        assert!(!allow.is_deny());
        
        let ask = PermissionRuleResult::Ask { message: "test".to_string() };
        assert!(!ask.is_allowed());
        assert!(ask.is_ask());
        assert!(!ask.is_deny());
        
        let deny = PermissionRuleResult::Deny { reason: "test".to_string() };
        assert!(!deny.is_allowed());
        assert!(!deny.is_ask());
        assert!(deny.is_deny());
    }

    #[test]
    fn test_permission_rule_result_to_permission_result() {
        let allow = PermissionRuleResult::Allow;
        assert_eq!(allow.to_permission_result(), Ok(true));
        
        let deny = PermissionRuleResult::Deny { reason: "blocked".to_string() };
        assert_eq!(deny.to_permission_result(), Err("blocked".to_string()));
        
        let ask = PermissionRuleResult::Ask { message: "confirm?".to_string() };
        assert_eq!(ask.to_permission_result(), Err("confirm?".to_string()));
    }

    #[test]
    fn test_evaluate_rules_write_path_matching() {
        let rules = vec![
            PermissionRule::new("write", "/etc", PermissionRuleAction::Deny),
        ];
        
        let result = evaluate_rules("write", &serde_json::json!({"path": "/etc/passwd"}), &rules);
        assert!(matches!(result, PermissionRuleResult::Deny { .. }));
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

    #[test]
    fn test_permission_checker_delegation_disabled() {
        let mut config = PermissionConfig::new();
        config.allow_delegation = false;
        let checker = PermissionChecker::new(config);

        let result = checker.check_delegation(0);
        assert_eq!(result.permission, Permission::Deny);
        assert!(result.reason.contains("disabled"));
    }

    #[test]
    fn test_agent_permission_should_ask_for_tool() {
        let perm = AgentPermission::new("general")
            .with_permission(Permission::Ask)
            .allow_tools(vec!["read".to_string()]);

        assert!(perm.should_ask_for_tool("read"));
        assert!(!perm.should_ask_for_tool("write")); // Not in allowed list
    }

    #[test]
    fn test_agent_permission_should_ask_denied_tool() {
        let perm = AgentPermission::new("general")
            .with_permission(Permission::Ask)
            .deny_tools(vec!["bash".to_string()]);

        assert!(!perm.should_ask_for_tool("bash")); // Denied tools never ask
    }

    #[test]
    fn test_agent_permission_should_ask_with_empty_allowed() {
        let perm = AgentPermission::new("general").with_permission(Permission::Ask);

        assert!(perm.should_ask_for_tool("any_tool"));
    }

    #[test]
    fn test_permission_config_is_tool_allowed_default_allow() {
        let mut config = PermissionConfig::new();
        config.default_permission = Permission::Allow;

        assert!(config.is_tool_allowed("unknown_agent", "any_tool"));
    }

    #[test]
    fn test_permission_config_is_tool_allowed_default_deny() {
        let mut config = PermissionConfig::new();
        config.default_permission = Permission::Deny;

        assert!(!config.is_tool_allowed("unknown_agent", "any_tool"));
    }

    #[test]
    fn test_permission_config_should_ask_default() {
        let mut config = PermissionConfig::new();
        config.default_permission = Permission::Ask;

        assert!(config.should_ask("unknown_agent", "any_tool"));
    }

    #[test]
    fn test_permission_checker_check_main_entry() {
        let mut config = PermissionConfig::new();
        config.default_permission = Permission::Allow;
        let checker = PermissionChecker::new(config);

        let result = checker.check("agent1", "resource1", Some("subagent"));
        assert_eq!(result.permission, Permission::Allow);
    }

    #[test]
    fn test_permission_checker_check_with_ask() {
        let mut config = PermissionConfig::new();
        config.default_permission = Permission::Ask;
        let checker = PermissionChecker::new(config);

        let result = checker.check("agent1", "resource1", None);
        assert_eq!(result.permission, Permission::Ask);
    }

    #[test]
    fn test_permission_checker_check_with_deny() {
        let mut config = PermissionConfig::new();
        config.default_permission = Permission::Deny;
        let checker = PermissionChecker::new(config);

        let result = checker.check("agent1", "resource1", None);
        assert_eq!(result.permission, Permission::Deny);
    }

    #[test]
    fn test_permission_checker_with_default_config() {
        let checker = PermissionChecker::with_default();
        let result = checker.check_delegation(0);
        assert_eq!(result.permission, Permission::Allow);
    }

    #[test]
    fn test_permission_checker_config_accessor() {
        let config = PermissionConfig::new();
        let checker = PermissionChecker::new(config);
        assert_eq!(checker.config().default_permission, Permission::Ask);
    }

    #[test]
    fn test_permission_check_result_helpers() {
        let allow = PermissionCheckResult::allow("because reasons");
        assert_eq!(allow.permission, Permission::Allow);
        assert_eq!(allow.reason, "because reasons");

        let deny = PermissionCheckResult::deny("not allowed");
        assert_eq!(deny.permission, Permission::Deny);

        let ask = PermissionCheckResult::ask("please confirm");
        assert_eq!(ask.permission, Permission::Ask);
    }

    #[test]
    fn test_permission_serde_roundtrip() {
        use serde_json;

        let perm = Permission::Allow;
        let json = serde_json::to_string(&perm).unwrap();
        let parsed: Permission = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, Permission::Allow);
    }

    #[test]
    fn test_agent_permission_serde_roundtrip() {
        use serde_json;

        let perm = AgentPermission::new("test")
            .with_permission(Permission::Allow)
            .allow_tools(vec!["read".to_string()]);

        let json = serde_json::to_string(&perm).unwrap();
        let parsed: AgentPermission = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.agent_type, "test");
        assert_eq!(parsed.permission, Permission::Allow);
    }

    #[test]
    fn test_permission_config_serde_roundtrip() {
        use serde_json;

        let config = PermissionConfig::new();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: PermissionConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.default_permission, Permission::Ask);
        assert!(parsed.allow_delegation);
    }
}
