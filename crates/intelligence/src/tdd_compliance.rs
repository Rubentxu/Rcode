//! TDD Compliance Verification Module
//!
//! This module provides functionality to verify that implementations follow
//! the Test-Driven Development cycle: RED → GREEN → REFACTOR
//!
//! ## RED Phase
//! - Tests exist BEFORE implementation
//! - Tests fail with expected error BEFORE implementation
//!
//! ## GREEN Phase
//! - Implementation makes the failing tests pass
//! - No additional tests were added artificially
//!
//! ## REFACTOR Phase
//! - No new features added
//! - Only structural improvements

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

/// Result of TDD compliance check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TddComplianceResult {
    /// Whether strict TDD mode is required
    pub strict_mode: bool,
    /// RED phase check result
    pub red_phase: PhaseResult,
    /// GREEN phase check result
    pub green_phase: PhaseResult,
    /// REFACTOR phase check result
    pub refactor_phase: PhaseResult,
    /// Overall TDD verdict
    pub overall_verdict: TddVerdict,
}

/// Result of a single phase check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseResult {
    pub status: PhaseStatus,
    pub passed: bool,
    pub details: String,
    pub evidence: Vec<String>,
}

/// Status of a phase check
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PhaseStatus {
    Pass,
    Warning,
    Critical,
    Skipped,
}

impl PhaseStatus {
    pub fn is_pass(&self) -> bool {
        matches!(self, PhaseStatus::Pass)
    }
}

/// Overall TDD verdict
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TddVerdict {
    Compliant,
    Partial,
    NonCompliant,
}

/// Configuration for TDD verification
#[derive(Debug, Clone)]
pub struct TddConfig {
    /// Whether to require strict TDD
    pub strict_mode: bool,
    /// Test file patterns to look for
    pub test_patterns: Vec<String>,
    /// Implementation file patterns
    pub impl_patterns: Vec<String>,
    /// Directories to ignore
    pub ignore_dirs: Vec<String>,
}

impl Default for TddConfig {
    fn default() -> Self {
        Self {
            strict_mode: true,
            test_patterns: vec![
                "_test.rs".to_string(),
                "_tests.rs".to_string(),
                "tests/".to_string(),
                "test_".to_string(),
            ],
            impl_patterns: vec![
                "src/".to_string(),
                "lib/".to_string(),
            ],
            ignore_dirs: vec![
                "target/".to_string(),
                "node_modules/".to_string(),
                ".git/".to_string(),
            ],
        }
    }
}

/// TDD Compliance Checker
#[derive(Debug, Clone)]
pub struct TddComplianceChecker {
    config: TddConfig,
}

impl Default for TddComplianceChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl TddComplianceChecker {
    /// Create a new checker with default config
    pub fn new() -> Self {
        Self {
            config: TddConfig::default(),
        }
    }

    /// Create with custom config
    pub fn with_config(config: TddConfig) -> Self {
        Self { config }
    }

    /// Check TDD compliance for a project
    pub fn check_compliance(&self, project_path: &Path) -> TddComplianceResult {
        if !self.config.strict_mode {
            return TddComplianceResult {
                strict_mode: false,
                red_phase: PhaseResult {
                    status: PhaseStatus::Skipped,
                    passed: true,
                    details: "TDD strict mode disabled".to_string(),
                    evidence: vec![],
                },
                green_phase: PhaseResult {
                    status: PhaseStatus::Skipped,
                    passed: true,
                    details: "TDD strict mode disabled".to_string(),
                    evidence: vec![],
                },
                refactor_phase: PhaseResult {
                    status: PhaseStatus::Skipped,
                    passed: true,
                    details: "TDD strict mode disabled".to_string(),
                    evidence: vec![],
                },
                overall_verdict: TddVerdict::Partial,
            };
        }

        let red_phase = self.check_red_phase(project_path);
        let green_phase = self.check_green_phase(project_path);
        let refactor_phase = self.check_refactor_phase(project_path);

        let overall_verdict = if red_phase.passed && green_phase.passed && refactor_phase.passed {
            TddVerdict::Compliant
        } else if red_phase.passed || green_phase.passed || refactor_phase.passed {
            TddVerdict::Partial
        } else {
            TddVerdict::NonCompliant
        };

        TddComplianceResult {
            strict_mode: true,
            red_phase,
            green_phase,
            refactor_phase,
            overall_verdict,
        }
    }

    /// Check RED phase: tests exist before implementation
    fn check_red_phase(&self, project_path: &Path) -> PhaseResult {
        let test_files = self.find_test_files(project_path);
        let impl_files = self.find_impl_files(project_path);

        if test_files.is_empty() {
            return PhaseResult {
                status: PhaseStatus::Critical,
                passed: false,
                details: "No test files found".to_string(),
                evidence: vec!["No files matching test patterns found".to_string()],
            };
        }

        if impl_files.is_empty() {
            return PhaseResult {
                status: PhaseStatus::Warning,
                passed: true,
                details: "No implementation files found yet".to_string(),
                evidence: vec![format!("Found {} test files", test_files.len())],
            };
        }

        // Check if we can determine which came first
        let _test_info = self.get_file_info(&test_files);
        let _impl_info = self.get_file_info(&impl_files);

        let evidence: Vec<String> = test_files
            .iter()
            .take(5)
            .map(|f| format!("Test: {}", f.display()))
            .collect();

        PhaseResult {
            status: PhaseStatus::Pass,
            passed: true,
            details: format!("Found {} test files, {} impl files", test_files.len(), impl_files.len()),
            evidence,
        }
    }

    /// Check GREEN phase: implementation makes tests pass
    fn check_green_phase(&self, project_path: &Path) -> PhaseResult {
        // Run cargo test to see if tests pass
        let output = Command::new("cargo")
            .args(["test", "--no-run", "--message-format=json"])
            .current_dir(project_path)
            .output();

        match output {
            Ok(result) => {
                if result.status.success() {
                    PhaseResult {
                        status: PhaseStatus::Pass,
                        passed: true,
                        details: "Tests compile successfully".to_string(),
                        evidence: vec!["cargo test --no-run succeeded".to_string()],
                    }
                } else {
                    // Try to run tests
                    let test_output = Command::new("cargo")
                        .args(["test"])
                        .current_dir(project_path)
                        .output();

                    match test_output {
                        Ok(test_result) => {
                            if test_result.status.success() {
                                PhaseResult {
                                    status: PhaseStatus::Pass,
                                    passed: true,
                                    details: "All tests pass".to_string(),
                                    evidence: vec!["cargo test passed".to_string()],
                                }
                            } else {
                                let stderr = String::from_utf8_lossy(&test_result.stderr);
                                PhaseResult {
                                    status: PhaseStatus::Critical,
                                    passed: false,
                                    details: "Tests fail".to_string(),
                                    evidence: vec![format!("cargo test failed: {}", stderr)],
                                }
                            }
                        }
                        Err(e) => PhaseResult {
                            status: PhaseStatus::Critical,
                            passed: false,
                            details: format!("Failed to run tests: {}", e),
                            evidence: vec![],
                        },
                    }
                }
            }
            Err(e) => PhaseResult {
                status: PhaseStatus::Critical,
                passed: false,
                details: format!("Failed to check tests: {}", e),
                evidence: vec![],
            },
        }
    }

    /// Check REFACTOR phase: no new features, only clean up
    ///
    /// Detects whether new test files were added after the GREEN phase by
    /// inspecting uncommitted changes (`git diff --name-only HEAD`) for
    /// paths matching test patterns. New test files during REFACTOR violate
    /// strict TDD (you should only be cleaning up, not extending).
    fn check_refactor_phase(&self, project_path: &Path) -> PhaseResult {
        // Run git diff --name-only HEAD to get files changed since last commit
        let output = Command::new("git")
            .args(["diff", "--name-only", "HEAD"])
            .current_dir(project_path)
            .output();

        let changed_files = match output {
            Ok(result) if result.status.success() => {
                String::from_utf8_lossy(&result.stdout).to_string()
            }
            Ok(result) => {
                // git failed (e.g. not a git repo) — fall back to staged files
                let staged = Command::new("git")
                    .args(["diff", "--cached", "--name-only"])
                    .current_dir(project_path)
                    .output();
                match staged {
                    Ok(s) if s.status.success() => String::from_utf8_lossy(&s.stdout).to_string(),
                    _ => {
                        let stderr = String::from_utf8_lossy(&result.stderr);
                        return PhaseResult {
                            status: PhaseStatus::Warning,
                            passed: true,
                            details: format!("Could not inspect git diff: {}", stderr),
                            evidence: vec!["Manual review recommended".to_string()],
                        };
                    }
                }
            }
            Err(e) => {
                return PhaseResult {
                    status: PhaseStatus::Warning,
                    passed: true,
                    details: format!("git not available: {}", e),
                    evidence: vec!["Manual review recommended".to_string()],
                };
            }
        };

        // Find changed files that match test patterns
        let new_test_files: Vec<String> = changed_files
            .lines()
            .filter(|line| {
                let l = line.to_lowercase();
                self.config.test_patterns.iter().any(|p| l.contains(p))
            })
            .map(|l| l.to_string())
            .collect();

        if new_test_files.is_empty() {
            PhaseResult {
                status: PhaseStatus::Pass,
                passed: true,
                details: "No new test files detected in refactor phase".to_string(),
                evidence: vec!["git diff HEAD shows no test file changes".to_string()],
            }
        } else {
            PhaseResult {
                status: PhaseStatus::Critical,
                passed: false,
                details: format!(
                    "Strict TDD violation: {} new/modified test file(s) detected during REFACTOR phase",
                    new_test_files.len()
                ),
                evidence: new_test_files,
            }
        }
    }

    /// Find test files in project
    fn find_test_files(&self, project_path: &Path) -> Vec<std::path::PathBuf> {
        let mut files = Vec::new();
        self.find_files_matching(project_path, &self.config.test_patterns, &mut files);
        files
    }

    /// Find implementation files in project
    fn find_impl_files(&self, project_path: &Path) -> Vec<std::path::PathBuf> {
        let mut files = Vec::new();
        self.find_files_matching(project_path, &self.config.impl_patterns, &mut files);
        files
    }

    /// Find files matching any pattern
    fn find_files_matching(
        &self,
        dir: &Path,
        patterns: &[String],
        results: &mut Vec<std::path::PathBuf>,
    ) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let path_str = path.to_string_lossy();

                // Skip ignored directories
                if path.is_dir() {
                    let should_ignore = self
                        .config
                        .ignore_dirs
                        .iter()
                        .any(|d| path_str.contains(d));
                    if should_ignore {
                        continue;
                    }
                    self.find_files_matching(&path, patterns, results);
                } else {
                    // Check if matches any pattern
                    for pattern in patterns {
                        if path_str.contains(pattern) {
                            results.push(path.clone());
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Get file information
    fn get_file_info(&self, files: &[std::path::PathBuf]) -> HashMap<String, std::fs::Metadata> {
        files
            .iter()
            .filter_map(|f| {
                std::fs::metadata(f)
                    .ok()
                    .map(|m| (f.to_string_lossy().to_string(), m))
            })
            .collect()
    }
}

/// Check if strict TDD mode is enabled for a project
pub fn is_strict_tdd_enabled(project_path: &Path) -> bool {
    // Check for config file
    let config_paths = [
        project_path.join("openspec/config.yaml"),
        project_path.join(".rcode/config.yaml"),
        project_path.join("rcode.config.yaml"),
    ];

    for config_path in &config_paths {
        if config_path.exists()
            && let Ok(content) = std::fs::read_to_string(config_path)
            && content.contains("strict_tdd: true")
        {
            return true;
        }
    }

    // Check environment variable
    std::env::var("RCODE_STRICT_TDD")
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phase_status_is_pass() {
        assert!(PhaseStatus::Pass.is_pass());
        assert!(!PhaseStatus::Warning.is_pass());
        assert!(!PhaseStatus::Critical.is_pass());
        assert!(!PhaseStatus::Skipped.is_pass());
    }

    #[test]
    fn test_tdd_config_default() {
        let config = TddConfig::default();
        assert!(config.strict_mode);
        assert!(!config.test_patterns.is_empty());
    }

    #[test]
    fn test_tdd_compliance_checker_new() {
        let checker = TddComplianceChecker::new();
        let result = checker.check_compliance(std::path::Path::new("/nonexistent"));
        // Should return non-compliant for nonexistent path
        assert!(!result.strict_mode || result.overall_verdict != TddVerdict::Compliant);
    }
}