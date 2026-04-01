//! Skill tool - executes skills by injecting instructions into prompts

use std::sync::Arc;
use async_trait::async_trait;
use serde_json::Value;

use rcode_core::{Tool, ToolContext, ToolResult, error::Result};

/// Tool for executing skills
pub struct SkillTool {
    registry: Arc<super::skill_registry::SkillRegistry>,
}

impl SkillTool {
    pub fn new(registry: Arc<super::skill_registry::SkillRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Tool for SkillTool {
    fn id(&self) -> &str {
        "skill"
    }

    fn name(&self) -> &str {
        "Skill"
    }

    fn description(&self) -> &str {
        "Execute a skill to get specialized instructions for a task"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "skill": {
                    "type": "string",
                    "description": "Name of the skill to execute"
                },
                "prompt": {
                    "type": "string",
                    "description": "The user's request or question for the skill"
                }
            },
            "required": ["skill", "prompt"]
        })
    }

    async fn execute(&self, args: Value, _context: &ToolContext) -> Result<ToolResult> {
        let skill_name = args["skill"]
            .as_str()
            .ok_or_else(|| rcode_core::OpenCodeError::Tool("Missing 'skill' argument".into()))?;
        
        let prompt = args["prompt"]
            .as_str()
            .ok_or_else(|| rcode_core::OpenCodeError::Tool("Missing 'prompt' argument".into()))?;

        // Load the skill
        let skill = self.registry.get(skill_name).await
            .map_err(|e| rcode_core::OpenCodeError::Tool(format!("Failed to load skill: {}", e)))?
            .ok_or_else(|| rcode_core::OpenCodeError::Tool(format!("Skill '{}' not found", skill_name)))?;

        // Build the response with skill instructions
        let trigger_str = match &skill.trigger {
            rcode_core::SkillTrigger::Keyword(k) => format!("keyword:{}", k),
            rcode_core::SkillTrigger::Command(c) => format!("/{}", c),
            rcode_core::SkillTrigger::FilePattern(p) => format!("pattern:{}", p),
        };

        let content = format!(
            "## {} Skill\n\n{}\n\n---\n\n**User Request:** {}\n\n**Instructions:**\n{}",
            skill.name,
            skill.description,
            prompt,
            skill.instructions
        );

        Ok(ToolResult {
            title: format!("Skill: {}", skill.name),
            content,
            metadata: Some(serde_json::json!({
                "skill_name": skill.name,
                "trigger": trigger_str,
            })),
            attachments: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;
    use rcode_core::ToolContext;

    fn test_context() -> ToolContext {
        ToolContext {
            session_id: "test".to_string(),
            project_path: std::path::PathBuf::from("/tmp"),
            cwd: std::path::PathBuf::from("/tmp"),
            user_id: None,
            agent: "test-agent".to_string(),
        }
    }

    #[tokio::test]
    async fn test_skill_tool_execute() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(skills_dir.join("test-skill")).unwrap();
        
        let skill_content = format!("---\nname: test-skill\ndescription: A test skill\n---\n\n# Test Skill Instructions\n\nThese are the skill instructions.");
        fs::write(
            skills_dir.join("test-skill/SKILL.md"),
            skill_content,
        ).unwrap();

        let discovery = Arc::new(
            crate::skill_discovery::SkillDiscovery::with_paths(vec![skills_dir])
        );
        let registry = Arc::new(
            crate::skill_registry::SkillRegistry::new(discovery)
        );
        let tool = SkillTool::new(registry);

        let args = serde_json::json!({
            "skill": "test-skill",
            "prompt": "How do I test Rust code?"
        });

        let result = tool.execute(args, &test_context()).await.unwrap();
        assert!(result.content.contains("Test Skill"));
        assert!(result.content.contains("How do I test Rust code?"));
    }
}
