//! Permission service implementations for tool execution control

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::time::{timeout, Duration};
use uuid::Uuid;

use rcode_core::permission::{Permission, PermissionRequest, PermissionResponse, PermissionRulesConfig, PermissionRule, PermissionRuleResult, evaluate_rules};
use rcode_event::{Event, EventBus};
use rcode_tools::bash_arity::resolve_command_with_arity;

#[allow(dead_code)]
const PERMISSION_TIMEOUT: Duration = Duration::from_secs(60); // 60 seconds default

/// Trait for permission checking services
#[async_trait::async_trait]
pub trait PermissionService: Send + Sync {
    /// Check if a tool call is allowed. Returns Ok(true) if allowed, Ok(false) if denied.
    /// In "ask" mode, may pause and wait for user response.
    async fn check(&self, request: &PermissionRequest) -> Result<bool, String>;
}

/// Check if a tool is considered sensitive (requires permission checks)
/// Returns true for ALL tools - every tool requires permission check
pub fn is_sensitive_tool(_tool_name: &str) -> bool {
    true
}

/// Auto permission service that always allows or denies based on mode
pub struct AutoPermissionService {
    mode: Permission,
}

impl AutoPermissionService {
    pub fn new(mode: Permission) -> Self {
        Self { mode }
    }

    /// Create for Allow mode
    pub fn allow() -> Self {
        Self {
            mode: Permission::Allow,
        }
    }

    /// Create for Deny mode
    pub fn deny() -> Self {
        Self {
            mode: Permission::Deny,
        }
    }
}

#[async_trait::async_trait]
impl PermissionService for AutoPermissionService {
    async fn check(&self, _request: &PermissionRequest) -> Result<bool, String> {
        match self.mode {
            Permission::Allow => {
                // Allow mode: always allow
                Ok(true)
            }
            Permission::Deny => {
                // Deny mode: always deny (since all tools are now sensitive)
                Ok(false)
            }
            Permission::Ask => {
                // Ask mode without interactive service: always deny
                // (Interactive service should be used for actual prompts)
                Ok(false)
            }
        }
    }
}

/// Rule-based permission service that evaluates permission rules.
///
/// This service:
/// 1. Evaluates permission rules for tool calls
/// 2. For bash commands: uses arity-resolved pattern matching
/// 3. Returns Allow, Deny, or Ask based on matching rules
/// 4. Falls back to InteractivePermissionService for Ask actions
pub struct RuleBasedPermissionService {
    /// Permission rules to evaluate
    rules: Vec<PermissionRule>,
    /// Fallback service for Ask actions (typically InteractivePermissionService)
    fallback: Option<Arc<dyn PermissionService>>,
}

impl RuleBasedPermissionService {
    /// Creates a new RuleBasedPermissionService with the given rules.
    pub fn new(rules: Vec<PermissionRule>) -> Self {
        Self {
            rules,
            fallback: None,
        }
    }

    /// Creates a new RuleBasedPermissionService from a PermissionRulesConfig.
    pub fn from_config(config: &PermissionRulesConfig) -> Self {
        Self {
            rules: config.rules.clone(),
            fallback: None,
        }
    }

    /// Sets the fallback service for Ask actions.
    pub fn with_fallback(mut self, fallback: Arc<dyn PermissionService>) -> Self {
        self.fallback = Some(fallback);
        self
    }

    /// Gets the permission rules.
    pub fn rules(&self) -> &[PermissionRule] {
        &self.rules
    }

    /// Evaluates permission rules for a tool call, returning the rule result.
    ///
    /// This method handles arity resolution for bash commands.
    fn evaluate_request(&self, request: &PermissionRequest) -> PermissionRuleResult {
        let tool_name = &request.tool_name;
        let args = &request.tool_input;

        // For bash commands, resolve arity and use the resolved command for pattern matching
        if tool_name == "bash" {
            if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
                let (arity, resolved_cmd) = resolve_command_with_arity(cmd);
                
                // Create a modified args with the arity-resolved command
                let resolved_args = if arity > 1 {
                    serde_json::json!({
                        "command": resolved_cmd
                    })
                } else {
                    args.clone()
                };
                
                return evaluate_rules(tool_name, &resolved_args, &self.rules);
            }
        }

        // For non-bash tools, evaluate directly
        evaluate_rules(tool_name, args, &self.rules)
    }
}

#[async_trait::async_trait]
impl PermissionService for RuleBasedPermissionService {
    async fn check(&self, request: &PermissionRequest) -> Result<bool, String> {
        let result = self.evaluate_request(request);
        
        match result {
            PermissionRuleResult::Allow => Ok(true),
            PermissionRuleResult::Deny { reason } => Err(reason),
            PermissionRuleResult::Ask { message } => {
                // If we have a fallback service, use it for Ask actions
                if let Some(ref fallback) = self.fallback {
                    fallback.check(request).await
                } else {
                    // No fallback configured - deny the request
                    Err(message)
                }
            }
        }
    }
}

/// Interactive permission service that waits for user confirmation via event bus and REST endpoints.
/// 
/// This service:
/// 1. Publishes `PermissionRequested` events to the event bus when a tool needs approval
/// 2. Blocks the executor until a grant/deny response is received via REST API
/// 3. Stores pending requests in an internal map keyed by request ID
/// 4. Times out after `timeout_secs` seconds (default 60)
pub struct InteractivePermissionService {
    always_allow: std::sync::Mutex<HashSet<String>>,
    always_deny: std::sync::Mutex<HashSet<String>>,
    pending: Arc<Mutex<HashMap<Uuid, oneshot::Sender<PermissionResponse>>>>,
    event_bus: Arc<EventBus>,
    session_id: String,
    timeout_secs: u64,
    /// Legacy mpsc channel for backward compatibility with old API
    legacy_tx: Option<mpsc::Sender<(PermissionRequest, oneshot::Sender<PermissionResponse>)>>,
}

impl InteractivePermissionService {
    /// Create a new InteractivePermissionService with event bus and session ID.
    /// This is the primary constructor for the new event-driven permission flow.
    pub fn new(event_bus: Arc<EventBus>, session_id: String) -> Self {
        Self {
            always_allow: std::sync::Mutex::new(HashSet::new()),
            always_deny: std::sync::Mutex::new(HashSet::new()),
            pending: Arc::new(Mutex::new(HashMap::new())),
            event_bus,
            session_id,
            timeout_secs: 60,
            legacy_tx: None,
        }
    }

    /// Create a new InteractivePermissionService with custom timeout.
    pub fn with_timeout(event_bus: Arc<EventBus>, session_id: String, timeout_secs: u64) -> Self {
        Self {
            always_allow: std::sync::Mutex::new(HashSet::new()),
            always_deny: std::sync::Mutex::new(HashSet::new()),
            pending: Arc::new(Mutex::new(HashMap::new())),
            event_bus,
            session_id,
            timeout_secs,
            legacy_tx: None,
        }
    }

    /// Legacy constructor for backward compatibility with tests.
    /// Uses mpsc channel instead of event bus.
    pub fn with_mpsc_channel(
        request_tx: mpsc::Sender<(PermissionRequest, oneshot::Sender<PermissionResponse>)>,
    ) -> Self {
        Self {
            always_allow: std::sync::Mutex::new(HashSet::new()),
            always_deny: std::sync::Mutex::new(HashSet::new()),
            pending: Arc::new(Mutex::new(HashMap::new())),
            event_bus: Arc::new(EventBus::new(100)),
            session_id: String::new(),
            timeout_secs: 300,
            legacy_tx: Some(request_tx),
        }
    }

    /// Grant permission for a pending request by ID.
    /// Called by the REST endpoint when user approves.
    pub async fn grant(&self, request_id: Uuid) -> Result<(), PermissionError> {
        self.resolve(request_id, PermissionResponse::Allow).await
    }

    /// Deny permission for a pending request by ID.
    /// Called by the REST endpoint when user denies.
    pub async fn deny(&self, request_id: Uuid) -> Result<(), PermissionError> {
        self.resolve(request_id, PermissionResponse::Deny).await
    }

    /// Resolve a pending permission request.
    /// Note: This is async because it awaits the tokio::sync::Mutex lock.
    async fn resolve(&self, request_id: Uuid, response: PermissionResponse) -> Result<(), PermissionError> {
        let mut pending = self.pending.lock().await;
        if let Some(tx) = pending.remove(&request_id) {
            let _ = tx.send(response);
            Ok(())
        } else {
            Err(PermissionError::NotFound)
        }
    }

    /// Add a tool to the always-allow list
    pub fn add_always_allow(&self, tool_name: String) -> Result<(), String> {
        let mut always_allow = self.always_allow.lock()
            .map_err(|e| format!("Permission lock poisoned: {}", e))?;
        always_allow.insert(tool_name);
        Ok(())
    }

    /// Add a tool to the always-deny list
    pub fn add_always_deny(&self, tool_name: String) -> Result<(), String> {
        let mut always_deny = self.always_deny.lock()
            .map_err(|e| format!("Permission lock poisoned: {}", e))?;
        always_deny.insert(tool_name);
        Ok(())
    }

    /// Check if a tool is in the always-allow list
    fn is_always_allowed(&self, tool_name: &str) -> Result<bool, String> {
        let always_allow = self.always_allow.lock()
            .map_err(|e| format!("Permission lock poisoned: {}", e))?;
        Ok(always_allow.contains(tool_name))
    }

    /// Check if a tool is in the always-deny list
    fn is_always_denied(&self, tool_name: &str) -> Result<bool, String> {
        let always_deny = self.always_deny.lock()
            .map_err(|e| format!("Permission lock poisoned: {}", e))?;
        Ok(always_deny.contains(tool_name))
    }
}

/// Error types for permission operations
#[derive(Debug, Clone)]
pub enum PermissionError {
    NotFound,
    AlreadyResolved,
    Timeout,
}

impl std::fmt::Display for PermissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PermissionError::NotFound => write!(f, "Permission request not found"),
            PermissionError::AlreadyResolved => write!(f, "Permission request already resolved"),
            PermissionError::Timeout => write!(f, "Permission request timed out"),
        }
    }
}

impl std::error::Error for PermissionError {}

#[async_trait::async_trait]
impl PermissionService for InteractivePermissionService {
    async fn check(&self, request: &PermissionRequest) -> Result<bool, String> {
        let tool_name = &request.tool_name;

        // Check always-allow list first
        if self.is_always_allowed(tool_name)? {
            return Ok(true);
        }

        // Check always-deny list
        if self.is_always_denied(tool_name)? {
            return Ok(false);
        }

        // Use legacy mpsc channel if available (for backward compatibility)
        if let Some(ref tx) = self.legacy_tx {
            let (response_tx, response_rx) = oneshot::channel();
            timeout(Duration::from_secs(300), tx.send((request.clone(), response_tx)))
                .await
                .map_err(|_| "Permission request timed out: UI not responding".to_string())?
                .map_err(|_| "Permission channel closed: UI disconnected".to_string())?;

            let response = timeout(Duration::from_secs(300), response_rx)
                .await
                .map_err(|_| "Permission response timed out: user did not respond".to_string())?
                .map_err(|_| "Permission response channel dropped".to_string())?;

            return match response {
                PermissionResponse::Allow => Ok(true),
                PermissionResponse::AllowAlways => {
                    self.add_always_allow(tool_name.clone())?;
                    Ok(true)
                }
                PermissionResponse::Deny => Ok(false),
                PermissionResponse::DenyAlways => {
                    self.add_always_deny(tool_name.clone())?;
                    Ok(false)
                }
            };
        }

        // New event-driven flow: publish event and wait for grant/deny
        let request_id = Uuid::new_v4();
        let (response_tx, response_rx) = oneshot::channel();

        // Store the sender in pending map
        {
            let mut pending = self.pending.lock().await;
            pending.insert(request_id, response_tx);
        }

        // Publish PermissionRequested event
        self.event_bus.publish(Event::PermissionRequested {
            request_id,
            session_id: self.session_id.clone(),
            tool_name: request.tool_name.clone(),
            tool_input: request.tool_input.clone(),
        });

        // Wait for response with timeout
        let (response, granted, reason) = match timeout(Duration::from_secs(self.timeout_secs), response_rx).await {
            Ok(Ok(resp)) => {
                let granted = match &resp {
                    PermissionResponse::Allow | PermissionResponse::AllowAlways => true,
                    PermissionResponse::Deny | PermissionResponse::DenyAlways => false,
                };
                (resp, granted, None)
            }
            _ => {
                // Timeout or channel closed - treat as deny
                (PermissionResponse::Deny, false, Some("timeout".to_string()))
            }
        };

        // Publish PermissionResolved event
        self.event_bus.publish(Event::PermissionResolved {
            request_id,
            session_id: self.session_id.clone(),
            granted,
            reason,
        });

        match response {
            PermissionResponse::Allow => Ok(true),
            PermissionResponse::AllowAlways => {
                self.add_always_allow(tool_name.clone())?;
                Ok(true)
            }
            PermissionResponse::Deny => Ok(false),
            PermissionResponse::DenyAlways => {
                self.add_always_deny(tool_name.clone())?;
                Ok(false)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::permission::PermissionRequest;
    use rcode_core::PermissionRuleAction;
    use rcode_event::EventBus;

    // Helper to create a permission request
    fn make_request(tool_name: &str) -> PermissionRequest {
        PermissionRequest {
            tool_name: tool_name.to_string(),
            tool_input: serde_json::json!({}),
            reason: None,
        }
    }

    // ========== AutoPermissionService tests ==========

    #[tokio::test]
    async fn test_auto_permission_allow_always_allows() {
        let service = AutoPermissionService::allow();
        let request = make_request("bash");

        let result = service.check(&request).await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_auto_permission_allow_allows_sensitive() {
        let service = AutoPermissionService::allow();
        let request = make_request("edit");

        let result = service.check(&request).await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_auto_permission_deny_denies() {
        let service = AutoPermissionService::deny();
        let request = make_request("bash");

        let result = service.check(&request).await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn test_auto_permission_ask_denies() {
        let service = AutoPermissionService::new(Permission::Ask);
        let request = make_request("bash");

        let result = service.check(&request).await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    // ========== is_sensitive_tool tests ==========

    #[test]
    fn test_is_sensitive_tool_returns_true_for_all() {
        // All tools are now sensitive
        assert!(is_sensitive_tool("bash"));
        assert!(is_sensitive_tool("read"));
        assert!(is_sensitive_tool("edit"));
        assert!(is_sensitive_tool("write"));
        assert!(is_sensitive_tool("glob"));
        assert!(is_sensitive_tool("grep"));
    }

    // ========== InteractivePermissionService tests (new event-driven API) ==========

    #[tokio::test]
    async fn test_interactive_permission_grant_resolves_check() {
        let event_bus = Arc::new(EventBus::new(100));
        let service = InteractivePermissionService::new(
            Arc::clone(&event_bus),
            "test-session".to_string(),
        );
        let service = Arc::new(service);

        // Start the check in background
        let service_clone = Arc::clone(&service);
        let handle = tokio::spawn(async move {
            let request = make_request("bash");
            service_clone.check(&request).await
        });

        // Subscribe to event bus to receive the PermissionRequested event
        let mut subscriber = event_bus.subscribe_for_session("test-session");
        
        // Wait for the PermissionRequested event
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), subscriber.recv())
            .await
            .unwrap()
            .unwrap();
        
        let request_id = match event {
            Event::PermissionRequested { request_id, .. } => request_id,
            _ => panic!("Expected PermissionRequested event"),
        };

        // Grant permission
        service.grant(request_id).await.unwrap();

        // Check should now complete
        let result = handle.await.unwrap();
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_interactive_permission_deny_resolves_check() {
        let event_bus = Arc::new(EventBus::new(100));
        let service = InteractivePermissionService::new(
            Arc::clone(&event_bus),
            "test-session".to_string(),
        );
        let service = Arc::new(service);

        // Start the check in background
        let service_clone = Arc::clone(&service);
        let handle = tokio::spawn(async move {
            let request = make_request("bash");
            service_clone.check(&request).await
        });

        // Subscribe to event bus
        let mut subscriber = event_bus.subscribe_for_session("test-session");
        
        // Wait for the PermissionRequested event
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), subscriber.recv())
            .await
            .unwrap()
            .unwrap();
        
        let request_id = match event {
            Event::PermissionRequested { request_id, .. } => request_id,
            _ => panic!("Expected PermissionRequested event"),
        };

        // Deny permission
        service.deny(request_id).await.unwrap();

        // Check should complete with denial
        let result = handle.await.unwrap();
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn test_interactive_permission_timeout_returns_deny() {
        let event_bus = Arc::new(EventBus::new(100));
        // Use a very short timeout for testing
        let service = InteractivePermissionService::with_timeout(
            Arc::clone(&event_bus),
            "test-session".to_string(),
            1, // 1 second timeout
        );
        let service = Arc::new(service);

        // Start the check in background
        let service_clone = Arc::clone(&service);
        let handle = tokio::spawn(async move {
            let request = make_request("bash");
            service_clone.check(&request).await
        });

        // Don't resolve - let it timeout
        let result = handle.await.unwrap();
        assert!(result.is_ok());
        assert!(!result.unwrap()); // Timeout means deny
    }

    #[tokio::test]
    async fn test_interactive_permission_publishes_permission_resolved_event() {
        let event_bus = Arc::new(EventBus::new(100));
        let service = InteractivePermissionService::new(
            Arc::clone(&event_bus),
            "test-session".to_string(),
        );
        let service = Arc::new(service);

        let service_clone = Arc::clone(&service);
        let handle = tokio::spawn(async move {
            let request = make_request("bash");
            service_clone.check(&request).await
        });

        // Subscribe to session events BEFORE the spawned task potentially publishes
        let mut subscriber = event_bus.subscribe_for_session("test-session");
        
        // Yield to let the spawned task run and publish PermissionRequested
        tokio::task::yield_now().await;
        
        // Receive PermissionRequested
        let request_id = loop {
            match tokio::time::timeout(std::time::Duration::from_secs(2), subscriber.recv()).await {
                Ok(Ok(Event::PermissionRequested { request_id, .. })) => break request_id,
                Ok(Ok(_)) => continue,
                Ok(Err(_)) | Err(_) => panic!("Timeout waiting for PermissionRequested"),
            }
        };

        // Grant permission
        service.grant(request_id).await.unwrap();

        // Receive PermissionResolved
        let resolved_event_received = loop {
            match tokio::time::timeout(std::time::Duration::from_secs(2), subscriber.recv()).await {
                Ok(Ok(Event::PermissionResolved { request_id: id, granted, .. })) => {
                    assert_eq!(id, request_id);
                    assert!(granted);
                    break true;
                }
                Ok(Ok(_)) => continue,
                Ok(Err(_)) | Err(_) => break false,
            }
        };

        let result = handle.await.unwrap();
        assert!(result.is_ok());
        assert!(result.unwrap());
        assert!(resolved_event_received);
    }

    #[tokio::test]
    async fn test_interactive_permission_unknown_request_id_returns_error() {
        let event_bus = Arc::new(EventBus::new(100));
        let service = InteractivePermissionService::new(
            Arc::clone(&event_bus),
            "test-session".to_string(),
        );

        let unknown_id = Uuid::new_v4();
        let result = service.grant(unknown_id).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PermissionError::NotFound));
    }

    #[tokio::test]
    async fn test_interactive_permission_uses_always_allow_list() {
        let event_bus = Arc::new(EventBus::new(100));
        let service = InteractivePermissionService::new(
            Arc::clone(&event_bus),
            "test-session".to_string(),
        );

        // Manually add to always allow
        service.add_always_allow("bash".to_string()).unwrap();

        let request = make_request("bash");
        let result = service.check(&request).await;

        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_interactive_permission_uses_always_deny_list() {
        let event_bus = Arc::new(EventBus::new(100));
        let service = InteractivePermissionService::new(
            Arc::clone(&event_bus),
            "test-session".to_string(),
        );

        // Manually add to always deny
        service.add_always_deny("edit".to_string()).unwrap();

        let request = make_request("edit");
        let result = service.check(&request).await;

        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    // ========== Legacy mpsc API tests (backward compatibility) ==========

    #[tokio::test]
    async fn test_interactive_legacy_mpsc_channel() {
        let (tx, mut rx) = mpsc::channel::<(PermissionRequest, oneshot::Sender<PermissionResponse>)>(1);
        let service = InteractivePermissionService::with_mpsc_channel(tx);

        // Start the check in background
        let service = Arc::new(service);
        let service_clone = Arc::clone(&service);
        let handle = tokio::spawn(async move {
            let request = make_request("bash");
            service_clone.check(&request).await
        });

        // Receive the permission request via legacy channel
        let (received_request, response_tx) = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            rx.recv(),
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(received_request.tool_name, "bash");

        // Send Allow response
        response_tx.send(PermissionResponse::Allow).unwrap();

        let result = handle.await.unwrap();
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_interactive_legacy_adds_to_always_allow_on_allow_always() {
        let (tx, mut rx) = mpsc::channel::<(PermissionRequest, oneshot::Sender<PermissionResponse>)>(1);
        let service = InteractivePermissionService::with_mpsc_channel(tx);

        let service = Arc::new(service);

        // First call
        let service_clone = Arc::clone(&service);
        let handle = tokio::spawn(async move {
            let request = make_request("bash");
            service_clone.check(&request).await
        });

        let (_received_request, response_tx) = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            rx.recv(),
        )
        .await
        .unwrap()
        .unwrap();

        // Send AllowAlways response
        response_tx.send(PermissionResponse::AllowAlways).unwrap();

        let result = handle.await.unwrap();
        assert!(result.is_ok());
        assert!(result.unwrap());

        // Second call should use the always_allow list
        let request = make_request("bash");
        let result = service.check(&request).await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    // ========== RuleBasedPermissionService tests ==========

    #[tokio::test]
    async fn test_rule_based_permission_allow_safe_command() {
        use rcode_core::permission::{PermissionRule, PermissionRuleAction, PermissionRulesConfig};
        
        let rules = vec![
            PermissionRule::new("bash", "ls", PermissionRuleAction::Allow),
        ];
        let service = RuleBasedPermissionService::new(rules);
        
        let request = make_request("bash");
        let result = service.check(&request).await;
        
        assert!(result.is_ok());
        assert!(result.unwrap()); // ls is allowed by the rule
    }

    #[tokio::test]
    async fn test_rule_based_permission_deny_dangerous_command() {
        use rcode_core::permission::{PermissionRule, PermissionRuleAction};
        
        let rules = vec![
            PermissionRule::new("bash", "rm -rf", PermissionRuleAction::Deny),
        ];
        let service = RuleBasedPermissionService::new(rules);
        
        let request = PermissionRequest {
            tool_name: "bash".to_string(),
            tool_input: serde_json::json!({"command": "rm -rf /tmp/build"}),
            reason: None,
        };
        let result = service.check(&request).await;
        
        assert!(result.is_err(), "rm -rf should be denied");
        let err = result.unwrap_err();
        assert!(err.contains("rm -rf"), "Error should mention the blocked pattern");
    }

    #[tokio::test]
    async fn test_rule_based_permission_ask_without_fallback() {
        use rcode_core::permission::PermissionRule;
        
        let rules = vec![
            PermissionRule::new("bash", "docker rm", PermissionRuleAction::Ask),
        ];
        let service = RuleBasedPermissionService::new(rules);
        
        let request = PermissionRequest {
            tool_name: "bash".to_string(),
            tool_input: serde_json::json!({"command": "docker rm container_id"}),
            reason: None,
        };
        let result = service.check(&request).await;
        
        // Without fallback, Ask returns Err (denied)
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_rule_based_permission_ask_with_fallback() {
        use rcode_core::permission::{PermissionRule, PermissionRuleAction};
        
        let rules = vec![
            PermissionRule::new("bash", "docker rm", PermissionRuleAction::Ask),
        ];
        let service = RuleBasedPermissionService::new(rules);
        
        // Create a fallback that always allows
        let fallback = Arc::new(AutoPermissionService::allow());
        let service = service.with_fallback(fallback);
        
        let request = PermissionRequest {
            tool_name: "bash".to_string(),
            tool_input: serde_json::json!({"command": "docker rm container_id"}),
            reason: None,
        };
        let result = service.check(&request).await;
        
        // With fallback that allows, Ask should be allowed
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_rule_based_permission_last_match_wins() {
        use rcode_core::permission::{PermissionRule, PermissionRuleAction};
        
        let rules = vec![
            PermissionRule::new("bash", "git", PermissionRuleAction::Allow),
            PermissionRule::new("bash", "git push", PermissionRuleAction::Deny),
        ];
        let service = RuleBasedPermissionService::new(rules);
        
        let request = PermissionRequest {
            tool_name: "bash".to_string(),
            tool_input: serde_json::json!({"command": "git push origin main"}),
            reason: None,
        };
        let result = service.check(&request).await;
        
        // git push matches both rules, last one wins (Deny)
        assert!(result.is_err(), "git push should be denied by last-match-wins rule");
        assert!(result.unwrap_err().contains("git push"));
    }

    #[tokio::test]
    async fn test_rule_based_permission_no_match_allow() {
        use rcode_core::permission::{PermissionRule, PermissionRuleAction};
        
        // Empty rules = allow by default (backward compatible)
        let rules = vec![
            PermissionRule::new("bash", "git push", PermissionRuleAction::Deny),
        ];
        let service = RuleBasedPermissionService::new(rules);
        
        let request = PermissionRequest {
            tool_name: "bash".to_string(),
            tool_input: serde_json::json!({"command": "ls -la"}),
            reason: None,
        };
        let result = service.check(&request).await;
        
        // ls doesn't match any rule, so allowed
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_rule_based_permission_empty_rules_allow() {
        let rules = vec![];
        let service = RuleBasedPermissionService::new(rules);
        
        let request = make_request("bash");
        let result = service.check(&request).await;
        
        // Empty rules = allow (backward compatible)
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_rule_based_permission_write_path_matching() {
        use rcode_core::permission::{PermissionRule, PermissionRuleAction};
        
        let rules = vec![
            PermissionRule::new("write", "/etc/**", PermissionRuleAction::Deny),
        ];
        let service = RuleBasedPermissionService::new(rules);
        
        let request = PermissionRequest {
            tool_name: "write".to_string(),
            tool_input: serde_json::json!({"path": "/etc/passwd"}),
            reason: None,
        };
        let result = service.check(&request).await;
        
        assert!(result.is_err(), "/etc/** should deny write to /etc/passwd");
        assert!(result.unwrap_err().contains("/etc/**"), "Error should mention the blocked pattern");
    }

    #[tokio::test]
    async fn test_rule_based_permission_edit_glob_matching() {
        use rcode_core::permission::{PermissionRule, PermissionRuleAction};
        
        let rules = vec![
            PermissionRule::new("edit", "*.tmp", PermissionRuleAction::Deny),
        ];
        let service = RuleBasedPermissionService::new(rules);
        
        let request = PermissionRequest {
            tool_name: "edit".to_string(),
            tool_input: serde_json::json!({"path": "file.tmp"}),
            reason: None,
        };
        let result = service.check(&request).await;
        
        assert!(result.is_err(), "*.tmp should deny edit to file.tmp");
    }

    #[test]
    fn test_rule_based_permission_from_config() {
        use rcode_core::permission::{PermissionRule, PermissionRuleAction, PermissionRulesConfig};
        
        let config = PermissionRulesConfig::with_rules(vec![
            PermissionRule::new("bash", "git push", PermissionRuleAction::Deny),
        ]);
        let service = RuleBasedPermissionService::from_config(&config);
        
        assert_eq!(service.rules().len(), 1);
    }

    #[tokio::test]
    async fn test_rule_based_permission_non_bash_tool() {
        use rcode_core::permission::PermissionRule;
        
        let rules = vec![
            PermissionRule::new("read", "/etc/**", PermissionRuleAction::Deny),
        ];
        let service = RuleBasedPermissionService::new(rules);
        
        let request = PermissionRequest {
            tool_name: "read".to_string(),
            tool_input: serde_json::json!({"path": "/etc/passwd"}),
            reason: None,
        };
        let result = service.check(&request).await;
        
        // Non-bash tools just check if rule.tool matches (no arity resolution)
        assert!(result.is_err(), "/etc/** should deny read to /etc/passwd");
        assert!(result.unwrap_err().contains("/etc/**"), "Error should mention the blocked pattern");
    }
}
