//! Language server registry for managing multiple LSP connections

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::RwLock;
use tracing::info;

use super::client::LspClient;
use super::error::Result;

/// Configuration for a language server
#[derive(Clone)]
pub struct LanguageServerConfig {
    /// Command to execute (first element is the binary)
    pub cmd: Vec<String>,
    /// Additional arguments
    pub args: Vec<String>,
}

/// Registry for managing language server instances
pub struct LanguageServerRegistry {
    servers: RwLock<HashMap<String, Arc<LspClient>>>,
    by_file: RwLock<HashMap<PathBuf, String>>,
    by_language: RwLock<HashMap<String, String>>,
    /// Pre-configured language server commands by language
    language_configs: RwLock<HashMap<String, LanguageServerConfig>>,
}

impl LanguageServerRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        let registry = Self {
            servers: RwLock::new(HashMap::new()),
            by_file: RwLock::new(HashMap::new()),
            by_language: RwLock::new(HashMap::new()),
            language_configs: RwLock::new(HashMap::new()),
        };
        registry.register_default_language_servers();
        registry
    }

    /// Register default language server configurations
    fn register_default_language_servers(&self) {
        // Rust - rust-analyzer
        self.register_language_server("rust", vec!["rust-analyzer".to_string()], vec![]);
        // JavaScript/TypeScript - typescript-language-server
        self.register_language_server("javascript", vec!["typescript-language-server".to_string(), "--stdio".to_string()], vec![]);
        self.register_language_server("typescript", vec!["typescript-language-server".to_string(), "--stdio".to_string()], vec![]);
        // Python - pylsp (Python Language Server)
        self.register_language_server("python", vec!["pylsp".to_string()], vec![]);
        // Go - gopls
        self.register_language_server("go", vec!["gopls".to_string()], vec![]);
        // Java - jdtls
        self.register_language_server("java", vec!["jdtls".to_string()], vec![]);
        // C/C++ - ccls
        self.register_language_server("c", vec!["ccls".to_string()], vec![]);
        self.register_language_server("cpp", vec!["ccls".to_string()], vec![]);
    }

    /// Register a language server configuration for a language
    pub fn register_language_server(&self, language: &str, cmd: Vec<String>, args: Vec<String>) {
        self.language_configs.write().insert(
            language.to_string(),
            LanguageServerConfig { cmd, args },
        );
    }

    /// Get or start a language server for the given language
    /// Returns None if the language is not supported or server fails to start
    pub async fn get_or_start_for_language(
        &self,
        language: &str,
        project_root: &Path,
    ) -> Option<Arc<LspClient>> {
        // Check if server is already running
        if let Some(client) = self.get_server_for_language(language) {
            return Some(client);
        }

        // Try to get language config
        let config = self.language_configs.read().get(language).cloned()?;
        
        // Build command
        let mut full_cmd = config.cmd.clone();
        full_cmd.extend(config.args.iter().cloned());
        let cmd_refs: Vec<&str> = full_cmd.iter().map(|s| s.as_str()).collect();
        
        // Start server
        info!("Lazy starting language server for '{}' in '{}'", language, project_root.display());
        
        // Generate a unique name for this server
        let name = format!("{}-lsp", language);
        
        let mut client = match LspClient::connect(&cmd_refs, project_root).await {
            Ok(c) => c,
            Err(e) => {
                info!("Failed to connect to language server for '{}': {}", language, e);
                return None;
            }
        };
        
        if let Err(e) = client.initialize().await {
            info!("Failed to initialize language server for '{}': {}", language, e);
            return None;
        }
        
        let client = Arc::new(client);
        
        // Cache the running server
        self.servers.write().insert(name.clone(), client.clone());
        self.by_language.write().insert(language.to_string(), name);
        
        Some(client)
    }

    /// Start a new language server
    pub async fn start_server(
        &self,
        name: String,
        cmd: &[&str],
        cwd: &Path,
        language: &str,
    ) -> Result<()> {
        info!("Starting language server '{}' for language '{}'", name, language);

        let mut client = LspClient::connect(cmd, cwd).await?;
        client.initialize().await?;

        let client = Arc::new(client);
        
        self.servers.write().insert(name.clone(), client);
        self.by_language.write().insert(language.to_string(), name.clone());
        
        info!("Language server '{}' started successfully", name);
        Ok(())
    }

    /// Stop a language server
    pub fn stop_server(&self, name: &str) -> Result<()> {
        let client = self.servers.write().remove(name);
        
        if let Some(_client) = client {
            // Note: shutdown is async, can't call it here directly
            // In a real implementation, we'd need to handle this differently
            info!("Stopping language server '{}'", name);
        }
        
        // Clean up language mappings
        let mut by_lang = self.by_language.write();
        by_lang.retain(|_, v| v != name);
        
        // Clean up file mappings
        let mut by_file = self.by_file.write();
        by_file.retain(|_, v| v != name);
        
        Ok(())
    }

    /// Get a server by name
    pub fn get_server(&self, name: &str) -> Option<Arc<LspClient>> {
        self.servers.read().get(name).cloned()
    }

    /// Get the appropriate server for a file
    pub fn get_server_for_file(&self, path: &Path) -> Option<Arc<LspClient>> {
        // First check direct file mapping
        let name = self.by_file.read().get(path).cloned()?;
        self.servers.read().get(&name).cloned()
    }

    /// Get the appropriate server for a language
    pub fn get_server_for_language(&self, language: &str) -> Option<Arc<LspClient>> {
        let name = self.by_language.read().get(language).cloned()?;
        self.servers.read().get(&name).cloned()
    }

    /// Register a file to be handled by a specific server
    pub fn register_file(&self, path: PathBuf, server_name: String) {
        self.by_file.write().insert(path, server_name);
    }

    /// Auto-detect language from file extension
    pub fn detect_language(path: &Path) -> Option<String> {
        let ext = path.extension()?.to_str()?;
        
        match ext.to_lowercase().as_str() {
            "rs" => Some("rust".to_string()),
            "js" => Some("javascript".to_string()),
            "ts" => Some("typescript".to_string()),
            "jsx" => Some("javascript".to_string()),
            "tsx" => Some("typescript".to_string()),
            "py" => Some("python".to_string()),
            "go" => Some("go".to_string()),
            "java" => Some("java".to_string()),
            "c" => Some("c".to_string()),
            "cpp" | "cc" | "cxx" => Some("cpp".to_string()),
            "h" | "hpp" => Some("c".to_string()),
            "cs" => Some("csharp".to_string()),
            "rb" => Some("ruby".to_string()),
            "php" => Some("php".to_string()),
            "swift" => Some("swift".to_string()),
            "kt" | "kts" => Some("kotlin".to_string()),
            "scala" => Some("scala".to_string()),
            "lua" => Some("lua".to_string()),
            "r" => Some("r".to_string()),
            "sh" | "bash" | "zsh" => Some("shellscript".to_string()),
            "ps1" | "psm1" => Some("powershell".to_string()),
            "ex" | "exs" => Some("elixir".to_string()),
            "erl" => Some("erlang".to_string()),
            "hs" => Some("haskell".to_string()),
            "ml" | "mli" => Some("ocaml".to_string()),
            "fs" | "fsx" => Some("fsharp".to_string()),
            "vue" => Some("vue".to_string()),
            "svelte" => Some("svelte".to_string()),
            _ => None,
        }
    }

    /// List all registered servers
    pub fn list_servers(&self) -> Vec<String> {
        self.servers.read().keys().cloned().collect()
    }
}

impl Default for LanguageServerRegistry {
    fn default() -> Self {
        Self::new()
    }
}
