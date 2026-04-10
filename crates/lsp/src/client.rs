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
                    "references": {},
                    "documentSymbol": {}
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

    /// Get document symbols (outline) for a file
    /// Handles both hierarchical (DocumentSymbol[]) and flat (SymbolInformation[]) responses
    pub async fn document_symbols(&self, uri: &str) -> Result<Vec<DocumentSymbol>> {
        let mut transport = self.transport.lock().await;
        
        let params = DocumentSymbolParams {
            text_document: TextDocumentIdentifier {
                uri: uri.to_string(),
            },
            work_done_progress_params: None,
            partial_result_params: None,
        };

        let result = transport.send_request("textDocument/documentSymbol", serde_json::to_value(params)?).await?;
        
        // Handle both hierarchical and flat responses
        let response: DocumentSymbolResponse = serde_json::from_value(result)
            .map_err(|_| LspError::InvalidResponse("Invalid document symbol response".to_string()))?;
        
        match response {
            DocumentSymbolResponse::Hierarchical(symbols) => Ok(symbols),
            DocumentSymbolResponse::Flat(symbols) => {
                // Normalize flat SymbolInformation to hierarchical DocumentSymbol
                Ok(symbols.into_iter().map(|si| DocumentSymbol {
                    name: si.name,
                    kind: si.kind,
                    detail: si.container_name,
                    range: si.location.range.clone(),
                    selection_range: si.location.range.clone(),
                    children: None,
                    tags: si.tags,
                    deprecated: si.deprecated,
                }).collect())
            }
        }
    }

    /// Send textDocument/didOpen notification
    /// This is fire-and-forget - no response is expected
    pub async fn did_open(&self, uri: &str, language_id: &str, version: i32, text: &str) -> Result<()> {
        let transport = self.transport.lock().await;
        
        let params = TextDocumentDidOpenParams {
            text_document: TextDocumentItem {
                uri: uri.to_string(),
                language_id: language_id.to_string(),
                version,
                text: text.to_string(),
            },
        };

        let msg = LspMessage::TextDocumentDidOpen(params);
        transport.send(msg).await?;
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_symbol_response_hierarchical_parsing() {
        // Test that hierarchical response is correctly parsed
        let json = r#"[
            {
                "name": "MyStruct",
                "kind": 23,
                "range": {"start": {"line": 0, "character": 0}, "end": {"line": 10, "character": 0}},
                "selectionRange": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 8}},
                "children": [
                    {
                        "name": "my_method",
                        "kind": 6,
                        "range": {"start": {"line": 2, "character": 0}, "end": {"line": 4, "character": 0}},
                        "selectionRange": {"start": {"line": 2, "character": 0}, "end": {"line": 2, "character": 10}}
                    }
                ]
            }
        ]"#;
        
        let response: DocumentSymbolResponse = serde_json::from_str(json).unwrap();
        match response {
            DocumentSymbolResponse::Hierarchical(symbols) => {
                assert_eq!(symbols.len(), 1);
                assert_eq!(symbols[0].name, "MyStruct");
                assert_eq!(symbols[0].kind, SymbolKind::Struct);
                assert!(symbols[0].children.is_some());
                assert_eq!(symbols[0].children.as_ref().unwrap().len(), 1);
                assert_eq!(symbols[0].children.as_ref().unwrap()[0].name, "my_method");
            }
            DocumentSymbolResponse::Flat(_) => panic!("Expected hierarchical"),
        }
    }

    #[test]
    fn test_document_symbol_response_flat_normalization() {
        // Test that flat SymbolInformation response is normalized to hierarchical
        let json = r#"[
            {
                "name": "my_function",
                "kind": 12,
                "location": {"uri": "file:///src/main.rs", "range": {"start": {"line": 0, "character": 0}, "end": {"line": 5, "character": 0}}}
            }
        ]"#;
        
        let response: DocumentSymbolResponse = serde_json::from_str(json).unwrap();
        match response {
            DocumentSymbolResponse::Hierarchical(_) => panic!("Expected flat"),
            DocumentSymbolResponse::Flat(symbols) => {
                assert_eq!(symbols.len(), 1);
                assert_eq!(symbols[0].name, "my_function");
                assert_eq!(symbols[0].kind, SymbolKind::Function);
                // The location.range should be the same as selection_range after normalization
            }
        }
    }

    #[test]
    fn test_did_open_notification_format() {
        // Test that did_open serializes to the correct LSP notification format
        let params = TextDocumentDidOpenParams {
            text_document: TextDocumentItem {
                uri: "file:///src/main.rs".to_string(),
                language_id: "rust".to_string(),
                version: 1,
                text: "fn main() {}".to_string(),
            },
        };
        
        let msg = LspMessage::TextDocumentDidOpen(params);
        let json = serde_json::to_string(&msg).unwrap();
        
        assert!(json.contains("textDocument/didOpen"));
        assert!(json.contains("\"textDocument\""));
        assert!(json.contains("\"languageId\":"));
        assert!(json.contains("\"version\":"));
    }

    #[test]
    fn test_document_symbol_request_format() {
        // Test that document symbol request serializes correctly
        let params = DocumentSymbolParams {
            text_document: TextDocumentIdentifier {
                uri: "file:///src/main.rs".to_string(),
            },
            work_done_progress_params: None,
            partial_result_params: None,
        };
        
        let json = serde_json::to_string(&params).unwrap();
        
        assert!(json.contains("\"textDocument\""));
        assert!(json.contains("file:///src/main.rs"));
    }
}
