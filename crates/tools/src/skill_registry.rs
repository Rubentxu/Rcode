//! Skill registry - manages loaded skills

use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use anyhow::Result;
use tracing::debug;

use rcode_core::Skill;

/// Registry for managing skills with lazy loading
pub struct SkillRegistry {
    skills: RwLock<HashMap<String, Skill>>,
    discovery: Arc<SkillDiscovery>,
}

impl SkillRegistry {
    /// Create a new SkillRegistry
    pub fn new(discovery: Arc<SkillDiscovery>) -> Self {
        Self {
            skills: RwLock::new(HashMap::new()),
            discovery,
        }
    }

    /// Get a skill by name, loading it if necessary
    pub async fn get(&self, name: &str) -> Result<Option<Skill>> {
        // Check if already loaded
        {
            let skills = self.skills.read();
            if let Some(skill) = skills.get(name) {
                return Ok(Some(skill.clone()));
            }
        }

        // Try to load from discovery
        if let Some(skill) = self.discovery.load_skill(name).await? {
            debug!("Loaded skill '{}' into registry", name);
            let mut skills = self.skills.write();
            skills.insert(name.to_string(), skill.clone());
            return Ok(Some(skill));
        }

        Ok(None)
    }

    /// Get a skill synchronously if already loaded
    pub fn get_loaded(&self, name: &str) -> Option<Skill> {
        let skills = self.skills.read();
        skills.get(name).cloned()
    }

    /// Check if a skill is loaded
    pub fn is_loaded(&self, name: &str) -> bool {
        let skills = self.skills.read();
        skills.contains_key(name)
    }

    /// List all loaded skill names
    pub fn list_loaded(&self) -> Vec<String> {
        let skills = self.skills.read();
        skills.keys().cloned().collect()
    }

    /// Register a skill directly (pre-loaded)
    pub fn register(&self, skill: Skill) {
        let mut skills = self.skills.write();
        skills.insert(skill.name.clone(), skill);
    }

    /// Get all skills (loads from discovery if not already loaded)
    pub async fn get_all(&self) -> Result<Vec<Skill>> {
        let skills = self.discovery.find_skills().await?;
        
        // Cache all discovered skills
        let mut cache = self.skills.write();
        for skill in &skills {
            cache.insert(skill.name.clone(), skill.clone());
        }
        
        Ok(skills)
    }

    /// Find skills matching a trigger
    pub async fn find_by_trigger(&self, trigger: &str) -> Result<Vec<Skill>> {
        let all_skills = self.get_all().await?;
        Ok(all_skills
            .into_iter()
            .filter(|s| match &s.trigger {
                rcode_core::SkillTrigger::Keyword(kw) => 
                    kw.to_lowercase().contains(&trigger.to_lowercase()),
                rcode_core::SkillTrigger::Command(cmd) => 
                    cmd.to_lowercase() == trigger.to_lowercase().trim_start_matches('/'),
                rcode_core::SkillTrigger::FilePattern(pattern) => 
                    glob::Pattern::new(pattern)
                        .map(|p| p.matches(trigger))
                        .unwrap_or(false),
            })
            .collect())
    }
}

// Import SkillDiscovery for use in the struct
use super::skill_discovery::SkillDiscovery;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    #[tokio::test]
    async fn test_registry_lazy_loading() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(skills_dir.join("lazy-skill")).unwrap();
        fs::write(
            skills_dir.join("lazy-skill/SKILL.md"),
            r#"---
name: lazy-skill
description: A lazily loaded skill
---

# Lazy Skill
"#
        ).unwrap();

        let discovery = Arc::new(
            SkillDiscovery::with_paths(vec![skills_dir])
        );
        let registry = SkillRegistry::new(discovery);

        assert!(!registry.is_loaded("lazy-skill"));

        let skill = registry.get("lazy-skill").await.unwrap();
        assert!(skill.is_some());
        assert_eq!(skill.unwrap().name, "lazy-skill");

        assert!(registry.is_loaded("lazy-skill"));
    }

    #[test]
    fn test_register_and_get_loaded() {
        let temp = TempDir::new().unwrap();
        let discovery = Arc::new(SkillDiscovery::with_paths(vec![temp.path().to_path_buf()]));
        let registry = SkillRegistry::new(discovery);

        let skill = Skill {
            name: "manual-skill".to_string(),
            description: "Manually registered".to_string(),
            instructions: "Do things".to_string(),
            trigger: rcode_core::SkillTrigger::Keyword("manual".to_string()),
            compatibility: rcode_core::SkillCompatibility::Opencode,
            metadata: None,
        };
        registry.register(skill.clone());
        assert!(registry.is_loaded("manual-skill"));
        let loaded = registry.get_loaded("manual-skill").unwrap();
        assert_eq!(loaded.name, "manual-skill");
    }

    #[test]
    fn test_list_loaded() {
        let temp = TempDir::new().unwrap();
        let discovery = Arc::new(SkillDiscovery::with_paths(vec![temp.path().to_path_buf()]));
        let registry = SkillRegistry::new(discovery);

        registry.register(Skill {
            name: "a".to_string(), description: "A".to_string(), instructions: "".to_string(),
            trigger: rcode_core::SkillTrigger::Keyword("a".to_string()),
            compatibility: rcode_core::SkillCompatibility::Opencode,
            metadata: None,
        });
        registry.register(Skill {
            name: "b".to_string(), description: "B".to_string(), instructions: "".to_string(),
            trigger: rcode_core::SkillTrigger::Keyword("b".to_string()),
            compatibility: rcode_core::SkillCompatibility::Opencode,
            metadata: None,
        });

        let loaded = registry.list_loaded();
        assert_eq!(loaded.len(), 2);
        assert!(loaded.contains(&"a".to_string()));
        assert!(loaded.contains(&"b".to_string()));
    }

    #[tokio::test]
    async fn test_get_nonexistent() {
        let temp = TempDir::new().unwrap();
        let discovery = Arc::new(SkillDiscovery::with_paths(vec![temp.path().to_path_buf()]));
        let registry = SkillRegistry::new(discovery);
        let result = registry.get("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_loaded_nonexistent() {
        let temp = TempDir::new().unwrap();
        let discovery = Arc::new(SkillDiscovery::with_paths(vec![temp.path().to_path_buf()]));
        let registry = SkillRegistry::new(discovery);
        assert!(registry.get_loaded("nope").is_none());
    }

    #[tokio::test]
    async fn test_get_all() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(skills_dir.join("skill-a")).unwrap();
        fs::create_dir_all(skills_dir.join("skill-b")).unwrap();
        fs::write(
            skills_dir.join("skill-a/SKILL.md"),
            r#"---
name: skill-a
description: Skill A
---
# Skill A
"#
        ).unwrap();
        fs::write(
            skills_dir.join("skill-b/SKILL.md"),
            r#"---
name: skill-b
description: Skill B
---
# Skill B
"#
        ).unwrap();

        let discovery = Arc::new(SkillDiscovery::with_paths(vec![skills_dir]));
        let registry = SkillRegistry::new(discovery);
        
        let all_skills = registry.get_all().await.unwrap();
        assert!(all_skills.len() >= 2);
        let names: Vec<_> = all_skills.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"skill-a") || names.contains(&"skill-b"));
    }

    #[tokio::test]
    async fn test_find_by_trigger_no_match() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        // Create the skill directory first
        fs::create_dir_all(skills_dir.join("skill")).unwrap();
        fs::write(
            skills_dir.join("skill/SKILL.md"),
            r#"---
name: skill
description: A skill
trigger:
  type: keyword
  value: specific
---
# Skill
"#
        ).unwrap();

        let discovery = Arc::new(SkillDiscovery::with_paths(vec![skills_dir]));
        let registry = SkillRegistry::new(discovery);
        
        let found = registry.find_by_trigger("nothing").await.unwrap();
        assert!(found.is_empty());
    }
}
