//! Webfetch tool - URL content fetching

use async_trait::async_trait;

use opencode_core::{Tool, ToolContext, ToolResult, error::Result};

pub struct WebfetchTool;

impl WebfetchTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WebfetchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct WebfetchParams {
    pub url: String,
    #[serde(default = "default_format")]
    pub format: String,
}

fn default_format() -> String {
    "text".to_string()
}

#[async_trait]
impl Tool for WebfetchTool {
    fn id(&self) -> &str { "webfetch" }
    fn name(&self) -> &str { "Fetch URL Content" }
    fn description(&self) -> &str { "Fetch the content of a URL" }
    
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch"
                },
                "format": {
                    "type": "string",
                    "description": "Output format: text or markdown",
                    "default": "text"
                }
            },
            "required": ["url"]
        })
    }
    
    async fn execute(&self, args: serde_json::Value, _context: &ToolContext) -> Result<ToolResult> {
        let params: WebfetchParams = serde_json::from_value(args)
            .map_err(|e| opencode_core::OpenCodeError::Tool(format!("Invalid parameters: {}", e)))?;
        
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| opencode_core::OpenCodeError::Tool(format!("Failed to create HTTP client: {}", e)))?;
        
        let response = client.get(&params.url)
            .send()
            .await
            .map_err(|e| opencode_core::OpenCodeError::Tool(format!("Failed to fetch URL: {}", e)))?;
        
        let content = response.text()
            .await
            .map_err(|e| opencode_core::OpenCodeError::Tool(format!("Failed to read response: {}", e)))?;
        
        let content_preview = if content.len() > 1000 {
            format!("{}...[truncated {} chars]", &content[..1000], content.len() - 1000)
        } else {
            content.clone()
        };
        
        Ok(ToolResult {
            title: format!("Fetched: {}", params.url),
            content: content_preview,
            metadata: Some(serde_json::json!({
                "url": params.url,
                "format": params.format,
                "length": content.len()
            })),
            attachments: vec![],
        })
    }
}