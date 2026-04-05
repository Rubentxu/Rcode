//! Skill discovery - finds and loads skills from configured paths

use std::path::{Path, PathBuf};
use anyhow::Result;
use tracing::{debug, warn};

use rcode_core::Skill;

/// Default search paths for skills
pub fn default_search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // Global user skills (config dir)
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".config/opencode/skills"));
    }

    // Global Claude-compatible skills
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".claude/skills"));
    }

    // Global agent-compatible skills
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".agents/skills"));
    }

    paths
}

/// Discovers skills from configured search paths
pub struct SkillDiscovery {
    search_paths: Vec<PathBuf>,
    project_path: Option<PathBuf>,
}

impl SkillDiscovery {
    /// Create a new SkillDiscovery with default search paths
    pub fn new() -> Self {
        Self {
            search_paths: default_search_paths(),
            project_path: None,
        }
    }

    /// Create with custom search paths
    pub fn with_paths(paths: Vec<PathBuf>) -> Self {
        Self {
            search_paths: paths,
            project_path: None,
        }
    }

    /// Set the project path for project-local skill discovery
    pub fn with_project_path(mut self, path: PathBuf) -> Self {
        self.project_path = Some(path);
        self
    }

    /// Get all configured search paths including project-local paths
    fn all_search_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // Add project-local paths if set
        if let Some(ref project) = self.project_path {
            // Walk up from project path looking for skills
            let mut current = project.as_path();
            loop {
                let opencode_skills = current.join(".opencode/skills");
                if opencode_skills.exists() {
                    paths.push(opencode_skills);
                }

                let claude_skills = current.join(".claude/skills");
                if claude_skills.exists() {
                    paths.push(claude_skills);
                }

                let agents_skills = current.join(".agents/skills");
                if agents_skills.exists() {
                    paths.push(agents_skills);
                }

                // Stop at git worktree root or filesystem root
                if current.parent().is_none() || current.join(".git").exists() {
                    break;
                }

                if let Some(parent) = current.parent() {
                    current = parent;
                } else {
                    break;
                }
            }
        }

        // Add global paths
        paths.extend(self.search_paths.iter().cloned());

        paths
    }

    /// Find all skills in the configured search paths
    pub async fn find_skills(&self) -> Result<Vec<Skill>> {
        let mut skills = Vec::new();

        for path in self.all_search_paths() {
            debug!("Searching for skills in: {}", path.display());
            match self.find_skills_in_dir(&path).await {
                Ok(found) => {
                    debug!("Found {} skills in {}", found.len(), path.display());
                    skills.extend(found);
                }
                Err(e) => {
                    warn!("Error searching {}: {}", path.display(), e);
                }
            }
        }

        // Deduplicate by name
        let mut seen = std::collections::HashSet::new();
        skills.retain(|s| seen.insert(s.name.clone()));

        Ok(skills)
    }

    /// Find skills in a specific directory
    async fn find_skills_in_dir(&self, dir: &Path) -> Result<Vec<Skill>> {
        let mut skills = Vec::new();

        if !dir.exists() {
            return Ok(skills);
        }

        let entries = std::fs::read_dir(dir)?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let skill_file = path.join("SKILL.md");
                if skill_file.exists() {
                    match self.load_skill_from_file(&skill_file).await {
                        Ok(skill) => {
                            debug!("Loaded skill: {}", skill.name);
                            skills.push(skill);
                        }
                        Err(e) => {
                            warn!("Failed to load skill from {}: {}", skill_file.display(), e);
                        }
                    }
                }
            }
        }

        Ok(skills)
    }

    /// Load a specific skill by name
    pub async fn load_skill(&self, name: &str) -> Result<Option<Skill>> {
        for path in self.all_search_paths() {
            let skill_file = path.join(name).join("SKILL.md");
            if skill_file.exists() {
                match self.load_skill_from_file(&skill_file).await {
                    Ok(skill) => return Ok(Some(skill)),
                    Err(e) => {
                        warn!("Failed to load skill '{}' from {}: {}", name, skill_file.display(), e);
                    }
                }
            }
        }
        Ok(None)
    }

    /// Load a skill from a specific file path
    async fn load_skill_from_file(&self, path: &Path) -> Result<Skill> {
        let content = tokio::fs::read_to_string(path).await?;
        let skill = rcode_core::Skill::parse(&content, path)?;
        Ok(skill)
    }
}

impl Default for SkillDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_find_skills_in_directory() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(skills_dir.join("test-skill")).unwrap();
        fs::write(
            skills_dir.join("test-skill/SKILL.md"),
            r#"---
name: test-skill
description: A test skill
---

# Test Skill
"#
        ).unwrap();

        let discovery = SkillDiscovery::with_paths(vec![skills_dir]);
        let skills = discovery.find_skills().await.unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "test-skill");
    }

    #[tokio::test]
    async fn test_find_skills_empty_dir() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("empty");
        fs::create_dir_all(&skills_dir).unwrap();
        let discovery = SkillDiscovery::with_paths(vec![skills_dir]);
        let skills = discovery.find_skills().await.unwrap();
        assert!(skills.is_empty());
    }

    #[tokio::test]
    async fn test_find_skills_nonexistent_dir() {
        let discovery = SkillDiscovery::with_paths(vec![PathBuf::from("/nonexistent/path")]);
        let skills = discovery.find_skills().await.unwrap();
        assert!(skills.is_empty());
    }

    #[tokio::test]
    async fn test_load_skill_by_name() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(skills_dir.join("my-skill")).unwrap();
        fs::write(
            skills_dir.join("my-skill/SKILL.md"),
            r#"---
name: my-skill
description: Test
---
# My Skill
"#
        ).unwrap();
        let discovery = SkillDiscovery::with_paths(vec![skills_dir]);
        let skill = discovery.load_skill("my-skill").await.unwrap();
        assert!(skill.is_some());
        assert_eq!(skill.unwrap().name, "my-skill");
    }

    #[tokio::test]
    async fn test_load_skill_not_found() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let discovery = SkillDiscovery::with_paths(vec![skills_dir]);
        let skill = discovery.load_skill("nonexistent").await.unwrap();
        assert!(skill.is_none());
    }

    #[tokio::test]
    async fn test_find_skills_deduplicates() {
        let temp = TempDir::new().unwrap();
        let dir1 = temp.path().join("d1");
        let dir2 = temp.path().join("d2");
        for dir in &[&dir1, &dir2] {
            fs::create_dir_all(dir.join("dup")).unwrap();
            fs::write(dir.join("dup/SKILL.md"), "---\nname: dup\ndescription: x\n---\n# Dup\n").unwrap();
        }
        let discovery = SkillDiscovery::with_paths(vec![dir1, dir2]);
        let skills = discovery.find_skills().await.unwrap();
        assert_eq!(skills.len(), 1);
    }

    #[test]
    fn test_default_search_paths() {
        let paths = default_search_paths();
        assert!(!paths.is_empty());
    }

    #[test]
    fn test_with_project_path() {
        let discovery = SkillDiscovery::new().with_project_path(PathBuf::from("/tmp"));
        let paths = discovery.all_search_paths();
        assert!(!paths.is_empty());
    }

    #[tokio::test]
    async fn test_load_skill_file_not_found() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let discovery = SkillDiscovery::with_paths(vec![skills_dir]);
        
        // Try to load a skill that doesn't exist
        let skill = discovery.load_skill("nonexistent_skill").await.unwrap();
        assert!(skill.is_none());
    }

    #[tokio::test]
    async fn test_find_skills_handles_read_dir_error() {
        // Use a path that's not readable (should just skip it)
        let discovery = SkillDiscovery::with_paths(vec![PathBuf::from("/proc/1/fd")]);
        let result = discovery.find_skills().await;
        // Should not panic, may return empty or partial results
        assert!(result.is_ok());
    }

    #[test]
    fn test_default_search_paths_includes_home() {
        let paths = default_search_paths();
        // Should have at least one path
        assert!(!paths.is_empty());
    }

    #[test]
    fn test_all_search_paths_without_project() {
        let discovery = SkillDiscovery::new();
        let paths = discovery.all_search_paths();
        // Should return global paths when no project is set
        assert!(!paths.is_empty());
    }

    #[test]
    fn test_all_search_paths_with_nonexistent_project() {
        let discovery = SkillDiscovery::new().with_project_path(PathBuf::from("/nonexistent/path/123"));
        let paths = discovery.all_search_paths();
        // Should still return global paths
        assert!(!paths.is_empty());
    }

    #[tokio::test]
    async fn test_find_skills_in_dir_with_invalid_skill_file() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(skills_dir.join("bad-skill")).unwrap();
        fs::write(skills_dir.join("bad-skill/SKILL.md"), "not valid frontmatter").unwrap();
        
        let discovery = SkillDiscovery::with_paths(vec![skills_dir]);
        let skills = discovery.find_skills().await.unwrap();
        // Should skip the invalid skill and not return it
        assert!(skills.is_empty() || !skills.iter().any(|s| s.name == "bad-skill"));
    }

    #[tokio::test]
    async fn test_find_skills_multiple_dirs() {
        let temp = TempDir::new().unwrap();
        let dir1 = temp.path().join("d1");
        let dir2 = temp.path().join("d2");
        fs::create_dir_all(dir1.join("skill1")).unwrap();
        fs::create_dir_all(dir2.join("skill2")).unwrap();
        fs::write(dir1.join("skill1/SKILL.md"), "---\nname: skill1\ndescription: s1\n---\n# S1\n").unwrap();
        fs::write(dir2.join("skill2/SKILL.md"), "---\nname: skill2\ndescription: s2\n---\n# S2\n").unwrap();
        
        let discovery = SkillDiscovery::with_paths(vec![dir1, dir2]);
        let skills = discovery.find_skills().await.unwrap();
        assert_eq!(skills.len(), 2);
    }
}
