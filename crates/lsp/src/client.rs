//! LSP client for communicating with language servers

use std::path::Path;
use std::sync::Arc;

use tokio::sync::Mutex as TokioMutex;

use super::error::{LspError, Result};
use super::transport::{StdioTransport, LspTransport};
use super::types::*;

/// Client for interacting with a language server
pub struct LspClient {
    transport: Arc<TokioMutex<StdioTransport>>,
    capabilities: ServerCapabilities,
}

impl LspClient {
    /// Connect to a language server by spawning the given command
    pub async fn connect(cmd: &[&str], cwd: &Path) -> Result<Self> {
        let transport = StdioTransport::spawn(cmd, cwd).await?;
        Ok(Self {
            transport: Arc::new(TokioMutex::new(transport)),
            capabilities: ServerCapabilities::default(),
        })
    }

    /// Initialize the language server connection
    pub async fn initialize(&mut self) -> Result<()> {
        let mut transport = self.transport.lock().await;

        // Create initialization params
        let params = serde_json::json!({
            "processId": std::process::id(),
            "rootUri": null,
            "capabilities": {
                "textDocument": {
                    "synchronization": {},
                    "completion": {
                        "completionItem": {
                            "snippetSupport": true,
                            "documentationFormat": ["markdown", "plaintext"]
                        }
                    },
                    "hover": {},
                    "definition": {},
                    "references": {}
                },
                "workspace": {}
            }
        });

        let result = transport.send_request("initialize", params).await?;
        
        // Parse server capabilities from result
        if let Some(capabilities) = result.get("capabilities") {
            self.capabilities = serde_json::from_value(capabilities.clone())
                .unwrap_or_default();
        }

        // Send initialized notification
        let notif: LspMessage = LspMessage::Initialized;
        transport.send(notif).await?;

        Ok(())
    }

    /// Shutdown the language server gracefully
    pub async fn shutdown(&self) -> Result<()> {
        let transport = self.transport.lock().await;
        
        let req: LspMessage = LspMessage::Shutdown;
        transport.send(req).await?;
        
        let exit: LspMessage = LspMessage::Exit;
        transport.send(exit).await?;
        
        Ok(())
    }

    /// Get the server capabilities
    pub fn capabilities(&self) -> &ServerCapabilities {
        &self.capabilities
    }

    /// Request text document completions at the given position
    pub async fn text_document_completion(&self, uri: &str, pos: Position) -> Result<Vec<CompletionItem>> {
        let mut transport = self.transport.lock().await;
        
        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": pos.line, "character": pos.character }
        });

        let result = transport.send_request("textDocument/completion", params).await?;
        
        // Handle both CompletionList and Vec<CompletionItem> responses
        if let Some(items) = result.get("items") {
            serde_json::from_value(items.clone())
                .map_err(|_| LspError::InvalidResponse("Invalid completion items".to_string()))
        } else if result.get("isIncomplete").is_some() {
            // Some servers return { isIncomplete: false, items: [...] }
            if let Some(items) = result.get("items") {
                serde_json::from_value(items.clone())
                    .map_err(|_| LspError::InvalidResponse("Invalid completion items".to_string()))
            } else {
                Ok(vec![])
            }
        } else {
            // Try parsing as direct array
            serde_json::from_value(result)
                .map_err(|_| LspError::InvalidResponse("Invalid completion response".to_string()))
        }
    }

    /// Go to definition at the given position
    pub async fn goto_definition(&self, uri: &str, pos: Position) -> Result<Option<Location>> {
        let mut transport = self.transport.lock().await;
        
        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": pos.line, "character": pos.character }
        });

        let result = transport.send_request("textDocument/definition", params).await?;
        
        // Definition can return null, a single Location, or a Location[]
        if result.is_null() {
            return Ok(None);
        }
        
        // Try as single location
        if let Ok(location) = serde_json::from_value(result.clone()) {
            return Ok(Some(location));
        }
        
        // Try as array and take first
        if let Ok(locations) = serde_json::from_value::<Vec<Location>>(result) {
            return Ok(locations.into_iter().next());
        }
        
        Err(LspError::InvalidResponse("Invalid definition response".to_string()))
    }

    /// Find all references to the symbol at the given position
    pub async fn find_references(&self, uri: &str, pos: Position) -> Result<Vec<Location>> {
        let mut transport = self.transport.lock().await;
        
        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": pos.line, "character": pos.character },
            "context": { "includeDeclaration": true }
        });

        let result = transport.send_request("textDocument/references", params).await?;
        
        if result.is_null() {
            return Ok(vec![]);
        }
        
        serde_json::from_value(result)
            .map_err(|_| LspError::InvalidResponse("Invalid references response".to_string()))
    }

    /// Get hover information at the given position
    pub async fn hover(&self, uri: &str, pos: Position) -> Result<Option<Hover>> {
        let mut transport = self.transport.lock().await;
        
        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": pos.line, "character": pos.character }
        });

        let result = transport.send_request("textDocument/hover", params).await?;
        
        if result.is_null() {
            return Ok(None);
        }
        
        serde_json::from_value(result)
            .map_err(|_| LspError::InvalidResponse("Invalid hover response".to_string()))
    }

    /// Get diagnostics for a document
    pub async fn diagnostics(&self, uri: &str) -> Result<Vec<Diagnostic>> {
        let mut transport = self.transport.lock().await;
        
        let params = serde_json::json!({
            "textDocument": { "uri": uri }
        });

        // Some servers support textDocument/diagnostic
        let result = transport.send_request("textDocument/diagnostic", params).await;
        
        match result {
            Ok(result) => {
                if let Some(items) = result.get("items") {
                    serde_json::from_value(items.clone())
                        .map_err(|_| LspError::InvalidResponse("Invalid diagnostic items".to_string()))
                } else {
                    Ok(vec![])
                }
            }
            Err(_) => {
                // Server doesn't support diagnostic pull
                Ok(vec![])
            }
        }
    }
}
