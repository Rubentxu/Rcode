//! Thompson Sampling for tool selection
//!
//! This module implements Bayesian Thompson Sampling for intelligent tool selection.
//! It uses conjugate priors (Beta distribution for binary outcomes) to model
//! tool success rates and samples from the posterior to make exploration/exploitation
//! tradeoffs.

use rand::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A tool with its associated prior distribution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPrior {
    /// Tool identifier
    pub tool_id: String,
    /// Number of successes (alpha - 1)
    pub successes: f64,
    /// Number of failures (beta - 1)
    pub failures: f64,
}

impl ToolPrior {
    /// Create a new tool prior with uniform prior (1 success, 1 failure)
    pub fn uniform(tool_id: &str) -> Self {
        Self {
            tool_id: tool_id.to_string(),
            successes: 1.0,
            failures: 1.0,
        }
    }

    /// Create a tool prior with initial beliefs
    pub fn with_counts(tool_id: &str, successes: u64, failures: u64) -> Self {
        Self {
            tool_id: tool_id.to_string(),
            successes: successes as f64 + 1.0,
            failures: failures as f64 + 1.0,
        }
    }

    /// Update the prior with a new outcome
    pub fn update(&mut self, success: bool) {
        if success {
            self.successes += 1.0;
        } else {
            self.failures += 1.0;
        }
    }

    /// Sample from the posterior Beta distribution
    pub fn sample<R: Rng>(&self, rng: &mut R) -> f64 {
        // Sample from Beta(successes, failures)
        // Using the relationship: X ~ Beta(α, β) where X = Y/(Y+Z) with Y~Gamma(α,1) and Z~Gamma(β,1)
        let alpha = self.successes.max(0.001); // Avoid zero
        let beta = self.failures.max(0.001);

        // Simple approximation using ratio of gamma samples
        let y = sample_gamma(rng, alpha);
        let z = sample_gamma(rng, beta);
        y / (y + z)
    }

    /// Get the expected value of the posterior
    pub fn expected_value(&self) -> f64 {
        let alpha = self.successes.max(0.001);
        let beta = self.failures.max(0.001);
        alpha / (alpha + beta)
    }
}

/// Sample from Gamma distribution using Marsaglia and Tsang's method
fn sample_gamma<R: Rng>(rng: &mut R, shape: f64) -> f64 {
    if shape < 1.0 {
        // Use shape + 1 and multiply by U^(1/shape)
        let x = sample_gamma(rng, shape + 1.0);
        let u: f64 = rng.sample(rand::distributions::Uniform::new(0.0, 1.0));
        x * u.powf(1.0 / shape)
    } else {
        // Use Marsaglia and Tsang's method
        let d = shape - 1.0 / 3.0;
        let c = 1.0 / (9.0 * d).sqrt();
        loop {
            let mut x: f64;
            let mut v: f64;
            loop {
                x = rand_normal(rng);
                v = 1.0 + c * x;
                if v > 0.0 {
                    break;
                }
            }
            v = v * v * v;
            let u: f64 = rng.sample(rand::distributions::Uniform::new(0.0, 1.0));
            if u < 1.0 - 0.0331 * (x * x) * (x * x) {
                return d * v;
            }
            if u.ln() < 0.5 * x * x + d * (1.0 - v + v.ln()) {
                return d * v;
            }
        }
    }
}

/// Sample from standard normal distribution using Box-Muller
fn rand_normal<R: Rng>(rng: &mut R) -> f64 {
    let u1: f64 = rng.sample(rand::distributions::Uniform::new(0.0, 1.0));
    let u2: f64 = rng.sample(rand::distributions::Uniform::new(0.0, 1.0));
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

/// Prior state for all tools
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPriorState {
    /// Priors for each tool
    priors: HashMap<String, ToolPrior>,
    /// Temporal decay factor (per time unit)
    #[serde(default = "default_decay")]
    decay_factor: f64,
}

fn default_decay() -> f64 {
    0.99
}

impl Default for ToolPriorState {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolPriorState {
    /// Create a new empty prior state
    pub fn new() -> Self {
        Self {
            priors: HashMap::new(),
            decay_factor: 0.99,
        }
    }

    /// Create with a decay factor
    pub fn with_decay(decay_factor: f64) -> Self {
        Self {
            priors: HashMap::new(),
            decay_factor: decay_factor.clamp(0.9, 1.0),
        }
    }

    /// Get or create a prior for a tool
    pub fn get_or_create(&mut self, tool_id: &str) -> &mut ToolPrior {
        if !self.priors.contains_key(tool_id) {
            self.priors.insert(tool_id.to_string(), ToolPrior::uniform(tool_id));
        }
        self.priors.get_mut(tool_id).unwrap()
    }

    /// Update a tool with a new outcome
    pub fn update(&mut self, tool_id: &str, success: bool) {
        let prior = self.get_or_create(tool_id);
        prior.update(success);
    }

    /// Apply temporal decay to all priors (reduces confidence over time)
    pub fn apply_decay(&mut self) {
        for prior in self.priors.values_mut() {
            prior.successes *= self.decay_factor;
            prior.failures *= self.decay_factor;
        }
    }

    /// Sample all tools and return their scores
    pub fn sample_all<R: Rng>(&self, rng: &mut R) -> HashMap<String, f64> {
        self.priors
            .iter()
            .map(|(id, prior)| (id.clone(), prior.sample(rng)))
            .collect()
    }

    /// Get expected values for all tools
    pub fn expected_values(&self) -> HashMap<String, f64> {
        self.priors
            .iter()
            .map(|(id, prior)| (id.clone(), prior.expected_value()))
            .collect()
    }

    /// List all known tool IDs
    pub fn tool_ids(&self) -> Vec<String> {
        self.priors.keys().cloned().collect()
    }
}

/// Result of tool evaluation with Thompson sampling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolScores {
    /// Scores for each tool (sampled values)
    pub scores: HashMap<String, f64>,
    /// Which tool has the highest score
    pub best_tool: Option<String>,
    /// Entropy of the score distribution (for exploration signal)
    pub entropy: f64,
}

impl ToolScores {
    /// Create tool scores from sampled values
    pub fn from_samples(samples: HashMap<String, f64>) -> Self {
        let best_tool = samples
            .iter()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(id, _)| id.clone());

        let values: Vec<f64> = samples.values().cloned().collect();
        let entropy = calculate_entropy(&values);

        Self {
            scores: samples,
            best_tool,
            entropy,
        }
    }

    /// Get the score for a specific tool
    pub fn get(&self, tool_id: &str) -> Option<f64> {
        self.scores.get(tool_id).copied()
    }

    /// Get the best tool if known
    pub fn best(&self) -> Option<&str> {
        self.best_tool.as_deref()
    }
}

/// Calculate entropy of a distribution
fn calculate_entropy(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }

    let sum: f64 = values.iter().sum();
    if sum == 0.0 {
        return 0.0;
    }

    // Normalize to get probabilities
    let probs: Vec<f64> = values.iter().map(|v| v / sum).collect();

    // Calculate Shannon entropy
    let entropy = -probs
        .iter()
        .filter(|&&p| p > 0.0)
        .map(|p| p * p.log2())
        .sum::<f64>();

    entropy.max(0.0)
}

/// Thompson Sampler for tool selection
#[derive(Debug, Clone)]
pub struct ThompsonSampler {
    /// Prior state for all tools
    state: ToolPriorState,
    /// Random number generator seed
    seed: u64,
}

impl Default for ThompsonSampler {
    fn default() -> Self {
        Self::new()
    }
}

impl ThompsonSampler {
    /// Create a new Thompson sampler
    pub fn new() -> Self {
        Self {
            state: ToolPriorState::new(),
            seed: rand::thread_rng().sample(rand::distributions::Uniform::new(0.0, u64::MAX as f64)) as u64,
        }
    }

    /// Create with custom decay factor
    pub fn with_decay(decay_factor: f64) -> Self {
        Self {
            state: ToolPriorState::with_decay(decay_factor),
            seed: rand::thread_rng().sample(rand::distributions::Uniform::new(0.0, u64::MAX as f64)) as u64,
        }
    }

    /// Update a tool with a new outcome
    pub fn update(&mut self, tool_id: &str, success: bool) {
        self.state.update(tool_id, success);
    }

    /// Apply temporal decay (call periodically)
    pub fn apply_decay(&mut self) {
        self.state.apply_decay();
    }

    /// Sample tools using Thompson sampling
    ///
    /// Returns scores for all known tools, sampled from their posterior distributions.
    /// The tool with highest score is the recommended one for exploration/exploitation.
    pub fn sample(&mut self) -> ToolScores {
        let mut rng = StdRng::seed_from_u64(self.seed);
        self.seed = rng.sample(rand::distributions::Uniform::new(0.0, u64::MAX as f64)) as u64;

        let samples = self.state.sample_all(&mut rng);
        ToolScores::from_samples(samples)
    }

    /// Get expected success rates (without sampling)
    pub fn expected_rates(&self) -> HashMap<String, f64> {
        self.state.expected_values()
    }

    /// Get the recommended tool (highest sampled score)
    pub fn recommend(&mut self) -> Option<String> {
        self.sample().best_tool
    }

    /// Check if the sampler has learned anything about a tool
    pub fn has_data(&self, tool_id: &str) -> bool {
        self.state.priors.contains_key(tool_id)
    }

    /// Get the number of observations for a tool
    pub fn observation_count(&self, tool_id: &str) -> Option<(u64, u64)> {
        self.state.priors.get(tool_id).map(|p| {
            ((p.successes - 1.0) as u64, (p.failures - 1.0) as u64)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_prior_uniform() {
        let prior = ToolPrior::uniform("test");
        assert_eq!(prior.successes, 1.0);
        assert_eq!(prior.failures, 1.0);
        assert_eq!(prior.expected_value(), 0.5);
    }

    #[test]
    fn test_tool_prior_update() {
        let mut prior = ToolPrior::uniform("test");
        prior.update(true);
        assert_eq!(prior.successes, 2.0);
        prior.update(false);
        assert_eq!(prior.failures, 2.0);
    }

    #[test]
    fn test_tool_prior_sample() {
        let prior = ToolPrior::uniform("test");
        let mut rng = StdRng::seed_from_u64(42);
        let sample = prior.sample(&mut rng);
        assert!((0.0..=1.0).contains(&sample));
    }

    #[test]
    fn test_prior_state_new() {
        let state = ToolPriorState::new();
        assert!(state.tool_ids().is_empty());
    }

    #[test]
    fn test_prior_state_update() {
        let mut state = ToolPriorState::new();
        state.update("read", true);
        state.update("read", true);
        state.update("read", false);

        assert!(state.priors.contains_key("read"));
        let prior = state.priors.get("read").unwrap();
        assert_eq!(prior.successes, 3.0); // 2 successes + 1
        assert_eq!(prior.failures, 2.0);  // 1 failure + 1
    }

    #[test]
    fn test_prior_state_decay() {
        let mut state = ToolPriorState::with_decay(0.5);
        state.update("read", true);
        state.update("read", true);

        let before = state.priors.get("read").unwrap().successes;
        state.apply_decay();
        let after = state.priors.get("read").unwrap().successes;

        assert!(after < before);
    }

    #[test]
    fn test_thompson_sampler_update() {
        let mut sampler = ThompsonSampler::new();
        sampler.update("read", true);
        sampler.update("read", true);
        sampler.update("glob", false);

        assert!(sampler.has_data("read"));
        assert!(sampler.has_data("glob"));
        assert!(!sampler.has_data("unknown"));
    }

    #[test]
    fn test_thompson_sampler_sample() {
        let mut sampler = ThompsonSampler::new();
        sampler.update("read", true);
        sampler.update("read", true);
        sampler.update("glob", false);

        let scores = sampler.sample();
        assert!(scores.scores.contains_key("read"));
        assert!(scores.scores.contains_key("glob"));
        assert!(scores.best_tool.is_some());
    }

    #[test]
    fn test_thompson_sampler_recommend() {
        let mut sampler = ThompsonSampler::new();
        // Strong signal for read being good
        for _ in 0..10 {
            sampler.update("read", true);
        }
        // Weak signal for glob being bad
        sampler.update("glob", false);

        let recommended = sampler.recommend();
        assert!(recommended.is_some());
        // read should likely be recommended due to strong success history
    }

    #[test]
    fn test_tool_scores_entropy() {
        let samples: HashMap<String, f64> = vec![
            ("a".to_string(), 0.5),
            ("b".to_string(), 0.5),
        ].into_iter().collect();

        let scores = ToolScores::from_samples(samples);
        assert!(scores.entropy > 0.0);
    }

    #[test]
    fn test_calculate_entropy_equal() {
        let values = vec![0.5, 0.5];
        let entropy = calculate_entropy(&values);
        assert!(entropy > 0.0);
    }

    #[test]
    fn test_calculate_entropy_skewed() {
        let values = vec![0.9, 0.1];
        let entropy = calculate_entropy(&values);
        assert!(entropy > 0.0);
        // More skewed = lower entropy
    }
}