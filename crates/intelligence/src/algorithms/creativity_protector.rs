//! Ergodic Creativity Protector
//!
//! Ensures sufficient exploration by protecting creative/unusual actions
//! that might be suppressed by purely exploitation-focused algorithms.

use rand::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A creative action with its novelty score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreativeAction {
    /// Action identifier
    pub id: String,
    /// How novel/unusual this action is (0.0 - 1.0)
    pub novelty: f64,
    /// Expected value if action succeeds
    pub expected_value: f64,
    /// Number of times this action has been tried
    pub trial_count: usize,
}

impl CreativeAction {
    /// Create a new creative action
    pub fn new(id: &str, novelty: f64, expected_value: f64) -> Self {
        Self {
            id: id.to_string(),
            novelty: novelty.clamp(0.0, 1.0),
            expected_value,
            trial_count: 0,
        }
    }

    /// Calculate protected score (novelty * expected_value with trial penalty)
    pub fn protected_score(&self, min_trials: usize) -> f64 {
        let trial_factor = if self.trial_count < min_trials {
            // Still in protected period - boost novelty
            1.0 + (1.0 - self.trial_count as f64 / min_trials as f64) * self.novelty
        } else {
            1.0
        };

        self.expected_value * trial_factor
    }
}

/// Configuration for the creativity protector
#[derive(Debug, Clone)]
pub struct ProtectorConfig {
    /// Minimum trials before action is "proven"
    pub min_trials: usize,
    /// Novelty weight in protected score
    pub novelty_weight: f64,
    /// Random exploration probability
    pub explore_probability: f64,
    /// Novelty decay factor
    pub novelty_decay: f64,
}

impl Default for ProtectorConfig {
    fn default() -> Self {
        Self {
            min_trials: 5,
            novelty_weight: 0.3,
            explore_probability: 0.1,
            novelty_decay: 0.95,
        }
    }
}

/// Tracks actions and protects creative ones
#[derive(Debug, Clone)]
pub struct CreativityProtector {
    /// Known actions
    actions: HashMap<String, CreativeAction>,
    /// Configuration
    config: ProtectorConfig,
    /// Total trials
    total_trials: usize,
}

impl Default for CreativityProtector {
    fn default() -> Self {
        Self::new()
    }
}

impl CreativityProtector {
    /// Create a new protector
    pub fn new() -> Self {
        Self {
            actions: HashMap::new(),
            config: ProtectorConfig::default(),
            total_trials: 0,
        }
    }

    /// Create with custom config
    pub fn with_config(config: ProtectorConfig) -> Self {
        Self {
            actions: HashMap::new(),
            config,
            total_trials: 0,
        }
    }

    /// Register a new action
    pub fn register_action(&mut self, id: &str, novelty: f64, expected_value: f64) {
        self.actions.insert(
            id.to_string(),
            CreativeAction::new(id, novelty, expected_value),
        );
    }

    /// Record an action execution
    pub fn record_execution(&mut self, action_id: &str) {
        if let Some(action) = self.actions.get_mut(action_id) {
            action.trial_count += 1;
            self.total_trials += 1;
        }
    }

    /// Update expected value of an action based on outcome
    pub fn update_value(&mut self, action_id: &str, outcome: f64, learning_rate: f64) {
        if let Some(action) = self.actions.get_mut(action_id) {
            // Exponential moving average
            action.expected_value = action.expected_value * (1.0 - learning_rate)
                + outcome * learning_rate;
        }
    }

    /// Apply novelty decay (reduces novelty over time)
    pub fn apply_novelty_decay(&mut self) {
        for action in self.actions.values_mut() {
            action.novelty *= self.config.novelty_decay;
        }
    }

    /// Select an action using protected selection
    ///
    /// With probability `explore_probability`, selects a random creative action.
    /// Otherwise, selects the action with highest protected score.
    pub fn select_action<R: Rng>(&self, rng: &mut R) -> Option<String> {
        if self.actions.is_empty() {
            return None;
        }

        // Random exploration
        let explore: f64 = rng.sample(rand::distributions::Uniform::new(0.0, 1.0));
        if explore < self.config.explore_probability {
            // Pick a random creative action
            let creative_actions: Vec<_> = self.actions
                .values()
                .filter(|a| a.novelty > 0.3)
                .collect();

            if !creative_actions.is_empty() {
                let len = creative_actions.len();
                let idx = rng.sample(rand::distributions::Uniform::new(0usize, len));
                return Some(creative_actions[idx].id.clone());
            }
        }

        // Pick best protected score
        self.actions
            .values()
            .max_by(|a, b| {
                a.protected_score(self.config.min_trials)
                    .partial_cmp(&b.protected_score(self.config.min_trials))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|a| a.id.clone())
    }

    /// Get the most creative untried action
    pub fn most_creative_untried(&self) -> Option<String> {
        self.actions
            .values()
            .filter(|a| a.trial_count == 0 && a.novelty > 0.0)
            .max_by(|a, b| a.novelty.partial_cmp(&b.novelty).unwrap_or(std::cmp::Ordering::Equal))
            .map(|a| a.id.clone())
    }

    /// Get action info
    pub fn get_action(&self, id: &str) -> Option<&CreativeAction> {
        self.actions.get(id)
    }

    /// Get all actions sorted by protected score
    pub fn ranked_actions(&self) -> Vec<&CreativeAction> {
        let mut actions: Vec<_> = self.actions.values().collect();
        actions.sort_by(|a, b| {
            b.protected_score(self.config.min_trials)
                .partial_cmp(&a.protected_score(self.config.min_trials))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        actions
    }

    /// Get statistics
    pub fn stats(&self) -> ProtectorStats {
        let total_novelty: f64 = self.actions.values().map(|a| a.novelty).sum();
        let avg_novelty = if self.actions.is_empty() {
            0.0
        } else {
            total_novelty / self.actions.len() as f64
        };

        ProtectorStats {
            action_count: self.actions.len(),
            total_trials: self.total_trials,
            average_novelty: avg_novelty,
        }
    }
}

/// Statistics about the protector
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtectorStats {
    pub action_count: usize,
    pub total_trials: usize,
    pub average_novelty: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_creative_action_new() {
        let action = CreativeAction::new("test", 0.5, 0.8);
        assert_eq!(action.id, "test");
        assert_eq!(action.novelty, 0.5);
        assert_eq!(action.expected_value, 0.8);
        assert_eq!(action.trial_count, 0);
    }

    #[test]
    fn test_creative_action_protected_score() {
        let action = CreativeAction::new("test", 1.0, 1.0);
        // With 0 trials, should boost score
        assert!(action.protected_score(5) > 1.0);
        // With 5 trials, should be normal
        let action2 = CreativeAction { trial_count: 5, ..action };
        assert_eq!(action2.protected_score(5), 1.0);
    }

    #[test]
    fn test_protector_new() {
        let protector = CreativityProtector::new();
        assert_eq!(protector.actions.len(), 0);
    }

    #[test]
    fn test_protector_register() {
        let mut protector = CreativityProtector::new();
        protector.register_action("read", 0.2, 0.9);
        protector.register_action("explore", 0.8, 0.5);
        assert_eq!(protector.actions.len(), 2);
    }

    #[test]
    fn test_protector_record() {
        let mut protector = CreativityProtector::new();
        protector.register_action("read", 0.2, 0.9);
        protector.record_execution("read");
        assert_eq!(protector.get_action("read").unwrap().trial_count, 1);
    }

    #[test]
    fn test_protector_update_value() {
        let mut protector = CreativityProtector::new();
        protector.register_action("read", 0.2, 0.9);
        protector.update_value("read", 1.0, 0.5);
        // Should move towards 1.0
        let val = protector.get_action("read").unwrap().expected_value;
        assert!(val > 0.9);
    }

    #[test]
    fn test_protector_select() {
        let protector = CreativityProtector::new();
        let mut rng = StdRng::seed_from_u64(42);
        let selected = protector.select_action(&mut rng);
        assert!(selected.is_none()); // No actions registered
    }

    #[test]
    fn test_protector_ranked() {
        let mut protector = CreativityProtector::new();
        protector.register_action("low", 0.1, 0.5);
        protector.register_action("high", 0.9, 0.9);

        let ranked = protector.ranked_actions();
        assert_eq!(ranked.len(), 2);
        // high novelty with high value should be first
        assert_eq!(ranked[0].id, "high");
    }

    #[test]
    fn test_protector_stats() {
        let mut protector = CreativityProtector::new();
        protector.register_action("a", 0.5, 0.5);
        protector.register_action("b", 0.3, 0.7);

        let stats = protector.stats();
        assert_eq!(stats.action_count, 2);
        assert!((stats.average_novelty - 0.4).abs() < 0.001);
    }
}