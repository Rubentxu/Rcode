//! Question tool - Ask the user a question and get a response

use async_trait::async_trait;
use serde::Deserialize;

use opencode_core::{Tool, ToolContext, ToolResult, error::{Result, OpenCodeError}};

pub struct QuestionTool;

impl QuestionTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for QuestionTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
pub struct QuestionParams {
    pub question: String,
    #[serde(default)]
    pub options: Option<Vec<String>>,
}

#[async_trait]
impl Tool for QuestionTool {
    fn id(&self) -> &str { "question" }
    fn name(&self) -> &str { "Ask Question" }
    fn description(&self) -> &str { "Ask the user a question and get a response" }
    
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "question": {
                    "type": "string",
                    "description": "The question to ask the user"
                },
                "options": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Optional predefined options the user can choose from"
                }
            },
            "required": ["question"]
        })
    }
    
    async fn execute(&self, args: serde_json::Value, _context: &ToolContext) -> Result<ToolResult> {
        let params: QuestionParams = serde_json::from_value(args)
            .map_err(|e| OpenCodeError::Validation {
                field: "params".to_string(),
                message: format!("Invalid parameters: {}", e),
            })?;
        
        let question = params.question;
        let options = params.options.clone();
        let options_text = options
            .as_ref()
            .map(|opts| format!("\nOptions: {}", opts.join(", ")))
            .unwrap_or_default();
        
        Ok(ToolResult {
            title: "Question".to_string(),
            content: format!("QUESTION: {}{}", question, options_text),
            metadata: Some(serde_json::json!({
                "type": "question",
                "question": question,
                "options": options,
            })),
            attachments: vec![],
        })
    }
}