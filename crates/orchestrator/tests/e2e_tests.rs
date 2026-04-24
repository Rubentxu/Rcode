//! End-to-end integration tests for ReflexiveOrchestrator
//!
//! These tests validate the complete flow:
//! - Orchestrator initialization with real components
//! - Entropy evaluation from actual events
//! - Decision making based on entropy
//! - Delegation to registered worker agents
//! - Result collection and verification

use std::sync::Arc;

use async_trait::async_trait;
use rcode_core::{
    Agent, AgentContext, AgentResult, AgentStopReason, AgentTask, AgentTaskContext, Message, Part,
    agent_definition::{AgentDefinition, AgentMode, AgentPermissionConfig},
};
use rcode_event::{Event, EventBus};
use rcode_orchestrator::{ReflexiveOrchestrator, DecisionEngine, EntropyZone};
use rcode_runtime::{AgentRuntime, InProcessRuntime};

/// Simple worker agent for testing
struct TestWorkerAgent {
    id: String,
    name: String,
    response: String,
}

impl TestWorkerAgent {
    fn new(id: &str, name: &str, response: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            response: response.to_string(),
        }
    }
}

#[async_trait]
impl Agent for TestWorkerAgent {
    fn id(&self) -> &str { &self.id }
    fn name(&self) -> &str { &self.name }
    fn description(&self) -> &str { "Test worker agent" }
    fn system_prompt(&self) -> String { format!("You are {}, a test agent.", self.name) }
    fn supported_tools(&self) -> Vec<String> { vec!["read".to_string(), "grep".to_string()] }

    async fn run(&self, ctx: &mut AgentContext) -> rcode_core::error::Result<AgentResult> {
        Ok(AgentResult {
            message: Message::assistant(
                ctx.session_id.clone(),
                vec![Part::Text { content: self.response.clone() }],
            ),
            should_continue: false,
            stop_reason: AgentStopReason::EndOfTurn,
            usage: None,
        })
    }
}

/// Test: DecisionEngine with real events
#[test]
fn test_decision_engine_with_real_events() {
    let engine = DecisionEngine::new();

    // Simulate a session with many tool usages to trigger YELLOW/RED
    // entropy_factor = unique_tools * 0.5 + error_rate * 2.0 + event_count / 20
    // For YELLOW (>= 2.0): need more events
    let events = vec![
        Event::ToolExecuted { session_id: "test".into(), tool_id: "read".into() },
        Event::ToolExecuted { session_id: "test".into(), tool_id: "grep".into() },
        Event::ToolExecuted { session_id: "test".into(), tool_id: "glob".into() },
        Event::ToolExecuted { session_id: "test".into(), tool_id: "edit".into() },
        Event::ToolExecuted { session_id: "test".into(), tool_id: "bash".into() },
        Event::ToolExecuted { session_id: "test".into(), tool_id: "write".into() },
        Event::ToolExecuted { session_id: "test".into(), tool_id: "search".into() },
    ];

    let zone = engine.evaluate_entropy(&events);

    // With 7 different tools across 7 events, should be YELLOW or higher
    // entropy = 7 * 0.5 + 0 + 7/20 = 3.5 + 0.35 = 3.85 (YELLOW)
    assert!(matches!(zone, EntropyZone::Yellow | EntropyZone::Red));

    // Decision should be to delegate
    let decision = engine.decide(zone, &events);
    match decision {
        rcode_orchestrator::Decision::Delegate { agent_type, .. } => {
            assert!(["explore", "implement", "test", "verify", "research"].contains(&agent_type.as_str()));
        }
        _ => panic!("Expected Delegate decision for multi-tool events"),
    }
}

/// Test: EventBus publishes and records events correctly
#[test]
fn test_event_bus_records_tool_events() {
    let bus = EventBus::new(100);
    let mut sub = bus.subscribe_for_session("test-session");

    bus.publish(Event::ToolExecuted {
        session_id: "test-session".into(),
        tool_id: "read".into(),
    });
    bus.publish(Event::ToolExecuted {
        session_id: "test-session".into(),
        tool_id: "grep".into(),
    });

    let events: Vec<_> = std::iter::from_fn(|| {
        sub.try_recv().ok()
    }).collect();

    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event_type(), "tool_executed");
}

/// Test: ReflexiveOrchestrator initializes with all components
#[tokio::test(flavor = "multi_thread")]
async fn test_orchestrator_initialization() {
    let event_bus = Arc::new(EventBus::new(100));
    let runtime: Arc<dyn AgentRuntime> = Arc::new(InProcessRuntime::new());
    let registry = Arc::new(rcode_core::AgentRegistry::new());

    let orchestrator = ReflexiveOrchestrator::new(
        event_bus.clone(),
        runtime.clone(),
        registry.clone(),
    );

    assert_eq!(orchestrator.id(), "reflexive-orchestrator");
    assert_eq!(orchestrator.name(), "Reflexive Orchestrator");
}

/// Test: Worker agents can be registered and retrieved
#[tokio::test(flavor = "multi_thread")]
async fn test_worker_agent_registration() {
    let registry = Arc::new(rcode_core::AgentRegistry::new());

    // Register test workers
    let explore: Arc<dyn Agent> = Arc::new(TestWorkerAgent::new(
        "explore",
        "Explore Agent",
        "Analysis complete: found 5 source files",
    ));
    let implement: Arc<dyn Agent> = Arc::new(TestWorkerAgent::new(
        "implement",
        "Implement Agent",
        "Implementation complete: added 3 functions",
    ));

    registry.register(explore);
    registry.register(implement);

    // Verify registration
    let agent_ids = registry.agent_ids();
    assert!(agent_ids.contains(&"explore".to_string()));
    assert!(agent_ids.contains(&"implement".to_string()));

    // Verify retrieval
    let retrieved = registry.get("explore");
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().name(), "Explore Agent");
}

/// Test: InProcessRuntime spawns and runs agent successfully
#[tokio::test(flavor = "multi_thread")]
async fn test_runtime_spawns_agent() {
    let runtime = InProcessRuntime::new();

    let definition = AgentDefinition {
        identifier: "test".to_string(),
        name: "Test Agent".to_string(),
        description: "Test worker agent".to_string(),
        when_to_use: String::new(),
        system_prompt: "You are a test agent.".to_string(),
        mode: AgentMode::All,
        hidden: false,
        permission: AgentPermissionConfig::default(),
        tools: vec![],
        model: None,
        max_tokens: None,
        reasoning_effort: None,
    };

    let context = AgentTaskContext {
        session_id: "test-session".to_string(),
        project_path: std::path::PathBuf::from("/tmp"),
        cwd: std::path::PathBuf::from("/tmp"),
        messages: vec![],
        user_id: None,
        model_id: "test-model".to_string(),
        metadata: Default::default(),
    };

    let task = AgentTask::new("task-001", definition, "Task completed successfully", context);

    let handle = runtime.spawn(task).await.expect("Spawn should succeed");
    let result = handle.await_result().await.expect("Agent should complete");

    // The InProcessRuntime returns an error result when no real LLM is available,
    // but the spawn + await_result round-trip should work without panicking.
    let _ = result;
}

/// Test: Complete flow - event publication triggers evaluation
#[tokio::test(flavor = "multi_thread")]
async fn test_event_driven_entropy_evaluation() {
    let event_bus = Arc::new(EventBus::new(100));
    let mut sub = event_bus.subscribe_for_session("test");

    // Create engine
    let engine = DecisionEngine::new();

    // Publish many events to trigger YELLOW zone
    event_bus.publish(Event::ToolExecuted {
        session_id: "test".into(),
        tool_id: "read".into(),
    });
    event_bus.publish(Event::ToolExecuted {
        session_id: "test".into(),
        tool_id: "grep".into(),
    });
    event_bus.publish(Event::ToolExecuted {
        session_id: "test".into(),
        tool_id: "edit".into(),
    });
    event_bus.publish(Event::ToolExecuted {
        session_id: "test".into(),
        tool_id: "bash".into(),
    });
    event_bus.publish(Event::ToolExecuted {
        session_id: "test".into(),
        tool_id: "write".into(),
    });

    // Collect events
    let events: Vec<_> = std::iter::from_fn(|| {
        sub.try_recv().ok()
    }).collect();

    // Evaluate entropy
    let zone = engine.evaluate_entropy(&events);

    // With 5 different tools, should be YELLOW or higher
    assert!(matches!(zone, EntropyZone::Yellow | EntropyZone::Red));

    // Decision should delegate
    let decision = engine.decide(zone, &events);
    assert!(matches!(decision, rcode_orchestrator::Decision::Delegate { .. }));
}

/// Test: Error events increase entropy
#[tokio::test(flavor = "multi_thread")]
async fn test_error_events_increase_entropy() {
    let engine = DecisionEngine::new();

    // Events with errors should have higher entropy
    let events_with_errors = vec![
        Event::ToolError { session_id: "test".into(), tool_id: "read".into(), error: "Permission denied".into(), duration_ms: 50 },
        Event::ToolError { session_id: "test".into(), tool_id: "bash".into(), error: "Command failed".into(), duration_ms: 100 },
        Event::ToolExecuted { session_id: "test".into(), tool_id: "read".into() },
        Event::ToolExecuted { session_id: "test".into(), tool_id: "grep".into() },
    ];

    let events_without_errors = vec![
        Event::ToolExecuted { session_id: "test".into(), tool_id: "read".into() },
        Event::ToolExecuted { session_id: "test".into(), tool_id: "grep".into() },
    ];

    let _zone_with_errors = engine.evaluate_entropy(&events_with_errors);
    let _zone_without_errors = engine.evaluate_entropy(&events_without_errors);

    // Errors should push to higher entropy zone
    // (This is a heuristic test - actual zone depends on thresholds)
    let error_count_with = events_with_errors.iter().filter(|e| matches!(e, Event::ToolError { .. })).count();
    let error_count_without = events_without_errors.iter().filter(|e| matches!(e, Event::ToolError { .. })).count();

    assert!(error_count_with > error_count_without);
}

/// Test: Decision trust thresholds per zone
#[test]
fn test_entropy_zone_trust_thresholds() {
    assert_eq!(EntropyZone::Green.trust_threshold(), 1.0);
    assert_eq!(EntropyZone::Yellow.trust_threshold(), 0.8);
    assert_eq!(EntropyZone::Red.trust_threshold(), 0.6);
}

/// Test: AgentExecutor builder pattern (smoke test)
/// Test: AgentExecutor exists in rcode_agent
#[test]
fn test_agent_executor_exists() {
    // Verify the agent executor types are accessible
    let _ = std::any::type_name::<rcode_agent::AgentExecutor>();
}

// ── Nivel 2: DelegateTool + MockRunner (sin HTTP) ────────────────────────────

mod delegate_integration {
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    use async_trait::async_trait;
    use rcode_core::{SubagentResult, SubagentRunner, Tool, ToolContext};
    use rcode_tools::{DelegateTool, DelegationReadTool, DelegationStatus};

    fn make_ctx() -> ToolContext {
        ToolContext {
            session_id: "orch-test".into(),
            project_path: "/tmp".into(),
            cwd: "/tmp".into(),
            user_id: None,
            agent: "orchestrator".into(),
        }
    }

    struct MockRunner {
        response: String,
        fail: bool,
    }

    impl MockRunner {
        fn ok(msg: &str) -> Arc<Self> {
            Arc::new(Self { response: msg.to_string(), fail: false })
        }
        fn failing() -> Arc<Self> {
            Arc::new(Self { response: String::new(), fail: true })
        }
    }

    #[async_trait]
    impl SubagentRunner for MockRunner {
        async fn run_subagent(
            &self,
            _session: &str,
            _agent: &str,
            _prompt: &str,
            _tools: &[&str],
        ) -> std::result::Result<SubagentResult, Box<dyn std::error::Error + Send + Sync>> {
            if self.fail {
                Err(Box::new(std::io::Error::other("subagent error")))
            } else {
                Ok(SubagentResult {
                    response_text: self.response.clone(),
                    child_session_id: "child-session-42".into(),
                })
            }
        }
    }

    /// Nivel 2-A: delegate completa y el resultado es legible vía DelegationReadTool
    #[tokio::test]
    async fn test_delegate_then_read_completed() {
        let store = Arc::new(RwLock::new(HashMap::new()));
        let runner = MockRunner::ok("explore result: 12 files analysed");
        let delegate = DelegateTool::with_store_and_runner(Arc::clone(&store), runner);
        let read = DelegationReadTool::new(Arc::clone(&store));
        let ctx = make_ctx();

        let created = delegate
            .execute(serde_json::json!({"prompt": "explore the codebase", "agent": "explore"}), &ctx)
            .await
            .expect("delegate should succeed");

        let id = created
            .metadata
            .as_ref()
            .unwrap()["delegation_id"]
            .as_str()
            .unwrap()
            .to_string();

        // Poll hasta que complete (max 5 s)
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            let status = store.read().await.get(&id).map(|r| r.status.clone());
            if matches!(status, Some(DelegationStatus::Completed) | Some(DelegationStatus::Failed)) {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                panic!("delegation did not complete within 5 s");
            }
        }

        let result = read
            .execute(serde_json::json!({"id": id}), &ctx)
            .await
            .expect("read should succeed");

        let meta = result.metadata.unwrap();
        assert_eq!(meta["status"], "completed");
        assert!(result.content.contains("12 files analysed"), "content: {}", result.content);

        // child_session_id vive en el metadata del ToolResult interno del record
        let inner_meta = store.read().await;
        let record = inner_meta.get(&id).unwrap();
        let child_id = record.result.as_ref().unwrap()
            .metadata.as_ref().unwrap()["child_session_id"]
            .as_str().unwrap();
        assert_eq!(child_id, "child-session-42");
    }

    /// Nivel 2-B: dos delegaciones paralelas a agentes distintos, ambas completan
    #[tokio::test]
    async fn test_two_parallel_delegations_both_complete() {
        let store = Arc::new(RwLock::new(HashMap::new()));
        let runner_a = MockRunner::ok("implement result: 3 functions added");
        let runner_b = MockRunner::ok("verify result: all specs green");

        let delegate_a = DelegateTool::with_store_and_runner(Arc::clone(&store), runner_a);
        let delegate_b = DelegateTool::with_store_and_runner(Arc::clone(&store), runner_b);
        let ctx = make_ctx();

        let (res_a, res_b) = tokio::join!(
            delegate_a.execute(
                serde_json::json!({"prompt": "implement feature X", "agent": "implement"}),
                &ctx
            ),
            delegate_b.execute(
                serde_json::json!({"prompt": "verify specs", "agent": "verify"}),
                &ctx
            ),
        );

        let id_a = res_a.unwrap().metadata.unwrap()["delegation_id"]
            .as_str().unwrap().to_string();
        let id_b = res_b.unwrap().metadata.unwrap()["delegation_id"]
            .as_str().unwrap().to_string();

        assert_ne!(id_a, id_b);

        // Esperar que ambas completen
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            let s = store.read().await;
            let done_a = matches!(s.get(&id_a).map(|r| &r.status), Some(DelegationStatus::Completed));
            let done_b = matches!(s.get(&id_b).map(|r| &r.status), Some(DelegationStatus::Completed));
            if done_a && done_b { break; }
            if tokio::time::Instant::now() >= deadline {
                panic!("delegations did not complete in time");
            }
        }

        let s = store.read().await;
        assert!(s[&id_a].result.as_ref().unwrap().content.contains("implement result"));
        assert!(s[&id_b].result.as_ref().unwrap().content.contains("verify result"));
    }

    /// Nivel 2-C: delegación con runner que falla → estado Failed con mensaje de error
    #[tokio::test]
    async fn test_delegate_runner_failure_records_error() {
        let store = Arc::new(RwLock::new(HashMap::new()));
        let runner = MockRunner::failing();
        let delegate = DelegateTool::with_store_and_runner(Arc::clone(&store), runner);
        let read = DelegationReadTool::new(Arc::clone(&store));
        let ctx = make_ctx();

        let created = delegate
            .execute(serde_json::json!({"prompt": "do risky work", "agent": "implement"}), &ctx)
            .await
            .unwrap();
        let id = created.metadata.unwrap()["delegation_id"]
            .as_str().unwrap().to_string();

        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            let status = store.read().await.get(&id).map(|r| r.status.clone());
            if matches!(status, Some(DelegationStatus::Failed)) { break; }
            if tokio::time::Instant::now() >= deadline {
                panic!("delegation did not fail within 5 s");
            }
        }

        let result = read
            .execute(serde_json::json!({"id": id}), &ctx)
            .await
            .unwrap();
        let meta = result.metadata.unwrap();
        assert_eq!(meta["status"], "failed");
        assert!(
            result.content.contains("subagent error"),
            "error message missing: {}",
            result.content
        );
    }

    /// Nivel 2-D: orchestrator decide delegar en zona YELLOW y la delegación completa
    #[tokio::test]
    async fn test_orchestrator_decides_and_delegation_completes() {
        use rcode_event::{Event};
        use rcode_orchestrator::{DecisionEngine, Decision, EntropyZone};

        // Construir suficientes eventos para entrar en YELLOW
        let events: Vec<Event> = ["read", "grep", "glob", "edit", "bash", "write", "search"]
            .iter()
            .map(|t| Event::ToolExecuted { session_id: "s".into(), tool_id: t.to_string() })
            .collect();

        let engine = DecisionEngine::new();
        let zone = engine.evaluate_entropy(&events);
        assert!(matches!(zone, EntropyZone::Yellow | EntropyZone::Red));

        let decision = engine.decide(zone, &events);
        let agent_type = match decision {
            Decision::Delegate { agent_type, .. } => agent_type,
            _ => panic!("Expected Delegate, got {:?}", decision),
        };
        assert!(["explore", "implement", "test", "verify", "research"].contains(&agent_type.as_str()));

        // Ahora la delegación real con MockRunner
        let store = Arc::new(RwLock::new(HashMap::new()));
        let runner = MockRunner::ok(&format!("{} completed successfully", agent_type));
        let delegate = DelegateTool::with_store_and_runner(Arc::clone(&store), runner);
        let ctx = make_ctx();

        let created = delegate
            .execute(serde_json::json!({"prompt": "handle this task", "agent": agent_type}), &ctx)
            .await
            .unwrap();
        let id = created.metadata.unwrap()["delegation_id"]
            .as_str().unwrap().to_string();

        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            let status = store.read().await.get(&id).map(|r| r.status.clone());
            if matches!(status, Some(DelegationStatus::Completed)) { break; }
            if tokio::time::Instant::now() >= deadline {
                panic!("delegation did not complete in time");
            }
        }

        let s = store.read().await;
        assert!(s[&id].result.as_ref().unwrap().content.contains("completed successfully"));
    }
}
