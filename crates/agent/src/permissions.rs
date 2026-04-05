//! Permission service implementations for tool execution control

use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{timeout, Duration};

use rcode_core::permission::{Permission, PermissionRequest, PermissionResponse};

const PERMISSION_TIMEOUT: Duration = Duration::from_secs(300); // 5 minutes

/// Trait for permission checking services
#[async_trait::async_trait]
pub trait PermissionService: Send + Sync {
    /// Check if a tool call is allowed. Returns Ok(true) if allowed, Ok(false) if denied.
    /// In "ask" mode, may pause and wait for user response.
    async fn check(&self, request: &PermissionRequest) -> Result<bool, String>;
}

/// Check if a tool is considered sensitive (requires permission checks)
pub fn is_sensitive_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "bash"
            | "edit"
            | "write"
            | "delete"
            | "patch"
            | "apply_patch"
            | "fs_write"
            | "fs_delete"
            | "shell"
            | "execute"
            | "run_command"
            | "applypatch"
    )
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
    async fn check(&self, request: &PermissionRequest) -> Result<bool, String> {
        match self.mode {
            Permission::Allow => {
                // Allow mode: always allow (sensitive tools always allowed)
                Ok(true)
            }
            Permission::Deny => {
                // Deny mode: only deny sensitive tools, non-sensitive always allowed
                if is_sensitive_tool(&request.tool_name) {
                    Ok(false)
                } else {
                    Ok(true)
                }
            }
            Permission::Ask => {
                // Ask mode without interactive service: allow non-sensitive, deny sensitive
                // (Interactive service should be used for actual prompts)
                if is_sensitive_tool(&request.tool_name) {
                    Ok(false)
                } else {
                    Ok(true)
                }
            }
        }
    }
}

/// Interactive permission service that waits for user confirmation
pub struct InteractivePermissionService {
    always_allow: std::sync::Mutex<HashSet<String>>,
    always_deny: std::sync::Mutex<HashSet<String>>,
    request_tx: mpsc::Sender<(PermissionRequest, oneshot::Sender<PermissionResponse>)>,
}

impl InteractivePermissionService {
    pub fn new(
        request_tx: mpsc::Sender<(PermissionRequest, oneshot::Sender<PermissionResponse>)>,
    ) -> Self {
        Self {
            always_allow: std::sync::Mutex::new(HashSet::new()),
            always_deny: std::sync::Mutex::new(HashSet::new()),
            request_tx,
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

#[async_trait::async_trait]
impl PermissionService for InteractivePermissionService {
    async fn check(&self, request: &PermissionRequest) -> Result<bool, String> {
        let tool_name = &request.tool_name;

        // Non-sensitive tools are always allowed without prompting
        if !is_sensitive_tool(tool_name) {
            return Ok(true);
        }

        // Check always-allow list
        if self.is_always_allowed(tool_name)? {
            return Ok(true);
        }

        // Check always-deny list
        if self.is_always_denied(tool_name)? {
            return Ok(false);
        }

        // Send permission request and wait for response with timeout
        let (response_tx, response_rx) = oneshot::channel();

        timeout(PERMISSION_TIMEOUT, self.request_tx.send((request.clone(), response_tx)))
            .await
            .map_err(|_| "Permission request timed out: UI not responding".to_string())?
            .map_err(|_| "Permission channel closed: UI disconnected".to_string())?;

        let response = timeout(PERMISSION_TIMEOUT, response_rx)
            .await
            .map_err(|_| "Permission response timed out: user did not respond".to_string())?
            .map_err(|_| "Permission response channel dropped".to_string())?;

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
    async fn test_auto_permission_deny_denies_sensitive() {
        let service = AutoPermissionService::deny();
        let request = make_request("bash");

        let result = service.check(&request).await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn test_auto_permission_deny_allows_non_sensitive() {
        let service = AutoPermissionService::deny();
        let request = make_request("read");

        let result = service.check(&request).await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_auto_permission_ask_denies_sensitive() {
        let service = AutoPermissionService::new(Permission::Ask);
        let request = make_request("bash");

        let result = service.check(&request).await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn test_auto_permission_ask_allows_non_sensitive() {
        let service = AutoPermissionService::new(Permission::Ask);
        let request = make_request("read");

        let result = service.check(&request).await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    // ========== is_sensitive_tool tests ==========

    #[test]
    fn test_is_sensitive_tool_sensitive() {
        let sensitive = vec![
            "bash",
            "edit",
            "write",
            "delete",
            "patch",
            "apply_patch",
            "fs_write",
            "fs_delete",
            "shell",
            "execute",
            "run_command",
            "applypatch",
        ];

        for tool in sensitive {
            assert!(
                is_sensitive_tool(tool),
                "Expected {} to be sensitive",
                tool
            );
        }
    }

    #[test]
    fn test_is_sensitive_tool_non_sensitive() {
        let non_sensitive = vec!["read", "grep", "glob", "search", "question", "plan", "todowrite"];

        for tool in non_sensitive {
            assert!(
                !is_sensitive_tool(tool),
                "Expected {} to be non-sensitive",
                tool
            );
        }
    }

    // ========== InteractivePermissionService tests ==========

    #[tokio::test]
    async fn test_interactive_allows_non_sensitive_without_prompt() {
        let (tx, _rx) = mpsc::channel(1);
        let service = InteractivePermissionService::new(tx);

        let request = make_request("read");
        let result = service.check(&request).await;

        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_interactive_uses_always_allow_list() {
        let (tx, _rx) = mpsc::channel(1);
        let service = InteractivePermissionService::new(tx);

        // Manually add to always allow
        service.add_always_allow("bash".to_string()).unwrap();

        let request = make_request("bash");
        let result = service.check(&request).await;

        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_interactive_uses_always_deny_list() {
        let (tx, _rx) = mpsc::channel(1);
        let service = InteractivePermissionService::new(tx);

        // Manually add to always deny
        service.add_always_deny("edit".to_string()).unwrap();

        let request = make_request("edit");
        let result = service.check(&request).await;

        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn test_interactive_prompts_for_sensitive_new_tool() {
        let (tx, mut rx) = mpsc::channel::<(PermissionRequest, oneshot::Sender<PermissionResponse>)>(1);

        let service = InteractivePermissionService::new(tx);

        // Start the check in background
        let handle = tokio::spawn({
            let service = Arc::new(service);
            async move {
                let request = make_request("bash");
                service.check(&request).await
            }
        });

        // Receive the permission request
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
    async fn test_interactive_adds_to_always_allow_on_allow_always() {
        let (tx, mut rx) = mpsc::channel::<(PermissionRequest, oneshot::Sender<PermissionResponse>)>(1);

        let service = InteractivePermissionService::new(tx);

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

    #[tokio::test]
    async fn test_interactive_adds_to_always_deny_on_deny_always() {
        let (tx, mut rx) = mpsc::channel::<(PermissionRequest, oneshot::Sender<PermissionResponse>)>(1);

        let service = InteractivePermissionService::new(tx);

        let service = Arc::new(service);

        // First call
        let service_clone = Arc::clone(&service);
        let handle = tokio::spawn(async move {
            let request = make_request("edit");
            service_clone.check(&request).await
        });

        let (_received_request, response_tx) = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            rx.recv(),
        )
        .await
        .unwrap()
        .unwrap();

        // Send DenyAlways response
        response_tx.send(PermissionResponse::DenyAlways).unwrap();

        let result = handle.await.unwrap();
        assert!(result.is_ok());
        assert!(!result.unwrap());

        // Second call should use the always_deny list
        let request = make_request("edit");
        let result = service.check(&request).await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }
}
