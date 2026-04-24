//! Intelligence algorithms module
//!
//! This module contains various algorithms for tool intelligence:
//! - BreathingHarness: Entropy-based complexity handling
//! - SkillEvolver: Cross-entropy based skill evolution
//! - InfoGainScorer: Information gain based tool scoring
//! - DriftDetector: KL-divergence based drift detection
//! - CreativityProtector: Protects creative exploration

pub mod harness;
pub mod skill_evolvers;
pub mod info_gain;
pub mod drift_detector;
pub mod creativity_protector;

// Re-exports
pub use harness::{
    BreathingHarness, EntropyEvent, EntropyResult, EventType, HarnessConfig, Zone,
};
pub use skill_evolvers::{Skill, SkillEvolver, SkillPopulation};
pub use info_gain::{Distribution, InfoGainScorer, ScoredTool};
pub use drift_detector::{
    CategoryDistribution, DriftDetector, DriftDetectorConfig, DriftResult,
};
pub use creativity_protector::{
    CreativityProtector, CreativeAction, ProtectorConfig, ProtectorStats,
};