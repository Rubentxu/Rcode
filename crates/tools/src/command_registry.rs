//! Command registry - stores and retrieves slash commands

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use opencode_core::SlashCommand;

/// Registry for storing and retrieving slash commands
pub struct CommandRegistry {
    commands: RwLock<HashMap<String, SlashCommand>>,
}

impl CommandRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            commands: RwLock::new(HashMap::new()),
        }
    }

    /// Register a slash command
    pub fn register(&self, command: SlashCommand) {
        self.commands.write().insert(command.name.clone(), command);
    }

    /// Get a command by name
    pub fn get(&self, name: &str) -> Option<SlashCommand> {
        self.commands.read().get(name).cloned()
    }

    /// List all registered commands
    pub fn list_all(&self) -> Vec<SlashCommand> {
        self.commands.read().values().cloned().collect()
    }

    /// Check if a command exists
    pub fn contains(&self, name: &str) -> bool {
        self.commands.read().contains_key(name)
    }

    /// Get the number of registered commands
    pub fn len(&self) -> usize {
        self.commands.read().len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.commands.read().is_empty()
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_command(name: &str, description: &str) -> SlashCommand {
        SlashCommand {
            name: name.to_string(),
            description: description.to_string(),
            instructions: format!("Instructions for {}", name),
        }
    }

    #[test]
    fn test_register_and_get() {
        let registry = CommandRegistry::new();
        registry.register(test_command("greet", "Greet the user"));

        assert!(registry.contains("greet"));
        assert!(!registry.contains("deploy"));

        let cmd = registry.get("greet").unwrap();
        assert_eq!(cmd.name, "greet");
        assert_eq!(cmd.description, "Greet the user");
    }

    #[test]
    fn test_list_all() {
        let registry = CommandRegistry::new();
        registry.register(test_command("greet", "Greet"));
        registry.register(test_command("deploy", "Deploy"));

        let all = registry.list_all();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_empty_registry() {
        let registry = CommandRegistry::new();
        assert!(registry.is_empty());
        assert!(registry.list_all().is_empty());
        assert!(registry.get("nonexistent").is_none());
    }
}
