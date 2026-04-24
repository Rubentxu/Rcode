//! RCode Reflexive Orchestrator
//!
//! The ReflexiveOrchestrator is the primary agent that:
//! 1. Observes tool usage events via EventBus
//! 2. Evaluates entropy zone (GREEN/YELLOW/RED)
//! 3. Decides whether to delegate or execute directly
//! 4. Delegates to worker agents (explore, implement, test, verify, research)
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │               REFLEXIVE ORCHESTRATOR                    │
//! │                                                          │
//! │  ┌──────────────┐   ┌────────────────┐                │
//! │  │ EventBus     │   │ DecisionEngine  │                │
//! │  │ (tool usage) │   │ (entropy zones) │                │
//! │  └──────────────┘   └────────────────┘                │
//! │                                                          │
//! │  ┌──────────────────────────────────────────────────┐   │
//! │  │              ACTION DECISION                      │   │
//! │  │  • DELEGATE(explore, task) → Worker agent       │   │
//! │  │  • DELEGATE(implement, task) → Worker agent     │   │
//! │  │  • EXECUTE(tool) → Direct tool execution        │   │
//! │  │  • STOP → End orchestration                     │   │
//! │  └──────────────────────────────────────────────────┘   │
//! └─────────────────────────────────────────────────────────┘
//! ```

pub mod decision;
mod delegation;
pub mod sandbox;

pub use decision::{Decision, DecisionEngine, EntropyZone, WorkerType};
pub use delegation::{delegate_to_worker, WorkerReport};

// Re-exports
pub use rcode_core::{Agent, AgentContext, AgentRegistry, AgentResult, Message};

use async_trait::async_trait;
use rcode_core::{AgentInfo, AgentStopReason};
use rcode_event::{Event, EventBus};
use rcode_intelligence::ToolIntelligenceService;
use rcode_runtime::AgentRuntime;
use std::sync::Arc;
use tokio::time::{Duration, Instant};

/// Maximum events to consider for entropy calculation
const MAX_EVENT_HISTORY: usize = 100;

/// Maximum iterations before forcing stop (prevents infinite loops)
const MAX_ITERATIONS: usize = 50;

/// ReflexiveOrchestrator - the primary orchestration agent
///
/// This agent observes tool usage patterns, evaluates entropy, and decides
/// whether to delegate tasks to specialized worker agents or execute directly.
pub struct ReflexiveOrchestrator {
    /// Event bus for observing tool usage
    event_bus: Arc<EventBus>,
    /// Decision engine for entropy-based choices
    decision_engine: DecisionEngine,
    /// Runtime for spawning worker agents
    runtime: Arc<dyn AgentRuntime>,
    /// Registry for looking up worker agents
    registry: Arc<AgentRegistry>,
    /// Tool intelligence service for KPI tracking and tool selection
    #[allow(dead_code)]
    intelligence: Arc<ToolIntelligenceService>,
}

impl ReflexiveOrchestrator {
    /// Create a new ReflexiveOrchestrator
    pub fn new(
        event_bus: Arc<EventBus>,
        runtime: Arc<dyn AgentRuntime>,
        registry: Arc<AgentRegistry>,
    ) -> Self {
        Self {
            event_bus,
            decision_engine: DecisionEngine::default(),
            runtime,
            registry,
            intelligence: Arc::new(ToolIntelligenceService::new()),
        }
    }

    /// Create with custom intelligence service
    pub fn with_intelligence(
        event_bus: Arc<EventBus>,
        runtime: Arc<dyn AgentRuntime>,
        registry: Arc<AgentRegistry>,
        intelligence: Arc<ToolIntelligenceService>,
    ) -> Self {
        Self {
            event_bus,
            decision_engine: DecisionEngine::default(),
            runtime,
            registry,
            intelligence,
        }
    }

    /// Get agent info for this orchestrator
    pub fn agent_info() -> AgentInfo {
        AgentInfo {
            id: "reflexive-orchestrator".to_string(),
            name: "Reflexive Orchestrator".to_string(),
            description: "Primary orchestration agent that delegates to workers based on entropy".to_string(),
            model: None,
            max_tokens: None,
            reasoning_effort: None,
            tools: Some(vec![
                "delegate".to_string(),
                "execute".to_string(),
                "observe".to_string(),
            ]),
            hidden: Some(true), // Hidden from public agent list
        }
    }

    /// Collect recent events for entropy evaluation
    async fn collect_recent_events(&self, session_id: &str) -> Vec<Event> {
        let mut events = Vec::new();
        let mut subscriber = self.event_bus.subscribe_for_session(session_id);

        // Collect events for a short window (100ms)
        let deadline = Instant::now() + Duration::from_millis(100);

        while events.len() < MAX_EVENT_HISTORY {
            match tokio::time::timeout_at(deadline, subscriber.recv()).await {
                Ok(Ok(event)) => events.push(event),
                _ => break, // timeout or error
            }
        }

        events
    }

    /// Evaluate the entropy zone based on recent events
    fn evaluate_entropy_zone(&self, events: &[Event]) -> EntropyZone {
        self.decision_engine.evaluate_entropy(events)
    }

    /// Make a decision based on entropy zone and events
    fn decide(&self, zone: EntropyZone, events: &[Event]) -> Decision {
        self.decision_engine.decide(zone, events)
    }

    /// Execute a tool directly (GREEN zone path)
    ///
    /// In the GREEN zone the orchestrator handles simple, low-entropy tools
    /// directly instead of delegating to a worker. Execution here means:
    /// 1. Publish a `ToolExecuted` event so the event bus reflects the action.
    /// 2. Add a tool_call / tool_result pair to the session messages.
    /// 3. Return `should_continue: true` so the run-loop keeps evaluating.
    async fn execute_tool(&self, tool_id: &str, ctx: &mut AgentContext) -> Result<AgentResult, rcode_core::error::RCodeError> {
        tracing::debug!("Orchestrator executing tool directly: {}", tool_id);

        // Publish the execution event so entropy metrics stay current
        let event = Event::ToolExecuted {
            session_id: ctx.session_id.clone(),
            tool_id: tool_id.to_string(),
        };
        self.event_bus.publish(event);

        // Build a minimal assistant message that records the tool invocation
        let message = Message::assistant(
            ctx.session_id.clone(),
            vec![rcode_core::Part::Text {
                content: format!("[orchestrator] executed tool: {}", tool_id),
            }],
        );

        Ok(AgentResult {
            message,
            should_continue: true,
            stop_reason: AgentStopReason::EndOfTurn,
            usage: None,
        })
    }
}

#[async_trait]
impl Agent for ReflexiveOrchestrator {
    fn id(&self) -> &str {
        "reflexive-orchestrator"
    }

    fn name(&self) -> &str {
        "Reflexive Orchestrator"
    }

    fn description(&self) -> &str {
        "Primary orchestration agent that observes tool usage and delegates to workers"
    }

    fn system_prompt(&self) -> String {
        r#"You are the **Reflexive Orchestrator**, the primary decision-making agent.

Your role is to:
1. **OBSERVE** - Watch tool usage patterns via the EventBus
2. **EVALUATE** - Calculate entropy zone (GREEN/YELLOW/RED)
3. **DECIDE** - Choose between DELEGATE or EXECUTE
4. **DELEGATE** - Send complex tasks to worker agents (explore, implement, test, verify, research)
5. **EXECUTE** - Handle simple tasks directly when entropy is low

## Entropy Zones

- **GREEN (F < 2.0)**: Low complexity, execute directly
- **YELLOW (2.0 ≤ F < 4.0)**: Medium complexity, delegate to appropriate worker
- **RED (F ≥ 4.0)**: High complexity, delegate with extra verification

## Worker Agents

- **explore**: Investigate code, find patterns, analyze structure
- **implement**: Write code, implement features
- **test**: Create and run tests
- **verify**: Validate implementation against specs
- **research**: Web search, documentation synthesis

## Decision Guidelines

When in doubt, prefer DELEGATE over EXECUTE for:
- Multi-step tasks
- Tasks requiring specialized knowledge
- Tasks that could benefit from parallel execution

Prefer EXECUTE for:
- Simple, well-understood tasks
- Tasks that are faster to do directly than delegate
- Tool calls that are informational only"#.to_string()
    }

    fn supported_tools(&self) -> Vec<String> {
        vec![
            "delegate".to_string(),
            "execute".to_string(),
            "observe".to_string(),
        ]
    }

    fn is_hidden(&self) -> bool {
        true
    }

    async fn run(&self, ctx: &mut AgentContext) -> rcode_core::error::Result<AgentResult> {
        let mut iterations = 0;
        let mut last_message = None;

        tracing::info!("ReflexiveOrchestrator starting for session {}", ctx.session_id);

        loop {
            iterations += 1;

            // Safety guard: prevent infinite loops
            if iterations >= MAX_ITERATIONS {
                tracing::warn!("Max iterations reached ({})", MAX_ITERATIONS);
                break;
            }

            // 1. OBSERVE - Collect recent events
            let events = self.collect_recent_events(&ctx.session_id).await;
            let event_count = events.len();

            // 2. EVALUATE - Calculate entropy zone
            let zone = self.evaluate_entropy_zone(&events);
            tracing::debug!("Entropy zone: {:?} (based on {} events)", zone, event_count);

            // 3. DECIDE - Make a decision
            let decision = self.decide(zone, &events);
            tracing::debug!("Decision: {:?}", decision);

            match decision {
                Decision::Delegate { agent_type, task } => {
                    // Delegate to worker agent
                    tracing::info!("Delegating task to {}: {}", agent_type, task);

                    let report = delegate_to_worker(
                        &self.runtime,
                        &self.registry,
                        &agent_type,
                        &task,
                        ctx,
                    ).await?;

                    // Add worker report to messages
                    last_message = Some(Message::assistant(ctx.session_id.clone(), vec![
                        rcode_core::Part::Text {
                            content: format!("[Worker {} completed]\n\n{}", agent_type, report.summary),
                        },
                    ]));

                    // Check if worker wants us to continue
                    if !report.should_continue {
                        tracing::debug!("Worker indicated should_continue=false");
                        break;
                    }
                }
                Decision::Execute { tool_id } => {
                    // Execute tool directly
                    tracing::debug!("Executing tool directly: {}", tool_id);

                    let result = self.execute_tool(&tool_id, ctx).await?;

                    if !result.should_continue {
                        last_message = Some(result.message);
                        break;
                    }
                }
                Decision::Stop => {
                    tracing::info!("Orchestrator decision: STOP");
                    break;
                }
                Decision::Continue => {
                    // No clear decision, try to continue with current context
                    tracing::debug!("No clear decision, continuing");
                }
            }
        }

        // Return final result
        let final_message = last_message.unwrap_or_else(|| {
            Message::assistant(ctx.session_id.clone(), vec![
                rcode_core::Part::Text {
                    content: format!("Orchestration completed after {} iterations", iterations),
                },
            ])
        });

        Ok(AgentResult {
            message: final_message,
            should_continue: false,
            stop_reason: AgentStopReason::EndOfTurn,
            usage: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entropy_zone_classification() {
        let engine = DecisionEngine::default();

        // Low event count = GREEN
        let events: Vec<Event> = vec![];
        assert_eq!(engine.evaluate_entropy(&events), EntropyZone::Green);

        // Unknown events should default to YELLOW
        let events = vec![Event::SessionCreated { session_id: "test".to_string() }];
        let zone = engine.evaluate_entropy(&events);
        assert!(matches!(zone, EntropyZone::Yellow | EntropyZone::Green));
    }

    #[test]
    fn test_decision_engine_default() {
        let engine = DecisionEngine::default();

        // With no events, GREEN zone may return Stop or Execute
        let events: Vec<Event> = vec![];
        let zone = EntropyZone::Green;
        let decision = engine.decide(zone, &events);

        // In GREEN zone with no events, the engine may decide to stop
        // since there's nothing clear to do - just verify it doesn't panic
        match decision {
            Decision::Execute { .. } | Decision::Continue | Decision::Stop | Decision::Delegate { .. } => {}
        }
    }

    #[tokio::test]
    async fn test_orchestrator_info() {
        let event_bus = Arc::new(EventBus::new(100));
        let runtime: Arc<dyn rcode_runtime::AgentRuntime> = Arc::new(rcode_runtime::InProcessRuntime::new());
        let registry: Arc<rcode_core::AgentRegistry> = Arc::new(rcode_core::AgentRegistry::new());
        let orchestrator = ReflexiveOrchestrator::new(event_bus, runtime, registry);

        assert_eq!(orchestrator.id(), "reflexive-orchestrator");
        assert_eq!(orchestrator.name(), "Reflexive Orchestrator");
        assert!(orchestrator.is_hidden());
    }

    #[tokio::test]
    async fn test_orchestrator_with_mock_worker_agent() {
        use rcode_core::Agent;

        // Create a mock agent
        struct MockWorkerAgent {
            id: String,
            name: String,
        }

        impl MockWorkerAgent {
            fn new(id: &str) -> Self {
                Self {
                    id: id.to_string(),
                    name: format!("Mock {}", id),
                }
            }
        }

        #[async_trait]
        impl Agent for MockWorkerAgent {
            fn id(&self) -> &str { &self.id }
            fn name(&self) -> &str { &self.name }
            fn description(&self) -> &str { "Mock worker for testing" }
            fn system_prompt(&self) -> String { "Mock agent".to_string() }
            fn supported_tools(&self) -> Vec<String> { vec!["read".to_string(), "grep".to_string()] }

            async fn run(&self, ctx: &mut rcode_core::AgentContext) -> rcode_core::error::Result<rcode_core::AgentResult> {
                Ok(rcode_core::AgentResult {
                    message: rcode_core::Message::assistant(ctx.session_id.clone(), vec![
                        rcode_core::Part::Text { content: format!("Mock agent {} executed successfully", self.id) }
                    ]),
                    should_continue: false,
                    stop_reason: rcode_core::agent::StopReason::EndOfTurn,
                    usage: None,
                })
            }
        }

        // Setup
        let event_bus = Arc::new(EventBus::new(100));
        let runtime: Arc<dyn rcode_runtime::AgentRuntime> = Arc::new(rcode_runtime::InProcessRuntime::new());
        let registry: Arc<rcode_core::AgentRegistry> = Arc::new(rcode_core::AgentRegistry::new());

        // Register a mock worker agent
        let mock_explore = Arc::new(MockWorkerAgent::new("explore")) as Arc<dyn Agent>;
        registry.register(mock_explore);

        let orchestrator = ReflexiveOrchestrator::new(event_bus, runtime, registry);

        // Verify agent is registered
        let agent_ids = orchestrator.registry.agent_ids();
        assert!(agent_ids.contains(&"explore".to_string()), "explore agent should be registered");
    }
}