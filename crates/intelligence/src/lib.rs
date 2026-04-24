//! RCode Intelligence Module
//!
//! This crate provides tool intelligence through:
//! - **Thompson Sampling**: Bayesian approach to tool selection with exploration/exploitation
//! - **Entropic evaluation**: Measuring uncertainty in tool outcomes
//! - **BreathingHarness**: Entropy-based complexity handling (GREEN/YELLOW/RED zones)
//! - **Skill Evolution**: Cross-entropy method for skill optimization
//! - **Information Gain**: Tool selection based on expected information gain
//! - **Drift Detection**: KL-divergence based distribution drift detection
//! - **Creativity Protection**: Protects novel actions from being suppressed
//! - **ToolIntelligenceService**: Unified service integrating all algorithms with KPIs
//!
//! # KPIs
//!
//! | KPI | Target | Description |
//! |-----|--------|-------------|
//! | TSR | > 85% | Tool Selection Recall |
//! | TER | < 15% | Tool Error Rate |
//! | FASR | > 90% | First Attempt Success Rate |
//! | CUI | > 80% | CogniCode Utility Index |
//!
//! # Overview
//!
//! ```text
//! Tool Intelligence
//! ├── ThompsonSampler — Bayesian tool selection
//! ├── BreathingHarness — Entropy zone evaluation
//! ├── SkillEvolver — Cross-entropy skill evolution
//! ├── InfoGainScorer — Information gain tool scoring
//! ├── DriftDetector — Distribution drift detection
//! ├── CreativityProtector — Exploration protection
//! └── ToolIntelligenceService — Unified service with KPIs
//! ```

mod thompson_sampler;
pub mod algorithms;
pub mod service;
pub mod tdd_compliance;

pub use thompson_sampler::{
    ThompsonSampler, ToolPrior, ToolPriorState, ToolScores,
};

pub use service::{
    ToolIntelligenceService, IntelligenceConfig, IntelligenceState,
    KPIs, KPIStatus, SharedIntelligence, create_shared_intelligence,
};

/// Re-export for convenience
pub use thompson_sampler::ToolScores as Scores;

// TDD Compliance exports
pub use tdd_compliance::{
    TddComplianceChecker, TddComplianceResult, TddConfig, TddVerdict,
    PhaseResult, PhaseStatus, is_strict_tdd_enabled,
};