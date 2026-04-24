//! Tool Intelligence Service
//!
//! This module integrates all intelligence algorithms into a unified service
//! for tool selection and task handling.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use parking_lot::RwLock;

use crate::algorithms::{
    BreathingHarness, CreativityProtector, DriftDetector, EntropyResult,
    InfoGainScorer, ProtectorConfig,
};
use crate::thompson_sampler::ThompsonSampler;

/// Tool Intelligence Service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntelligenceConfig {
    /// Enable Thompson sampling
    pub thompson_sampling: bool,
    /// Enable breathing harness
    pub breathing_harness: bool,
    /// Enable creativity protection
    pub creativity_protection: bool,
    /// Enable drift detection
    pub drift_detection: bool,
    /// Target TSR (Tool Selection Recall)
    pub target_tsr: f64,
    /// Target TER (Tool Error Rate)
    pub target_ter: f64,
    /// Target FASR (First Attempt Success Rate)
    pub target_fasr: f64,
    /// Target CUI (CogniCode Utility Index)
    pub target_cui: f64,
}

impl Default for IntelligenceConfig {
    fn default() -> Self {
        Self {
            thompson_sampling: true,
            breathing_harness: true,
            creativity_protection: true,
            drift_detection: true,
            target_tsr: 0.85,
            target_ter: 0.15,
            target_fasr: 0.90,
            target_cui: 0.80,
        }
    }
}

/// Current intelligence state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntelligenceState {
    /// Current entropy zone
    pub zone: String,
    /// Whether drift was detected
    pub drift_detected: bool,
    /// Current KPIs
    pub kpis: KPIs,
}

/// Key Performance Indicators
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KPIs {
    /// Tool Selection Recall - fraction of correct tool selections
    pub tsr: f64,
    /// Tool Error Rate - fraction of tool executions that error
    pub ter: f64,
    /// First Attempt Success Rate - fraction of tasks succeeding on first try
    pub fasr: f64,
    /// CogniCode Utility Index - composite utility metric
    pub cui: f64,
    /// Total tool selections
    pub total_selections: u64,
    /// Correct tool selections (for TSR computation)
    pub correct_selections: u64,
    /// Total tool errors
    pub total_errors: u64,
    /// Total tasks completed
    pub total_tasks: u64,
    /// Tasks succeeding on first attempt
    pub first_attempt_successes: u64,
}

impl KPIs {
    /// Update with a tool selection outcome
    pub fn record_selection(&mut self, correct: bool) {
        self.total_selections += 1;
        if correct {
            self.correct_selections += 1;
        } else {
            self.total_errors += 1;
        }
    }

    /// Update with a task completion
    pub fn record_task(&mut self, success: bool, first_attempt: bool) {
        self.total_tasks += 1;
        if success && first_attempt {
            self.first_attempt_successes += 1;
        }
    }

    /// Calculate TSR (Tool Selection Recall)
    pub fn calculate_tsr(&self) -> f64 {
        if self.total_selections == 0 {
            return 0.0;
        }
        self.correct_selections as f64 / self.total_selections as f64
    }

    /// Calculate TER (Tool Error Rate)
    pub fn calculate_ter(&self) -> f64 {
        if self.total_selections == 0 {
            return 0.0;
        }
        self.total_errors as f64 / self.total_selections as f64
    }

    /// Calculate FASR (First Attempt Success Rate)
    pub fn calculate_fasr(&self) -> f64 {
        if self.total_tasks == 0 {
            return 0.0;
        }
        self.first_attempt_successes as f64 / self.total_tasks as f64
    }

    /// Calculate CUI (CogniCode Utility Index)
    pub fn calculate_cui(&self) -> f64 {
        // Composite metric: weighted average of normalized KPIs
        let tsr_score = self.calculate_tsr();
        let fasr_score = self.calculate_fasr();
        let quality_score = 1.0 - self.calculate_ter();

        (tsr_score * 0.3 + fasr_score * 0.4 + quality_score * 0.3).min(1.0)
    }
}

/// Tool Intelligence Service
///
/// This service integrates all intelligence algorithms to provide
/// intelligent tool selection and task handling.
pub struct ToolIntelligenceService {
    config: IntelligenceConfig,
    thompson_sampler: ThompsonSampler,
    breathing_harness: BreathingHarness,
    creativity_protector: CreativityProtector,
    drift_detector: DriftDetector,
    info_gain_scorer: InfoGainScorer,
    kpis: KPIs,
    known_tools: Vec<String>,
}

impl Default for ToolIntelligenceService {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolIntelligenceService {
    /// Create a new tool intelligence service
    pub fn new() -> Self {
        Self::with_config(IntelligenceConfig::default())
    }

    /// Create with custom configuration
    pub fn with_config(config: IntelligenceConfig) -> Self {
        Self {
            config,
            thompson_sampler: ThompsonSampler::new(),
            breathing_harness: BreathingHarness::new(),
            creativity_protector: CreativityProtector::with_config(ProtectorConfig::default()),
            drift_detector: DriftDetector::new(vec![
                "read".to_string(),
                "write".to_string(),
                "edit".to_string(),
                "glob".to_string(),
                "grep".to_string(),
                "bash".to_string(),
            ]),
            info_gain_scorer: InfoGainScorer::new(10),
            kpis: KPIs::default(),
            known_tools: vec![
                "read".to_string(),
                "write".to_string(),
                "edit".to_string(),
                "glob".to_string(),
                "grep".to_string(),
                "bash".to_string(),
            ],
        }
    }

    /// Register a new tool
    pub fn register_tool(&mut self, tool_id: &str) {
        if !self.known_tools.contains(&tool_id.to_string()) {
            self.known_tools.push(tool_id.to_string());
            self.thompson_sampler.update(tool_id, true); // Initialize with success
            self.creativity_protector.register_action(tool_id, 0.5, 0.5);
        }
    }

    /// Record a tool execution result
    pub fn record_tool_result(&mut self, tool_id: &str, success: bool) {
        // Update Thompson sampler
        self.thompson_sampler.update(tool_id, success);

        // Update breathing harness
        if success {
            self.breathing_harness.add_tool_execution(tool_id, true);
        } else {
            self.breathing_harness.add_tool_error(tool_id);
        }

        // Update drift detector
        self.drift_detector.observe(tool_id);

        // Update KPIs
        self.kpis.record_selection(success);

        // Update creativity protector
        self.creativity_protector.record_execution(tool_id);
        if success {
            self.creativity_protector.update_value(tool_id, 1.0, 0.1);
        } else {
            self.creativity_protector.update_value(tool_id, 0.0, 0.1);
        }
    }

    /// Get tool recommendations using Thompson sampling
    pub fn recommend_tools(&mut self, _context: &str, count: usize) -> Vec<(String, f64)> {
        let scores = self.thompson_sampler.sample();
        let mut tool_scores: Vec<_> = scores.scores.into_iter().collect();
        tool_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        tool_scores.truncate(count);
        tool_scores
    }

    /// Evaluate current entropy zone
    pub fn evaluate_zone(&self) -> EntropyResult {
        self.breathing_harness.evaluate()
    }

    /// Check for drift in tool usage patterns
    pub fn check_drift(&self) -> bool {
        self.drift_detector.check_drift().drift_detected
    }

    /// Get current KPIs
    pub fn get_kpis(&self) -> KPIs {
        let mut kpis = self.kpis.clone();
        kpis.tsr = kpis.calculate_tsr();
        kpis.ter = kpis.calculate_ter();
        kpis.fasr = kpis.calculate_fasr();
        kpis.cui = kpis.calculate_cui();
        kpis
    }

    /// Check if KPIs meet targets
    pub fn check_kpi_targets(&self) -> KPIStatus {
        let kpis = self.get_kpis();
        KPIStatus {
            tsr_ok: kpis.tsr >= self.config.target_tsr,
            ter_ok: kpis.ter <= self.config.target_ter,
            fasr_ok: kpis.fasr >= self.config.target_fasr,
            cui_ok: kpis.cui >= self.config.target_cui,
            current: kpis,
            targets: self.config.clone(),
        }
    }

    /// Get current intelligence state
    pub fn get_state(&self) -> IntelligenceState {
        let zone = self.breathing_harness.evaluate();
        IntelligenceState {
            zone: format!("{:?}", zone.zone),
            drift_detected: self.drift_detector.check_drift().drift_detected,
            kpis: self.get_kpis(),
        }
    }

    /// Apply periodic decay (call periodically to reduce old event weight)
    pub fn apply_decay(&mut self) {
        self.thompson_sampler.apply_decay();
        self.breathing_harness.apply_decay();
        self.creativity_protector.apply_novelty_decay();
    }

    /// Get information gain scores for tools
    ///
    /// Info gain per tool = expected entropy reduction if we select that tool.
    /// We model each tool as a Bernoulli variable with success probability p
    /// (obtained from the Thompson sampler mean), and compute:
    ///   IG(tool) = info_gain([p, 1-p])
    pub fn get_info_gain_scores(&mut self) -> Vec<(String, f64)> {
        let sampled = self.thompson_sampler.sample();
        let mut scores = Vec::new();
        for tool in &self.known_tools {
            // Get the sampled success probability for this tool (default 0.5)
            let p = sampled.scores.get(tool).copied().unwrap_or(0.5).clamp(1e-9, 1.0 - 1e-9);
            // Info gain from a Bernoulli observation: H(prior) - E[H(posterior)]
            // Using [p, 1-p] as the outcome probability vector
            let gain = self.info_gain_scorer.info_gain(&[p, 1.0 - p]);
            scores.push((tool.clone(), gain));
        }
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores
    }
}

/// KPI status check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KPIStatus {
    pub tsr_ok: bool,
    pub ter_ok: bool,
    pub fasr_ok: bool,
    pub cui_ok: bool,
    pub current: KPIs,
    pub targets: IntelligenceConfig,
}

impl KPIStatus {
    /// Check if all KPIs meet targets
    pub fn all_ok(&self) -> bool {
        self.tsr_ok && self.ter_ok && self.fasr_ok && self.cui_ok
    }

    /// Get overall health score (0.0 - 1.0)
    pub fn health_score(&self) -> f64 {
        let mut score = 0.0;
        if self.tsr_ok { score += 0.25; }
        if self.ter_ok { score += 0.25; }
        if self.fasr_ok { score += 0.25; }
        if self.cui_ok { score += 0.25; }
        score
    }
}

/// Thread-safe wrapper for ToolIntelligenceService
pub type SharedIntelligence = Arc<RwLock<ToolIntelligenceService>>;

/// Create a new shared intelligence service
pub fn create_shared_intelligence() -> SharedIntelligence {
    Arc::new(RwLock::new(ToolIntelligenceService::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::algorithms::Zone;

    #[test]
    fn test_kpis_default() {
        let kpis = KPIs::default();
        assert_eq!(kpis.total_selections, 0);
        assert_eq!(kpis.total_errors, 0);
    }

    #[test]
    fn test_kpis_record_selection() {
        let mut kpis = KPIs::default();
        kpis.record_selection(true);
        assert_eq!(kpis.total_selections, 1);
    }

    #[test]
    fn test_tool_intelligence_new() {
        let service = ToolIntelligenceService::new();
        assert_eq!(service.known_tools.len(), 6);
    }

    #[test]
    fn test_tool_intelligence_register_tool() {
        let mut service = ToolIntelligenceService::new();
        service.register_tool("new_tool");
        assert!(service.known_tools.contains(&"new_tool".to_string()));
    }

    #[test]
    fn test_tool_intelligence_record_result() {
        let mut service = ToolIntelligenceService::new();
        service.record_tool_result("read", true);
        service.record_tool_result("bash", false);
        let kpis = service.get_kpis();
        assert_eq!(kpis.total_selections, 2);
    }

    #[test]
    fn test_evaluate_zone() {
        let service = ToolIntelligenceService::new();
        let zone = service.evaluate_zone();
        assert_eq!(zone.zone, Zone::Green);
    }

    #[test]
    fn test_kpi_status_all_ok() {
        let status = KPIStatus {
            tsr_ok: true,
            ter_ok: true,
            fasr_ok: true,
            cui_ok: true,
            current: KPIs::default(),
            targets: IntelligenceConfig::default(),
        };
        assert!(status.all_ok());
        assert_eq!(status.health_score(), 1.0);
    }

    #[test]
    fn test_kpi_status_some_failing() {
        let status = KPIStatus {
            tsr_ok: true,
            ter_ok: false,
            fasr_ok: true,
            cui_ok: false,
            current: KPIs::default(),
            targets: IntelligenceConfig::default(),
        };
        assert!(!status.all_ok());
        assert_eq!(status.health_score(), 0.5);
    }
}