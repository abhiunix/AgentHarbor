use crate::adapters::traits::{
    AdapterCapabilities, AgentAdapter, AgentConfig, ChangeType, ConfigDiffEntry,
    DeployResult, DeployStrategy, RemoveResult,
};
use crate::models::{AgentDefinition, CompositeId, UniversalCapability};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct WindsurfAdapter;

impl WindsurfAdapter {
    pub fn new() -> Self {
        Self
    }

    fn global_mcp_path() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(".codeium")
            .join("windsurf")
            .join("mcp_config.json")
    }

    fn rules_dir(&self, project_path: &Path) -> PathBuf {
        project_path.join(".windsurf").join("rules")
    }

    fn global_rules_path() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(".codeium").join("windsurf").join("memories").join("global_rules.md")
    }

    fn skills_dir(&self, project_path: &Path) -> PathBuf {
        project_path.join(".windsurf").join("skills")
    }

    fn read_global_mcp_config() -> Result<Value, String> {
        let path = Self::global_mcp_path();
        if !path.exists() {
            return Ok(json!({}));
        }
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read global mcp_config.json: {}", e))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse global mcp_config.json: {}", e))
    }

    fn write_file_atomic(&self, path: &Path, content: &str) -> Result<(), String> {
        crate::utils::paths::atomic_write_str(path, content)
    }

    fn deploy_mcp_servers(
        &self,
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

        let global_path = Self::global_mcp_path();
        let mut config = Self::read_global_mcp_config()?;

        let servers = config
            .as_object_mut()
            .ok_or("Config must be an object")?
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
                    .map(|(k, v)| {
                        let resolved = crate::adapters::claude_code::resolve_env_value(k, v);
                        // Windsurf uses ${env:KEY} placeholder syntax when no actual value
                        if resolved.starts_with("${") {
                            (k.clone(), format!("${{env:{}}}", k))
                        } else {
                            (k.clone(), resolved)
                        }
                    })
                    .collect();
                server_config["env"] = json!(env_map);
            }

            if let Some(true) = mcp.disabled {
                server_config["disabled"] = json!(true);
            }
            if let Some(ref tools) = mcp.always_allow {
                if !tools.is_empty() {
                    server_config["alwaysAllow"] = json!(tools);
                }
            }
            if let Some(ref tools) = mcp.disabled_tools {
                if !tools.is_empty() {
                    server_config["disabledTools"] = json!(tools);
                }
            }

            let key = mcp.id.artifact_name(&mcp.name);
            servers[&key] = server_config;
        }

        let content = serde_json::to_string_pretty(&config)
            .map_err(|e| format!("Failed to serialize mcp config: {}", e))?;
        self.write_file_atomic(&global_path, &content)?;

        Ok(vec![global_path])
    }

    fn deploy_custom(
        &self,
        project_path: &Path,
        capabilities: &[UniversalCapability],
    ) -> Result<Vec<PathBuf>, String> {
        let customs: Vec<_> = capabilities
            .iter()
            .filter_map(|c| {
                if let UniversalCapability::Custom(custom) = c {
                    Some(custom)
                } else {
                    None
                }
            })
            .collect();

        if customs.is_empty() {
            return Ok(vec![]);
        }

        let mut written_files = vec![];

        for custom in customs {
            if let Some(config) = custom.adapter_configs.get("windsurf") {
                if let Some(files) = config.get("files").and_then(|v| v.as_array()) {
                    for file in files {
                        let deploy_path = file
                            .get("deploy_path")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let content = file.get("content").and_then(|v| v.as_str()).unwrap_or("");
                        if deploy_path.is_empty() || content.is_empty() {
                            continue;
                        }
                        let full_path = project_path.join(deploy_path);
                        self.write_file_atomic(&full_path, content)?;
                        written_files.push(full_path);
                    }
                } else {
                    let deploy_path = config
                        .get("deploy_path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let content = config
                        .get("content")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if !deploy_path.is_empty() && !content.is_empty() {
                        let full_path = project_path.join(deploy_path);
                        self.write_file_atomic(&full_path, content)?;
                        written_files.push(full_path);
                    }
                }
            }
        }

        Ok(written_files)
    }

    fn deploy_rules(
        &self,
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

        let rules_dir = self.rules_dir(project_path);
        fs::create_dir_all(&rules_dir)
            .map_err(|e| format!("Failed to create rules directory: {}", e))?;

        let mut written_files = vec![];

        for rule in rules {
            let artifact = rule.id.artifact_name(&rule.name);
            let file_path = rules_dir.join(format!("{}.md", artifact));
            self.write_file_atomic(&file_path, &rule.content)?;
            written_files.push(file_path);
        }

        Ok(written_files)
    }

    fn deploy_skills(
        &self,
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

        let skills_dir = self.skills_dir(project_path);
        fs::create_dir_all(&skills_dir)
            .map_err(|e| format!("Failed to create skills directory: {}", e))?;

        let mut written_files = vec![];

        for skill in skills {
            let artifact = skill.id.artifact_name(&skill.name);
            let skill_folder = skills_dir.join(&artifact);
            fs::create_dir_all(&skill_folder)
                .map_err(|e| format!("Failed to create skill folder: {}", e))?;

            // Build Windsurf-specific SKILL.md with frontmatter
            let frontmatter = crate::adapters::claude_code::build_windsurf_skill_frontmatter(skill);

            let body = skill
                .files
                .iter()
                .find(|f| f.path == "SKILL.md" || f.path == "skill.md" || (!f.path.is_empty() && !f.content.is_empty() && skill.files.len() == 1))
                .map(|f| f.content.as_str())
                .unwrap_or("");

            let skill_md_content = format!("{}\n{}", frontmatter, body);
            let skill_md_path = skill_folder.join("SKILL.md");
            self.write_file_atomic(&skill_md_path, &skill_md_content)?;
            written_files.push(skill_md_path);

            // Deploy all supporting files
            for file in &skill.files {
                if file.path == "SKILL.md" || file.path == "skill.md" || file.path.is_empty() {
                    continue;
                }
                let file_path = skill_folder.join(&file.path);
                if let Some(parent) = file_path.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| format!("Failed to create directory for {}: {}", file.path, e))?;
                }
                self.write_file_atomic(&file_path, &file.content)?;
                written_files.push(file_path);
            }
        }

        Ok(written_files)
    }
}

impl Default for WindsurfAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentAdapter for WindsurfAdapter {
    fn id(&self) -> &str {
        "windsurf"
    }

    fn name(&self) -> &str {
        "Windsurf"
    }

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            mcp: true,
            rules: true,
            skills: true,
            hooks: false,
            plugins: false,
            agents: false,
            custom: true,
        }
    }

    fn detect(&self, project_path: &Path) -> bool {
        let windsurf_dir = project_path.join(".windsurf");
        windsurf_dir.exists()
    }

    fn read_config(&self, _project_path: &Path) -> Result<AgentConfig, String> {
        let mut config = AgentConfig::default();

        let mcp_config = Self::read_global_mcp_config()?;

        if let Some(mcp_servers) = mcp_config.get("mcpServers").and_then(|v| v.as_object()) {
            config.mcp_servers = mcp_servers.keys().cloned().collect();
        }

        Ok(config)
    }

    fn diff(
        &self,
        project_path: &Path,
        capabilities: &[UniversalCapability],
        _agents: &[AgentDefinition],
        options: Option<&serde_json::Value>,
    ) -> Result<Vec<ConfigDiffEntry>, String> {
        let is_global = options.and_then(|o| o.get("global")).and_then(|v| v.as_bool()).unwrap_or(false);
        let mut diffs = vec![];

        // -- MCP: build proposed global mcp_config.json
        {
            let mcp_servers: Vec<_> = capabilities.iter().filter_map(|c| {
                if let UniversalCapability::Mcp(mcp) = c { Some(mcp) } else { None }
            }).collect();

            if !mcp_servers.is_empty() {
                let global_path = Self::global_mcp_path();
                let mut config = Self::read_global_mcp_config()?;
                let servers = config.as_object_mut().ok_or("Config must be an object")?
                    .entry("mcpServers").or_insert(json!({}));

                for mcp in &mcp_servers {
                    let transport = if mcp.transport.is_empty() { "stdio" } else { &mcp.transport };
                    let mut server_config = if transport == "stdio" {
                        json!({ "command": mcp.command, "args": mcp.args })
                    } else {
                        json!({ "url": mcp.url })
                    };
                    if !mcp.env.is_empty() {
                        let env_map: HashMap<String, String> = mcp.env.iter()
                            .map(|(k, v)| {
                        let resolved = crate::adapters::claude_code::resolve_env_value(k, v);
                        // Windsurf uses ${env:KEY} placeholder syntax when no actual value
                        if resolved.starts_with("${") {
                            (k.clone(), format!("${{env:{}}}", k))
                        } else {
                            (k.clone(), resolved)
                        }
                    }).collect();
                        server_config["env"] = json!(env_map);
                    }
                    if let Some(true) = mcp.disabled {
                        server_config["disabled"] = json!(true);
                    }
                    if let Some(ref tools) = mcp.always_allow {
                        if !tools.is_empty() {
                            server_config["alwaysAllow"] = json!(tools);
                        }
                    }
                    if let Some(ref tools) = mcp.disabled_tools {
                        if !tools.is_empty() {
                            server_config["disabledTools"] = json!(tools);
                        }
                    }
                    let key = mcp.id.artifact_name(&mcp.name);
                    servers[&key] = server_config;
                }

                let current = if global_path.exists() {
                    Some(fs::read_to_string(&global_path).unwrap_or_default())
                } else { None };
                let proposed = serde_json::to_string_pretty(&config).unwrap_or_default();

                diffs.push(ConfigDiffEntry {
                    file_path: global_path,
                    change_type: if current.is_some() { ChangeType::Modify } else { ChangeType::Add },
                    current_content: current,
                    proposed_content: proposed,
                    merged_content: None,
                });
            }
        }

        // -- Rules: global → global_rules.md; project → .windsurf/rules/*.md
        {
            let rules: Vec<_> = capabilities.iter().filter_map(|c| {
                if let UniversalCapability::Rule(r) = c { Some(r) } else { None }
            }).collect();

            if is_global && !rules.is_empty() {
                let path = Self::global_rules_path();
                let current = if path.exists() {
                    Some(fs::read_to_string(&path).unwrap_or_default())
                } else { None };
                let mut proposed = current.clone().unwrap_or_default();
                for rule in &rules {
                    proposed = crate::utils::rule_block::inject_rule(
                        &proposed,
                        &rule.id.to_string(),
                        &rule.name,
                        &rule.content,
                    );
                }
                diffs.push(ConfigDiffEntry {
                    file_path: path,
                    change_type: if current.is_some() { ChangeType::Modify } else { ChangeType::Add },
                    current_content: current,
                    proposed_content: proposed,
                    merged_content: None,
                });
            } else if !is_global {
                for rule in &rules {
                    let artifact = rule.id.artifact_name(&rule.name);
                    let path = self.rules_dir(project_path).join(format!("{}.md", artifact));
                    let current = if path.exists() {
                        Some(fs::read_to_string(&path).unwrap_or_default())
                    } else { None };
                    diffs.push(ConfigDiffEntry {
                        file_path: path,
                        change_type: if current.is_some() { ChangeType::Modify } else { ChangeType::Add },
                        current_content: current,
                        proposed_content: rule.content.clone(),
                        merged_content: None,
                    });
                }
            }
        }

        // -- Skills: each as directory with SKILL.md (project-scoped)
        if !is_global {
            for cap in capabilities {
                if let UniversalCapability::Skill(skill) = cap {
                    let artifact = skill.id.artifact_name(&skill.name);
                    let skill_folder = self.skills_dir(project_path).join(&artifact);
                    let skill_md_path = skill_folder.join("SKILL.md");

                    let frontmatter = crate::adapters::claude_code::build_windsurf_skill_frontmatter(skill);
                    let body = skill.files.iter()
                        .find(|f| f.path == "SKILL.md" || f.path == "skill.md" || (!f.path.is_empty() && !f.content.is_empty() && skill.files.len() == 1))
                        .map(|f| f.content.as_str())
                        .unwrap_or("");
                    let proposed = format!("{}\n{}", frontmatter, body);

                    let current = if skill_md_path.exists() {
                        Some(fs::read_to_string(&skill_md_path).unwrap_or_default())
                    } else { None };

                    diffs.push(ConfigDiffEntry {
                        file_path: skill_md_path,
                        change_type: if current.is_some() { ChangeType::Modify } else { ChangeType::Add },
                        current_content: current,
                        proposed_content: proposed,
                        merged_content: None,
                    });

                    // Supporting files
                    for file in &skill.files {
                        if file.path == "SKILL.md" || file.path == "skill.md" || file.path.is_empty() {
                            continue;
                        }
                        let file_path = skill_folder.join(&file.path);
                        let current = if file_path.exists() {
                            Some(fs::read_to_string(&file_path).unwrap_or_default())
                        } else { None };
                        diffs.push(ConfigDiffEntry {
                            file_path,
                            change_type: if current.is_some() { ChangeType::Modify } else { ChangeType::Add },
                            current_content: current,
                            proposed_content: file.content.clone(),
                            merged_content: None,
                        });
                    }
                }
            }
        }

        // -- Custom: each custom capability with windsurf adapter config
        for cap in capabilities {
            if let UniversalCapability::Custom(custom) = cap {
                if let Some(config) = custom.adapter_configs.get("windsurf") {
                    let mut entries: Vec<(String, String)> = vec![];
                    if let Some(files) = config.get("files").and_then(|v| v.as_array()) {
                        for file in files {
                            let deploy_path = file.get("deploy_path").and_then(|v| v.as_str()).unwrap_or("");
                            let content = file.get("content").and_then(|v| v.as_str()).unwrap_or("");
                            if !deploy_path.is_empty() && !content.is_empty() {
                                entries.push((deploy_path.to_string(), content.to_string()));
                            }
                        }
                    } else {
                        let deploy_path = config.get("deploy_path").and_then(|v| v.as_str()).unwrap_or("");
                        let content = config.get("content").and_then(|v| v.as_str()).unwrap_or("");
                        if !deploy_path.is_empty() && !content.is_empty() {
                            entries.push((deploy_path.to_string(), content.to_string()));
                        }
                    }
                    for (deploy_path, content) in entries {
                        let path = project_path.join(&deploy_path);
                        let current = if path.exists() {
                            Some(fs::read_to_string(&path).unwrap_or_default())
                        } else { None };
                        diffs.push(ConfigDiffEntry {
                            file_path: path,
                            change_type: if current.is_some() { ChangeType::Modify } else { ChangeType::Add },
                            current_content: current,
                            proposed_content: content,
                            merged_content: None,
                        });
                    }
                }
            }
        }

        Ok(diffs)
    }

    fn deploy(
        &self,
        project_path: &Path,
        capabilities: &[UniversalCapability],
        _agents: &[AgentDefinition],
        _strategy: DeployStrategy,
        options: Option<&serde_json::Value>,
    ) -> Result<DeployResult, String> {
        let is_global = options.and_then(|o| o.get("global")).and_then(|v| v.as_bool()).unwrap_or(false);
        let mut all_files = vec![];

        let mcp_files = self.deploy_mcp_servers(capabilities)?;
        all_files.extend(mcp_files);

        // Global rules → ~/.codeium/windsurf/memories/global_rules.md
        if is_global {
            let rules: Vec<_> = capabilities.iter().filter_map(|c| {
                if let UniversalCapability::Rule(r) = c { Some(r) } else { None }
            }).collect();
            if !rules.is_empty() {
                let path = Self::global_rules_path();
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| format!("Failed to create windsurf memories dir: {}", e))?;
                }
                let mut content = if path.exists() {
                    fs::read_to_string(&path).unwrap_or_default()
                } else {
                    String::new()
                };
                for rule in rules {
                    content = crate::utils::rule_block::inject_rule(
                        &content,
                        &rule.id.to_string(),
                        &rule.name,
                        &rule.content,
                    );
                }
                self.write_file_atomic(&path, &content)?;
                all_files.push(path);
            }
        }

        // Rules and skills are project-scoped; skip in global deploy mode
        if !is_global {
            let rule_files = self.deploy_rules(project_path, capabilities)?;
            all_files.extend(rule_files);

            let skill_files = self.deploy_skills(project_path, capabilities)?;
            all_files.extend(skill_files);
        }

        let custom_files = self.deploy_custom(project_path, capabilities)?;
        all_files.extend(custom_files);

        if !is_global {
            if let Ok(items) = crate::utils::project_inventory::collect_installed_items(project_path) {
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
        _agent_ids: &[CompositeId],
    ) -> Result<RemoveResult, String> {
        let mut removed_files = vec![];

        let mcp_ids: Vec<_> = capability_ids.to_vec();
        if !mcp_ids.is_empty() {
            let global_path = Self::global_mcp_path();
            if global_path.exists() {
                let mut config = Self::read_global_mcp_config()?;
                if let Some(servers) = config
                    .as_object_mut()
                    .and_then(|o| o.get_mut("mcpServers"))
                    .and_then(|s| s.as_object_mut())
                {
                    for id in &mcp_ids {
                        // For community items, artifact_name returns id.name directly.
                        // For private items, artifact_name returns slug-hash, but we only have the id here.
                        // Use artifact_name with id.name as fallback display name to match deploy key format.
                        let key = id.artifact_name(&id.name);
                        servers.remove(&key);
                    }
                }

                let content = serde_json::to_string_pretty(&config)
                    .map_err(|e| format!("Failed to serialize mcp config: {}", e))?;
                self.write_file_atomic(&global_path, &content)?;
                removed_files.push(global_path);
            }
        }

        // Remove rules from global_rules.md if present
        if !capability_ids.is_empty() {
            let global_rules = Self::global_rules_path();
            if global_rules.exists() {
                if let Ok(content) = fs::read_to_string(&global_rules) {
                    let mut new_content = content.clone();
                    for id in capability_ids {
                        new_content = crate::utils::rule_block::remove_rule(&new_content, &id.to_string());
                    }
                    if new_content != content {
                        let _ = self.write_file_atomic(&global_rules, &new_content);
                        removed_files.push(global_rules);
                    }
                }
            }
        }

        let _ = crate::utils::manifest::rebuild_all_manifests(project_path);

        Ok(RemoveResult::success(removed_files))
    }

    fn managed_paths(&self, project_path: &Path) -> Vec<PathBuf> {
        vec![
            Self::global_mcp_path(),
            Self::global_rules_path(),
            self.rules_dir(project_path),
            self.skills_dir(project_path),
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
            compatible_agents: vec!["windsurf".to_string()],
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
            compatible_agents: vec!["windsurf".to_string()],
            scope: "project".to_string(),
            content: "# Test Rule\nAlways be helpful.".to_string(),
            env: std::collections::HashMap::new(),
        })
    }

    #[test]
    fn test_adapter_id_and_name() {
        let adapter = WindsurfAdapter::new();
        assert_eq!(adapter.id(), "windsurf");
        assert_eq!(adapter.name(), "Windsurf");
    }

    #[test]
    fn test_adapter_capabilities() {
        let adapter = WindsurfAdapter::new();
        let caps = adapter.capabilities();
        assert!(caps.mcp);
        assert!(caps.rules);
        assert!(caps.skills);
        assert!(!caps.hooks);
        assert!(!caps.plugins);
        assert!(!caps.agents);
    }

    #[test]
    fn test_detect_with_windsurf_dir() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = WindsurfAdapter::new();

        assert!(!adapter.detect(temp_dir.path()));

        fs::create_dir(temp_dir.path().join(".windsurf")).unwrap();
        assert!(adapter.detect(temp_dir.path()));
    }

    #[test]
    fn test_deploy_rule_as_individual_md() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = WindsurfAdapter::new();
        let rule = create_test_rule();

        let result = adapter.deploy(
            temp_dir.path(),
            &[rule],
            &[],
            DeployStrategy::Merge,
            None,
        ).unwrap();

        assert!(result.success);

        let rules_dir = temp_dir.path().join(".windsurf").join("rules");
        assert!(rules_dir.exists());

        let entries: Vec<_> = fs::read_dir(&rules_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(entries.len(), 1);

        let file_name = entries[0].file_name().to_string_lossy().to_string();
        assert!(file_name.ends_with(".md"));

        let content = fs::read_to_string(entries[0].path()).unwrap();
        assert!(content.contains("Test Rule"));
        assert!(content.contains("Always be helpful"));
        assert!(!content.contains("---"));
    }

    #[test]
    fn test_managed_paths() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = WindsurfAdapter::new();
        
        let paths = adapter.managed_paths(temp_dir.path());
        assert_eq!(paths.len(), 3);
        assert!(paths.iter().any(|p| p.to_string_lossy().contains("mcp_config.json")));
        assert!(paths.iter().any(|p| p.ends_with("rules")));
        assert!(paths.iter().any(|p| p.ends_with("skills")));
    }

    #[test]
    fn test_diff_generates_entries() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = WindsurfAdapter::new();
        let rule = create_test_rule();

        let diffs = adapter.diff(temp_dir.path(), &[rule], &[], None).unwrap();

        assert!(!diffs.is_empty());
        assert!(diffs.iter().any(|d| d.change_type == ChangeType::Add));
    }

    #[test]
    fn test_diff_mcp_uses_global_path() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = WindsurfAdapter::new();
        let mcp = create_test_mcp();

        let diffs = adapter.diff(temp_dir.path(), &[mcp], &[], None).unwrap();

        assert!(!diffs.is_empty());
        let mcp_diff = &diffs[0];
        assert!(mcp_diff.file_path.to_string_lossy().contains(".codeium"));
    }
}
