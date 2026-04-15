use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::adapters::traits::*;
use crate::models::{AgentDefinition, CompositeId, Skill, UniversalCapability};

pub struct CopilotAdapter;

impl CopilotAdapter {
    pub fn new() -> Self {
        Self
    }

    fn skills_dir(&self, project_path: &Path) -> PathBuf {
        project_path.join(".github").join("copilot").join("skills")
    }

    fn write_file_atomic(&self, path: &Path, content: &str) -> Result<(), String> {
        crate::utils::paths::atomic_write_str(path, content)
    }

    fn deploy_skills(
        &self,
        project_path: &Path,
        capabilities: &[UniversalCapability],
    ) -> Result<Vec<PathBuf>, String> {
        let skills: Vec<&Skill> = capabilities
            .iter()
            .filter_map(|c| match c {
                UniversalCapability::Skill(s) => Some(s),
                _ => None,
            })
            .collect();

        if skills.is_empty() {
            return Ok(vec![]);
        }

        let skills_dir = self.skills_dir(project_path);
        fs::create_dir_all(&skills_dir)
            .map_err(|e| format!("Failed to create skills dir: {}", e))?;

        let mut written = Vec::new();

        for skill in &skills {
            let artifact = skill.id.artifact_name(&skill.name);
            let skill_folder = skills_dir.join(&artifact);
            fs::create_dir_all(&skill_folder)
                .map_err(|e| format!("Failed to create skill folder: {}", e))?;

            let frontmatter = build_copilot_skill_frontmatter(skill);
            let body = skill
                .files
                .iter()
                .find(|f| {
                    let lower = f.path.to_lowercase();
                    lower == "skill.md" || lower.ends_with("/skill.md")
                })
                .map(|f| f.content.as_str())
                .or_else(|| skill.files.first().map(|f| f.content.as_str()))
                .unwrap_or("");

            let skill_md_content = format!("{}\n{}", frontmatter, body);
            let skill_md_path = skill_folder.join("SKILL.md");
            self.write_file_atomic(&skill_md_path, &skill_md_content)?;
            written.push(skill_md_path);

            // Deploy supporting files
            for file in &skill.files {
                let lower = file.path.to_lowercase();
                if lower == "skill.md" || lower.ends_with("/skill.md") {
                    continue;
                }
                let file_path = skill_folder.join(&file.path);
                if let Some(parent) = file_path.parent() {
                    fs::create_dir_all(parent).ok();
                }
                self.write_file_atomic(&file_path, &file.content)?;
                written.push(file_path);
            }
        }

        Ok(written)
    }
}

fn build_copilot_skill_frontmatter(skill: &Skill) -> String {
    let mut fm = String::from("---\n");
    fm.push_str(&format!("name: {}\n", skill.name));
    fm.push_str(&format!(
        "description: \"{}\"\n",
        skill.description.replace('"', "\\\"")
    ));
    if let Some(ref license) = skill.license {
        fm.push_str(&format!("license: {}\n", license));
    }
    if let Some(ref tools) = skill.allowed_tools {
        if !tools.is_empty() {
            fm.push_str(&format!("allowed-tools: {}\n", tools.join(" ")));
        }
    }
    fm.push_str("metadata:\n");
    fm.push_str(&format!("  author: {}\n", skill.author));
    fm.push_str(&format!("  version: {}\n", skill.version));
    fm.push_str("---\n");
    fm
}

impl AgentAdapter for CopilotAdapter {
    fn id(&self) -> &str {
        "copilot"
    }

    fn name(&self) -> &str {
        "GitHub Copilot"
    }

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            mcp: false,
            rules: false,
            skills: true,
            hooks: false,
            plugins: false,
            agents: false,
            custom: false,
        }
    }

    fn detect(&self, project_path: &Path) -> bool {
        project_path.join(".github").join("copilot").exists()
    }

    fn read_config(&self, project_path: &Path) -> Result<AgentConfig, String> {
        let mut config = AgentConfig::default();
        let skills_dir = self.skills_dir(project_path);
        if skills_dir.exists() {
            if let Ok(entries) = fs::read_dir(&skills_dir) {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        if let Some(name) = entry.file_name().to_str() {
                            config.skills.push(name.to_string());
                        }
                    }
                }
            }
        }
        Ok(config)
    }

    fn diff(
        &self,
        project_path: &Path,
        capabilities: &[UniversalCapability],
        _agents: &[AgentDefinition],
        _options: Option<&serde_json::Value>,
    ) -> Result<Vec<ConfigDiffEntry>, String> {
        let mut entries = Vec::new();

        // Skills diff
        let skills: Vec<&Skill> = capabilities
            .iter()
            .filter_map(|c| match c {
                UniversalCapability::Skill(s) => Some(s),
                _ => None,
            })
            .collect();

        let skills_dir = self.skills_dir(project_path);
        for skill in &skills {
            let artifact = skill.id.artifact_name(&skill.name);
            let skill_folder = skills_dir.join(&artifact);
            let skill_md_path = skill_folder.join("SKILL.md");

            let frontmatter = build_copilot_skill_frontmatter(skill);
            let body = skill
                .files
                .iter()
                .find(|f| f.path.to_lowercase() == "skill.md")
                .map(|f| f.content.as_str())
                .or_else(|| skill.files.first().map(|f| f.content.as_str()))
                .unwrap_or("");
            let proposed = format!("{}\n{}", frontmatter, body);

            let current = fs::read_to_string(&skill_md_path).ok();
            let change_type = if current.is_some() {
                ChangeType::Modify
            } else {
                ChangeType::Add
            };

            entries.push(ConfigDiffEntry {
                file_path: skill_md_path,
                change_type,
                current_content: current,
                proposed_content: proposed,
                merged_content: None,
            });
        }

        Ok(entries)
    }

    fn deploy(
        &self,
        project_path: &Path,
        capabilities: &[UniversalCapability],
        _agents: &[AgentDefinition],
        _strategy: DeployStrategy,
        _options: Option<&serde_json::Value>,
    ) -> Result<DeployResult, String> {
        let mut all_files = Vec::new();
        let mut all_errors = Vec::new();

        match self.deploy_skills(project_path, capabilities) {
            Ok(files) => all_files.extend(files),
            Err(e) => all_errors.push(e),
        }

        if all_errors.is_empty() {
            Ok(DeployResult::success(all_files))
        } else {
            Ok(DeployResult {
                success: all_files.len() > 0,
                files_written: all_files,
                errors: all_errors,
            })
        }
    }

    fn remove(
        &self,
        project_path: &Path,
        capability_ids: &[CompositeId],
        _agent_ids: &[CompositeId],
    ) -> Result<RemoveResult, String> {
        let mut removed = Vec::new();
        let skills_dir = self.skills_dir(project_path);

        for id in capability_ids {
            let artifact = id.name.replace(' ', "-").to_lowercase();
            let skill_folder = skills_dir.join(&artifact);
            if skill_folder.exists() {
                fs::remove_dir_all(&skill_folder).ok();
                removed.push(skill_folder);
            }
        }

        Ok(RemoveResult::success(removed))
    }

    fn managed_paths(&self, project_path: &Path) -> Vec<PathBuf> {
        vec![self.skills_dir(project_path)]
    }
}
