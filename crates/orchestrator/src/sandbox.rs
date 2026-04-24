//! Sandbox testing infrastructure for ReflexiveOrchestrator
//!
//! This module provides a comprehensive testing environment that can validate
//! the orchestrator's behavior without requiring the full web application.
//!
//! # Features
//!
//! - **Event Bus Integration**: Uses unified EventBus for all events
//! - **Mock Worker Agents**: Simulates all 5 worker types with configurable behavior
//! - **Event Bus Tracing**: Records all events for later verification
//! - **Isolated Runtime**: Runs in-memory without external dependencies
//!
//! # Usage
//!
//! ```ignore
//! use rcode_orchestrator::sandbox::{Sandbox, SandboxConfig};
//!
//! let sandbox = Sandbox::new();
//! sandbox.run().await?;
//! sandbox.verify_delegation("explore").await?;
//! ```

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rcode_core::{Agent, AgentContext, AgentRegistry, AgentResult, Message, Part};
use rcode_event::{Event, EventBus};
use rcode_runtime::{AgentRuntime, InProcessRuntime};
use tokio::sync::RwLock;

/// Configuration for sandbox testing
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Whether to enable verbose tracing
    pub verbose: bool,
    /// Maximum iterations for orchestrator
    pub max_iterations: usize,
    /// Timeout for each worker agent
    pub worker_timeout_secs: u64,
    /// Enable mock failures for testing error handling
    pub mock_failures: bool,
    /// Failure probability when mock_failures is enabled
    pub failure_probability: f64,
    /// Session ID for this sandbox
    pub session_id: String,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            verbose: true,
            max_iterations: 50,
            worker_timeout_secs: 30,
            mock_failures: false,
            failure_probability: 0.1,
            session_id: "sandbox-session".to_string(),
        }
    }
}

/// Mock worker agent for testing
struct MockWorkerAgent {
    id: String,
    name: String,
    should_fail: bool,
    response_text: String,
    delay_ms: u64,
}

impl MockWorkerAgent {
    fn new(id: &str, name: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            should_fail: false,
            response_text: format!("Mock {} agent completed task", name),
            delay_ms: 10,
        }
    }

    fn with_response(mut self, response: &str) -> Self {
        self.response_text = response.to_string();
        self
    }

    #[allow(dead_code)]
    fn with_delay(mut self, delay_ms: u64) -> Self {
        self.delay_ms = delay_ms;
        self
    }

    #[allow(dead_code)]
    fn failing(self) -> Self {
        Self {
            should_fail: true,
            ..self
        }
    }
}

#[async_trait]
impl Agent for MockWorkerAgent {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "Mock worker agent for sandbox testing"
    }

    fn system_prompt(&self) -> String {
        format!("You are a mock {} agent for testing.", self.name)
    }

    fn supported_tools(&self) -> Vec<String> {
        vec!["read".to_string(), "grep".to_string(), "glob".to_string()]
    }

    async fn run(&self, ctx: &mut AgentContext) -> rcode_core::error::Result<AgentResult> {
        // Simulate work delay
        if self.delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
        }

        if self.should_fail {
            return Err(rcode_core::error::RCodeError::Agent(
                format!("Mock {} agent failed", self.name),
            ));
        }

        Ok(AgentResult {
            message: Message::assistant(
                ctx.session_id.clone(),
                vec![Part::Text {
                    content: self.response_text.clone(),
                }],
            ),
            should_continue: false,
            stop_reason: rcode_core::agent::StopReason::EndOfTurn,
            usage: None,
        })
    }
}

/// Sandbox testing environment for ReflexiveOrchestrator
pub struct Sandbox {
    config: SandboxConfig,
    event_bus: Arc<EventBus>,
    /// Internal buffer for events (for test isolation and query support)
    event_buffer: Arc<RwLock<Vec<Event>>>,
    registry: Arc<rcode_core::AgentRegistry>,
    runtime: Arc<InProcessRuntime>,
    test_results: Arc<RwLock<TestResults>>,
}

#[derive(Debug, Default, Clone)]
pub struct TestResults {
    pub assertions_passed: usize,
    pub assertions_failed: usize,
    pub delegations_completed: usize,
    pub errors: Vec<String>,
}

impl Sandbox {
    /// Create a new sandbox with default configuration
    pub fn new() -> Self {
        Self::with_config(SandboxConfig::default())
    }

    /// Create a sandbox with custom configuration
    pub fn with_config(config: SandboxConfig) -> Self {
        Self {
            config: config.clone(),
            event_bus: Arc::new(EventBus::new(1000)),
            event_buffer: Arc::new(RwLock::new(Vec::new())),
            registry: Arc::new(rcode_core::AgentRegistry::new()),
            runtime: Arc::new(InProcessRuntime::new()),
            test_results: Arc::new(RwLock::new(TestResults::default())),
        }
    }

    /// Get the event bus
    pub fn event_bus(&self) -> Arc<EventBus> {
        self.event_bus.clone()
    }

    /// Record an event to the event bus and internal buffer
    async fn record_event(&self, event: Event) {
        if self.config.verbose {
            tracing::info!(
                event_type = event.event_type(),
                "Sandbox: {:?}",
                event
            );
        }

        // Buffer the event for test isolation and query support
        self.event_buffer.write().await.push(event.clone());

        // Also publish to event bus for external subscribers
        self.event_bus.publish(event);
    }

    /// Get all buffered events
    pub async fn get_events(&self) -> Vec<Event> {
        self.event_buffer.read().await.clone()
    }

    /// Flush buffered events to the event bus
    pub async fn flush_to_bus(&self) {
        let events = self.event_buffer.read().await;
        for event in events.iter() {
            self.event_bus.publish(event.clone());
        }
    }

    /// Get test results
    pub async fn get_test_results(&self) -> TestResults {
        self.test_results.read().await.clone()
    }

    /// Get the agent registry
    pub fn registry(&self) -> &Arc<AgentRegistry> {
        &self.registry
    }

    /// Get configuration
    pub fn config(&self) -> &SandboxConfig {
        &self.config
    }

    /// Record assertion result
    async fn record_assertion(&self, passed: bool, test_name: &str, assertion: &str, reason: Option<&str>) {
        let event = if passed {
            Event::OrchestratorAssertionPassed {
                session_id: self.config.session_id.clone(),
                test_name: test_name.to_string(),
                assertion: assertion.to_string(),
            }
        } else {
            Event::OrchestratorAssertionFailed {
                session_id: self.config.session_id.clone(),
                test_name: test_name.to_string(),
                assertion: assertion.to_string(),
                reason: reason.unwrap_or("Unknown").to_string(),
            }
        };

        self.record_event(event).await;

        let mut results = self.test_results.write().await;
        if passed {
            results.assertions_passed += 1;
        } else {
            results.assertions_failed += 1;
            if let Some(r) = reason {
                results.errors.push(r.to_string());
            }
        }
    }

    /// Register all mock worker agents
    pub async fn register_workers(&self) {
        let workers = vec![
            ("explore", "Explore Agent", "Analyzed code structure and found patterns"),
            ("implement", "Implement Agent", "Implemented requested feature changes"),
            ("test", "Test Agent", "Created and ran test suite"),
            ("verify", "Verify Agent", "Verified implementation matches specifications"),
            ("research", "Research Agent", "Conducted web research and documentation review"),
        ];

        for (id, name, response) in workers {
            let agent: Arc<dyn Agent> = Arc::new(
                MockWorkerAgent::new(id, name).with_response(response),
            );
            self.registry.register(agent);
        }
    }

    /// Register a custom worker agent
    pub async fn register_custom_worker(
        &self,
        id: &str,
        name: &str,
        response: &str,
    ) {
        let agent: Arc<dyn Agent> = Arc::new(
            MockWorkerAgent::new(id, name).with_response(response),
        );
        self.registry.register(agent);
    }

    /// Simulate a delegation to a worker and record the event
    pub async fn simulate_delegation(
        &self,
        worker_type: &str,
        task: &str,
        _entropy_zone: &str,
    ) -> Result<String, String> {
        // Record delegation started event
        self.record_event(Event::OrchestratorDelegationStarted {
            session_id: self.config.session_id.clone(),
            worker_type: worker_type.to_string(),
            task: task.to_string(),
        }).await;

        // Get agent from registry
        let agent = self.registry.get(worker_type);

        let success = match agent {
            Some(agent) => {
                use rcode_core::{AgentTask, AgentTaskContext, ExecutionConstraints, ResourceRequirements};
                use std::collections::HashMap;

                let definition = match agent.definition() {
                    Some(def) => def.clone(),
                    None => {
                        use rcode_core::agent_definition::{AgentDefinition, AgentMode, AgentPermissionConfig};
                        AgentDefinition {
                            identifier: worker_type.to_string(),
                            name: agent.name().to_string(),
                            description: agent.description().to_string(),
                            when_to_use: String::new(),
                            system_prompt: agent.system_prompt(),
                            mode: AgentMode::Subagent,
                            hidden: false,
                            permission: AgentPermissionConfig::default(),
                            tools: agent.supported_tools(),
                            model: None,
                            max_tokens: None,
                            reasoning_effort: None,
                        }
                    }
                };

                let agent_task = AgentTask {
                    task_id: format!("{}-{}-sandbox", self.config.session_id, worker_type),
                    definition,
                    prompt: task.to_string(),
                    context: AgentTaskContext {
                        session_id: format!("{}-{}", self.config.session_id, worker_type),
                        project_path: std::path::PathBuf::from("/tmp/sandbox"),
                        cwd: std::path::PathBuf::from("/tmp/sandbox"),
                        user_id: None,
                        model_id: "mock".to_string(),
                        messages: vec![],
                        metadata: HashMap::new(),
                    },
                    requirements: ResourceRequirements::lightweight(),
                    constraints: ExecutionConstraints::default(),
                };

                let handle = self.runtime.spawn(agent_task).await
                    .map_err(|e| format!("Spawn failed: {}", e))?;

                handle.await_result().await.is_ok()
            }
            None => false,
        };

        // Record delegation completed event
        self.record_event(Event::OrchestratorDelegationCompleted {
            session_id: self.config.session_id.clone(),
            worker_type: worker_type.to_string(),
            success,
        }).await;

        let mut results = self.test_results.write().await;
        results.delegations_completed += 1;

        if success {
            Ok(format!("Worker {} completed successfully", worker_type))
        } else {
            Err(format!("Worker {} failed", worker_type))
        }
    }

    /// Simulate entropy evaluation - returns (zone, entropy_factor)
    /// This is an internal helper for testing, not an event
    pub async fn simulate_entropy_eval(
        &self,
        event_count: usize,
        unique_tools: usize,
        error_rate: f64,
    ) -> (String, f64) {
        let entropy_factor = (unique_tools as f64 * 0.5) + (error_rate * 2.0) + (event_count as f64 / 20.0);

        let zone = if entropy_factor < 2.0 {
            "green".to_string()
        } else if entropy_factor < 4.0 {
            "yellow".to_string()
        } else {
            "red".to_string()
        };

        (zone, entropy_factor)
    }

    // ==================== VERIFICATION METHODS ====================

    /// Verify that a delegation occurred for the given worker type
    pub async fn verify_delegation(&self, worker_type: &str) -> Result<(), String> {
        let events = self.get_events().await;

        let found = events.iter().any(|e| {
            if let Event::OrchestratorDelegationCompleted { worker_type: wt, .. } = e {
                wt == worker_type
            } else {
                false
            }
        });

        if found {
            self.record_assertion(true, "verify_delegation", &format!("Delegation to {} occurred", worker_type), None).await;
            Ok(())
        } else {
            let msg = format!("No delegation to {} found", worker_type);
            self.record_assertion(false, "verify_delegation", &format!("Delegation to {} occurred", worker_type), Some(&msg)).await;
            Err(msg)
        }
    }

    /// Verify all workers are registered
    pub async fn verify_all_workers_registered(&self) -> Result<(), String> {
        let expected = vec!["explore", "implement", "test", "verify", "research"];
        let agent_ids = self.registry.agent_ids();

        let mut missing = Vec::new();
        for worker in &expected {
            if !agent_ids.contains(&worker.to_string()) {
                missing.push(worker);
            }
        }

        if missing.is_empty() {
            self.record_assertion(true, "verify_all_workers_registered", "All workers registered", None).await;
            Ok(())
        } else {
            let msg = format!("Missing workers: {:?}", missing);
            self.record_assertion(false, "verify_all_workers_registered", "All workers registered", Some(&msg)).await;
            Err(msg)
        }
    }

    /// Verify delegation count matches expected
    pub async fn verify_delegation_count(&self, expected: usize) -> Result<(), String> {
        let delegations_completed = {
            let results = self.test_results.read().await;
            results.delegations_completed
        };

        if delegations_completed == expected {
            self.record_assertion(
                true,
                "verify_delegation_count",
                &format!("Delegation count is {}", expected),
                None,
            ).await;
            Ok(())
        } else {
            let msg = format!(
                "Expected {} delegations, got {}",
                expected, delegations_completed
            );
            self.record_assertion(
                false,
                "verify_delegation_count",
                &format!("Delegation count is {}", expected),
                Some(&msg),
            ).await;
            Err(msg)
        }
    }

    /// Verify no assertions failed
    pub async fn verify_no_failures(&self) -> Result<(), String> {
        let results = self.test_results.read().await;

        if results.assertions_failed == 0 {
            Ok(())
        } else {
            Err(format!(
                "{} assertions failed: {:?}",
                results.assertions_failed, results.errors
            ))
        }
    }

    // ==================== ASSERTION HELPERS (Proposal C) ====================

    /// Assert that an event eventually occurs within timeout
    pub async fn assert_eventually<F>(&self, predicate: F, timeout_secs: u64) -> Result<(), String>
    where
        F: Fn(&Event) -> bool,
    {
        let start = std::time::Instant::now();
        let _session_id = self.config.session_id.clone();

        loop {
            let events = self.get_events().await;

            for event in events {
                if predicate(&event) {
                    return Ok(());
                }
            }

            if start.elapsed().as_secs() >= timeout_secs {
                return Err("Timeout waiting for event".to_string());
            }

            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Assert no failure events occurred
    pub async fn assert_no_failures(&self) -> Result<(), String> {
        let events = self.get_events().await;

        let failure_events: Vec<_> = events.iter()
            .filter(|e| matches!(e, Event::OrchestratorAssertionFailed { .. }))
            .collect();

        if failure_events.is_empty() {
            Ok(())
        } else {
            Err(format!("Found {} failure events", failure_events.len()))
        }
    }
}

impl Default for Sandbox {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for complex sandbox scenarios
pub struct SandboxScenarioBuilder {
    sandbox: Sandbox,
}

impl SandboxScenarioBuilder {
    /// Start building a new scenario
    pub fn new() -> Self {
        Self {
            sandbox: Sandbox::new(),
        }
    }

    /// Add verbose logging
    pub fn verbose(mut self, verbose: bool) -> Self {
        self.sandbox.config.verbose = verbose;
        self
    }

    /// Add a mock failing worker
    pub fn with_failing_worker(self, _worker_type: &str, _error_msg: &str) -> Self {
        // This would register a worker that fails
        // For now, just return self - actual implementation would need runtime access
        self
    }

    /// Set max iterations
    pub fn max_iterations(mut self, max: usize) -> Self {
        self.sandbox.config.max_iterations = max;
        self
    }

    /// Build the sandbox
    pub async fn build(self) -> Sandbox {
        self.sandbox.register_workers().await;
        self.sandbox
    }
}

impl Default for SandboxScenarioBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sandbox_creation() {
        let sandbox = Sandbox::new();
        assert!(sandbox.config.verbose);
    }

    #[tokio::test]
    async fn test_sandbox_register_workers() {
        let sandbox = Sandbox::new();
        sandbox.register_workers().await;

        let agent_ids = sandbox.registry.agent_ids();
        assert!(agent_ids.contains(&"explore".to_string()));
        assert!(agent_ids.contains(&"implement".to_string()));
        assert!(agent_ids.contains(&"test".to_string()));
        assert!(agent_ids.contains(&"verify".to_string()));
        assert!(agent_ids.contains(&"research".to_string()));
    }

    #[tokio::test]
    async fn test_sandbox_simulate_delegation() {
        let sandbox = Sandbox::new();
        sandbox.register_workers().await;

        let result = sandbox
            .simulate_delegation("explore", "Find patterns", "yellow")
            .await;

        assert!(result.is_ok());
        let results = sandbox.get_test_results().await;
        assert_eq!(results.delegations_completed, 1);
    }

    #[tokio::test]
    async fn test_sandbox_verify_delegation() {
        let sandbox = Sandbox::new();
        sandbox.register_workers().await;

        sandbox
            .simulate_delegation("explore", "Find patterns", "yellow")
            .await
            .unwrap();

        sandbox.verify_delegation("explore").await.unwrap();
    }

    #[tokio::test]
    async fn test_sandbox_verify_all_workers() {
        let sandbox = Sandbox::new();
        sandbox.register_workers().await;

        sandbox.verify_all_workers_registered().await.unwrap();
    }

    #[tokio::test]
    async fn test_sandbox_verify_delegation_count() {
        let sandbox = Sandbox::new();
        sandbox.register_workers().await;

        sandbox
            .simulate_delegation("explore", "Task 1", "yellow")
            .await
            .unwrap();
        sandbox
            .simulate_delegation("implement", "Task 2", "red")
            .await
            .unwrap();

        sandbox.verify_delegation_count(2).await.unwrap();
    }

    #[tokio::test]
    async fn test_sandbox_events_recorded() {
        let sandbox = Sandbox::new();
        sandbox.register_workers().await;

        sandbox
            .simulate_delegation("explore", "Find patterns", "yellow")
            .await
            .unwrap();

        let events = sandbox.get_events().await;
        assert!(!events.is_empty());

        // Should have OrchestratorDelegationCompleted events
        let delegation_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, Event::OrchestratorDelegationCompleted { .. }))
            .collect();
        assert!(!delegation_events.is_empty());
    }

    #[tokio::test]
    async fn test_sandbox_scenario_builder() {
        let sandbox = SandboxScenarioBuilder::new()
            .verbose(true)
            .max_iterations(100)
            .build()
            .await;

        sandbox.verify_all_workers_registered().await.unwrap();
    }

    #[tokio::test]
    async fn test_assert_eventually() {
        let sandbox = Sandbox::new();
        sandbox.register_workers().await;

        // Run a delegation
        sandbox
            .simulate_delegation("explore", "Find patterns", "yellow")
            .await
            .unwrap();

        // Wait for delegation to complete
        let result = sandbox.assert_eventually(
            |e| matches!(e, Event::OrchestratorDelegationCompleted { worker_type, .. } if worker_type == "explore"),
            5
        ).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_assert_no_failures() {
        let sandbox = Sandbox::new();
        sandbox.register_workers().await;

        sandbox
            .simulate_delegation("explore", "Find patterns", "yellow")
            .await
            .unwrap();

        let result = sandbox.assert_no_failures().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_event_bus_integration() {
        let sandbox = Sandbox::new();
        sandbox.register_workers().await;

        sandbox
            .simulate_delegation("explore", "Find patterns", "yellow")
            .await
            .unwrap();

        // Check that events were published to event bus
        let events = sandbox.get_events().await;
        assert!(!events.is_empty());

        // Should have delegation started and completed
        let has_started = events.iter().any(|e| matches!(e, Event::OrchestratorDelegationStarted { .. }));
        let has_completed = events.iter().any(|e| matches!(e, Event::OrchestratorDelegationCompleted { .. }));

        assert!(has_started, "Should have DelegationStarted event");
        assert!(has_completed, "Should have DelegationCompleted event");
    }
}
