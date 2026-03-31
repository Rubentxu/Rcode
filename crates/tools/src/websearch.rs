//! Websearch tool - web search using DuckDuckGo

use async_trait::async_trait;
use serde::Deserialize;

use opencode_core::{Tool, ToolContext, ToolResult, error::Result};

pub struct WebsearchTool;

impl WebsearchTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WebsearchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
pub struct WebsearchParams {
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    10
}

#[async_trait]
impl Tool for WebsearchTool {
    fn id(&self) -> &str { "websearch" }
    fn name(&self) -> &str { "Web Search" }
    fn description(&self) -> &str { "Search the web for information" }
    
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum results to return",
                    "default": 10
                }
            },
            "required": ["query"]
        })
    }
    
    async fn execute(&self, args: serde_json::Value, _context: &ToolContext) -> Result<ToolResult> {
        let params: WebsearchParams = serde_json::from_value(args)
            .map_err(|e| opencode_core::OpenCodeError::Tool(format!("Invalid parameters: {}", e)))?;
        
        // Simple web search using DuckDuckGo HTML
        let url = format!(
            "https://html.duckduckgo.com/html/?q={}",
            urlencoding::encode(&params.query)
        );
        
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| opencode_core::OpenCodeError::Tool(format!("Failed to create HTTP client: {}", e)))?;
        
        let response = client.get(&url)
            .send()
            .await
            .map_err(|e| opencode_core::OpenCodeError::Tool(format!("Failed to perform search: {}", e)))?;
        
        let body = response.text()
            .await
            .map_err(|e| opencode_core::OpenCodeError::Tool(format!("Failed to read response: {}", e)))?;
        
        // Parse simple results (a very basic parser)
        let results = parse_duckduckgo_results(&body, params.limit);
        
        let content = if results.is_empty() {
            format!("No results found for: {}", params.query)
        } else {
            results.join("\n\n")
        };
        
        Ok(ToolResult {
            title: format!("Search: {}", params.query),
            content,
            metadata: Some(serde_json::json!({
                "query": params.query,
                "result_count": results.len()
            })),
            attachments: vec![],
        })
    }
}

fn parse_duckduckgo_results(html: &str, limit: usize) -> Vec<String> {
    let mut results = Vec::new();
    for line in html.lines() {
        if line.contains("result__snippet") {
            let cleaned = line
                .replace("<b>", "")
                .replace("</b>", "")
                .replace("...", "");
            if !cleaned.is_empty() && results.len() < limit {
                results.push(cleaned.trim().to_string());
            }
        }
    }
    results
}