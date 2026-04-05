//! Skill types for RCode

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
    use serde_json;
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

    #[test]
    fn test_parse_skill_keyword_trigger() {
        let content = r#"---
name: rust-help
description: Help with Rust
trigger: keyword:rust
---

# Rust Help"#;

        let skill = Skill::parse(content, Path::new("rust-help/SKILL.md")).unwrap();
        match skill.trigger {
            SkillTrigger::Keyword(k) => assert_eq!(k, "rust"),
            _ => panic!("Expected Keyword trigger"),
        }
    }

    #[test]
    fn test_parse_skill_file_pattern_trigger() {
        let content = r#"---
name: rust-analyzer
description: Analyze Rust files
trigger: pattern:**/*.rs
---

# Rust Analyzer"#;

        let skill = Skill::parse(content, Path::new("rust-analyzer/SKILL.md")).unwrap();
        match skill.trigger {
            SkillTrigger::FilePattern(p) => assert_eq!(p, "**/*.rs"),
            _ => panic!("Expected FilePattern trigger"),
        }
    }

    #[test]
    fn test_parse_skill_without_frontmatter() {
        let content = "Just plain instructions without frontmatter.";

        let skill = Skill::parse(content, Path::new("plain-skill/SKILL.md")).unwrap();
        assert_eq!(skill.name, "SKILL"); // file_stem of "SKILL.md" is "SKILL"
        assert!(skill.instructions.contains("plain instructions"));
        // Default trigger should be Command("")
        match skill.trigger {
            SkillTrigger::Command(cmd) => assert_eq!(cmd, ""),
            _ => panic!("Expected Command trigger"),
        }
    }

    #[test]
    fn test_parse_skill_compatibility_claude() {
        let content = r#"---
name: claude-skill
description: Claude compatible skill
compatibility: claude
---

# Claude Skill"#;

        let skill = Skill::parse(content, Path::new("claude-skill/SKILL.md")).unwrap();
        assert_eq!(skill.compatibility, SkillCompatibility::Claude);
    }

    #[test]
    fn test_parse_skill_compatibility_agent() {
        let content = r#"---
name: agent-skill
description: Agent compatible skill
compatibility: agent
---

# Agent Skill"#;

        let skill = Skill::parse(content, Path::new("agent-skill/SKILL.md")).unwrap();
        assert_eq!(skill.compatibility, SkillCompatibility::Agent);
    }

    #[test]
    fn test_parse_skill_unknown_compatibility_defaults_to_opencode() {
        let content = r#"---
name: unknown-skill
description: Unknown compatibility
compatibility: unknown
---

# Unknown Skill"#;

        let skill = Skill::parse(content, Path::new("unknown-skill/SKILL.md")).unwrap();
        assert_eq!(skill.compatibility, SkillCompatibility::Opencode);
    }

    #[test]
    fn test_parse_skill_empty_name_uses_filename() {
        let content = r#"---
description: Has description but no name
---

# Content"#;

        let skill = Skill::parse(content, Path::new("my-skill/SKILL.md")).unwrap();
        assert_eq!(skill.name, "SKILL");
    }

    #[test]
    fn test_parse_skill_empty_description_uses_first_instruction_line() {
        let content = r#"---
name: test-skill
---

# This is the first line
And this is the second line"#;

        let skill = Skill::parse(content, Path::new("test-skill/SKILL.md")).unwrap();
        assert_eq!(skill.description, "This is the first line");
    }

    #[test]
    fn test_skill_serialization_roundtrip() {
        let skill = Skill {
            name: "test-skill".to_string(),
            description: "A test skill".to_string(),
            trigger: SkillTrigger::Command("test".to_string()),
            instructions: "Instructions here".to_string(),
            compatibility: SkillCompatibility::Opencode,
            metadata: Some(serde_json::json!({"key": "value"})),
        };

        let json = serde_json::to_string(&skill).unwrap();
        let parsed: Skill = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.name, skill.name);
        assert_eq!(parsed.description, skill.description);
        assert_eq!(parsed.instructions, skill.instructions);
        assert_eq!(parsed.compatibility, skill.compatibility);
    }

    #[test]
    fn test_skill_parse_error_type() {
        let content = "invalid";
        let result = Skill::parse(content, Path::new("test/SKILL.md"));
        // parse returns Result<Skill, SkillParseError>
        assert!(result.is_ok()); // Current implementation doesn't return errors
    }

    #[test]
    fn test_skill_compatibility_default() {
        assert_eq!(SkillCompatibility::default(), SkillCompatibility::Opencode);
    }

    #[test]
    fn test_skill_compatibility_serde() {
        use serde_json;

        let compat = SkillCompatibility::Claude;
        let json = serde_json::to_string(&compat).unwrap();
        assert!(json.contains("claude"));

        let parsed: SkillCompatibility = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, SkillCompatibility::Claude);
    }

    #[test]
    fn test_skill_trigger_keyword_variant() {
        let trigger = SkillTrigger::Keyword("test".to_string());
        match trigger {
            SkillTrigger::Keyword(k) => assert_eq!(k, "test"),
            _ => panic!("Expected Keyword"),
        }
    }

    #[test]
    fn test_skill_trigger_file_pattern_variant() {
        let trigger = SkillTrigger::FilePattern("*.rs".to_string());
        match trigger {
            SkillTrigger::FilePattern(p) => assert_eq!(p, "*.rs"),
            _ => panic!("Expected FilePattern"),
        }
    }

    #[test]
    fn test_skill_trigger_serde() {
        use serde_json;

        // Test Command roundtrip
        let trigger = SkillTrigger::Command("test-cmd".to_string());
        let json = serde_json::to_string(&trigger).unwrap();
        let parsed: SkillTrigger = serde_json::from_str(&json).unwrap();
        match (&parsed, &trigger) {
            (SkillTrigger::Command(p), SkillTrigger::Command(t)) => assert_eq!(p, t),
            _ => panic!("Expected Command variant"),
        }

        // Test Keyword roundtrip
        let trigger2 = SkillTrigger::Keyword("test-key".to_string());
        let json2 = serde_json::to_string(&trigger2).unwrap();
        let parsed2: SkillTrigger = serde_json::from_str(&json2).unwrap();
        match (&parsed2, &trigger2) {
            (SkillTrigger::Keyword(p), SkillTrigger::Keyword(t)) => assert_eq!(p, t),
            _ => panic!("Expected Keyword variant"),
        }

        // Test FilePattern roundtrip
        let trigger3 = SkillTrigger::FilePattern("*.go".to_string());
        let json3 = serde_json::to_string(&trigger3).unwrap();
        let parsed3: SkillTrigger = serde_json::from_str(&json3).unwrap();
        match (&parsed3, &trigger3) {
            (SkillTrigger::FilePattern(p), SkillTrigger::FilePattern(t)) => assert_eq!(p, t),
            _ => panic!("Expected FilePattern variant"),
        }
    }
}
