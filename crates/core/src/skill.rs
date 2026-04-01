//! Skill types for opencode-rust

use serde::{Deserialize, Serialize};

/// Represents a skill with its metadata and instructions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub trigger: SkillTrigger,
    pub instructions: String,
    pub compatibility: SkillCompatibility,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

/// What triggers a skill to be considered
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum SkillTrigger {
    /// Triggered by a keyword in the conversation
    Keyword(String),
    /// Triggered by a command (e.g., /command)
    Command(String),
    /// Triggered when a file matches a pattern
    FilePattern(String),
}

/// Skill compatibility with different agent systems
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SkillCompatibility {
    Opencode,
    Claude,
    Agent,
}

impl Default for SkillCompatibility {
    fn default() -> Self {
        Self::Opencode
    }
}

impl Skill {
    /// Parse a skill from SKILL.md content
    pub fn parse(content: &str, path: &std::path::Path) -> Result<Self, SkillParseError> {
        // Simple frontmatter parsing
        let mut name = String::new();
        let mut description = String::new();
        let mut compatibility = SkillCompatibility::Opencode;
        let mut trigger = SkillTrigger::Command(String::new());
        let metadata = None;
        let instructions;

        let mut in_frontmatter = false;
        let mut frontmatter_content = String::new();
        let mut after_frontmatter = String::new();
        let mut found_closing = false;

        for line in content.lines() {
            if line.trim() == "---" {
                if !in_frontmatter {
                    in_frontmatter = true;
                    continue;
                } else if !found_closing {
                    in_frontmatter = false;
                    found_closing = true;
                    continue;
                }
            }

            if in_frontmatter {
                frontmatter_content.push_str(line);
                frontmatter_content.push('\n');
            } else if found_closing {
                after_frontmatter.push_str(line);
                after_frontmatter.push('\n');
            }
        }

        // If no frontmatter found, treat entire content as instructions
        if !found_closing && frontmatter_content.is_empty() {
            instructions = content.trim().to_string();
            name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
            description = format!("Skill from {}", path.display());
            return Ok(Skill {
                name,
                description,
                trigger,
                instructions,
                compatibility,
                metadata,
            });
        }

        // Parse frontmatter
        for line in frontmatter_content.lines() {
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim();
                let value = value.trim().trim_matches('"');
                match key {
                    "name" => name = value.to_string(),
                    "description" => description = value.to_string(),
                    "compatibility" => {
                        compatibility = match value.to_lowercase().as_str() {
                            "claude" => SkillCompatibility::Claude,
                            "agent" => SkillCompatibility::Agent,
                            _ => SkillCompatibility::Opencode,
                        }
                    }
                    "trigger" => {
                        if value.starts_with('/') {
                            trigger =
                                SkillTrigger::Command(value.trim_start_matches('/').to_string());
                        } else if value.starts_with("keyword:") {
                            trigger = SkillTrigger::Keyword(
                                value.trim_start_matches("keyword:").to_string(),
                            );
                        } else if value.starts_with("pattern:") {
                            trigger = SkillTrigger::FilePattern(
                                value.trim_start_matches("pattern:").to_string(),
                            );
                        } else {
                            trigger = SkillTrigger::Keyword(value.to_string());
                        }
                    }
                    _ => {}
                }
            }
        }

        instructions = after_frontmatter.trim().to_string();

        // Use filename as name if not specified
        if name.is_empty() {
            name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
        }

        // Use first line of instructions as description if not specified
        if description.is_empty() && !instructions.is_empty() {
            description = instructions
                .lines()
                .next()
                .unwrap_or("")
                .trim_start_matches("# ")
                .to_string();
        }

        Ok(Skill {
            name,
            description,
            trigger,
            instructions,
            compatibility,
            metadata,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SkillParseError {
    #[error("Failed to parse skill: {0}")]
    Parse(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_parse_skill_with_frontmatter() {
        let content = r#"---
name: rust-testing
description: Rust testing patterns for CLI applications
compatibility: opencode
---

# Rust Testing Skill

Instructions for testing Rust code..."#;

        let skill = Skill::parse(content, Path::new("rust-testing/SILL.md")).unwrap();
        assert_eq!(skill.name, "rust-testing");
        assert_eq!(
            skill.description,
            "Rust testing patterns for CLI applications"
        );
        assert!(skill.instructions.contains("Rust Testing Skill"));
    }

    #[test]
    fn test_parse_skill_command_trigger() {
        let content = r#"---
name: git-release
description: Create consistent releases
compatibility: opencode
trigger: /release
---

# Git Release"#;

        let skill = Skill::parse(content, Path::new("git-release/SKILL.md")).unwrap();
        match skill.trigger {
            SkillTrigger::Command(cmd) => assert_eq!(cmd, "release"),
            _ => panic!("Expected Command trigger"),
        }
    }
}
