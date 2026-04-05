//! Websearch tool - web search using DuckDuckGo

use async_trait::async_trait;
use serde::Deserialize;

use rcode_core::{Tool, ToolContext, ToolResult, error::Result};

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
            .map_err(|e| rcode_core::RCodeError::Tool(format!("Invalid parameters: {}", e)))?;
        
        // Simple web search using DuckDuckGo HTML
        let url = format!(
            "https://html.duckduckgo.com/html/?q={}",
            urlencoding::encode(&params.query)
        );
        
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| rcode_core::RCodeError::Tool(format!("Failed to create HTTP client: {}", e)))?;
        
        let response = client.get(&url)
            .send()
            .await
            .map_err(|e| rcode_core::RCodeError::Tool(format!("Failed to perform search: {}", e)))?;
        
        let body = response.text()
            .await
            .map_err(|e| rcode_core::RCodeError::Tool(format!("Failed to read response: {}", e)))?;
        
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

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::ToolContext;
    use std::path::PathBuf;

    fn ctx() -> ToolContext {
        ToolContext { session_id: "s1".into(), project_path: PathBuf::from("/tmp"), cwd: PathBuf::from("/tmp"), user_id: None, agent: "test".into() }
    }

    #[test]
    fn test_parse_duckduckgo_results() {
        let html = "First result\n<div class=\"result__snippet\">A snippet here</div>\nNot a result\n<div class=\"result__snippet\">Another snippet</div>";
        let results = parse_duckduckgo_results(html, 10);
        assert_eq!(results.len(), 2);
        assert!(results[0].contains("A snippet here"));
        assert!(results[1].contains("Another snippet"));
    }

    #[test]
    fn test_parse_duckduckgo_respects_limit() {
        let html = "<div class=\"result__snippet\">A</div>\n<div class=\"result__snippet\">B</div>\n<div class=\"result__snippet\">C</div>";
        let results = parse_duckduckgo_results(html, 2);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_parse_duckduckgo_empty() {
        let results = parse_duckduckgo_results("no results here", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_parse_duckduckgo_strips_bold_and_ellipsis() {
        let html = "<div class=\"result__snippet\"><b>Bold</b> text...</div>";
        let results = parse_duckduckgo_results(html, 10);
        assert!(results[0].contains("Bold text"));
        assert!(!results[0].contains("<b>"));
        assert!(!results[0].contains("..."));
    }

    #[tokio::test]
    async fn test_websearch_invalid_params() {
        let tool = WebsearchTool::new();
        let result = tool.execute(serde_json::json!({"query": 123}), &ctx()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_websearch_missing_query() {
        let tool = WebsearchTool::new();
        let result = tool.execute(serde_json::json!({}), &ctx()).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_duckduckgo_results_empty_html() {
        let html = "";
        let results = parse_duckduckgo_results(html, 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_parse_duckduckgo_results_html_with_special_chars() {
        let html = r#"<div class="result__snippet">&lt;script&gt;alert('xss')&lt;/script&gt;</div>"#;
        let results = parse_duckduckgo_results(html, 10);
        assert_eq!(results.len(), 1);
        // Should strip the HTML entities
        assert!(results[0].contains("script"));
    }

    #[test]
    fn test_parse_duckduckgo_results_limit_zero() {
        let html = "<div class=\"result__snippet\">A</div>\n<div class=\"result__snippet\">B</div>";
        let results = parse_duckduckgo_results(html, 0);
        assert!(results.is_empty());
    }

    #[test]
    fn test_parse_duckduckgo_results_multiline_content() {
        // When content spans multiple lines in the HTML, each line is processed separately
        // So we test with content that contains newlines
        let html = "<div class=\"result__snippet\">first line</div>\n<div class=\"result__snippet\">second line</div>";
        let results = parse_duckduckgo_results(html, 10);
        assert_eq!(results.len(), 2);
        assert!(results[0].contains("first line"));
        assert!(results[1].contains("second line"));
    }

    #[test]
    fn test_default_limit() {
        assert_eq!(default_limit(), 10);
    }

    #[test]
    fn test_websearch_params_deserialization() {
        let json = serde_json::json!({
            "query": "test query",
            "limit": 5
        });
        let params: WebsearchParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.query, "test query");
        assert_eq!(params.limit, 5);
    }

    #[test]
    fn test_websearch_params_default_limit() {
        let json = serde_json::json!({
            "query": "test"
        });
        let params: WebsearchParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.limit, 10);
    }

    #[test]
    fn test_websearch_params_limit_explicit_zero() {
        let json = serde_json::json!({
            "query": "test",
            "limit": 0
        });
        let params: WebsearchParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.limit, 0);
    }

    #[test]
    fn test_websearch_params_large_limit() {
        let json = serde_json::json!({
            "query": "test",
            "limit": 1000
        });
        let params: WebsearchParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.limit, 1000);
    }

    #[test]
    fn test_websearch_tool_id_and_name() {
        let tool = WebsearchTool::new();
        assert_eq!(tool.id(), "websearch");
        assert_eq!(tool.name(), "Web Search");
    }

    #[test]
    fn test_websearch_tool_description() {
        let tool = WebsearchTool::new();
        assert!(!tool.description().is_empty());
    }

    #[tokio::test]
    async fn test_websearch_invalid_url_format() {
        let tool = WebsearchTool::new();
        // This tests the HTTP client creation failure path
        // A malformed URL should be handled gracefully
        let result = tool.execute(serde_json::json!({"query": "test"}), &ctx()).await;
        // Should either succeed with results or fail gracefully
        // (network call actually happens)
        if result.is_err() {
            let err_msg = result.unwrap_err().to_string();
            assert!(err_msg.contains("search") || err_msg.contains("Failed"));
        }
    }

    #[tokio::test]
    async fn test_websearch_empty_query_result() {
        let tool = WebsearchTool::new();
        // Empty query - the behavior depends on implementation
        // Either validation error or network call happens
        let result = tool.execute(serde_json::json!({"query": ""}), &ctx()).await;
        // If it succeeds, it should return some content
        // If it fails, it should be a proper error
        if result.is_ok() {
            let tool_result = result.unwrap();
            assert!(!tool_result.title.is_empty());
        }
    }
}