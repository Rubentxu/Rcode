//! Hook system for extending agent behavior at key trigger points.
//!
//! This module provides two complementary hook systems:
//! 1. Direct hooks ([Hook]) - for intercepting tool results and messages at trigger points
//! 2. Event hooks ([EventHook]) - for subscribing to event bus events asynchronously
//! 3. Plugin system ([Plugin]) - combines metadata, event hooks, and lifecycle management

use rcode_core::error::Result as CoreResult;
use rcode_core::ToolResult;
use rcode_event::bus::{Event, EventBus, EventSubscriber, EventType};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use parking_lot::RwLock;
use tokio::sync::broadcast::error::RecvError;
use tracing::{debug, warn};

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

// ============================================================================================
// EventHook - Event-based hooks that subscribe to event bus events
// ============================================================================================

/// Event-based hook that subscribes to event bus events.
///
/// Unlike [Hook] which intercepts at specific sync trigger points,
/// [EventHook] receives events asynchronously from the event bus,
/// allowing plugins to react to agent behavior in a decoupled way.
#[async_trait::async_trait]
pub trait EventHook: Send + Sync {
    /// Returns the unique identifier for this hook.
    fn id(&self) -> &str;

    /// Returns the list of event types this hook is subscribed to.
    fn subscribed_events(&self) -> Vec<EventType>;

    /// Called when an event matching this hook's subscriptions is published.
    /// Errors are logged but do not block other hooks or the event system.
    async fn on_event(&self, event: &Event) -> CoreResult<()>;
}

/// Thread-safe registry for managing event hooks.
///
/// [EventHookRegistry] subscribes to the event bus and dispatches events
/// to registered [EventHook] implementations based on their subscriptions.
pub struct EventHookRegistry {
    event_bus: Arc<EventBus>,
    hooks: Arc<RwLock<HashMap<String, Arc<dyn EventHook>>>>,
    shutdown_flag: Arc<AtomicBool>,
    task_handle: RwLock<Option<tokio::task::JoinHandle<()>>>,
}

impl EventHookRegistry {
    /// Creates a new empty event hook registry.
    pub fn new(event_bus: Arc<EventBus>) -> Self {
        Self {
            event_bus,
            hooks: Arc::new(RwLock::new(HashMap::new())),
            shutdown_flag: Arc::new(AtomicBool::new(false)),
            task_handle: RwLock::new(None),
        }
    }

    /// Registers an event hook with the registry.
    /// If a hook with the same ID already exists, it is replaced.
    pub fn register(&self, hook: Arc<dyn EventHook>) {
        let id = hook.id().to_string();
        debug!(hook_id = %id, "registering event hook");
        self.hooks.write().insert(id, hook);
    }

    /// Unregisters an event hook by its ID.
    /// Returns the unregistered hook if it existed.
    pub fn unregister(&self, id: &str) -> Option<Arc<dyn EventHook>> {
        debug!(hook_id = %id, "unregistering event hook");
        self.hooks.write().remove(id)
    }

    /// Gets a hook by its ID.
    pub fn get(&self, id: &str) -> Option<Arc<dyn EventHook>> {
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

    /// Returns true if the registry is currently running.
    /// Returns false if stopped or not yet started.
    pub fn is_running(&self) -> bool {
        self.task_handle.read().is_some() && !self.shutdown_flag.load(Ordering::SeqCst)
    }

    /// Collects all unique event types from registered hooks.
    fn collect_subscribed_events(&self) -> Vec<EventType> {
        let hooks = self.hooks.read();
        let mut types = Vec::new();
        for hook in hooks.values() {
            for et in hook.subscribed_events() {
                if !types.contains(&et) {
                    types.push(et);
                }
            }
        }
        types
    }

    /// Starts the event hook registry, spawning a background task to listen for events.
    ///
    /// This method returns immediately after spawning the task.
    /// The background task will run until [stop] is called or the registry is dropped.
    /// Returns an error if the registry is already running.
    pub async fn start(&self) -> CoreResult<()> {
        // Check if already running
        if self.is_running() {
            return Ok(());
        }

        let subscribed_events = self.collect_subscribed_events();
        if subscribed_events.is_empty() {
            debug!("no event hooks registered, skipping start");
            return Ok(());
        }

        // Reset shutdown flag
        self.shutdown_flag.store(false, Ordering::SeqCst);

        let mut subscriber = self.event_bus.subscribe();
        let hooks = Arc::clone(&self.hooks);
        let shutdown_flag = Arc::clone(&self.shutdown_flag);

        debug!(event_types = ?subscribed_events, "starting event hook registry");

        let handle = tokio::spawn(async move {
            loop {
                if shutdown_flag.load(Ordering::SeqCst) {
                    debug!("event hook registry shutting down");
                    break;
                }

                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_millis(10)) => {
                        // Check shutdown flag periodically
                        continue;
                    }
                    result = subscriber.recv() => {
                        match result {
                            Ok(event) => {
                                let event_type = Self::map_event_to_type(&event);
                                // Collect matching hooks before awaiting (RwLockReadGuard is not Send)
                                let matching_hooks: Vec<Arc<dyn EventHook>> = {
                                    let hooks_guard = hooks.read();
                                    hooks_guard
                                        .values()
                                        .filter(|hook| hook.subscribed_events().contains(&event_type))
                                        .cloned()
                                        .collect()
                                };
                                // Now invoke hooks (guard is dropped)
                                for hook in matching_hooks {
                                    let hook_id = hook.id().to_string();
                                    match hook.on_event(&event).await {
                                        Ok(()) => {
                                            tracing::trace!(hook_id = %hook_id, event_type = %event.event_type(), "event hook executed successfully");
                                        }
                                        Err(e) => {
                                            warn!(hook_id = %hook_id, event_type = %event.event_type(), error = %e, "event hook error - continuing");
                                        }
                                    }
                                }
                            }
                            Err(RecvError::Lagged(_)) => {
                                // Subscriber lagged - continue processing
                                continue;
                            }
                            Err(RecvError::Closed) => {
                                debug!("event bus channel closed, stopping event hook registry");
                                break;
                            }
                        }
                    }
                }
            }
        });

        *self.task_handle.write() = Some(handle);
        Ok(())
    }

    /// Stops the event hook registry gracefully.
    pub fn stop(&self) {
        debug!("stopping event hook registry");
        self.shutdown_flag.store(true, Ordering::SeqCst);
    }

    /// Maps an Event to its corresponding EventType.
    fn map_event_to_type(event: &Event) -> EventType {
        match event {
            Event::SessionCreated { .. } => EventType::SessionCreated,
            Event::SessionUpdated { .. } => EventType::SessionUpdated,
            Event::SessionDeleted { .. } => EventType::SessionDeleted,
            Event::MessageAdded { .. } => EventType::MessageAdded,
            Event::ToolExecuted { .. } => EventType::ToolExecuted,
            Event::AgentStarted { .. } => EventType::AgentStarted,
            Event::AgentFinished { .. } => EventType::AgentFinished,
            Event::ConfigChanged { .. } => EventType::ConfigChanged,
            Event::CompactionPerformed { .. } => EventType::CompactionPerformed,
            Event::StreamingProgress { .. } => EventType::StreamingProgress,
            Event::AppStarted { .. } => EventType::AppStarted,
            Event::AppShutdown { .. } => EventType::AppShutdown,
            Event::AgentError { .. } => EventType::AgentError,
            Event::ToolError { .. } => EventType::ToolError,
            Event::ProviderConnected { .. } => EventType::ProviderConnected,
            Event::ProviderDisconnected { .. } => EventType::ProviderDisconnected,
            Event::ProviderError { .. } => EventType::ProviderError,
            Event::PluginInstalled { .. } => EventType::PluginInstalled,
            Event::PluginActivated { .. } => EventType::PluginActivated,
            Event::PluginDeactivated { .. } => EventType::PluginDeactivated,
            Event::PermissionRequested { .. } => EventType::PermissionRequested,
            Event::PermissionResolved { .. } => EventType::PermissionResolved,
            Event::StreamTextDelta { .. } => EventType::StreamTextDelta,
            Event::StreamReasoningDelta { .. } => EventType::StreamReasoningDelta,
            Event::StreamToolCallStart { .. } => EventType::StreamToolCallStart,
            Event::StreamToolCallArg { .. } => EventType::StreamToolCallArg,
            Event::StreamToolCallEnd { .. } => EventType::StreamToolCallEnd,
            Event::StreamToolResult { .. } => EventType::StreamToolResult,
            Event::StreamAssistantCommitted { .. } => EventType::StreamAssistantCommitted,
            Event::ToolSuggested { .. } => EventType::ToolSuggested,
            Event::ToolApproved { .. } => EventType::ToolApproved,
            Event::ToolDenied { .. } => EventType::ToolDenied,
            Event::OrchestratorDelegationStarted { .. } => EventType::OrchestratorDelegationStarted,
            Event::OrchestratorDelegationCompleted { .. } => EventType::OrchestratorDelegationCompleted,
            Event::OrchestratorAssertionPassed { .. } => EventType::OrchestratorAssertionPassed,
            Event::OrchestratorAssertionFailed { .. } => EventType::OrchestratorAssertionFailed,
        }
    }
}

// ============================================================================================
// Plugin System - Combines metadata, event hooks, and lifecycle management
// ============================================================================================

/// Plugin metadata containing identification information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginMetadata {
    /// Unique identifier for the plugin
    pub id: String,
    /// Human-readable name of the plugin
    pub name: String,
    /// Version string of the plugin (e.g., "1.0.0")
    pub version: String,
    /// Brief description of what the plugin does
    pub description: String,
}

impl PluginMetadata {
    /// Creates a new PluginMetadata instance.
    pub fn new(id: &str, name: &str, version: &str, description: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            version: version.to_string(),
            description: description.to_string(),
        }
    }
}

/// Plugin trait - combines metadata + event hooks + lifecycle.
///
/// A plugin is a self-contained unit that provides functionality through
/// event hooks and lifecycle methods. Plugins are registered with the
/// [PluginManager] which handles their lifecycle and hook registration.
#[async_trait::async_trait]
pub trait Plugin: Send + Sync {
    /// Returns plugin metadata.
    fn metadata(&self) -> &PluginMetadata;

    /// Returns event hooks for this plugin.
    /// These hooks will be automatically registered when the plugin is registered
    /// and unregistered when the plugin is unregistered.
    fn event_hooks(&self) -> Vec<Arc<dyn EventHook>>;

    /// Called when plugin is being initialized (before event loop starts).
    /// Use this to acquire resources, validate configuration, or perform setup.
    /// Errors are logged and the plugin will not be started.
    async fn init(&self) -> CoreResult<()> {
        Ok(())
    }

    /// Called when plugin should start (after event loop starts).
    /// Use this to begin plugin operations, spawn background tasks, etc.
    /// Errors are logged but other plugins continue to start.
    async fn start(&self) -> CoreResult<()> {
        Ok(())
    }

    /// Called when plugin should stop gracefully.
    /// Use this to flush buffers, cancel background tasks, release resources.
    /// Errors are logged but do not prevent other plugins from stopping.
    async fn stop(&self) -> CoreResult<()> {
        Ok(())
    }
}

/// Thread-safe manager for plugins and their lifecycle.
///
/// [PluginManager] manages the registration, lifecycle, and event hook coordination
/// for plugins. It holds an internal [EventHookRegistry] and automatically registers
/// and unregisters plugin event hooks as plugins are added or removed.
pub struct PluginManager {
    /// Internal event hook registry for plugin hooks
    event_hook_registry: Arc<EventHookRegistry>,
    /// Registered plugins by ID
    plugins: RwLock<HashMap<String, Arc<dyn Plugin>>>,
    /// Flag indicating if lifecycle has started
    lifecycle_started: AtomicBool,
}

impl Default for PluginManager {
    fn default() -> Self {
        // This is technically unreachable since PluginManager::new requires an EventBus
        // but we need to implement Default for some use cases
        panic!("PluginManager::default() is not supported, use PluginManager::new(event_bus)")
    }
}

impl PluginManager {
    /// Creates a new empty plugin manager.
    pub fn new(event_bus: Arc<EventBus>) -> Self {
        Self {
            event_hook_registry: Arc::new(EventHookRegistry::new(event_bus)),
            plugins: RwLock::new(HashMap::new()),
            lifecycle_started: AtomicBool::new(false),
        }
    }

    /// Returns a reference to the internal event hook registry.
    /// This allows direct hook registration for non-plugin hooks.
    pub fn event_hook_registry(&self) -> Arc<EventHookRegistry> {
        Arc::clone(&self.event_hook_registry)
    }

    /// Registers a plugin and its event hooks.
    ///
    /// If a plugin with the same ID is already registered, it will be replaced.
    /// If lifecycle has already started, the new plugin's `start()` will be called immediately.
    ///
    /// # Arguments
    /// * `plugin` - The plugin to register
    ///
    /// # Example
    /// ```
    /// use rcode_agent::hooks::{PluginManager, Plugin, PluginMetadata};
    /// use rcode_event::bus::EventBus;
    /// use std::sync::Arc;
    ///
///     struct MyPlugin {
///         metadata: PluginMetadata,
///     }
///     impl MyPlugin {
///         fn new() -> Self {
///             Self { metadata: PluginMetadata::new("my-plugin", "My Plugin", "1.0.0", "A test plugin") }
///         }
///     }
///     impl Plugin for MyPlugin {
///         fn metadata(&self) -> &PluginMetadata {
///             &self.metadata
///         }
///         fn event_hooks(&self) -> Vec<Arc<dyn rcode_agent::hooks::EventHook>> {
///             vec![]
///         }
///     }
///     ```
    pub async fn register(&self, plugin: Arc<dyn Plugin>) -> CoreResult<()> {
        let plugin_id = plugin.metadata().id.clone();

        // Call init before registration
        let init_result = plugin.init().await;
        if init_result.is_err() {
            warn!(plugin_id = %plugin_id, "plugin init failed, skipping registration");
            return init_result;
        }

        debug!(plugin_id = %plugin_id, "registering plugin");

        // Register plugin event hooks
        for hook in plugin.event_hooks() {
            self.event_hook_registry.register(hook);
        }

        // Store plugin
        self.plugins.write().insert(plugin_id.clone(), plugin);

        // If lifecycle already started, call start on the new plugin
        if self.lifecycle_started.load(Ordering::SeqCst) {
            let plugin = self.plugins.read().get(&plugin_id).cloned();
            if let Some(p) = plugin {
                if let Err(e) = p.start().await {
                    warn!(plugin_id = %plugin_id, error = %e, "plugin start failed");
                }
            }
        }

        Ok(())
    }

    /// Unregisters a plugin and its event hooks.
    ///
    /// Calls the plugin's `stop()` method before removal if lifecycle has started.
    ///
    /// # Arguments
    /// * `plugin_id` - The ID of the plugin to unregister
    ///
    /// # Returns
    /// The unregistered plugin if it existed, `None` otherwise.
    pub async fn unregister(&self, plugin_id: &str) -> Option<Arc<dyn Plugin>> {
        debug!(plugin_id = %plugin_id, "unregistering plugin");

        // Get plugin reference before removal
        let plugin = self.plugins.write().remove(plugin_id);

        if let Some(ref p) = plugin {
            // Call stop if lifecycle was started
            if self.lifecycle_started.load(Ordering::SeqCst) {
                if let Err(e) = p.stop().await {
                    warn!(plugin_id = %plugin_id, error = %e, "plugin stop failed during unregister");
                }
            }

            // Unregister plugin event hooks
            for hook in p.event_hooks() {
                self.event_hook_registry.unregister(hook.id());
            }
        }

        plugin
    }

    /// Gets a plugin by its ID.
    pub fn get(&self, plugin_id: &str) -> Option<Arc<dyn Plugin>> {
        self.plugins.read().get(plugin_id).cloned()
    }

    /// Returns a list of all loaded plugins.
    pub fn list_plugins(&self) -> Vec<Arc<dyn Plugin>> {
        self.plugins.read().values().cloned().collect()
    }

    /// Returns the number of registered plugins.
    pub fn len(&self) -> usize {
        self.plugins.read().len()
    }

    /// Returns true if no plugins are registered.
    pub fn is_empty(&self) -> bool {
        self.plugins.read().is_empty()
    }

    /// Starts all registered plugins and the event hook registry.
    ///
    /// Calls `start()` on all plugins and then starts the event hook registry.
    /// Idempotent - calling when already started is a no-op.
    pub async fn start_all(&self) -> CoreResult<()> {
        if self.lifecycle_started.load(Ordering::SeqCst) {
            debug!("lifecycle already started, skipping");
            return Ok(());
        }

        debug!("starting all plugins");

        // Call start on all plugins
        let plugins: Vec<Arc<dyn Plugin>> = self.plugins.read().values().cloned().collect();
        for plugin in plugins {
            let plugin_id = plugin.metadata().id.clone();
            if let Err(e) = plugin.start().await {
                warn!(plugin_id = %plugin_id, error = %e, "plugin start failed");
            }
        }

        // Mark lifecycle as started
        self.lifecycle_started.store(true, Ordering::SeqCst);

        // Start event hook registry
        self.event_hook_registry.start().await
    }

    /// Stops all registered plugins and the event hook registry gracefully.
    ///
    /// Calls `stop()` on all plugins and then stops the event hook registry.
    /// Idempotent - calling when already stopped is a no-op.
    pub async fn stop_all(&self) -> CoreResult<()> {
        if !self.lifecycle_started.load(Ordering::SeqCst) {
            debug!("lifecycle not started, skipping stop");
            return Ok(());
        }

        debug!("stopping all plugins");

        // Stop event hook registry first
        self.event_hook_registry.stop();

        // Call stop on all plugins
        let plugins: Vec<Arc<dyn Plugin>> = self.plugins.read().values().cloned().collect();
        for plugin in plugins {
            let plugin_id = plugin.metadata().id.clone();
            if let Err(e) = plugin.stop().await {
                warn!(plugin_id = %plugin_id, error = %e, "plugin stop failed");
            }
        }

        // Mark lifecycle as stopped
        self.lifecycle_started.store(false, Ordering::SeqCst);

        Ok(())
    }
}

// ============================================================================================
// Tests for HookRegistry (existing)
// ============================================================================================

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

// ============================================================================================
// Tests for EventHookRegistry
// ============================================================================================

#[cfg(test)]
mod event_hook_tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::timeout;

    /// Test EventHook implementation that counts invocations
    struct CountingEventHook {
        id: String,
        subscribed: Vec<EventType>,
        call_count: Arc<std::sync::atomic::AtomicUsize>,
        last_event: std::sync::Arc<RwLock<Option<Event>>>,
    }

    impl CountingEventHook {
        fn new(id: &str, subscribed: Vec<EventType>) -> (Self, Arc<std::sync::atomic::AtomicUsize>, Arc<RwLock<Option<Event>>>) {
            let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let last_event = Arc::new(RwLock::new(None));
            (
                Self {
                    id: id.to_string(),
                    subscribed,
                    call_count: call_count.clone(),
                    last_event: last_event.clone(),
                },
                call_count,
                last_event,
            )
        }
    }

    #[async_trait::async_trait]
    impl EventHook for CountingEventHook {
        fn id(&self) -> &str {
            &self.id
        }

        fn subscribed_events(&self) -> Vec<EventType> {
            self.subscribed.clone()
        }

        async fn on_event(&self, event: &Event) -> CoreResult<()> {
            self.call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            *self.last_event.write() = Some(event.clone());
            Ok(())
        }
    }

    /// Test EventHook that returns an error
    struct ErrorEventHook {
        id: String,
        subscribed: Vec<EventType>,
        error_msg: String,
    }

    impl ErrorEventHook {
        fn new(id: &str, subscribed: Vec<EventType>, error_msg: &str) -> Self {
            Self {
                id: id.to_string(),
                subscribed,
                error_msg: error_msg.to_string(),
            }
        }
    }

    #[async_trait::async_trait]
    impl EventHook for ErrorEventHook {
        fn id(&self) -> &str {
            &self.id
        }

        fn subscribed_events(&self) -> Vec<EventType> {
            self.subscribed.clone()
        }

        async fn on_event(&self, _event: &Event) -> CoreResult<()> {
            Err(rcode_core::error::RCodeError::Agent(self.error_msg.clone()))
        }
    }

    #[test]
    fn test_event_hook_registry_new_is_empty() {
        let bus = Arc::new(EventBus::new(10));
        let registry = EventHookRegistry::new(bus);
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_event_hook_registry_register_and_get() {
        let bus = Arc::new(EventBus::new(10));
        let registry = EventHookRegistry::new(bus);
        let (hook, _, _) = CountingEventHook::new("hook1", vec![EventType::SessionCreated]);

        registry.register(Arc::new(hook));

        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.get("hook1").unwrap().id(), "hook1");
    }

    #[test]
    fn test_event_hook_registry_unregister() {
        let bus = Arc::new(EventBus::new(10));
        let registry = EventHookRegistry::new(bus);
        let (hook, _, _) = CountingEventHook::new("hook1", vec![EventType::SessionCreated]);

        registry.register(Arc::new(hook));
        let removed = registry.unregister("hook1");

        assert!(removed.is_some());
        assert!(registry.is_empty());
        assert!(registry.get("hook1").is_none());
    }

    #[test]
    fn test_event_hook_registry_unregister_nonexistent() {
        let bus = Arc::new(EventBus::new(10));
        let registry = EventHookRegistry::new(bus);
        let removed = registry.unregister("nonexistent");
        assert!(removed.is_none());
    }

    #[tokio::test]
    async fn test_event_hook_registry_receives_matching_event() {
        let bus = Arc::new(EventBus::new(10));
        let registry = EventHookRegistry::new(Arc::clone(&bus));
        let (hook, call_count, _) = CountingEventHook::new("hook1", vec![EventType::SessionCreated]);

        registry.register(Arc::new(hook));
        registry.start().await.expect("start should succeed");

        // Give the task time to start
        tokio::task::yield_now().await;

        // Publish an event
        bus.publish(Event::SessionCreated { session_id: "s1".to_string() });

        // Wait for the event to be processed
        timeout(Duration::from_millis(100), async {
            loop {
                if call_count.load(std::sync::atomic::Ordering::SeqCst) > 0 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        }).await.expect("timeout waiting for event");

        registry.stop();
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_event_hook_registry_ignores_non_matching_event() {
        let bus = Arc::new(EventBus::new(10));
        let registry = EventHookRegistry::new(Arc::clone(&bus));
        let (hook, call_count, _) = CountingEventHook::new("hook1", vec![EventType::SessionCreated]);

        registry.register(Arc::new(hook));
        registry.start().await.expect("start should succeed");

        // Give the task time to start
        tokio::task::yield_now().await;

        // Publish a non-matching event
        bus.publish(Event::AgentStarted { session_id: "s1".to_string() });

        // Wait a bit to ensure no event was processed
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;

        registry.stop();
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_event_hook_registry_multiple_hooks_same_event() {
        let bus = Arc::new(EventBus::new(10));
        let registry = EventHookRegistry::new(Arc::clone(&bus));
        let (hook1, count1, _) = CountingEventHook::new("hook1", vec![EventType::SessionCreated]);
        let (hook2, count2, _) = CountingEventHook::new("hook2", vec![EventType::SessionCreated]);

        registry.register(Arc::new(hook1));
        registry.register(Arc::new(hook2));
        registry.start().await.expect("start should succeed");

        // Give the task time to start
        tokio::task::yield_now().await;

        // Publish an event
        bus.publish(Event::SessionCreated { session_id: "s1".to_string() });

        // Wait for both hooks to be called
        timeout(Duration::from_millis(100), async {
            loop {
                let total = count1.load(std::sync::atomic::Ordering::SeqCst)
                    + count2.load(std::sync::atomic::Ordering::SeqCst);
                if total >= 2 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        }).await.expect("timeout waiting for events");

        registry.stop();
        assert_eq!(count1.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(count2.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_event_hook_registry_hook_error_does_not_crash() {
        let bus = Arc::new(EventBus::new(10));
        let registry = EventHookRegistry::new(Arc::clone(&bus));
        let (hook, call_count, _) = CountingEventHook::new("hook1", vec![EventType::SessionCreated]);
        let error_hook = ErrorEventHook::new("error-hook", vec![EventType::SessionCreated], "intentional error");

        registry.register(Arc::new(hook));
        registry.register(Arc::new(error_hook));
        registry.start().await.expect("start should succeed");

        // Give the task time to start
        tokio::task::yield_now().await;

        // Publish an event - both hooks should be called despite error
        bus.publish(Event::SessionCreated { session_id: "s1".to_string() });

        // Wait for the event to be processed
        timeout(Duration::from_millis(100), async {
            loop {
                if call_count.load(std::sync::atomic::Ordering::SeqCst) > 0 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        }).await.expect("timeout waiting for event");

        registry.stop();
        // The counting hook should have been called (error hook's failure shouldn't prevent it)
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_event_hook_registry_empty_start_is_no_op() {
        let bus = Arc::new(EventBus::new(10));
        let registry = EventHookRegistry::new(bus);

        // Start with no hooks registered - should not panic
        registry.start().await.expect("start should succeed");

        // The registry should still be empty
        assert!(registry.is_empty());
    }
}

// ============================================================================================
// Tests for PluginManager
// ============================================================================================

#[cfg(test)]
mod plugin_tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;

    /// Test plugin that tracks lifecycle calls
    struct TestPlugin {
        metadata: PluginMetadata,
        event_hooks: Vec<Arc<dyn EventHook>>,
        init_called: std::sync::Arc<std::sync::atomic::AtomicBool>,
        start_called: std::sync::Arc<std::sync::atomic::AtomicBool>,
        stop_called: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }

    impl TestPlugin {
        fn new(id: &str, name: &str) -> (Self, Arc<std::sync::atomic::AtomicBool>, Arc<std::sync::atomic::AtomicBool>, Arc<std::sync::atomic::AtomicBool>) {
            let init_called = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let start_called = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let stop_called = Arc::new(std::sync::atomic::AtomicBool::new(false));
            (
                Self {
                    metadata: PluginMetadata::new(id, name, "1.0.0", "A test plugin"),
                    event_hooks: vec![],
                    init_called: init_called.clone(),
                    start_called: start_called.clone(),
                    stop_called: stop_called.clone(),
                },
                init_called,
                start_called,
                stop_called,
            )
        }

        fn with_event_hooks(mut self, hooks: Vec<Arc<dyn EventHook>>) -> Self {
            self.event_hooks = hooks;
            self
        }
    }

    #[async_trait::async_trait]
    impl Plugin for TestPlugin {
        fn metadata(&self) -> &PluginMetadata {
            &self.metadata
        }

        fn event_hooks(&self) -> Vec<Arc<dyn EventHook>> {
            self.event_hooks.clone()
        }

        async fn init(&self) -> CoreResult<()> {
            self.init_called.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }

        async fn start(&self) -> CoreResult<()> {
            self.start_called.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }

        async fn stop(&self) -> CoreResult<()> {
            self.stop_called.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
    }

    /// Test EventHook implementation for plugin tests
    struct TestPluginEventHook {
        id: String,
        subscribed: Vec<EventType>,
    }

    impl TestPluginEventHook {
        fn new(id: &str) -> Self {
            Self {
                id: id.to_string(),
                subscribed: vec![EventType::SessionCreated],
            }
        }
    }

    #[async_trait::async_trait]
    impl EventHook for TestPluginEventHook {
        fn id(&self) -> &str {
            &self.id
        }

        fn subscribed_events(&self) -> Vec<EventType> {
            self.subscribed.clone()
        }

        async fn on_event(&self, _event: &Event) -> CoreResult<()> {
            Ok(())
        }
    }

    #[test]
    fn test_plugin_metadata_new() {
        let metadata = PluginMetadata::new("test-plugin", "Test Plugin", "1.0.0", "A test plugin");
        assert_eq!(metadata.id, "test-plugin");
        assert_eq!(metadata.name, "Test Plugin");
        assert_eq!(metadata.version, "1.0.0");
        assert_eq!(metadata.description, "A test plugin");
    }

    #[test]
    fn test_plugin_metadata_debug() {
        let metadata = PluginMetadata::new("test-plugin", "Test Plugin", "1.0.0", "A test plugin");
        let debug_str = format!("{:?}", metadata);
        assert!(debug_str.contains("test-plugin"));
        assert!(debug_str.contains("Test Plugin"));
    }

    #[test]
    fn test_plugin_metadata_clone() {
        let metadata = PluginMetadata::new("test-plugin", "Test Plugin", "1.0.0", "A test plugin");
        let cloned = metadata.clone();
        assert_eq!(cloned, metadata);
    }

    #[test]
    fn test_plugin_manager_new_is_empty() {
        let bus = Arc::new(EventBus::new(10));
        let manager = PluginManager::new(bus);
        assert!(manager.is_empty());
        assert_eq!(manager.len(), 0);
    }

    #[tokio::test]
    async fn test_plugin_manager_register_single_plugin() {
        let bus = Arc::new(EventBus::new(10));
        let manager = PluginManager::new(Arc::clone(&bus));
        let (plugin, _, _, _) = TestPlugin::new("plugin1", "Plugin 1");

        manager.register(Arc::new(plugin)).await.unwrap();

        assert!(!manager.is_empty());
        assert_eq!(manager.len(), 1);
        assert!(manager.get("plugin1").is_some());
    }

    #[tokio::test]
    async fn test_plugin_manager_register_calls_init() {
        let bus = Arc::new(EventBus::new(10));
        let manager = PluginManager::new(Arc::clone(&bus));
        let (plugin, init_called, _, _) = TestPlugin::new("plugin1", "Plugin 1");

        manager.register(Arc::new(plugin)).await.unwrap();

        assert!(init_called.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_plugin_manager_register_duplicate_replaces() {
        let bus = Arc::new(EventBus::new(10));
        let manager = PluginManager::new(Arc::clone(&bus));
        let (plugin1, _, _, _) = TestPlugin::new("plugin1", "Plugin 1");
        let (plugin2, _, _, _) = TestPlugin::new("plugin1", "Plugin 1 Updated");

        manager.register(Arc::new(plugin1)).await.unwrap();
        manager.register(Arc::new(plugin2)).await.unwrap();

        assert_eq!(manager.len(), 1);
        // Should return updated plugin
        assert_eq!(manager.get("plugin1").unwrap().metadata().name, "Plugin 1 Updated");
    }

    #[test]
    fn test_plugin_manager_get_nonexistent() {
        let bus = Arc::new(EventBus::new(10));
        let manager = PluginManager::new(bus);
        assert!(manager.get("nonexistent").is_none());
    }

    #[tokio::test]
    async fn test_plugin_manager_list_plugins() {
        let bus = Arc::new(EventBus::new(10));
        let manager = PluginManager::new(Arc::clone(&bus));
        let (plugin1, _, _, _) = TestPlugin::new("plugin1", "Plugin 1");
        let (plugin2, _, _, _) = TestPlugin::new("plugin2", "Plugin 2");

        manager.register(Arc::new(plugin1)).await.unwrap();
        manager.register(Arc::new(plugin2)).await.unwrap();

        let plugins = manager.list_plugins();
        assert_eq!(plugins.len(), 2);
    }

    #[tokio::test]
    async fn test_plugin_manager_start_all_calls_start() {
        let bus = Arc::new(EventBus::new(10));
        let manager = PluginManager::new(Arc::clone(&bus));
        let (plugin1, _, start1, _) = TestPlugin::new("plugin1", "Plugin 1");
        let (plugin2, _, start2, _) = TestPlugin::new("plugin2", "Plugin 2");

        manager.register(Arc::new(plugin1)).await.unwrap();
        manager.register(Arc::new(plugin2)).await.unwrap();

        manager.start_all().await.unwrap();

        assert!(start1.load(std::sync::atomic::Ordering::SeqCst));
        assert!(start2.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_plugin_manager_start_all_starts_event_registry() {
        let bus = Arc::new(EventBus::new(10));
        let manager = PluginManager::new(Arc::clone(&bus));
        let (plugin, _, _, _) = TestPlugin::new("plugin1", "Plugin 1");
        let hook = Arc::new(TestPluginEventHook::new("hook1"));

        // Create plugin with event hook
        let plugin = TestPlugin::new("plugin1", "Plugin 1").0;
        let plugin = plugin.with_event_hooks(vec![hook]);

        manager.register(Arc::new(plugin)).await.unwrap();
        manager.start_all().await.unwrap();

        // Event hook registry should be running
        assert!(manager.event_hook_registry().is_running());
    }

    #[tokio::test]
    async fn test_plugin_manager_stop_all_calls_stop() {
        let bus = Arc::new(EventBus::new(10));
        let manager = PluginManager::new(Arc::clone(&bus));
        let (plugin1, _, _, stop1) = TestPlugin::new("plugin1", "Plugin 1");
        let (plugin2, _, _, stop2) = TestPlugin::new("plugin2", "Plugin 2");

        manager.register(Arc::new(plugin1)).await.unwrap();
        manager.register(Arc::new(plugin2)).await.unwrap();

        manager.start_all().await.unwrap();
        manager.stop_all().await.unwrap();

        assert!(stop1.load(std::sync::atomic::Ordering::SeqCst));
        assert!(stop2.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_plugin_manager_stop_all_stops_event_registry() {
        let bus = Arc::new(EventBus::new(10));
        let manager = PluginManager::new(Arc::clone(&bus));
        let (plugin, _, _, _) = TestPlugin::new("plugin1", "Plugin 1");

        manager.register(Arc::new(plugin)).await.unwrap();
        manager.start_all().await.unwrap();
        manager.stop_all().await.unwrap();

        // Event hook registry should not be running after stop
        assert!(!manager.event_hook_registry().is_running());
    }

    #[tokio::test]
    async fn test_plugin_manager_unregister_calls_stop() {
        let bus = Arc::new(EventBus::new(10));
        let manager = PluginManager::new(Arc::clone(&bus));
        let (plugin, _, _, stop_called) = TestPlugin::new("plugin1", "Plugin 1");

        manager.register(Arc::new(plugin)).await.unwrap();
        manager.start_all().await.unwrap();

        manager.unregister("plugin1").await;

        assert!(stop_called.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_plugin_manager_unregister_removes_hooks() {
        let bus = Arc::new(EventBus::new(10));
        let manager = PluginManager::new(Arc::clone(&bus));
        let hook = Arc::new(TestPluginEventHook::new("hook1"));
        let hook_id = hook.id().to_string();

        let plugin = TestPlugin::new("plugin1", "Plugin 1").0;
        let plugin = plugin.with_event_hooks(vec![hook]);

        manager.register(Arc::new(plugin)).await.unwrap();
        manager.start_all().await.unwrap();

        // Hook should be registered
        assert!(manager.event_hook_registry().get(&hook_id).is_some());

        manager.unregister("plugin1").await;

        // Hook should be unregistered
        assert!(manager.event_hook_registry().get(&hook_id).is_none());
    }

    #[tokio::test]
    async fn test_plugin_manager_unregister_nonexistent() {
        let bus = Arc::new(EventBus::new(10));
        let manager = PluginManager::new(Arc::clone(&bus));

        let result = manager.unregister("nonexistent").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_plugin_manager_plugin_hooks_are_registered() {
        let bus = Arc::new(EventBus::new(10));
        let manager = PluginManager::new(Arc::clone(&bus));
        let hook1 = Arc::new(TestPluginEventHook::new("hook1"));
        let hook2 = Arc::new(TestPluginEventHook::new("hook2"));

        let plugin = TestPlugin::new("plugin1", "Plugin 1").0;
        let plugin = plugin.with_event_hooks(vec![hook1, hook2]);

        manager.register(Arc::new(plugin)).await.unwrap();

        // Both hooks should be registered
        assert!(manager.event_hook_registry().get("hook1").is_some());
        assert!(manager.event_hook_registry().get("hook2").is_some());
        assert_eq!(manager.event_hook_registry().len(), 2);
    }

    #[tokio::test]
    async fn test_plugin_manager_multiple_plugins_coexist() {
        let bus = Arc::new(EventBus::new(10));
        let manager = PluginManager::new(Arc::clone(&bus));
        let hook1 = Arc::new(TestPluginEventHook::new("hook1"));
        let hook2 = Arc::new(TestPluginEventHook::new("hook2"));

        let plugin1 = TestPlugin::new("plugin1", "Plugin 1").0;
        let plugin1 = plugin1.with_event_hooks(vec![hook1]);

        let plugin2 = TestPlugin::new("plugin2", "Plugin 2").0;
        let plugin2 = plugin2.with_event_hooks(vec![hook2]);

        manager.register(Arc::new(plugin1)).await.unwrap();
        manager.register(Arc::new(plugin2)).await.unwrap();

        assert_eq!(manager.len(), 2);
        assert!(manager.get("plugin1").is_some());
        assert!(manager.get("plugin2").is_some());
        assert!(manager.event_hook_registry().get("hook1").is_some());
        assert!(manager.event_hook_registry().get("hook2").is_some());
    }

    #[tokio::test]
    async fn test_plugin_manager_register_after_start_calls_start() {
        let bus = Arc::new(EventBus::new(10));
        let manager = PluginManager::new(Arc::clone(&bus));
        let (plugin1, _, _, _) = TestPlugin::new("plugin1", "Plugin 1");
        let (plugin2, _, start2, _) = TestPlugin::new("plugin2", "Plugin 2");

        manager.register(Arc::new(plugin1)).await.unwrap();
        manager.start_all().await.unwrap();

        // Register new plugin after start
        manager.register(Arc::new(plugin2)).await.unwrap();

        // New plugin's start should have been called
        assert!(start2.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_plugin_manager_idempotent_start_stop() {
        let bus = Arc::new(EventBus::new(10));
        let manager = PluginManager::new(Arc::clone(&bus));
        let hook = Arc::new(TestPluginEventHook::new("hook1"));

        let plugin = TestPlugin::new("plugin1", "Plugin 1").0;
        let plugin = plugin.with_event_hooks(vec![hook]);

        manager.register(Arc::new(plugin)).await.unwrap();

        // Start twice should be idempotent
        manager.start_all().await.unwrap();
        manager.start_all().await.unwrap();

        assert!(manager.event_hook_registry().is_running());

        // Stop twice should be idempotent
        manager.stop_all().await.unwrap();
        manager.stop_all().await.unwrap();

        assert!(!manager.event_hook_registry().is_running());
    }

    #[tokio::test]
    async fn test_plugin_manager_event_hook_receives_events() {
        let bus = Arc::new(EventBus::new(10));
        let manager = PluginManager::new(Arc::clone(&bus));
        let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        // Create a hook that counts invocations
        struct CountingHook {
            id: String,
            subscribed: Vec<EventType>,
            call_count: Arc<std::sync::atomic::AtomicUsize>,
        }

        impl CountingHook {
            fn new(id: &str, call_count: Arc<std::sync::atomic::AtomicUsize>) -> Self {
                Self {
                    id: id.to_string(),
                    subscribed: vec![EventType::SessionCreated],
                    call_count,
                }
            }
        }

        #[async_trait::async_trait]
        impl EventHook for CountingHook {
            fn id(&self) -> &str { &self.id }
            fn subscribed_events(&self) -> Vec<EventType> { self.subscribed.clone() }
            async fn on_event(&self, _event: &Event) -> CoreResult<()> {
                self.call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            }
        }

        let hook = Arc::new(CountingHook::new("counting-hook", call_count_clone));
        let plugin = TestPlugin::new("plugin1", "Plugin 1").0;
        let plugin = plugin.with_event_hooks(vec![hook]);

        manager.register(Arc::new(plugin)).await.unwrap();
        manager.start_all().await.unwrap();

        // Give the task time to start
        tokio::task::yield_now().await;

        // Publish an event
        bus.publish(Event::SessionCreated { session_id: "s1".to_string() });

        // Wait for the event to be processed
        timeout(Duration::from_millis(100), async {
            loop {
                if call_count.load(std::sync::atomic::Ordering::SeqCst) > 0 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        }).await.expect("timeout waiting for event");

        manager.stop_all().await.unwrap();
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_plugin_manager_stop_all_before_start() {
        let bus = Arc::new(EventBus::new(10));
        let manager = PluginManager::new(Arc::clone(&bus));
        let (plugin, _, _, stop_called) = TestPlugin::new("plugin1", "Plugin 1");

        manager.register(Arc::new(plugin)).await.unwrap();

        // Stop before start should be a no-op (stop not called because never started)
        manager.stop_all().await.unwrap();

        // Plugin was never started, so stop should not be called
        assert!(!stop_called.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn test_plugin_manager_event_hook_registry_access() {
        let bus = Arc::new(EventBus::new(10));
        let manager = PluginManager::new(Arc::clone(&bus));

        // Should be able to access event hook registry directly
        let registry = manager.event_hook_registry();
        assert!(registry.is_empty());

        // Register a hook directly on the registry
        let hook = Arc::new(TestPluginEventHook::new("direct-hook"));
        registry.register(hook);

        assert!(!registry.is_empty());
    }
}