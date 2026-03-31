//! Command discovery - finds and loads slash commands from configured paths

use std::path::{Path, PathBuf};
use anyhow::Result;
use tracing::{debug, warn};

use opencode_core::SlashCommand;

/// Default search paths for slash commands
pub fn default_search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // Global opencode commands
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".config/opencode/commands"));
    }

    // Project-local commands
    if let Ok(cwd) = std::env::current_dir() {
        paths.push(cwd.join(".opencode/commands"));
    }

    paths
}

/// Discovers slash commands from configured search paths
pub struct CommandDiscovery {
    search_paths: Vec<PathBuf>,
}

impl CommandDiscovery {
    /// Create a new CommandDiscovery with default search paths
    pub fn new() -> Self {
        Self {
            search_paths: default_search_paths(),
        }
    }

    /// Create with custom search paths
    pub fn with_paths(paths: Vec<PathBuf>) -> Self {
        Self {
            search_paths: paths,
        }
    }

    /// Find all commands in the configured search paths
    pub async fn discover_commands(&self) -> Result<Vec<SlashCommand>> {
        let mut commands = Vec::new();

        for path in &self.search_paths {
            debug!("Searching for commands in: {}", path.display());
            match self.discover_commands_in_dir(path).await {
                Ok(found) => {
                    debug!("Found {} commands in {}", found.len(), path.display());
                    commands.extend(found);
                }
                Err(e) => {
                    warn!("Error searching {}: {}", path.display(), e);
                }
            }
        }

        // Deduplicate by name
        let mut seen = std::collections::HashSet::new();
        commands.retain(|c| seen.insert(c.name.clone()));

        Ok(commands)
    }

    /// Find commands in a specific directory
    async fn discover_commands_in_dir(&self, dir: &Path) -> Result<Vec<SlashCommand>> {
        let mut commands = Vec::new();

        if !dir.exists() {
            return Ok(commands);
        }

        // Use synchronous directory walk (max_depth 2 for command files)
        Self::walk_dir_recursive_sync(dir, 0, &mut commands);

        Ok(commands)
    }

    /// Recursively walk directory up to max depth, finding .md files (synchronous)
    fn walk_dir_recursive_sync(
        dir: &Path,
        current_depth: usize,
        commands: &mut Vec<SlashCommand>,
    ) {
        const MAX_DEPTH: usize = 2;

        if current_depth > MAX_DEPTH {
            return;
        }

        let Ok(entries) = std::fs::read_dir(dir) else { return; };
        
        for entry in entries.flatten() {
            let path = entry.path();
            
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md") {
                match SlashCommand::from_file(&path) {
                    Ok(cmd) => {
                        debug!("Loaded command: {}", cmd.name);
                        commands.push(cmd);
                    }
                    Err(e) => {
                        warn!("Failed to load command from {}: {}", path.display(), e);
                    }
                }
            } else if path.is_dir() {
                Self::walk_dir_recursive_sync(&path, current_depth + 1, commands);
            }
        }
    }
}

impl Default for CommandDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_discover_commands_in_directory() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join("commands");
        fs::create_dir_all(&commands_dir).unwrap();
        
        fs::write(
            commands_dir.join("greet.md"),
            r#"---
name: greet
description: Greet the user
---

# Greeting
"#
        ).unwrap();

        fs::write(
            commands_dir.join("deploy.md"),
            r#"---
name: deploy
description: Deploy the app
---

# Deploy
"#
        ).unwrap();

        let discovery = CommandDiscovery::with_paths(vec![commands_dir]);
        let commands = discovery.discover_commands().await.unwrap();
        assert_eq!(commands.len(), 2);
    }

    #[tokio::test]
    async fn test_discover_nested_commands() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join("commands");
        fs::create_dir_all(commands_dir.join("subdir")).unwrap();
        
        fs::write(
            commands_dir.join("greet.md"),
            r#"---
name: greet
description: Greet the user
---

# Greeting
"#
        ).unwrap();

        fs::write(
            commands_dir.join("subdir/nested.md"),
            r#"---
name: nested
description: A nested command
---

# Nested
"#
        ).unwrap();

        let discovery = CommandDiscovery::with_paths(vec![commands_dir]);
        let commands = discovery.discover_commands().await.unwrap();
        assert_eq!(commands.len(), 2);
    }
}