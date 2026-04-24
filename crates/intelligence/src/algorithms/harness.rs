//! Entropy-based breathing harness for adaptive complexity handling
//!
//! This module implements the BreathingHarness - a system that evaluates
//! task complexity and assigns entropy zones to determine appropriate
//! delegation strategies.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Events from tool usage to evaluate
#[derive(Debug, Clone)]
pub struct EntropyEvent {
    /// Event type
    pub event_type: EventType,
    /// Tool ID if applicable
    pub tool_id: Option<String>,
    /// Whether the event was successful
    pub success: bool,
    /// Complexity weight of the event
    pub complexity: f64,
}

/// Type of entropy event
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventType {
    /// Tool was executed
    ToolExecuted,
    /// Tool execution failed
    ToolError,
    /// Delegation occurred
    Delegation,
    /// Worker reported back
    WorkerReport,
    /// Error occurred
    Error,
}

impl Default for EntropyEvent {
    fn default() -> Self {
        Self {
            event_type: EventType::ToolExecuted,
            tool_id: None,
            success: true,
            complexity: 1.0,
        }
    }
}

/// Entropy zone classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Zone {
    /// Low complexity - execute directly
    Green,
    /// Medium complexity - delegate with normal trust
    Yellow,
    /// High complexity - delegate with extra verification
    Red,
}

impl Zone {
    /// Returns the trust threshold for this zone
    pub fn trust_threshold(&self) -> f64 {
        match self {
            Zone::Green => 1.0,
            Zone::Yellow => 0.8,
            Zone::Red => 0.6,
        }
    }

    /// Returns whether delegation is recommended
    pub fn should_delegate(&self) -> bool {
        matches!(self, Zone::Yellow | Zone::Red)
    }
}

/// BreathingHarness configuration
#[derive(Debug, Clone)]
pub struct HarnessConfig {
    /// Green zone upper bound (F < this)
    pub green_threshold: f64,
    /// Yellow zone upper bound (F < this)
    pub yellow_threshold: f64,
    /// Weights for different event types
    pub event_weights: HashMap<EventType, f64>,
    /// Decay factor for old events (0.0 - 1.0)
    pub decay_factor: f64,
}

impl Default for HarnessConfig {
    fn default() -> Self {
        let mut event_weights = HashMap::new();
        event_weights.insert(EventType::ToolError, 2.0);
        event_weights.insert(EventType::Delegation, 1.5);
        event_weights.insert(EventType::ToolExecuted, 1.0);
        event_weights.insert(EventType::WorkerReport, 1.0);
        event_weights.insert(EventType::Error, 2.5);

        Self {
            green_threshold: 2.0,
            yellow_threshold: 4.0,
            event_weights,
            decay_factor: 0.95,
        }
    }
}

/// Result of entropy evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntropyResult {
    /// The assigned zone
    pub zone: Zone,
    /// Raw entropy factor
    pub entropy_factor: f64,
    /// Number of events analyzed
    pub event_count: usize,
    /// Unique tools used
    pub unique_tools: usize,
    /// Error rate
    pub error_rate: f64,
}

impl Default for EntropyResult {
    fn default() -> Self {
        Self {
            zone: Zone::Green,
            entropy_factor: 0.0,
            event_count: 0,
            unique_tools: 0,
            error_rate: 0.0,
        }
    }
}

/// BreathingHarness - evaluates entropy and assigns zones
#[derive(Debug, Clone)]
pub struct BreathingHarness {
    config: HarnessConfig,
    event_history: Vec<EntropyEvent>,
    tool_usage_counts: HashMap<String, usize>,
}

impl Default for BreathingHarness {
    fn default() -> Self {
        Self::new()
    }
}

impl BreathingHarness {
    /// Create a new BreathingHarness with default config
    pub fn new() -> Self {
        Self {
            config: HarnessConfig::default(),
            event_history: Vec::new(),
            tool_usage_counts: HashMap::new(),
        }
    }

    /// Create with custom config
    pub fn with_config(config: HarnessConfig) -> Self {
        Self {
            config,
            event_history: Vec::new(),
            tool_usage_counts: HashMap::new(),
        }
    }

    /// Add an event to the history
    pub fn add_event(&mut self, event: EntropyEvent) {
        // Update tool usage counts
        if let Some(tool_id) = &event.tool_id {
            *self.tool_usage_counts.entry(tool_id.clone()).or_insert(0) += 1;
        }

        self.event_history.push(event);

        // Trim history if too long (keep last 100 events)
        if self.event_history.len() > 100 {
            self.event_history.remove(0);
        }
    }

    /// Add a simple tool execution event
    pub fn add_tool_execution(&mut self, tool_id: &str, success: bool) {
        self.add_event(EntropyEvent {
            event_type: EventType::ToolExecuted,
            tool_id: Some(tool_id.to_string()),
            success,
            complexity: 1.0,
        });
    }

    /// Add a tool error event
    pub fn add_tool_error(&mut self, tool_id: &str) {
        self.add_event(EntropyEvent {
            event_type: EventType::ToolError,
            tool_id: Some(tool_id.to_string()),
            success: false,
            complexity: 2.0,
        });
    }

    /// Add a delegation event
    pub fn add_delegation(&mut self) {
        self.add_event(EntropyEvent {
            event_type: EventType::Delegation,
            tool_id: None,
            success: true,
            complexity: 1.5,
        });
    }

    /// Add a worker report event
    pub fn add_worker_report(&mut self, success: bool) {
        self.add_event(EntropyEvent {
            event_type: EventType::WorkerReport,
            tool_id: None,
            success,
            complexity: 1.0,
        });
    }

    /// Apply decay to event history (reduces weight of old events)
    pub fn apply_decay(&mut self) {
        for event in &mut self.event_history {
            event.complexity *= self.config.decay_factor;
        }
    }

    /// Evaluate entropy and return zone
    pub fn evaluate(&self) -> EntropyResult {
        if self.event_history.is_empty() {
            return EntropyResult::default();
        }

        let event_count = self.event_history.len();

        // Count errors
        let error_count = self.event_history
            .iter()
            .filter(|e| !e.success)
            .count();

        // Count unique tools
        let unique_tools = self.tool_usage_counts.len();

        // Calculate weighted complexity sum
        let total_complexity: f64 = self.event_history
            .iter()
            .map(|e| {
                let weight = self.config
                    .event_weights
                    .get(&e.event_type)
                    .copied()
                    .unwrap_or(1.0);
                e.complexity * weight
            })
            .sum();

        // Calculate entropy factor
        let error_rate = if event_count > 0 {
            error_count as f64 / event_count as f64
        } else {
            0.0
        };

        // F = (weighted_complexity / event_count) + (error_rate * 2.0) + (unique_tools * 0.3)
        let entropy_factor = (total_complexity / event_count as f64)
            + (error_rate * 2.0)
            + (unique_tools as f64 * 0.3);

        // Classify into zone
        let zone = if entropy_factor < self.config.green_threshold {
            Zone::Green
        } else if entropy_factor < self.config.yellow_threshold {
            Zone::Yellow
        } else {
            Zone::Red
        };

        EntropyResult {
            zone,
            entropy_factor,
            event_count,
            unique_tools,
            error_rate,
        }
    }

    /// Get current event history
    pub fn event_history(&self) -> &[EntropyEvent] {
        &self.event_history
    }

    /// Get tool usage counts
    pub fn tool_usage_counts(&self) -> &HashMap<String, usize> {
        &self.tool_usage_counts
    }

    /// Clear event history
    pub fn reset(&mut self) {
        self.event_history.clear();
        self.tool_usage_counts.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_breathing_harness_new() {
        let harness = BreathingHarness::new();
        let result = harness.evaluate();
        assert_eq!(result.zone, Zone::Green);
        assert_eq!(result.event_count, 0);
    }

    #[test]
    fn test_breathing_harness_add_event() {
        let mut harness = BreathingHarness::new();
        harness.add_tool_execution("read", true);
        harness.add_tool_execution("grep", true);

        let result = harness.evaluate();
        assert_eq!(result.event_count, 2);
        assert_eq!(result.unique_tools, 2);
    }

    #[test]
    fn test_breathing_harness_green_zone() {
        let mut harness = BreathingHarness::new();
        // Add a few simple tool executions
        harness.add_tool_execution("read", true);
        harness.add_tool_execution("read", true);

        let result = harness.evaluate();
        assert_eq!(result.zone, Zone::Green);
    }

    #[test]
    fn test_breathing_harness_red_zone() {
        let mut harness = BreathingHarness::new();
        // Add many errors to push into red zone
        for _ in 0..10 {
            harness.add_tool_error("bash");
        }

        let result = harness.evaluate();
        assert_eq!(result.zone, Zone::Red);
    }

    #[test]
    fn test_breathing_harness_decay() {
        let mut harness = BreathingHarness::new();
        harness.add_tool_execution("read", true);

        let before = harness.event_history[0].complexity;
        harness.apply_decay();
        let after = harness.event_history[0].complexity;

        assert!(after < before);
    }

    #[test]
    fn test_zone_trust_thresholds() {
        assert_eq!(Zone::Green.trust_threshold(), 1.0);
        assert_eq!(Zone::Yellow.trust_threshold(), 0.8);
        assert_eq!(Zone::Red.trust_threshold(), 0.6);
    }

    #[test]
    fn test_zone_delegation() {
        assert!(!Zone::Green.should_delegate());
        assert!(Zone::Yellow.should_delegate());
        assert!(Zone::Red.should_delegate());
    }
}