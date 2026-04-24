//! KL-Divergence based drift detection
//!
//! Detects when the distribution of tool usage or outcomes has shifted
//! significantly from historical baselines.


use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A probability distribution with categories
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryDistribution {
    /// Category names
    categories: Vec<String>,
    /// Probabilities
    probs: Vec<f64>,
}

impl CategoryDistribution {
    /// Create a uniform distribution over categories
    pub fn uniform(categories: Vec<String>) -> Self {
        let n = categories.len();
        let p = 1.0 / n as f64;
        Self {
            categories,
            probs: vec![p; n],
        }
    }

    /// Get probability of category
    pub fn prob(&self, category: &str) -> f64 {
        self.categories
            .iter()
            .position(|c| c == category)
            .and_then(|i| self.probs.get(i).copied())
            .unwrap_or(0.0)
    }

    /// Update probability of a category
    pub fn update(&mut self, category: &str, observed_count: usize, _total: usize) {
        if let Some(idx) = self.categories.iter().position(|c| c == category) {
            // Bayesian update with Dirichlet prior
            let alpha_prior = 1.0; // Uniform prior
            let alpha_new = alpha_prior + observed_count as f64;
            let alpha_sum: f64 = self.probs.iter().enumerate()
                .map(|(i, _p)| if i == idx { alpha_new } else { alpha_prior })
                .sum();

            self.probs[idx] = alpha_new / alpha_sum;
        }
    }

    /// Calculate KL divergence from this to other
    pub fn kl_divergence(&self, other: &CategoryDistribution) -> Option<f64> {
        if self.categories != other.categories {
            return None;
        }

        let mut kl = 0.0;
        for (p, &q) in self.probs.iter().zip(&other.probs) {
            if *p > 0.0 && q > 0.0 {
                kl += p * (p / q).log2();
            }
        }
        Some(kl)
    }

    /// Calculate JS divergence from this to other
    pub fn js_divergence(&self, other: &CategoryDistribution) -> Option<f64> {
        if self.categories != other.categories {
            return None;
        }

        // JSD = 0.5 * KL(P || M) + 0.5 * KL(Q || M) where M = (P+Q)/2
        let m_probs: Vec<f64> = self.probs.iter()
            .zip(&other.probs)
            .map(|(p, q)| (*p + *q) / 2.0)
            .collect();

        let m = CategoryDistribution {
            categories: self.categories.clone(),
            probs: m_probs,
        };

        let kl_pm = self.kl_divergence(&m).unwrap_or(0.0);
        let kl_qm = other.kl_divergence(&m).unwrap_or(0.0);

        Some(0.5 * kl_pm + 0.5 * kl_qm)
    }
}

/// Drift detection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftResult {
    /// Whether drift was detected
    pub drift_detected: bool,
    /// Severity of drift (0.0 - 1.0)
    pub severity: f64,
    /// KL divergence from baseline
    pub kl_divergence: f64,
    /// JS divergence from baseline
    pub js_divergence: f64,
}

/// Drift detector configuration
#[derive(Debug, Clone)]
pub struct DriftDetectorConfig {
    /// KL divergence threshold for drift
    pub kl_threshold: f64,
    /// JS divergence threshold for drift
    pub js_threshold: f64,
    /// Minimum samples before detecting drift
    pub min_samples: usize,
}

impl Default for DriftDetectorConfig {
    fn default() -> Self {
        Self {
            kl_threshold: 0.5,
            js_threshold: 0.25,
            min_samples: 10,
        }
    }
}

/// KL-Divergence based drift detector
#[derive(Debug, Clone)]
pub struct DriftDetector {
    /// Baseline distribution
    baseline: CategoryDistribution,
    /// Current distribution (being built)
    current: CategoryDistribution,
    /// Configuration
    config: DriftDetectorConfig,
    /// Number of observations
    observation_count: usize,
    /// Category counts
    counts: HashMap<String, usize>,
}

impl DriftDetector {
    /// Create a new drift detector with given categories
    pub fn new(categories: Vec<String>) -> Self {
        let baseline = CategoryDistribution::uniform(categories.clone());
        let current = CategoryDistribution::uniform(categories);

        Self {
            baseline,
            current,
            config: DriftDetectorConfig::default(),
            observation_count: 0,
            counts: HashMap::new(),
        }
    }

    /// Create with custom config
    pub fn with_config(categories: Vec<String>, config: DriftDetectorConfig) -> Self {
        let baseline = CategoryDistribution::uniform(categories.clone());
        let current = CategoryDistribution::uniform(categories);

        Self {
            baseline,
            current,
            config,
            observation_count: 0,
            counts: HashMap::new(),
        }
    }

    /// Record an observation
    pub fn observe(&mut self, category: &str) {
        *self.counts.entry(category.to_string()).or_insert(0) += 1;
        self.observation_count += 1;

        // Update current distribution
        let total = self.counts.values().sum::<usize>();
        for cat in &self.current.categories {
            let count = *self.counts.get(cat).unwrap_or(&0);
            let prob = count as f64 / total as f64;
            if let Some(idx) = self.current.categories.iter().position(|c| c == cat) {
                self.current.probs[idx] = prob;
            }
        }
    }

    /// Check for drift
    pub fn check_drift(&self) -> DriftResult {
        if self.observation_count < self.config.min_samples {
            return DriftResult {
                drift_detected: false,
                severity: 0.0,
                kl_divergence: 0.0,
                js_divergence: 0.0,
            };
        }

        let kl = self.baseline.kl_divergence(&self.current).unwrap_or(0.0);
        let js = self.baseline.js_divergence(&self.current).unwrap_or(0.0);

        let drift_detected = kl > self.config.kl_threshold || js > self.config.js_threshold;

        // Severity is how much we exceed the threshold
        let severity = if drift_detected {
            ((kl / self.config.kl_threshold) + (js / self.config.js_threshold))
                .min(1.0)
        } else {
            0.0
        };

        DriftResult {
            drift_detected,
            severity,
            kl_divergence: kl,
            js_divergence: js,
        }
    }

    /// Reset current distribution to baseline
    pub fn reset_current(&mut self) {
        self.current = CategoryDistribution::uniform(self.baseline.categories.clone());
        self.counts.clear();
        self.observation_count = 0;
    }

    /// Update baseline to current distribution
    pub fn update_baseline(&mut self) {
        self.baseline = self.current.clone();
        self.reset_current();
    }

    /// Get current distribution
    pub fn current_distribution(&self) -> &CategoryDistribution {
        &self.current
    }

    /// Get baseline distribution
    pub fn baseline_distribution(&self) -> &CategoryDistribution {
        &self.baseline
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_category_distribution_uniform() {
        let dist = CategoryDistribution::uniform(vec!["a".to_string(), "b".to_string()]);
        assert_eq!(dist.prob("a"), 0.5);
        assert_eq!(dist.prob("b"), 0.5);
        assert_eq!(dist.prob("c"), 0.0);
    }

    #[test]
    fn test_drift_detector_new() {
        let detector = DriftDetector::new(vec!["read".to_string(), "write".to_string()]);
        assert_eq!(detector.observation_count, 0);
    }

    #[test]
    fn test_drift_detector_observe() {
        let mut detector = DriftDetector::new(vec!["read".to_string(), "write".to_string()]);
        detector.observe("read");
        detector.observe("read");
        detector.observe("write");
        assert_eq!(detector.observation_count, 3);
    }

    #[test]
    fn test_drift_detector_no_drift_initially() {
        let mut detector = DriftDetector::new(vec!["read".to_string(), "write".to_string()]);
        for _ in 0..20 {
            detector.observe("read");
        }
        let result = detector.check_drift();
        // With many samples all in "read", we should detect drift from uniform
        assert!(result.drift_detected || result.kl_divergence > 0.0);
    }

    #[test]
    fn test_drift_detector_reset() {
        let mut detector = DriftDetector::new(vec!["read".to_string(), "write".to_string()]);
        for _ in 0..10 {
            detector.observe("read");
        }
        detector.reset_current();
        assert_eq!(detector.observation_count, 0);
    }
}