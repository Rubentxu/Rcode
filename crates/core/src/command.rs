//! Slash command types for opencode-rust

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Represents a slash command with its metadata and instructions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlashCommand {
    pub name: String,
    pub description: String,
    pub instructions: String,
}

impl SlashCommand {
    /// Parse a slash command from a command file (.md) content
    pub fn parse(content: &str, path: &Path) -> Result<Self, CommandParseError> {
        let mut name = String::new();
        let mut description = String::new();
        let mut instructions = String::new();

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
            // Use filename as name
            name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
            // Don't return early - let the description extraction happen below
        } else {
            // Parse frontmatter
            for line in frontmatter_content.lines() {
                if let Some((key, value)) = line.split_once(':') {
                    let key = key.trim();
                    let value = value.trim().trim_matches('"');
                    match key {
                        "name" => name = value.to_string(),
                        "description" => description = value.to_string(),
                        _ => {}
                    }
                }
            }

            instructions = after_frontmatter.trim().to_string();
        }

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

        Ok(SlashCommand {
            name,
            description,
            instructions,
        })
    }

    /// Load a slash command from a file
    pub fn from_file(path: &Path) -> Result<Self, CommandParseError> {
        let content = std::fs::read_to_string(path)?;
        Self::parse(&content, path)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CommandParseError {
    #[error("Failed to parse command: {0}")]
    Parse(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_parse_command_with_frontmatter() {
        let content = r#"---
name: greet
description: Greet the user in a friendly way
---

# Greeting Command

This command greets the user with a personalized message."#;

        let cmd = SlashCommand::parse(content, Path::new("greet.md")).unwrap();
        assert_eq!(cmd.name, "greet");
        assert_eq!(cmd.description, "Greet the user in a friendly way");
        assert!(cmd.instructions.contains("Greeting Command"));
    }

    #[test]
    fn test_parse_command_without_frontmatter() {
        let content = r#"# Deploy Command

This command deploys the application to production."#;

        let cmd = SlashCommand::parse(content, Path::new("deploy.md")).unwrap();
        assert_eq!(cmd.name, "deploy");
        assert!(cmd.description.contains("Deploy Command"));
    }

    #[test]
    fn test_parse_command_name_from_filename() {
        let content = "Just instructions without frontmatter.";

        let cmd = SlashCommand::parse(content, Path::new("test-cmd.md")).unwrap();
        assert_eq!(cmd.name, "test-cmd");
    }
}
