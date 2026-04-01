//! LSP tool adapter for integrating with the agent's tool system

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tracing::{debug, error};

use rcode_core::{Tool, ToolContext, ToolResult, error::Result as OpenCodeResult};

use super::client::LspClient;
use super::error::LspError;
use super::registry::LanguageServerRegistry;
use super::types::Position;

/// LSP tool adapter for code intelligence operations
#[allow(dead_code)]
pub struct LspToolAdapter {
    client: Arc<LspClient>,
    registry: Arc<LanguageServerRegistry>,
}

impl LspToolAdapter {
    /// Create a new LSP tool adapter
    pub fn new(client: Arc<LspClient>, registry: Arc<LanguageServerRegistry>) -> Self {
        Self { client, registry }
    }

    /// Create from registry, auto-detecting server for the file
    pub fn from_registry(uri: &str, registry: Arc<LanguageServerRegistry>) -> Option<Self> {
        // Parse URI to get path
        let path = if uri.starts_with("file://") {
            uri.trim_start_matches("file://")
        } else {
            uri
        };

        let client = if let Some(client) = registry.get_server_for_file(std::path::Path::new(path)) {
            client
        } else if let Some(lang) = LanguageServerRegistry::detect_language(std::path::Path::new(path)) {
            registry.get_server_for_language(&lang)?
        } else {
            return None;
        };

        Some(Self { client, registry })
    }
}

#[async_trait]
impl Tool for LspToolAdapter {
    fn id(&self) -> &str {
        "lsp"
    }

    fn name(&self) -> &str {
        "Code Intelligence"
    }

    fn description(&self) -> &str {
        "LSP-based code intelligence: goto-definition, find-references, completions, diagnostics"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Command to execute: goto-definition, find-references, completions, diagnostics, hover"
                },
                "uri": {
                    "type": "string",
                    "description": "URI of the file (file:///path/to/file)"
                },
                "line": {
                    "type": "integer",
                    "description": "Line number (0-based)"
                },
                "character": {
                    "type": "integer",
                    "description": "Character position on line (0-based)"
                }
            },
            "required": ["command", "uri", "line", "character"]
        })
    }

    async fn execute(&self, args: Value, _context: &ToolContext) -> OpenCodeResult<ToolResult> {
        let command = args["command"].as_str().unwrap_or("");
        let uri = args["uri"].as_str().unwrap_or("");
        let line = args["line"].as_u64().unwrap_or(0) as u32;
        let character = args["character"].as_u64().unwrap_or(0) as u32;

        let pos = Position::new(line, character);

        debug!("LSP tool: command={}, uri={}, pos={}:{}", command, uri, line, character);

        let result = match command {
            "goto-definition" => {
                match self.client.goto_definition(uri, pos).await {
                    Ok(Some(loc)) => format!("{:?}", loc),
                    Ok(None) => "No definition found".to_string(),
                    Err(e) => return Err(rcode_core::OpenCodeError::Tool(e.to_string())),
                }
            }
            "find-references" => {
                match self.client.find_references(uri, pos).await {
                    Ok(locs) if !locs.is_empty() => {
                        let mut s = String::new();
                        for loc in locs {
                            s.push_str(&format!("{}:{}:{}\n", loc.uri, loc.range.start.line, loc.range.start.character));
                        }
                        s
                    }
                    Ok(_) => "No references found".to_string(),
                    Err(e) => return Err(rcode_core::OpenCodeError::Tool(e.to_string())),
                }
            }
            "completions" => {
                match self.client.text_document_completion(uri, pos).await {
                    Ok(items) if !items.is_empty() => {
                        let mut s = String::new();
                        for item in items {
                            s.push_str(&format!("{}: {}\n", item.label, item.detail.as_deref().unwrap_or("")));
                        }
                        s
                    }
                    Ok(_) => "No completions available".to_string(),
                    Err(e) => return Err(rcode_core::OpenCodeError::Tool(e.to_string())),
                }
            }
            "hover" => {
                match self.client.hover(uri, pos).await {
                    Ok(Some(hover)) => {
                        match hover.contents {
                            super::types::MarkedString::String(s) => s,
                            super::types::MarkedString::Markdown(s) => s,
                        }
                    }
                    Ok(None) => "No hover information available".to_string(),
                    Err(e) => return Err(rcode_core::OpenCodeError::Tool(e.to_string())),
                }
            }
            "diagnostics" => {
                match self.client.diagnostics(uri).await {
                    Ok(diags) if !diags.is_empty() => {
                        let mut s = String::new();
                        for diag in diags {
                            let sev = diag.severity.map(|s| format!("{:?}", s)).unwrap_or_default();
                            s.push_str(&format!("{}: {} at {}:{}\n", sev, diag.message, diag.range.start.line, diag.range.start.character));
                        }
                        s
                    }
                    Ok(_) => "No diagnostics".to_string(),
                    Err(e) => return Err(rcode_core::OpenCodeError::Tool(e.to_string())),
                }
            }
            _ => {
                error!("Unknown LSP command: {}", command);
                return Err(rcode_core::OpenCodeError::Tool(format!("Unknown command: {}", command)));
            }
        };

        Ok(ToolResult {
            title: format!("LSP: {} on {}:{}", command, uri, line),
            content: result,
            metadata: None,
            attachments: vec![],
        })
    }
}

impl From<LspError> for rcode_core::OpenCodeError {
    fn from(e: LspError) -> Self {
        rcode_core::OpenCodeError::Tool(e.to_string())
    }
}
