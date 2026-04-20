//! Hook system for extending agent behavior at key trigger points.
//!
//! This module provides a simple hook system that allows plugins to observe and
//! optionally mutate tool results and messages at defined trigger points.

use rcode_core::error::Result as CoreResult;
use rcode_core::ToolResult;
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use tracing::warn;

/// Hook trigger points where plugins can intercept and modify behavior.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookTrigger {
    /// Called after a tool executes successfully
    AfterToolExecute,
    /// Called when an assistant message is being created
    OnMessage,
}

impl std::fmt::Display for HookTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HookTrigger::AfterToolExecute => write!(f, "after_tool_execute"),
            HookTrigger::OnMessage => write!(f, "on_message"),
        }
    }
}

/// A hook that can intercept and optionally modify tool results and messages.
#[async_trait::async_trait]
pub trait Hook: Send + Sync {
    /// Returns the unique identifier for this hook.
    fn id(&self) -> &str;

    /// Returns the list of triggers this hook is interested in.
    fn triggers(&self) -> Vec<HookTrigger>;

    /// Called after a tool executes.
    /// The hook may modify the tool result in place.
    /// Errors are logged but do not block the executor.
    async fn on_tool_result(&self, tool_id: &str, result: &mut ToolResult) -> CoreResult<()> {
        let _ = tool_id;
        let _ = result;
        Ok(())
    }

    /// Called when an assistant message is being created.
    /// The hook may modify the message content in place.
    /// Errors are logged but do not block the executor.
    async fn on_message(&self, role: &str, content: &mut String) -> CoreResult<()> {
        let _ = role;
        let _ = content;
        Ok(())
    }
}

/// Thread-safe registry for managing hooks and their lifecycle.
pub struct HookRegistry {
    hooks: RwLock<HashMap<String, Arc<dyn Hook>>>,
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl HookRegistry {
    /// Creates a new empty hook registry.
    pub fn new() -> Self {
        Self {
            hooks: RwLock::new(HashMap::new()),
        }
    }

    /// Registers a hook with the registry.
    /// If a hook with the same ID already exists, it is replaced.
    pub fn register(&self, hook: Arc<dyn Hook>) {
        let id = hook.id().to_string();
        tracing::debug!(hook_id = %id, "registering hook");
        self.hooks.write().insert(id, hook);
    }

    /// Unregisters a hook by its ID.
    /// Returns the unregistered hook if it existed.
    pub fn unregister(&self, id: &str) -> Option<Arc<dyn Hook>> {
        tracing::debug!(hook_id = %id, "unregistering hook");
        self.hooks.write().remove(id)
    }

    /// Gets a hook by its ID.
    pub fn get(&self, id: &str) -> Option<Arc<dyn Hook>> {
        self.hooks.read().get(id).cloned()
    }

    /// Returns true if no hooks are registered.
    pub fn is_empty(&self) -> bool {
        self.hooks.read().is_empty()
    }

    /// Returns the number of registered hooks.
    pub fn len(&self) -> usize {
        self.hooks.read().len()
    }

    /// Runs all hooks registered for the AfterToolExecute trigger.
    /// Hooks that error are logged but do not block other hooks.
    /// The original result is used if all hooks fail.
    pub async fn run_tool_result_hooks(
        &self,
        tool_id: &str,
        result: &mut ToolResult,
    ) -> CoreResult<()> {
        // Fast path: no hooks registered
        if self.is_empty() {
            return Ok(());
        }

        let hooks: Vec<Arc<dyn Hook>> = self.hooks.read().values().cloned().collect();

        for hook in hooks {
            if !hook.triggers().contains(&HookTrigger::AfterToolExecute) {
                continue;
            }

            let hook_id = hook.id().to_string();
            match hook.on_tool_result(tool_id, result).await {
                Ok(()) => {
                    tracing::trace!(hook_id = %hook_id, tool_id = %tool_id, "hook executed successfully");
                }
                Err(e) => {
                    // Log error but continue with original result
                    warn!(hook_id = %hook_id, tool_id = %tool_id, error = %e, "hook error - using original result");
                }
            }
        }

        Ok(())
    }

    /// Runs all hooks registered for the OnMessage trigger.
    /// Hooks that error are logged but do not block other hooks.
    /// The original content is used if all hooks fail.
    pub async fn run_message_hooks(&self, role: &str, content: &mut String) -> CoreResult<()> {
        // Fast path: no hooks registered
        if self.is_empty() {
            return Ok(());
        }

        let hooks: Vec<Arc<dyn Hook>> = self.hooks.read().values().cloned().collect();

        for hook in hooks {
            if !hook.triggers().contains(&HookTrigger::OnMessage) {
                continue;
            }

            let hook_id = hook.id().to_string();
            match hook.on_message(role, content).await {
                Ok(()) => {
                    tracing::trace!(hook_id = %hook_id, role = %role, "hook executed successfully");
                }
                Err(e) => {
                    // Log error but continue with original content
                    warn!(hook_id = %hook_id, role = %role, error = %e, "hook error - using original content");
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::ToolResult;

    /// A test hook that modifies tool results
    struct TestModifyHook {
        id: String,
        modify_content: String,
        triggers: Vec<HookTrigger>,
    }

    impl TestModifyHook {
        fn new(id: &str, modify_content: &str) -> Self {
            Self {
                id: id.to_string(),
                modify_content: modify_content.to_string(),
                triggers: vec![HookTrigger::AfterToolExecute],
            }
        }

        /// Creates a hook that modifies message content
        fn new_for_message(id: &str, modify_content: &str) -> Self {
            Self {
                id: id.to_string(),
                modify_content: modify_content.to_string(),
                triggers: vec![HookTrigger::OnMessage],
            }
        }
    }

    #[async_trait::async_trait]
    impl Hook for TestModifyHook {
        fn id(&self) -> &str {
            &self.id
        }

        fn triggers(&self) -> Vec<HookTrigger> {
            self.triggers.clone()
        }

        async fn on_tool_result(&self, _tool_id: &str, result: &mut ToolResult) -> CoreResult<()> {
            result.content = self.modify_content.clone();
            Ok(())
        }

        async fn on_message(&self, _role: &str, content: &mut String) -> CoreResult<()> {
            *content = self.modify_content.clone();
            Ok(())
        }
    }

    /// A test hook that counts invocations
    struct TestCountingHook {
        id: String,
        call_count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    impl TestCountingHook {
        fn new(id: &str) -> (Self, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
            let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let count_clone = count.clone();
            (
                Self {
                    id: id.to_string(),
                    call_count: count_clone,
                },
                count,
            )
        }
    }

    #[async_trait::async_trait]
    impl Hook for TestCountingHook {
        fn id(&self) -> &str {
            &self.id
        }

        fn triggers(&self) -> Vec<HookTrigger> {
            vec![HookTrigger::AfterToolExecute, HookTrigger::OnMessage]
        }

        async fn on_tool_result(&self, _tool_id: &str, _result: &mut ToolResult) -> CoreResult<()> {
            self.call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }

        async fn on_message(&self, _role: &str, _content: &mut String) -> CoreResult<()> {
            self.call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn test_hook_registry_new_is_empty() {
        let registry = HookRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_hook_registry_register_and_get() {
        let registry = HookRegistry::new();
        let hook = Arc::new(TestModifyHook::new("test-hook", "modified"));
        registry.register(hook.clone());

        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);
        // Check that we can retrieve a hook with the same id
        let retrieved = registry.get("test-hook");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id(), "test-hook");
    }

    #[test]
    fn test_hook_registry_unregister() {
        let registry = HookRegistry::new();
        let hook = Arc::new(TestModifyHook::new("test-hook", "modified"));
        registry.register(hook.clone());

        let removed = registry.unregister("test-hook");
        assert!(removed.is_some());
        assert!(registry.is_empty());
        assert!(registry.get("test-hook").is_none());
    }

    #[test]
    fn test_hook_registry_unregister_nonexistent() {
        let registry = HookRegistry::new();
        let removed = registry.unregister("nonexistent");
        assert!(removed.is_none());
    }

    #[test]
    fn test_hook_registry_replace_existing() {
        let registry = HookRegistry::new();
        let hook1 = Arc::new(TestModifyHook::new("test-hook", "content1"));
        let hook2 = Arc::new(TestModifyHook::new("test-hook", "content2"));

        registry.register(hook1);
        registry.register(hook2);

        assert_eq!(registry.len(), 1);
        // The second hook replaces the first
        let retrieved = registry.get("test-hook").unwrap();
        // Both have same ID, but we're just checking replacement works
        drop(retrieved);
    }

    #[tokio::test]
    async fn test_run_tool_result_hooks_empty_registry() {
        let registry = HookRegistry::new();
        let mut result = ToolResult {
            title: "test".to_string(),
            content: "original".to_string(),
            metadata: None,
            attachments: vec![],
        };

        registry.run_tool_result_hooks("bash", &mut result).await.unwrap();
        assert_eq!(result.content, "original");
    }

    #[tokio::test]
    async fn test_run_tool_result_hooks_modifies_result() {
        let registry = HookRegistry::new();
        let hook = Arc::new(TestModifyHook::new("modifier", "modified content"));
        registry.register(hook);

        let mut result = ToolResult {
            title: "test".to_string(),
            content: "original".to_string(),
            metadata: None,
            attachments: vec![],
        };

        registry.run_tool_result_hooks("bash", &mut result).await.unwrap();
        assert_eq!(result.content, "modified content");
    }

    #[tokio::test]
    async fn test_run_tool_result_hooks_multiple_hooks() {
        let registry = HookRegistry::new();
        let (hook1, count1) = TestCountingHook::new("hook1");
        let (hook2, count2) = TestCountingHook::new("hook2");
        registry.register(Arc::new(hook1));
        registry.register(Arc::new(hook2));

        let mut result = ToolResult {
            title: "test".to_string(),
            content: "original".to_string(),
            metadata: None,
            attachments: vec![],
        };

        registry.run_tool_result_hooks("bash", &mut result).await.unwrap();
        // Both hooks should have been called (order not guaranteed, but both should run)
        let count1_val = count1.load(std::sync::atomic::Ordering::SeqCst);
        let count2_val = count2.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(count1_val + count2_val, 2, "both hooks should have been called");
    }

    #[tokio::test]
    async fn test_run_message_hooks_modifies_content() {
        let registry = HookRegistry::new();
        // Use new_for_message to create a hook that triggers on OnMessage
        let hook = Arc::new(TestModifyHook::new_for_message("modifier", "modified message"));
        registry.register(hook);

        let mut content = "original message".to_string();

        registry.run_message_hooks("assistant", &mut content).await.unwrap();
        assert_eq!(content, "modified message");
    }

    #[tokio::test]
    async fn test_run_message_hooks_empty_registry() {
        let registry = HookRegistry::new();
        let mut content = "original".to_string();

        registry.run_message_hooks("assistant", &mut content).await.unwrap();
        assert_eq!(content, "original");
    }

    #[tokio::test]
    async fn test_hook_only_runs_for_relevant_trigger() {
        let registry = HookRegistry::new();
        let (hook, count) = TestCountingHook::new("counting-hook");
        let hook = Arc::new(hook);
        registry.register(hook);

        // Only trigger tool_result, not message
        let mut result = ToolResult {
            title: "test".to_string(),
            content: "original".to_string(),
            metadata: None,
            attachments: vec![],
        };

        registry.run_tool_result_hooks("bash", &mut result).await.unwrap();
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);

        // Now trigger message - should increment again
        let mut content = "test".to_string();
        registry.run_message_hooks("assistant", &mut content).await.unwrap();
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_hook_error_does_not_crash() {
        use rcode_core::error::RCodeError;

        let registry = HookRegistry::new();

        struct FailingHook;
        #[async_trait::async_trait]
        impl Hook for FailingHook {
            fn id(&self) -> &str { "failing" }
            fn triggers(&self) -> Vec<HookTrigger> {
                vec![HookTrigger::AfterToolExecute]
            }
            async fn on_tool_result(&self, _: &str, _: &mut ToolResult) -> CoreResult<()> {
                Err(RCodeError::Agent("hook failed".to_string()))
            }
        }

        registry.register(Arc::new(FailingHook));

        let mut result = ToolResult {
            title: "test".to_string(),
            content: "original".to_string(),
            metadata: None,
            attachments: vec![],
        };

        // Should not panic, just log the error
        registry.run_tool_result_hooks("bash", &mut result).await.unwrap();
        // Original result should be preserved
        assert_eq!(result.content, "original");
    }
}
