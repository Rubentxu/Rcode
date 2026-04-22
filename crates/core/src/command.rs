//! Slash command types for RCode

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

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

/// CommandLoader discovers and loads slash commands from a directory.
///
/// Search paths are checked in order, with earlier paths taking precedence.
/// Commands from later paths are appended (later paths don't override earlier ones).
#[derive(Debug, Clone)]
pub struct CommandLoader {
    search_paths: Vec<PathBuf>,
}

impl CommandLoader {
    /// Create a new CommandLoader with the given search paths.
    pub fn new(search_paths: Vec<PathBuf>) -> Self {
        Self { search_paths }
    }

    /// Get default search paths for commands, in order of precedence:
    /// 1. ~/.config/opencode/commands/
    /// 2. ~/.config/rcode/commands/
    /// 3. ./.opencode/commands/
    pub fn default_paths() -> Vec<PathBuf> {
        vec![
            dirs::home_dir().map(|p| p.join(".config/opencode/commands")),
            dirs::home_dir().map(|p| p.join(".config/rcode/commands")),
            std::env::current_dir()
                .ok()
                .map(|p| p.join(".opencode/commands")),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    /// Get the opencode commands directory path (~/.config/opencode/commands/).
    pub fn get_commands_dir() -> Option<PathBuf> {
        dirs::home_dir().map(|p| p.join(".config/opencode/commands"))
    }

    /// Load all slash commands from the configured search paths.
    ///
    /// Commands are loaded from all `.md` files found in the search directories.
    /// Files are processed in order of search paths, and within each path,
    /// in directory order.
    ///
    /// Returns an empty vector if no commands are found (not an error).
    pub async fn load_commands(&self) -> Result<Vec<SlashCommand>, CommandLoaderError> {
        let mut commands = Vec::new();

        for search_path in &self.search_paths {
            if search_path.exists() && search_path.is_dir() {
                match self.load_commands_from_dir(search_path).await {
                    Ok(mut cmds) => commands.append(&mut cmds),
                    Err(e) => {
                        tracing::warn!("Failed to load commands from {:?}: {}", search_path, e);
                    }
                }
            }
        }

        Ok(commands)
    }

    /// Load commands from a specific directory (non-recursive).
    async fn load_commands_from_dir(&self, dir: &Path) -> Result<Vec<SlashCommand>, CommandLoaderError> {
        let mut commands = Vec::new();

        if !dir.is_dir() {
            return Ok(commands);
        }

        for entry in WalkDir::new(dir)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                match SlashCommand::from_file(path) {
                    Ok(cmd) => {
                        tracing::debug!("Loaded command '{}' from {:?}", cmd.name, path);
                        commands.push(cmd);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to parse command file {:?}: {}", path, e);
                    }
                }
            }
        }

        Ok(commands)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CommandLoaderError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Command parse error: {0}")]
    Parse(#[from] CommandParseError),
}

impl Default for CommandLoader {
    fn default() -> Self {
        Self::new(Self::default_paths())
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

    // =============================================================================
    // CommandLoader tests
    // =============================================================================

    #[tokio::test]
    async fn test_command_loader_default_paths() {
        let loader = CommandLoader::default();
        let paths = loader.search_paths;

        // Should have at least 3 default paths
        assert!(paths.len() >= 3);

        // First path should be ~/.config/opencode/commands
        assert!(paths[0]
            .to_string_lossy()
            .contains(".config/opencode/commands"));
    }

    #[tokio::test]
    async fn test_command_loader_load_from_empty_directory() {
        let temp = tempfile::tempdir().unwrap();
        let loader = CommandLoader::new(vec![temp.path().to_path_buf()]);

        let commands = loader.load_commands().await.unwrap();
        assert!(commands.is_empty());
    }

    #[tokio::test]
    async fn test_command_loader_load_single_command() {
        let temp = tempfile::tempdir().unwrap();
        let cmd_path = temp.path().join("greet.md");
        std::fs::write(
            &cmd_path,
            r#"---
name: greet
description: Greet the user
---

# Greeting

This command greets the user with a friendly message."#,
        )
        .unwrap();

        let loader = CommandLoader::new(vec![temp.path().to_path_buf()]);
        let commands = loader.load_commands().await.unwrap();

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name, "greet");
        assert_eq!(commands[0].description, "Greet the user");
    }

    #[tokio::test]
    async fn test_command_loader_load_multiple_commands() {
        let temp = tempfile::tempdir().unwrap();

        // Create multiple command files
        std::fs::write(
            temp.path().join("greet.md"),
            r#"---
name: greet
description: Greet the user
---

# Greeting command"#,
        )
        .unwrap();

        std::fs::write(
            temp.path().join("deploy.md"),
            r#"---
name: deploy
description: Deploy to production
---

# Deploy command"#,
        )
        .unwrap();

        std::fs::write(
            temp.path().join("analyze.md"),
            r#"---
name: analyze
description: Analyze code
---

# Analyze command"#,
        )
        .unwrap();

        let loader = CommandLoader::new(vec![temp.path().to_path_buf()]);
        let commands = loader.load_commands().await.unwrap();

        assert_eq!(commands.len(), 3);
        let names: Vec<&str> = commands.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"greet"));
        assert!(names.contains(&"deploy"));
        assert!(names.contains(&"analyze"));
    }

    #[tokio::test]
    async fn test_command_loader_skips_non_md_files() {
        let temp = tempfile::tempdir().unwrap();

        // Create a non-.md file
        std::fs::write(temp.path().join("readme.txt"), "Not a command").unwrap();
        std::fs::write(
            temp.path().join("config.json"),
            r#"{"name": "test"}"#,
        )
        .unwrap();

        let loader = CommandLoader::new(vec![temp.path().to_path_buf()]);
        let commands = loader.load_commands().await.unwrap();

        assert!(commands.is_empty());
    }

    #[tokio::test]
    async fn test_command_loader_with_nonexistent_directory() {
        let loader = CommandLoader::new(vec![PathBuf::from("/nonexistent/commands/dir")]);
        let commands = loader.load_commands().await.unwrap();
        assert!(commands.is_empty());
    }

    #[tokio::test]
    async fn test_command_loader_loads_all_valid_commands() {
        let temp = tempfile::tempdir().unwrap();

        // Create multiple command files - both parse successfully
        // (SlashCommand::parse treats content without frontmatter as instructions)
        std::fs::write(
            temp.path().join("bad.md"),
            "{ invalid content - treated as instructions",
        )
        .unwrap();

        std::fs::write(
            temp.path().join("good.md"),
            r#"---
name: good
description: A good command
---

# Good command"#,
        )
        .unwrap();

        let loader = CommandLoader::new(vec![temp.path().to_path_buf()]);
        let commands = loader.load_commands().await.unwrap();

        // Both commands load - parse doesn't fail on "malformed" content,
        // it just uses the content as instructions if no frontmatter
        assert_eq!(commands.len(), 2);
    }

    #[test]
    fn test_get_commands_dir() {
        let commands_dir = CommandLoader::get_commands_dir();
        assert!(commands_dir.is_some());
        assert!(commands_dir
            .unwrap()
            .to_string_lossy()
            .contains(".config/opencode/commands"));
    }
}
