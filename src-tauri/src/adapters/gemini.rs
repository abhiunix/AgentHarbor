use crate::adapters::traits::{
    AdapterCapabilities, AgentAdapter, AgentConfig, ChangeType, ConfigDiffEntry,
    DeployResult, DeployStrategy, RemoveResult,
};
use crate::models::{AgentDefinition, CompositeId, UniversalCapability};
use crate::utils::markdown::generate_agent_md;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct GeminiAdapter;

impl GeminiAdapter {
    pub fn new() -> Self {
        Self
    }

    fn global_settings_path() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(".gemini").join("settings.json")
    }

    fn gemini_md_path(project_path: &Path) -> PathBuf {
        project_path.join("GEMINI.md")
    }

    fn skills_dir(project_path: &Path) -> PathBuf {
        project_path.join(".gemini").join("skills")
    }

    fn agents_dir(project_path: &Path) -> PathBuf {
        project_path.join(".gemini").join("agents")
    }

    fn read_settings() -> Result<Value, String> {
        let path = Self::global_settings_path();
        if !path.exists() {
            return Ok(json!({}));
        }
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read settings.json: {}", e))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse settings.json: {}", e))
    }

    fn write_file_atomic(path: &Path, content: &str) -> Result<(), String> {
        crate::utils::paths::atomic_write_str(path, content)
    }

    fn deploy_mcp_servers(
        capabilities: &[UniversalCapability],
    ) -> Result<Vec<PathBuf>, String> {
        let mcp_servers: Vec<_> = capabilities
            .iter()
            .filter_map(|c| {
                if let UniversalCapability::Mcp(mcp) = c {
                    Some(mcp)
                } else {
                    None
                }
            })
            .collect();

        if mcp_servers.is_empty() {
            return Ok(vec![]);
        }

        let settings_path = Self::global_settings_path();
        let mut settings = Self::read_settings()?;

        let servers = settings
            .as_object_mut()
            .ok_or("Settings must be an object")?
            .entry("mcpServers")
            .or_insert(json!({}));

        for mcp in mcp_servers {
            let transport = if mcp.transport.is_empty() {
                "stdio"
            } else {
                &mcp.transport
            };

            let mut server_config = if transport == "stdio" {
                json!({
                    "command": mcp.command,
                    "args": mcp.args,
                })
            } else {
                json!({
                    "url": mcp.url,
                })
            };

            if !mcp.env.is_empty() {
                let env_map: HashMap<String, String> = mcp
                    .env
                    .iter()
                    .map(|(k, _)| (k.clone(), format!("${{{}}}", k)))
                    .collect();
                server_config["env"] = json!(env_map);
            }

            let key = mcp.id.artifact_name(&mcp.name);
            servers[&key] = server_config;
        }

        let content = serde_json::to_string_pretty(&settings)
            .map_err(|e| format!("Failed to serialize settings: {}", e))?;
        Self::write_file_atomic(&settings_path, &content)?;

        Ok(vec![settings_path])
    }

    fn deploy_rules(
        project_path: &Path,
        capabilities: &[UniversalCapability],
    ) -> Result<Vec<PathBuf>, String> {
        let rules: Vec<_> = capabilities
            .iter()
            .filter_map(|c| {
                if let UniversalCapability::Rule(rule) = c {
                    Some(rule)
                } else {
                    None
                }
            })
            .collect();

        if rules.is_empty() {
            return Ok(vec![]);
        }

        // Append rules to GEMINI.md
        let gemini_md = Self::gemini_md_path(project_path);
        let mut content = if gemini_md.exists() {
            fs::read_to_string(&gemini_md).unwrap_or_default()
        } else {
            String::new()
        };

        for rule in rules {
            if !content.contains(&rule.content) {
                if !content.is_empty() && !content.ends_with('\n') {
                    content.push('\n');
                }
                content.push_str(&format!("\n## {}\n\n{}\n", rule.name, rule.content));
            }
        }

        Self::write_file_atomic(&gemini_md, &content)?;
        Ok(vec![gemini_md])
    }

    fn deploy_agents(
        project_path: &Path,
        agents: &[AgentDefinition],
    ) -> Result<Vec<PathBuf>, String> {
        if agents.is_empty() {
            return Ok(vec![]);
        }

        let agents_dir = Self::agents_dir(project_path);
        fs::create_dir_all(&agents_dir)
            .map_err(|e| format!("Failed to create agents directory: {}", e))?;

        let mut written_files = vec![];

        for agent in agents {
            let filename = agent.id.artifact_name(&agent.name);
            let md_content = generate_agent_md(agent);
            let file_path = agents_dir.join(format!("{}.md", filename));
            Self::write_file_atomic(&file_path, &md_content)?;
            written_files.push(file_path);
        }

        Ok(written_files)
    }

    fn deploy_hooks(
        capabilities: &[UniversalCapability],
    ) -> Result<Vec<PathBuf>, String> {
        let hooks: Vec<_> = capabilities
            .iter()
            .filter_map(|c| {
                if let UniversalCapability::Hook(hook) = c {
                    Some(hook)
                } else {
                    None
                }
            })
            .collect();

        if hooks.is_empty() {
            return Ok(vec![]);
        }

        let settings_path = Self::global_settings_path();
        let mut settings = Self::read_settings()?;

        let hooks_obj = settings
            .as_object_mut()
            .ok_or("Settings must be an object")?
            .entry("hooks")
            .or_insert(json!({}));

        for hook in hooks {
            // Map hooks to Gemini's beforeTool/afterTool format
            let event = match hook.event.as_str() {
                "pre-tool" | "before" => "beforeTool",
                "post-tool" | "after" => "afterTool",
                other => other,
            };
            let hook_entry = json!({
                "command": hook.command,
                "matcher": hook.matcher,
            });
            let arr = hooks_obj
                .as_object_mut()
                .ok_or("hooks must be an object")?
                .entry(event)
                .or_insert(json!([]));
            if let Some(entries) = arr.as_array_mut() {
                entries.push(hook_entry);
            }
        }

        let content = serde_json::to_string_pretty(&settings)
            .map_err(|e| format!("Failed to serialize settings: {}", e))?;
        Self::write_file_atomic(&settings_path, &content)?;

        Ok(vec![settings_path])
    }

    fn deploy_skills(
        project_path: &Path,
        capabilities: &[UniversalCapability],
    ) -> Result<Vec<PathBuf>, String> {
        let skills: Vec<_> = capabilities
            .iter()
            .filter_map(|c| {
                if let UniversalCapability::Skill(skill) = c {
                    Some(skill)
                } else {
                    None
                }
            })
            .collect();

        if skills.is_empty() {
            return Ok(vec![]);
        }

        let skills_dir = Self::skills_dir(project_path);
        let mut written_files = vec![];

        for skill in skills {
            let name = skill.id.artifact_name(&skill.name);
            let skill_dir = skills_dir.join(&name);
            fs::create_dir_all(&skill_dir)
                .map_err(|e| format!("Failed to create skill directory: {}", e))?;

            // Write each skill file; use the first file as SKILL.md if path matches
            for file in &skill.files {
                let file_name = if file.path.ends_with("SKILL.md") || skill.files.len() == 1 {
                    "SKILL.md".to_string()
                } else {
                    Path::new(&file.path)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "SKILL.md".to_string())
                };
                let target = skill_dir.join(&file_name);
                Self::write_file_atomic(&target, &file.content)?;
                written_files.push(target);
            }
        }

        Ok(written_files)
    }
}

impl Default for GeminiAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentAdapter for GeminiAdapter {
    fn id(&self) -> &str {
        "gemini"
    }

    fn name(&self) -> &str {
        "Gemini CLI"
    }

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            mcp: true,
            rules: true,
            skills: true,
            hooks: true,
            plugins: false,
            agents: true,
            custom: false,
        }
    }

    fn detect(&self, project_path: &Path) -> bool {
        let gemini_dir = project_path.join(".gemini");
        let gemini_md = project_path.join("GEMINI.md");
        gemini_dir.exists() || gemini_md.exists()
    }

    fn read_config(&self, project_path: &Path) -> Result<AgentConfig, String> {
        let mut config = AgentConfig::default();

        // Read MCP servers from settings
        let settings = Self::read_settings()?;
        if let Some(mcp_servers) = settings.get("mcpServers").and_then(|v| v.as_object()) {
            config.mcp_servers = mcp_servers.keys().cloned().collect();
        }

        // Read agents
        let agents_dir = Self::agents_dir(project_path);
        if agents_dir.exists() {
            if let Ok(entries) = fs::read_dir(&agents_dir) {
                for entry in entries.flatten() {
                    if entry.path().extension().map(|e| e == "md").unwrap_or(false) {
                        if let Some(name) = entry.path().file_stem() {
                            config.agents.push(name.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }

        // Read skills
        let skills_dir = Self::skills_dir(project_path);
        if skills_dir.exists() {
            if let Ok(entries) = fs::read_dir(&skills_dir) {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        config.skills.push(
                            entry.file_name().to_string_lossy().to_string(),
                        );
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
        agents: &[AgentDefinition],
        _options: Option<&Value>,
    ) -> Result<Vec<ConfigDiffEntry>, String> {
        let mut diffs = vec![];

        // MCP diff
        let mcp_servers: Vec<_> = capabilities
            .iter()
            .filter_map(|c| {
                if let UniversalCapability::Mcp(mcp) = c {
                    Some(mcp)
                } else {
                    None
                }
            })
            .collect();

        if !mcp_servers.is_empty() {
            let settings_path = Self::global_settings_path();
            let mut settings = Self::read_settings()?;
            let servers = settings
                .as_object_mut()
                .ok_or("Settings must be an object")?
                .entry("mcpServers")
                .or_insert(json!({}));

            for mcp in &mcp_servers {
                let transport = if mcp.transport.is_empty() { "stdio" } else { &mcp.transport };
                let server_config = if transport == "stdio" {
                    json!({ "command": mcp.command, "args": mcp.args })
                } else {
                    json!({ "url": mcp.url })
                };
                let key = mcp.id.artifact_name(&mcp.name);
                servers[&key] = server_config;
            }

            let current = if settings_path.exists() {
                Some(fs::read_to_string(&settings_path).unwrap_or_default())
            } else {
                None
            };
            let proposed = serde_json::to_string_pretty(&settings).unwrap_or_default();

            diffs.push(ConfigDiffEntry {
                file_path: settings_path,
                change_type: if current.is_some() { ChangeType::Modify } else { ChangeType::Add },
                current_content: current,
                proposed_content: proposed,
                merged_content: None,
            });
        }

        // Rules diff (appended to GEMINI.md)
        let rules: Vec<_> = capabilities
            .iter()
            .filter_map(|c| {
                if let UniversalCapability::Rule(rule) = c { Some(rule) } else { None }
            })
            .collect();

        if !rules.is_empty() {
            let gemini_md = Self::gemini_md_path(project_path);
            let current = if gemini_md.exists() {
                Some(fs::read_to_string(&gemini_md).unwrap_or_default())
            } else {
                None
            };
            let mut proposed = current.clone().unwrap_or_default();
            for rule in &rules {
                if !proposed.contains(&rule.content) {
                    if !proposed.is_empty() && !proposed.ends_with('\n') {
                        proposed.push('\n');
                    }
                    proposed.push_str(&format!("\n## {}\n\n{}\n", rule.name, rule.content));
                }
            }
            diffs.push(ConfigDiffEntry {
                file_path: gemini_md,
                change_type: if current.is_some() { ChangeType::Modify } else { ChangeType::Add },
                current_content: current,
                proposed_content: proposed,
                merged_content: None,
            });
        }

        // Skills diff
        let skills: Vec<_> = capabilities
            .iter()
            .filter_map(|c| {
                if let UniversalCapability::Skill(skill) = c { Some(skill) } else { None }
            })
            .collect();

        if !skills.is_empty() {
            let skills_dir = Self::skills_dir(project_path);
            for skill in &skills {
                let name = skill.id.artifact_name(&skill.name);
                let skill_dir = skills_dir.join(&name);
                let skill_md_path = skill_dir.join("SKILL.md");

                let body = skill.files.iter()
                    .find(|f| {
                        let lower = f.path.to_lowercase();
                        lower == "skill.md" || lower.ends_with("/skill.md")
                    })
                    .map(|f| f.content.as_str())
                    .or_else(|| skill.files.first().map(|f| f.content.as_str()))
                    .unwrap_or("");

                let proposed = body.to_string();
                let current = if skill_md_path.exists() {
                    Some(fs::read_to_string(&skill_md_path).unwrap_or_default())
                } else {
                    None
                };

                diffs.push(ConfigDiffEntry {
                    file_path: skill_md_path,
                    change_type: if current.is_some() { ChangeType::Modify } else { ChangeType::Add },
                    current_content: current,
                    proposed_content: proposed,
                    merged_content: None,
                });
            }
        }

        // Agent diffs
        for agent in agents {
            let filename = agent.id.artifact_name(&agent.name);
            let path = Self::agents_dir(project_path).join(format!("{}.md", filename));
            let current = if path.exists() {
                Some(fs::read_to_string(&path).unwrap_or_default())
            } else {
                None
            };
            let proposed = generate_agent_md(agent);
            diffs.push(ConfigDiffEntry {
                file_path: path,
                change_type: if current.is_some() { ChangeType::Modify } else { ChangeType::Add },
                current_content: current,
                proposed_content: proposed,
                merged_content: None,
            });
        }

        Ok(diffs)
    }

    fn deploy(
        &self,
        project_path: &Path,
        capabilities: &[UniversalCapability],
        agents: &[AgentDefinition],
        _strategy: DeployStrategy,
        _options: Option<&Value>,
    ) -> Result<DeployResult, String> {
        let mut all_files = vec![];

        let mcp_files = Self::deploy_mcp_servers(capabilities)?;
        all_files.extend(mcp_files);

        let rule_files = Self::deploy_rules(project_path, capabilities)?;
        all_files.extend(rule_files);

        let agent_files = Self::deploy_agents(project_path, agents)?;
        all_files.extend(agent_files);

        let hook_files = Self::deploy_hooks(capabilities)?;
        all_files.extend(hook_files);

        let skill_files = Self::deploy_skills(project_path, capabilities)?;
        all_files.extend(skill_files);

        // Write capability manifest to GEMINI.md and AGENTS.md
        if let Ok(items) = crate::utils::project_inventory::collect_installed_items(project_path) {
            let manifest = crate::utils::manifest::build_capability_manifest(&items);
            if !manifest.is_empty() {
                // Update GEMINI.md
                let gemini_md_path = project_path.join("GEMINI.md");
                let existing = std::fs::read_to_string(&gemini_md_path).unwrap_or_default();
                let updated = crate::utils::manifest::replace_manifest_section(&existing, &manifest);
                if let Ok(()) = crate::utils::paths::atomic_write_str(&gemini_md_path, &updated) {
                    all_files.push(gemini_md_path);
                }
                // Update AGENTS.md
                if let Ok(agents_path) = crate::utils::manifest::write_agents_md(project_path, &items) {
                    all_files.push(agents_path);
                }
            }
        }

        all_files.sort();
        all_files.dedup();

        Ok(DeployResult::success(all_files))
    }

    fn remove(
        &self,
        project_path: &Path,
        capability_ids: &[CompositeId],
        agent_ids: &[CompositeId],
    ) -> Result<RemoveResult, String> {
        let mut removed_files = vec![];

        // Remove MCP servers from settings
        if !capability_ids.is_empty() {
            let settings_path = Self::global_settings_path();
            if settings_path.exists() {
                let mut settings = Self::read_settings()?;
                if let Some(servers) = settings
                    .as_object_mut()
                    .and_then(|o| o.get_mut("mcpServers"))
                    .and_then(|s| s.as_object_mut())
                {
                    for id in capability_ids {
                        let key = id.artifact_name(&id.name);
                        servers.remove(&key);
                    }
                }
                let content = serde_json::to_string_pretty(&settings)
                    .map_err(|e| format!("Failed to serialize settings: {}", e))?;
                Self::write_file_atomic(&settings_path, &content)?;
                removed_files.push(settings_path);
            }
        }

        // Remove agent files
        for id in agent_ids {
            let filename = id.artifact_name(&id.name);
            let path = Self::agents_dir(project_path).join(format!("{}.md", filename));
            if path.exists() {
                fs::remove_file(&path)
                    .map_err(|e| format!("Failed to remove agent file: {}", e))?;
                removed_files.push(path);
            }
        }

        let _ = crate::utils::manifest::rebuild_all_manifests(project_path);

        Ok(RemoveResult::success(removed_files))
    }

    fn managed_paths(&self, project_path: &Path) -> Vec<PathBuf> {
        vec![
            Self::global_settings_path(),
            Self::gemini_md_path(project_path),
            Self::agents_dir(project_path),
            Self::skills_dir(project_path),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{McpServer, Rule, Visibility};
    use tempfile::TempDir;

    fn create_test_mcp() -> UniversalCapability {
        UniversalCapability::Mcp(McpServer {
            id: CompositeId::new("community", "test-mcp").unwrap(),
            name: "Test MCP".to_string(),
            description: "Test MCP server".to_string(),
            version: "1.0.0".to_string(),
            author: "test".to_string(),
            visibility: Visibility::Public,
            tags: vec![],
            compatible_agents: vec!["gemini".to_string()],
            disabled: None,
            always_allow: None,
            disabled_tools: None,
            tool_list: None,
            transport: "stdio".to_string(),
            command: "npx".to_string(),
            args: vec!["-y".to_string(), "@test/mcp".to_string()],
            url: String::new(),
            env: std::collections::HashMap::new(),
        })
    }

    fn create_test_rule() -> UniversalCapability {
        UniversalCapability::Rule(Rule {
            id: CompositeId::new("community", "test-rule").unwrap(),
            name: "Test Rule".to_string(),
            description: "Test rule".to_string(),
            version: "1.0.0".to_string(),
            author: "test".to_string(),
            visibility: Visibility::Public,
            tags: vec![],
            compatible_agents: vec!["gemini".to_string()],
            scope: "project".to_string(),
            content: "Always be helpful.".to_string(),
            env: std::collections::HashMap::new(),
        })
    }

    #[test]
    fn test_adapter_id_and_name() {
        let adapter = GeminiAdapter::new();
        assert_eq!(adapter.id(), "gemini");
        assert_eq!(adapter.name(), "Gemini CLI");
    }

    #[test]
    fn test_adapter_capabilities() {
        let adapter = GeminiAdapter::new();
        let caps = adapter.capabilities();
        assert!(caps.mcp);
        assert!(caps.rules);
        assert!(caps.skills);
        assert!(caps.hooks);
        assert!(!caps.plugins);
        assert!(caps.agents);
    }

    #[test]
    fn test_detect_with_gemini_dir() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = GeminiAdapter::new();

        assert!(!adapter.detect(temp_dir.path()));

        fs::create_dir(temp_dir.path().join(".gemini")).unwrap();
        assert!(adapter.detect(temp_dir.path()));
    }

    #[test]
    fn test_detect_with_gemini_md() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = GeminiAdapter::new();

        assert!(!adapter.detect(temp_dir.path()));

        fs::write(temp_dir.path().join("GEMINI.md"), "# Instructions").unwrap();
        assert!(adapter.detect(temp_dir.path()));
    }

    #[test]
    fn test_read_config_empty() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = GeminiAdapter::new();
        let config = adapter.read_config(temp_dir.path()).unwrap();
        assert!(config.mcp_servers.is_empty());
        assert!(config.agents.is_empty());
        assert!(config.skills.is_empty());
    }

    #[test]
    fn test_deploy_rules_creates_gemini_md() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = GeminiAdapter::new();
        let rule = create_test_rule();

        let result = adapter
            .deploy(temp_dir.path(), &[rule], &[], DeployStrategy::Merge, None)
            .unwrap();

        assert!(result.success);

        let gemini_md = temp_dir.path().join("GEMINI.md");
        assert!(gemini_md.exists());

        let content = fs::read_to_string(&gemini_md).unwrap();
        assert!(content.contains("Test Rule"));
        assert!(content.contains("Always be helpful"));
    }

    #[test]
    fn test_diff_generates_entries() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = GeminiAdapter::new();
        let rule = create_test_rule();

        let diffs = adapter.diff(temp_dir.path(), &[rule], &[], None).unwrap();
        assert!(!diffs.is_empty());
        assert!(diffs.iter().any(|d| d.change_type == ChangeType::Add));
    }

    #[test]
    fn test_managed_paths() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = GeminiAdapter::new();

        let paths = adapter.managed_paths(temp_dir.path());
        assert_eq!(paths.len(), 4);
        assert!(paths
            .iter()
            .any(|p| p.to_string_lossy().contains("settings.json")));
        assert!(paths
            .iter()
            .any(|p| p.to_string_lossy().contains("GEMINI.md")));
    }

    #[test]
    fn test_default_impl() {
        let adapter = GeminiAdapter::default();
        assert_eq!(adapter.id(), "gemini");
    }
}
