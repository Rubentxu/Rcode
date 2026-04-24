//! Decision engine for the ReflexiveOrchestrator
//!
//! This module implements the entropy-based decision making that determines
//! whether to DELEGATE or EXECUTE based on observed events.

use rcode_event::Event;
use serde::{Deserialize, Serialize};

/// Entropy zones based on complexity/uncertainty
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntropyZone {
    /// Low complexity (F < 2.0) - Execute directly
    Green,
    /// Medium complexity (2.0 ≤ F < 4.0) - Delegate to worker
    Yellow,
    /// High complexity (F ≥ 4.0) - Delegate with extra verification
    Red,
}

impl EntropyZone {
    /// Returns true if this zone recommends delegation
    pub fn should_delegate(&self) -> bool {
        matches!(self, EntropyZone::Yellow | EntropyZone::Red)
    }

    /// Returns the trust threshold for worker reports
    pub fn trust_threshold(&self) -> f64 {
        match self {
            EntropyZone::Green => 1.0,
            EntropyZone::Yellow => 0.8,
            EntropyZone::Red => 0.6,
        }
    }
}

/// Possible actions the orchestrator can take
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Decision {
    /// Delegate to a worker agent
    Delegate {
        /// The worker agent type (explore, implement, test, verify, research)
        agent_type: String,
        /// The task description for the worker
        task: String,
    },
    /// Execute a tool directly
    Execute {
        /// The tool ID to execute
        tool_id: String,
    },
    /// Continue with current context (no clear decision)
    Continue,
    /// Stop orchestration
    Stop,
}

/// Worker agent types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerType {
    Explore,
    Implement,
    Test,
    Verify,
    Research,
}

impl WorkerType {
    /// Returns the agent ID for this worker type
    pub fn agent_id(&self) -> &'static str {
        match self {
            WorkerType::Explore => "explore",
            WorkerType::Implement => "implement",
            WorkerType::Test => "test",
            WorkerType::Verify => "verify",
            WorkerType::Research => "research",
        }
    }

    /// Parse from string
    pub fn from_str_value(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "explore" => Some(WorkerType::Explore),
            "implement" => Some(WorkerType::Implement),
            "test" => Some(WorkerType::Test),
            "verify" => Some(WorkerType::Verify),
            "research" => Some(WorkerType::Research),
            _ => None,
        }
    }
}

/// Decision engine that evaluates entropy and makes decisions
#[derive(Debug, Clone)]
pub struct DecisionEngine {
    /// Baseline tool frequency (for entropy calculation)
    #[allow(dead_code)]
    tool_baseline: std::collections::HashMap<String, f64>,
}

impl Default for DecisionEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl DecisionEngine {
    /// Create a new decision engine with default baselines
    pub fn new() -> Self {
        let mut tool_baseline = std::collections::HashMap::new();
        // Most common tools have baseline frequency
        tool_baseline.insert("read".to_string(), 0.3);
        tool_baseline.insert("glob".to_string(), 0.15);
        tool_baseline.insert("grep".to_string(), 0.2);
        tool_baseline.insert("bash".to_string(), 0.1);
        tool_baseline.insert("edit".to_string(), 0.15);
        tool_baseline.insert("write".to_string(), 0.05);
        tool_baseline.insert("search".to_string(), 0.05);

        Self { tool_baseline }
    }

    /// Evaluate entropy based on recent events
    ///
    /// Returns an entropy zone (GREEN/YELLOW/RED) based on:
    /// - Number of different tools used
    /// - Frequency of tool usage
    /// - Error rates
    /// - Session complexity
    pub fn evaluate_entropy(&self, events: &[Event]) -> EntropyZone {
        if events.is_empty() {
            return EntropyZone::Green;
        }

        // Count tool-related events
        let tool_events: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Event::ToolExecuted { tool_id, .. } => Some(tool_id),
                Event::ToolError { tool_id, .. } => Some(tool_id),
                _ => None,
            })
            .collect();

        let event_count = events.len();
        let _tool_count = tool_events.len();
        let error_count = events.iter().filter(|e| matches!(e, Event::ToolError { .. })).count();

        // Calculate entropy factor
        // More different tools + more errors = higher entropy
        let unique_tools = tool_events.iter().collect::<std::collections::HashSet<_>>().len();
        let error_rate = if event_count > 0 {
            error_count as f64 / event_count as f64
        } else {
            0.0
        };

        // Entropy formula (simplified)
        // F = base + error_rate * 2.0 + scale_bonus
        // where:
        // - base = 0 for 1 tool (simple), 0.5 per additional unique tool
        // - scale_bonus = small penalty for many operations, only when there's tool diversity
        // This ensures 2 identical operations stay GREEN, but 2 different tools trigger YELLOW
        let base = if unique_tools == 1 {
            0.0  // Single tool type is always low complexity
        } else {
            (unique_tools as f64 - 1.0) * 0.7
        };
        let scale_bonus = if unique_tools > 1 {
            (event_count as f64 / 50.0).min(0.3)  // Only penalize scale when there's diversity
        } else {
            0.0
        };
        let entropy_factor = base + (error_rate * 2.0) + scale_bonus;

        tracing::trace!(
            "Entropy calculation: unique_tools={}, error_rate={:.2}, event_count={}, F={:.2}",
            unique_tools, error_rate, event_count, entropy_factor
        );

        if entropy_factor < 2.0 {
            EntropyZone::Green
        } else if entropy_factor < 4.0 {
            EntropyZone::Yellow
        } else {
            EntropyZone::Red
        }
    }

    /// Make a decision based on entropy zone and events
    pub fn decide(&self, zone: EntropyZone, events: &[Event]) -> Decision {
        match zone {
            EntropyZone::Green => {
                // Low complexity - check if there's a clear task to execute
                if let Some(tool) = self.suggest_simple_tool(events) {
                    Decision::Execute { tool_id: tool }
                } else {
                    // No clear action, signal stop
                    Decision::Stop
                }
            }
            EntropyZone::Yellow | EntropyZone::Red => {
                // Medium/high complexity - delegate to appropriate worker
                let agent_type = self.suggest_worker(events);
                let task = self.construct_task(events);
                Decision::Delegate { agent_type, task }
            }
        }
    }

    /// Suggest a simple tool for GREEN zone
    fn suggest_simple_tool(&self, events: &[Event]) -> Option<String> {
        // Look at recent tool executions
        for event in events.iter().rev() {
            if let Event::ToolExecuted { tool_id, .. } = event {
                // Simple, common tools can be executed directly
                if self.is_simple_tool(tool_id) {
                    return Some(tool_id.clone());
                }
            }
        }
        None
    }

    /// Check if a tool is simple enough for direct execution
    fn is_simple_tool(&self, tool_id: &str) -> bool {
        matches!(tool_id, "read" | "glob" | "grep" | "bash")
    }

    /// Suggest which worker agent to use based on recent events
    fn suggest_worker(&self, events: &[Event]) -> String {
        // Analyze tool usage patterns to determine best worker
        let mut tool_counts = std::collections::HashMap::new();

        for event in events {
            if let Event::ToolExecuted { tool_id, .. } = event {
                *tool_counts.entry(tool_id.clone()).or_insert(0) += 1;
            }
        }

        // Heuristic: pick worker based on most used tools
        let read_count = *tool_counts.get("read").unwrap_or(&0);
        let edit_count = *tool_counts.get("edit").unwrap_or(&0) + *tool_counts.get("write").unwrap_or(&0);
        let test_count = *tool_counts.get("test").unwrap_or(&0);

        if test_count > 0 {
            "test".to_string()
        } else if edit_count > read_count {
            "implement".to_string()
        } else if read_count > 0 {
            "explore".to_string()
        } else {
            "research".to_string()
        }
    }

    /// Construct a task description from recent events
    fn construct_task(&self, events: &[Event]) -> String {
        // Build a task description based on recent events
        let tool_executions: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Event::ToolExecuted { tool_id, .. } => Some(tool_id.as_str()),
                _ => None,
            })
            .collect();

        if tool_executions.is_empty() {
            "Analyze current context and determine next action".to_string()
        } else {
            format!(
                "Continue analysis based on recent tool usage: {}",
                tool_executions.join(", ")
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entropy_zone_green() {
        let engine = DecisionEngine::new();
        let events = vec![];
        assert_eq!(engine.evaluate_entropy(&events), EntropyZone::Green);
    }

    #[test]
    fn test_entropy_zone_with_low_complexity() {
        let engine = DecisionEngine::new();
        let events = vec![
            Event::ToolExecuted { session_id: "s1".to_string(), tool_id: "read".to_string() },
            Event::ToolExecuted { session_id: "s1".to_string(), tool_id: "read".to_string() },
        ];
        let zone = engine.evaluate_entropy(&events);
        assert_eq!(zone, EntropyZone::Green);
    }

    #[test]
    fn test_entropy_zone_with_errors() {
        let engine = DecisionEngine::new();
        let events = vec![
            Event::ToolError { session_id: "s1".to_string(), tool_id: "read".to_string(), error: "err".to_string(), duration_ms: 100 },
            Event::ToolError { session_id: "s1".to_string(), tool_id: "bash".to_string(), error: "err".to_string(), duration_ms: 100 },
            Event::ToolExecuted { session_id: "s1".to_string(), tool_id: "read".to_string() },
            Event::ToolExecuted { session_id: "s1".to_string(), tool_id: "grep".to_string() },
            Event::ToolExecuted { session_id: "s1".to_string(), tool_id: "glob".to_string() },
        ];
        let zone = engine.evaluate_entropy(&events);
        // With errors + multiple tools, should be Yellow or Red
        assert!(matches!(zone, EntropyZone::Yellow | EntropyZone::Red));
    }

    #[test]
    fn test_decision_green_zone() {
        let engine = DecisionEngine::new();
        let events = vec![
            Event::ToolExecuted { session_id: "s1".to_string(), tool_id: "read".to_string() },
        ];
        let decision = engine.decide(EntropyZone::Green, &events);
        // Should suggest executing a simple tool
        if let Decision::Execute { tool_id } = decision {
            assert!(["read", "glob", "grep", "bash"].contains(&tool_id.as_str()));
        }
    }

    #[test]
    fn test_decision_yellow_zone() {
        let engine = DecisionEngine::new();
        let events = vec![
            Event::ToolExecuted { session_id: "s1".to_string(), tool_id: "read".to_string() },
            Event::ToolExecuted { session_id: "s1".to_string(), tool_id: "edit".to_string() },
            Event::ToolExecuted { session_id: "s1".to_string(), tool_id: "grep".to_string() },
        ];
        let decision = engine.decide(EntropyZone::Yellow, &events);
        match decision {
            Decision::Delegate { agent_type, .. } => {
                assert!(["explore", "implement", "test", "verify", "research"].contains(&agent_type.as_str()));
            }
            _ => panic!("Expected Delegate in Yellow zone"),
        }
    }

    #[test]
    fn test_worker_type_from_str() {
        assert_eq!(WorkerType::from_str_value("explore"), Some(WorkerType::Explore));
        assert_eq!(WorkerType::from_str_value("IMPLEMENT"), Some(WorkerType::Implement));
        assert_eq!(WorkerType::from_str_value("unknown"), None);
    }
}