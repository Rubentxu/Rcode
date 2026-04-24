//! Information gain scoring for tool selection
//!
//! This algorithm evaluates tools based on the expected information gain
//! they provide - selecting tools that reduce uncertainty about the task.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Probability distribution over possible states
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Distribution {
    /// Probability for each state
    probs: Vec<f64>,
}

impl Distribution {
    /// Create a uniform distribution over n states
    pub fn uniform(n: usize) -> Self {
        let p = 1.0 / n as f64;
        Self {
            probs: vec![p; n],
        }
    }

    /// Create from existing probabilities
    pub fn from_probs(probs: Vec<f64>) -> Self {
        Self { probs }
    }

    /// Get probability of state i
    pub fn prob(&self, i: usize) -> f64 {
        self.probs.get(i).copied().unwrap_or(0.0)
    }

    /// Shannon entropy of this distribution
    pub fn entropy(&self) -> f64 {
        -self.probs
            .iter()
            .filter(|&&p| p > 0.0)
            .map(|&p| p * p.log2())
            .sum::<f64>()
    }

    /// Expected value of a function over this distribution
    pub fn expectation<F>(&self, f: &[f64]) -> f64
    where
        F: Fn(usize) -> f64,
    {
        self.probs
            .iter()
            .enumerate()
            .map(|(i, &p)| p * f[i])
            .sum()
    }

    /// Update with new observation
    pub fn update(&mut self, observed_state: usize, learning_rate: f64) {
        // Bayesian update: increase probability of observed state
        if let Some(prob) = self.probs.get_mut(observed_state) {
            *prob *= 1.0 + learning_rate;
        }

        // Renormalize
        let sum: f64 = self.probs.iter().sum();
        if sum > 0.0 {
            for prob in &mut self.probs {
                *prob /= sum;
            }
        }
    }
}

/// Information gain calculator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfoGainScorer {
    /// Prior distribution over states
    prior: Distribution,
    /// Learning rate for updates
    learning_rate: f64,
    /// State space size
    state_count: usize,
}

impl Default for InfoGainScorer {
    fn default() -> Self {
        Self::new(10)
    }
}

impl InfoGainScorer {
    /// Create with given state space size
    pub fn new(state_count: usize) -> Self {
        Self {
            prior: Distribution::uniform(state_count),
            learning_rate: 0.1,
            state_count,
        }
    }

    /// Create with custom learning rate
    pub fn with_learning_rate(state_count: usize, learning_rate: f64) -> Self {
        Self {
            prior: Distribution::uniform(state_count),
            learning_rate,
            state_count,
        }
    }

    /// Calculate information gain from a tool
    ///
    /// Returns expected reduction in entropy if we were to observe the tool's output
    pub fn info_gain(&self, tool_outcomes: &[f64]) -> f64 {
        if tool_outcomes.is_empty() || self.state_count == 0 {
            return 0.0;
        }

        let prior_entropy = self.prior.entropy();
        if prior_entropy == 0.0 {
            return 0.0;
        }

        // Expected entropy after observing tool outcome
        let mut expected_post_entropy = 0.0;

        for &outcome_prob in tool_outcomes {
            // Simplified: assume outcome maps to state reduction
            let post_entropy = prior_entropy * (1.0 - outcome_prob.abs());
            expected_post_entropy += outcome_prob.abs() * post_entropy;
        }

        prior_entropy - expected_post_entropy
    }

    /// Calculate the value of perfect information
    pub fn value_of_perfect_information(&self) -> f64 {
        // Maximum possible information gain = current entropy
        // (if we could observe the true state directly)
        self.prior.entropy()
    }

    /// Update with observed outcome
    pub fn update(&mut self, observed_state: usize) {
        self.prior.update(observed_state, self.learning_rate);
    }

    /// Get current entropy
    pub fn current_entropy(&self) -> f64 {
        self.prior.entropy()
    }

    /// Reset to uniform distribution
    pub fn reset(&mut self) {
        self.prior = Distribution::uniform(self.state_count);
    }

    /// Get probability distribution
    pub fn distribution(&self) -> &Distribution {
        &self.prior
    }
}

/// Tool with expected information gain
#[derive(Debug, Clone)]
pub struct ScoredTool {
    pub tool_id: String,
    pub info_gain: f64,
    pub expected_value: f64,
}

impl ScoredTool {
    /// Compare by info gain descending
    pub fn cmp_by_gain(&self, other: &Self) -> std::cmp::Ordering {
        other.info_gain.partial_cmp(&self.info_gain).unwrap_or(std::cmp::Ordering::Equal)
    }
}

/// Score multiple tools by information gain
pub fn score_tools(
    tools: &[String],
    outcome_probs: &HashMap<String, Vec<f64>>,
    scorer: &InfoGainScorer,
) -> Vec<ScoredTool> {
    tools
        .iter()
        .filter_map(|tool_id| {
            outcome_probs.get(tool_id).map(|outcomes| {
                ScoredTool {
                    tool_id: tool_id.clone(),
                    info_gain: scorer.info_gain(outcomes),
                    expected_value: outcomes.iter().sum::<f64>() / outcomes.len() as f64,
                }
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_distribution_uniform() {
        let dist = Distribution::uniform(4);
        assert_eq!(dist.prob(0), 0.25);
        assert_eq!(dist.prob(3), 0.25);
    }

    #[test]
    fn test_distribution_entropy() {
        let dist = Distribution::uniform(2);
        // Entropy of fair coin = 1 bit
        let entropy = dist.entropy();
        assert!((entropy - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_info_gain_scorer_new() {
        let scorer = InfoGainScorer::new(10);
        assert_eq!(scorer.state_count, 10);
        assert_eq!(scorer.current_entropy(), (10f64).log2());
    }

    #[test]
    fn test_info_gain_calculation() {
        let scorer = InfoGainScorer::new(10);
        let tool_outcomes = vec![0.5, 0.3, 0.2];
        let gain = scorer.info_gain(&tool_outcomes);
        assert!(gain >= 0.0);
    }

    #[test]
    fn test_info_gain_update() {
        let mut scorer = InfoGainScorer::new(10);
        let before = scorer.current_entropy();
        scorer.update(5);
        let after = scorer.current_entropy();
        // After update, distribution should be less uniform = lower entropy
        assert!(after <= before);
    }

    #[test]
    fn test_score_tools() {
        let scorer = InfoGainScorer::new(10);
        let tools = vec!["read".to_string(), "grep".to_string()];
        let mut outcome_probs = HashMap::new();
        outcome_probs.insert("read".to_string(), vec![0.6, 0.4]);
        outcome_probs.insert("grep".to_string(), vec![0.3, 0.7]);

        let scores = score_tools(&tools, &outcome_probs, &scorer);
        assert_eq!(scores.len(), 2);
    }
}