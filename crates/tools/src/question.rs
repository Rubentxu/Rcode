//! Question tool - Ask the user a question and get a response

use async_trait::async_trait;
use serde::Deserialize;

use rcode_core::{Tool, ToolContext, ToolResult, error::{Result, RCodeError}};

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
            .map_err(|e| RCodeError::Validation {
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

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_core::ToolContext;
    use std::path::PathBuf;

    fn ctx() -> ToolContext {
        ToolContext { session_id: "s1".into(), project_path: PathBuf::from("/tmp"), cwd: PathBuf::from("/tmp"), user_id: None, agent: "test".into() }
    }

    #[tokio::test]
    async fn test_question_simple() {
        let tool = QuestionTool::new();
        let result = tool.execute(serde_json::json!({"question": "What is 2+2?"}), &ctx()).await.unwrap();
        assert_eq!(result.title, "Question");
        assert!(result.content.contains("What is 2+2?"));
        assert!(result.metadata.unwrap()["type"] == "question");
    }

    #[tokio::test]
    async fn test_question_with_options() {
        let tool = QuestionTool::new();
        let result = tool.execute(serde_json::json!({"question": "Pick one", "options": ["A", "B", "C"]}), &ctx()).await.unwrap();
        assert!(result.content.contains("Options: A, B, C"));
    }

    #[tokio::test]
    async fn test_question_invalid_params() {
        let tool = QuestionTool::new();
        let result = tool.execute(serde_json::json!({"question": 123}), &ctx()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_question_missing_question() {
        let tool = QuestionTool::new();
        let result = tool.execute(serde_json::json!({}), &ctx()).await;
        assert!(result.is_err());
    }
}