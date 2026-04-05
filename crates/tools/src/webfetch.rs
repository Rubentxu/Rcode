//! Webfetch tool - URL content fetching

use async_trait::async_trait;

use rcode_core::{Tool, ToolContext, ToolResult, error::Result};

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
            .map_err(|e| rcode_core::RCodeError::Tool(format!("Invalid parameters: {}", e)))?;
        
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| rcode_core::RCodeError::Tool(format!("Failed to create HTTP client: {}", e)))?;
        
        let response = client.get(&params.url)
            .send()
            .await
            .map_err(|e| rcode_core::RCodeError::Tool(format!("Failed to fetch URL: {}", e)))?;
        
        let content = response.text()
            .await
            .map_err(|e| rcode_core::RCodeError::Tool(format!("Failed to read response: {}", e)))?;
        
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

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::ToolContext;
    use std::path::PathBuf;

    fn ctx() -> ToolContext {
        ToolContext { session_id: "s1".into(), project_path: PathBuf::from("/tmp"), cwd: PathBuf::from("/tmp"), user_id: None, agent: "test".into() }
    }

    #[tokio::test]
    async fn test_webfetch_invalid_params() {
        let tool = WebfetchTool::new();
        let result = tool.execute(serde_json::json!({"url": 123}), &ctx()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_webfetch_missing_url() {
        let tool = WebfetchTool::new();
        let result = tool.execute(serde_json::json!({}), &ctx()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_webfetch_invalid_url() {
        let tool = WebfetchTool::new();
        let result = tool.execute(serde_json::json!({"url": "not-a-valid-url"}), &ctx()).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_webfetch_params_default_format() {
        let json = serde_json::json!({
            "url": "https://example.com"
        });
        let params: WebfetchParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.format, "text");
    }

    #[test]
    fn test_webfetch_params_explicit_format() {
        let json = serde_json::json!({
            "url": "https://example.com",
            "format": "markdown"
        });
        let params: WebfetchParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.format, "markdown");
    }

    #[test]
    fn test_webfetch_params_url_only() {
        let json = serde_json::json!({
            "url": "https://example.com/page"
        });
        let params: WebfetchParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.url, "https://example.com/page");
    }

    #[test]
    fn test_default_format() {
        assert_eq!(default_format(), "text");
    }

    #[tokio::test]
    async fn test_webfetch_tool_id_and_name() {
        let tool = WebfetchTool::new();
        assert_eq!(tool.id(), "webfetch");
        assert_eq!(tool.name(), "Fetch URL Content");
    }

    #[tokio::test]
    async fn test_webfetch_tool_description() {
        let tool = WebfetchTool::new();
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn test_webfetch_params_with_format_text() {
        let json = serde_json::json!({
            "url": "https://example.com",
            "format": "text"
        });
        let params: WebfetchParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.format, "text");
    }

    #[test]
    fn test_webfetch_params_with_format_markdown() {
        let json = serde_json::json!({
            "url": "https://example.com",
            "format": "markdown"
        });
        let params: WebfetchParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.format, "markdown");
    }

    #[test]
    fn test_webfetch_params_url_validation() {
        let json = serde_json::json!({
            "url": "https://example.com/path?query=value"
        });
        let params: WebfetchParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.url, "https://example.com/path?query=value");
    }
}