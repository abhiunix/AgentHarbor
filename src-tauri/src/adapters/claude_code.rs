use crate::adapters::traits::{
    AdapterCapabilities, AgentAdapter, AgentConfig, ChangeType, ConfigDiffEntry,
    DeployResult, DeployStrategy, RemoveResult,
};
use crate::models::{AgentDefinition, CompositeId, MemoryScope, Skill, UniversalCapability};
use crate::utils::markdown::generate_agent_md;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Build Claude Code SKILL.md frontmatter with all supported fields.
fn build_claude_code_skill_frontmatter(skill: &Skill) -> String {
    let mut fm = String::from("---\n");
    fm.push_str(&format!("name: {}\n", skill.name));
    fm.push_str(&format!(
        "description: \"{}\"\n",
        skill.description.replace('"', "\\\"")
    ));
    // Hardcode invocation defaults for Claude Code
    fm.push_str("user-invocable: true\n");
    fm.push_str("disable-model-invocation: false\n");
    if let Some(ref tools) = skill.allowed_tools {
        if !tools.is_empty() {
            fm.push_str(&format!("allowed-tools: {}\n", tools.join(" ")));
        }
    }
    if let Some(ref model) = skill.model {
        fm.push_str(&format!("model: {}\n", model));
    }
    if let Some(ref context) = skill.context {
        fm.push_str(&format!("context: {}\n", context));
    }
    if let Some(ref agent) = skill.agent {
        fm.push_str(&format!("agent: {}\n", agent));
    }
    if let Some(ref hint) = skill.argument_hint {
        fm.push_str(&format!("argument-hint: \"{}\"\n", hint));
    }
    if let Some(ref license) = skill.license {
        fm.push_str(&format!("license: {}\n", license));
    }
    fm.push_str("---\n");
    fm
}

/// Build Cursor SKILL.md frontmatter with Cursor-specific fields.
pub fn build_cursor_skill_frontmatter(skill: &Skill) -> String {
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

/// Build Windsurf SKILL.md frontmatter with Windsurf-specific fields.
pub fn build_windsurf_skill_frontmatter(skill: &Skill) -> String {
    let mut fm = String::from("---\n");
    fm.push_str(&format!("name: {}\n", skill.name));
    fm.push_str(&format!(
        "description: \"{}\"\n",
        skill.description.replace('"', "\\\"")
    ));
    fm.push_str("metadata:\n");
    fm.push_str(&format!("  author: {}\n", skill.author));
    fm.push_str(&format!("  version: {}\n", skill.version));
    fm.push_str("---\n");
    fm
}

/// Resolve an env variable to its actual value for writing into MCP config files.
/// Returns the real value (plain text) if available, otherwise falls back to placeholder.
pub fn resolve_env_value(key: &str, env_var: &crate::models::EnvVariable) -> String {
    // 1. If value field has actual content, use it
    if let Some(ref val) = env_var.value {
        if !val.is_empty() {
            return val.clone();
        }
    }
    // 2. If label is not a placeholder, use it as the value
    if !env_var.label.is_empty() && !env_var.label.contains("${") {
        return env_var.label.clone();
    }
    // 3. Fallback to placeholder (no real value available)
    format!("${{{}}}", key)
}

pub struct ClaudeCodeAdapter;

impl ClaudeCodeAdapter {
    pub fn new() -> Self {
        Self
    }

    fn settings_path(&self, project_path: &Path) -> PathBuf {
        project_path.join(".claude").join("settings.json")
    }

    /// Resolve settings file path by target: "local" -> .claude/settings.local.json,
    /// "user" -> ~/.claude/settings.json, otherwise .claude/settings.json.
    fn settings_path_for_target(&self, project_path: &Path, target: Option<&str>) -> PathBuf {
        match target {
            Some("local") => project_path.join(".claude").join("settings.local.json"),
            Some("user") => {
                let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
                home.join(".claude").join("settings.json")
            }
            _ => self.settings_path(project_path),
        }
    }

    fn mcp_config_path(&self, project_path: &Path) -> PathBuf {
        project_path.join(".mcp.json")
    }

    /// Global MCP config path: ~/.claude.json (NOT ~/.claude/settings.json)
    fn global_mcp_path() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(".claude.json")
    }

    fn claude_md_path(&self, project_path: &Path) -> PathBuf {
        project_path.join("CLAUDE.md")
    }

    fn skills_dir(&self, project_path: &Path) -> PathBuf {
        project_path.join(".claude").join("skills")
    }

    fn agents_dir(&self, project_path: &Path) -> PathBuf {
        project_path.join(".claude").join("agents")
    }

    fn agent_memory_dir(&self, project_path: &Path) -> PathBuf {
        project_path.join(".claude").join("agent-memory")
    }

    fn read_settings(&self, project_path: &Path) -> Result<Value, String> {
        self.read_settings_at(&self.settings_path(project_path))
    }

    fn read_settings_at(&self, path: &Path) -> Result<Value, String> {
        if !path.exists() {
            return Ok(json!({}));
        }
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read settings: {}", e))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse settings: {}", e))
    }

    fn read_mcp_config(&self, project_path: &Path) -> Result<Value, String> {
        let path = self.mcp_config_path(project_path);
        if !path.exists() {
            return Ok(json!({}));
        }
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read .mcp.json: {}", e))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse .mcp.json: {}", e))
    }

    fn write_file_atomic(&self, path: &Path, content: &str) -> Result<(), String> {
        crate::utils::paths::atomic_write_str(path, content)
    }

    fn deploy_mcp_servers(
        &self,
        project_path: &Path,
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

        let mut mcp_config = self.read_mcp_config(project_path)?;
        let servers = mcp_config
            .as_object_mut()
            .ok_or("MCP config must be an object")?
            .entry("mcpServers")
            .or_insert(json!({}));

        for mcp in mcp_servers {
            let transport = if mcp.transport.is_empty() {
                "stdio".to_string()
            } else {
                mcp.transport.clone()
            };

            let env_value = if mcp.env.is_empty() {
                json!({})
            } else {
                let env_map: HashMap<String, String> = mcp
                    .env
                    .iter()
                    .map(|(k, v)| (k.clone(), resolve_env_value(k, v)))
                    .collect();
                json!(env_map)
            };

            let server_config = if transport == "stdio" {
                json!({
                    "type": transport,
                    "command": mcp.command,
                    "args": mcp.args,
                    "env": env_value,
                })
            } else {
                let mut cfg = json!({
                    "type": transport,
                    "url": mcp.url,
                });
                if !mcp.env.is_empty() {
                    cfg["env"] = env_value;
                }
                cfg
            };

            let key = mcp.id.artifact_name(&mcp.name);
            servers[&key] = server_config;
        }

        let mcp_path = self.mcp_config_path(project_path);
        let content = serde_json::to_string_pretty(&mcp_config)
            .map_err(|e| format!("Failed to serialize MCP config: {}", e))?;
        self.write_file_atomic(&mcp_path, &content)?;

        Ok(vec![mcp_path])
    }

    /// Write MCPs into an existing settings.json (global deploy: ~/.claude/settings.json).
    fn deploy_mcp_servers_to_settings(
        &self,
        capabilities: &[UniversalCapability],
        settings_path: &Path,
    ) -> Result<Vec<PathBuf>, String> {
        let mcp_servers: Vec<_> = capabilities
            .iter()
            .filter_map(|c| if let UniversalCapability::Mcp(mcp) = c { Some(mcp) } else { None })
            .collect();

        if mcp_servers.is_empty() {
            return Ok(vec![]);
        }

        let mut settings = self.read_settings_at(settings_path)?;
        let servers = settings
            .as_object_mut()
            .ok_or("Settings must be an object")?
            .entry("mcpServers")
            .or_insert(json!({}));

        for mcp in mcp_servers {
            let transport = if mcp.transport.is_empty() { "stdio".to_string() } else { mcp.transport.clone() };
            let env_value = if mcp.env.is_empty() {
                json!({})
            } else {
                let env_map: HashMap<String, String> = mcp.env.iter()
                    .map(|(k, v)| (k.clone(), resolve_env_value(k, v))).collect();
                json!(env_map)
            };
            let server_config = if transport == "stdio" {
                json!({ "type": transport, "command": mcp.command, "args": mcp.args, "env": env_value })
            } else {
                let mut cfg = json!({ "type": transport, "url": mcp.url });
                if !mcp.env.is_empty() { cfg["env"] = env_value; }
                cfg
            };
            let key = mcp.id.artifact_name(&mcp.name);
            servers[&key] = server_config;
        }

        let content = serde_json::to_string_pretty(&settings)
            .map_err(|e| format!("Failed to serialize settings: {}", e))?;
        self.write_file_atomic(settings_path, &content)?;
        Ok(vec![settings_path.to_path_buf()])
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

        let claude_md_path = self.claude_md_path(project_path);
        let mut content = if claude_md_path.exists() {
            fs::read_to_string(&claude_md_path)
                .map_err(|e| format!("Failed to read CLAUDE.md: {}", e))?
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

        self.write_file_atomic(&claude_md_path, &content)?;
        Ok(vec![claude_md_path])
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

            // Build Claude Code SKILL.md with rich frontmatter
            let frontmatter = build_claude_code_skill_frontmatter(skill);

            // Find the SKILL.md body content from files
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

            // Deploy all supporting files (scripts/, references/, etc.)
            for file in &skill.files {
                if file.path == "SKILL.md" || file.path == "skill.md" || file.path.is_empty() {
                    continue; // Already handled above
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

    fn deploy_hooks(
        &self,
        project_path: &Path,
        capabilities: &[UniversalCapability],
        settings: &mut Value,
        settings_path: &Path,
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

        let mut written_files = vec![];

        // adapter_configs["claude-code"].files (Custom-style): write each file as-is
        for hook in &hooks {
            if let Some(adapter_val) = hook.adapter_configs.get("claude-code") {
                if let Some(files_arr) = adapter_val.get("files").and_then(|v| v.as_array()) {
                    for file in files_arr {
                        let path = file.get("deploy_path").and_then(|v| v.as_str()).unwrap_or("");
                        let content = file.get("content").and_then(|v| v.as_str()).unwrap_or("");
                        if !path.is_empty() && !content.is_empty() {
                            let full = project_path.join(path);
                            self.write_file_atomic(&full, content)?;
                            written_files.push(full);
                        }
                    }
                    continue;
                }
                if let Some(scripts_arr) = adapter_val.get("scripts").and_then(|v| v.as_array()) {
                    for script in scripts_arr {
                        if let (Some(path), Some(content)) = (
                            script.get("path").and_then(|v| v.as_str()),
                            script.get("content").and_then(|v| v.as_str()),
                        ) {
                            if !path.is_empty() && !content.is_empty() {
                                let script_full = project_path.join(path);
                                self.write_file_atomic(&script_full, content)?;
                                written_files.push(script_full);
                            }
                        }
                    }
                } else if let (Some(script_path), Some(script_content)) = (
                    adapter_val.get("script_path").and_then(|v| v.as_str()),
                    adapter_val.get("script_content").and_then(|v| v.as_str()),
                ) {
                    if !script_path.is_empty() && !script_content.is_empty() {
                        let script_full = project_path.join(script_path);
                        self.write_file_atomic(&script_full, script_content)?;
                        written_files.push(script_full);
                    }
                }
            }
        }

        let legacy_hooks: Vec<_> = hooks
            .iter()
            .filter(|h| {
                h.adapter_configs
                    .get("claude-code")
                    .and_then(|v| v.get("files").and_then(|v| v.as_array()))
                    .is_none()
            })
            .collect();

        if legacy_hooks.is_empty() {
            return Ok(written_files);
        }

        let hooks_obj = settings
            .as_object_mut()
            .ok_or("Settings must be an object")?
            .entry("hooks")
            .or_insert(json!({}));

        let mut events_deployed: std::collections::HashSet<String> = std::collections::HashSet::new();

        for hook in legacy_hooks {
            // If adapter_configs has a "claude-code" entry, merge its hooks object directly
            if let Some(adapter_val) = hook.adapter_configs.get("claude-code") {
                if let Some(adapter_hooks) = adapter_val.get("hooks").and_then(|h| h.as_object()) {
                    let hooks_map = hooks_obj
                        .as_object_mut()
                        .ok_or("Hooks must be an object")?;
                    for (event_name, event_arr) in adapter_hooks {
                        let new_entries = event_arr.as_array().cloned().unwrap_or_default();
                        if new_entries.is_empty() {
                            continue;
                        }
                        let existing = hooks_map.get_mut(event_name).and_then(|v| v.as_array_mut());
                        if let Some(existing_arr) = existing {
                            for entry in new_entries {
                                existing_arr.push(entry);
                            }
                        } else {
                            hooks_map.insert(event_name.clone(), json!(new_entries));
                        }
                    }
                }
                continue;
            }

            // Legacy path: transform event/matcher/command fields
            let hook_config = json!({
                "matcher": hook.matcher,
                "hooks": [
                    {
                        "type": "command",
                        "command": hook.command,
                    }
                ]
            });

            let event_hooks = hooks_obj
                .as_object_mut()
                .ok_or("Hooks must be an object")?
                .entry(&hook.event)
                .or_insert(json!([]));

            if !events_deployed.contains(&hook.event) {
                if let Some(arr) = event_hooks.as_array_mut() {
                    arr.clear();
                }
                events_deployed.insert(hook.event.clone());
            }

            if let Some(arr) = event_hooks.as_array_mut() {
                arr.push(hook_config);
            }
        }

        let content = serde_json::to_string_pretty(settings)
            .map_err(|e| format!("Failed to serialize settings: {}", e))?;
        self.write_file_atomic(settings_path, &content)?;
        written_files.push(settings_path.to_path_buf());

        Ok(written_files)
    }

    fn deploy_plugins(
        &self,
        _project_path: &Path,
        capabilities: &[UniversalCapability],
        settings: &mut Value,
        settings_path: &Path,
    ) -> Result<Vec<PathBuf>, String> {
        let plugins: Vec<_> = capabilities
            .iter()
            .filter_map(|c| {
                if let UniversalCapability::Plugin(plugin) = c {
                    Some(plugin)
                } else {
                    None
                }
            })
            .collect();

        if plugins.is_empty() {
            return Ok(vec![]);
        }

        let plugins_obj = settings
            .as_object_mut()
            .ok_or("Settings must be an object")?
            .entry("plugins")
            .or_insert(json!({}));

        for plugin in plugins {
            let plugin_config = json!({
                "install_command": plugin.install_command,
                "config": plugin.config,
            });
            plugins_obj[&plugin.id.name] = plugin_config;
        }

        let content = serde_json::to_string_pretty(settings)
            .map_err(|e| format!("Failed to serialize settings: {}", e))?;
        self.write_file_atomic(settings_path, &content)?;

        Ok(vec![settings_path.to_path_buf()])
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
            if let Some(config) = custom.adapter_configs.get("claude-code") {
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

    fn deploy_agents_internal(
        &self,
        project_path: &Path,
        agents: &[AgentDefinition],
    ) -> Result<Vec<PathBuf>, String> {
        if agents.is_empty() {
            return Ok(vec![]);
        }

        let agents_dir = self.agents_dir(project_path);
        fs::create_dir_all(&agents_dir)
            .map_err(|e| format!("Failed to create agents directory: {}", e))?;

        let mut written_files = vec![];

        for agent in agents {
            let artifact = agent.id.artifact_name(&agent.name);
            let md_content = generate_agent_md(agent);
            let file_path = agents_dir.join(format!("{}.md", artifact));
            self.write_file_atomic(&file_path, &md_content)?;
            written_files.push(file_path);

            if agent.memory == MemoryScope::Project {
                let memory_dir = self.agent_memory_dir(project_path).join(&artifact);
                fs::create_dir_all(&memory_dir)
                    .map_err(|e| format!("Failed to create agent memory directory: {}", e))?;
            }
        }

        Ok(written_files)
    }
}

impl Default for ClaudeCodeAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentAdapter for ClaudeCodeAdapter {
    fn id(&self) -> &str {
        "claude-code"
    }

    fn name(&self) -> &str {
        "Claude Code"
    }

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities::all()
    }

    fn detect(&self, project_path: &Path) -> bool {
        let claude_dir = project_path.join(".claude");
        let claude_md = project_path.join("CLAUDE.md");
        let mcp_json = project_path.join(".mcp.json");
        claude_dir.exists() || claude_md.exists() || mcp_json.exists()
    }

    fn read_config(&self, project_path: &Path) -> Result<AgentConfig, String> {
        let mut config = AgentConfig::default();

        let mcp_config = self.read_mcp_config(project_path)?;
        if let Some(mcp_servers) = mcp_config.get("mcpServers").and_then(|v| v.as_object()) {
            config.mcp_servers = mcp_servers.keys().cloned().collect();
        }

        let settings = self.read_settings(project_path)?;
        if let Some(hooks) = settings.get("hooks").and_then(|v| v.as_object()) {
            config.hooks = hooks.keys().cloned().collect();
        }

        if let Some(plugins) = settings.get("plugins").and_then(|v| v.as_object()) {
            config.plugins = plugins.keys().cloned().collect();
        }

        let skills_dir = self.skills_dir(project_path);
        if skills_dir.exists() {
            if let Ok(entries) = fs::read_dir(&skills_dir) {
                for entry in entries.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                            config.skills.push(name.to_string());
                        }
                    }
                }
            }
        }

        let agents_dir = self.agents_dir(project_path);
        if agents_dir.exists() {
            if let Ok(entries) = fs::read_dir(&agents_dir) {
                for entry in entries.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        if name.ends_with(".md") {
                            config.agents.push(name.to_string());
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
        agents: &[AgentDefinition],
        options: Option<&serde_json::Value>,
    ) -> Result<Vec<ConfigDiffEntry>, String> {
        let mut diffs = vec![];
        let settings_target = options
            .and_then(|o| o.get("claude_settings_target"))
            .and_then(|v| v.as_str());
        // "user" target means global deploy — skip project-scoped items (skills, hooks, plugins, agents)
        let is_global = settings_target == Some("user");
        let settings_path = self.settings_path_for_target(project_path, settings_target);

        // Simulate deploys to build proposed file contents
        // -- MCP: project → .mcp.json; global → ~/.claude/settings.json (mcpServers key)
        {
            let mcp_servers: Vec<_> = capabilities.iter().filter_map(|c| {
                if let UniversalCapability::Mcp(mcp) = c { Some(mcp) } else { None }
            }).collect();

            if !mcp_servers.is_empty() {
                let (mcp_path, mut mcp_config) = if is_global {
                    let global_mcp = Self::global_mcp_path();
                    let config = if global_mcp.exists() {
                        let content = fs::read_to_string(&global_mcp).unwrap_or_else(|_| "{}".to_string());
                        serde_json::from_str(&content).unwrap_or(json!({}))
                    } else {
                        json!({})
                    };
                    (global_mcp, config)
                } else {
                    (self.mcp_config_path(project_path), self.read_mcp_config(project_path)?)
                };
                let servers = mcp_config
                    .as_object_mut()
                    .ok_or("MCP config must be an object")?
                    .entry("mcpServers")
                    .or_insert(json!({}));

                for mcp in &mcp_servers {
                    let transport = if mcp.transport.is_empty() { "stdio".to_string() } else { mcp.transport.clone() };
                    let env_value = if mcp.env.is_empty() {
                        json!({})
                    } else {
                        let env_map: HashMap<String, String> = mcp.env.iter()
                            .map(|(k, v)| (k.clone(), resolve_env_value(k, v))).collect();
                        json!(env_map)
                    };
                    let server_config = if transport == "stdio" {
                        json!({ "type": transport, "command": mcp.command, "args": mcp.args, "env": env_value })
                    } else {
                        let mut cfg = json!({ "type": transport, "url": mcp.url });
                        if !mcp.env.is_empty() { cfg["env"] = env_value; }
                        cfg
                    };
                    let key = mcp.id.artifact_name(&mcp.name);
                    servers[&key] = server_config;
                }

                let current = if mcp_path.exists() {
                    Some(fs::read_to_string(&mcp_path).unwrap_or_default())
                } else { None };
                let proposed = serde_json::to_string_pretty(&mcp_config).unwrap_or_default();

                diffs.push(ConfigDiffEntry {
                    file_path: mcp_path,
                    change_type: if current.is_some() { ChangeType::Modify } else { ChangeType::Add },
                    current_content: current,
                    proposed_content: proposed,
                    merged_content: None,
                });
            }

            // Tool filters diff — show settings.json changes for allowedTools/disallowedTools
            let has_tool_filters = mcp_servers.iter().any(|m| m.always_allow.is_some() || m.disabled_tools.is_some());
            if has_tool_filters {
                let mut proposed_settings = self.read_settings_at(&settings_path)?;
                for mcp in &mcp_servers {
                    let server_key = mcp.id.artifact_name(&mcp.name);
                    let settings_obj = proposed_settings.as_object_mut().unwrap();
                    if let Some(ref tools) = mcp.always_allow {
                        let allowed = settings_obj.entry("allowedTools").or_insert(json!([]));
                        if let Some(arr) = allowed.as_array_mut() {
                            for tool in tools {
                                let entry = format!("mcp__{}__{}",
                                    server_key.replace('-', "_"), tool);
                                if !arr.iter().any(|v| v.as_str() == Some(&entry)) {
                                    arr.push(json!(entry));
                                }
                            }
                        }
                    }
                    if let Some(ref tools) = mcp.disabled_tools {
                        let disallowed = settings_obj.entry("disallowedTools").or_insert(json!([]));
                        if let Some(arr) = disallowed.as_array_mut() {
                            for tool in tools {
                                let entry = format!("mcp__{}__{}",
                                    server_key.replace('-', "_"), tool);
                                if !arr.iter().any(|v| v.as_str() == Some(&entry)) {
                                    arr.push(json!(entry));
                                }
                            }
                        }
                    }
                }
                let current_settings = if settings_path.exists() {
                    Some(fs::read_to_string(&settings_path).unwrap_or_default())
                } else { None };
                let proposed_str = serde_json::to_string_pretty(&proposed_settings).unwrap_or_default();
                // Only add diff if settings actually changed
                let changed = current_settings.as_deref() != Some(&proposed_str);
                if changed {
                    diffs.push(ConfigDiffEntry {
                        file_path: settings_path.clone(),
                        change_type: if current_settings.is_some() { ChangeType::Modify } else { ChangeType::Add },
                        current_content: current_settings,
                        proposed_content: proposed_str,
                        merged_content: None,
                    });
                }
            }
        }

        // -- Rules: build proposed CLAUDE.md
        {
            let rules: Vec<_> = capabilities.iter().filter_map(|c| {
                if let UniversalCapability::Rule(r) = c { Some(r) } else { None }
            }).collect();

            if !rules.is_empty() {
                let path = self.claude_md_path(project_path);
                let mut content = if path.exists() {
                    fs::read_to_string(&path).unwrap_or_default()
                } else { String::new() };

                for rule in &rules {
                    content = crate::utils::rule_block::inject_rule(
                        &content,
                        &rule.id.to_string(),
                        &rule.name,
                        &rule.content,
                    );
                }

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

        // -- Skills (project-scoped, skipped for global deploy)
        if !is_global {
        for cap in capabilities {
            if let UniversalCapability::Skill(skill) = cap {
                let artifact = skill.id.artifact_name(&skill.name);
                let skill_folder = self.skills_dir(project_path).join(&artifact);

                // SKILL.md with generated frontmatter
                let frontmatter = build_claude_code_skill_frontmatter(skill);
                let body = skill.files.iter()
                    .find(|f| f.path == "SKILL.md" || f.path == "skill.md" || (!f.path.is_empty() && !f.content.is_empty() && skill.files.len() == 1))
                    .map(|f| f.content.as_str())
                    .unwrap_or("");
                let proposed = format!("{}\n{}", frontmatter, body);

                let skill_md_path = skill_folder.join("SKILL.md");
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

                // Supporting files (scripts/, references/, etc.)
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
        } // end if !is_global (skills)

        // -- Hooks: adapter_configs["claude-code"].files (Custom-style) + build proposed settings for legacy
        // (project-scoped, skipped for global deploy)
        if !is_global {
        {
            let hooks: Vec<_> = capabilities.iter().filter_map(|c| {
                if let UniversalCapability::Hook(h) = c { Some(h) } else { None }
            }).collect();

            if !hooks.is_empty() {
                // Diff entries for adapter_configs["claude-code"].files (one per file)
                for hook in &hooks {
                    if let Some(adapter_val) = hook.adapter_configs.get("claude-code") {
                        if let Some(files_arr) = adapter_val.get("files").and_then(|v| v.as_array()) {
                            for file in files_arr {
                                let path = file.get("deploy_path").and_then(|v| v.as_str()).unwrap_or("");
                                let content = file.get("content").and_then(|v| v.as_str()).unwrap_or("");
                                if !path.is_empty() && !content.is_empty() {
                                    let full = project_path.join(path);
                                    let current = if full.exists() {
                                        Some(fs::read_to_string(&full).unwrap_or_default())
                                    } else {
                                        None
                                    };
                                    diffs.push(ConfigDiffEntry {
                                        file_path: full,
                                        change_type: if current.is_some() { ChangeType::Modify } else { ChangeType::Add },
                                        current_content: current,
                                        proposed_content: content.to_string(),
                                        merged_content: None,
                                    });
                                }
                            }
                            continue;
                        }
                    }
                }

                let legacy_hooks: Vec<_> = hooks.iter().filter(|h| {
                    h.adapter_configs.get("claude-code").and_then(|v| v.get("files").and_then(|v| v.as_array())).is_none()
                }).collect();

                if !legacy_hooks.is_empty() {
                let mut settings = self.read_settings_at(&settings_path)?;
                let hooks_obj = settings
                    .as_object_mut()
                    .ok_or("Settings must be an object")?
                    .entry("hooks")
                    .or_insert(json!({}));

                for hook in &legacy_hooks {
                    if let Some(adapter_val) = hook.adapter_configs.get("claude-code") {
                        if let Some(adapter_hooks) = adapter_val.get("hooks").and_then(|h| h.as_object()) {
                            let hooks_map = hooks_obj.as_object_mut().ok_or("Hooks must be an object")?;
                            for (event_name, event_arr) in adapter_hooks {
                                let new_entries = event_arr.as_array().cloned().unwrap_or_default();
                                if new_entries.is_empty() {
                                    continue;
                                }
                                let existing = hooks_map.get_mut(event_name).and_then(|v| v.as_array_mut());
                                if let Some(existing_arr) = existing {
                                    for entry in new_entries {
                                        existing_arr.push(entry);
                                    }
                                } else {
                                    hooks_map.insert(event_name.clone(), json!(new_entries));
                                }
                            }
                        }
                    } else {
                        let hook_config = json!({
                            "matcher": hook.matcher,
                            "hooks": [{ "type": "command", "command": hook.command }]
                        });
                        let event_hooks = hooks_obj.as_object_mut().ok_or("Hooks must be an object")?
                            .entry(&hook.event).or_insert(json!([]));
                        if let Some(arr) = event_hooks.as_array_mut() {
                            arr.push(hook_config);
                        }
                    }
                }

                let current = if settings_path.exists() {
                    Some(fs::read_to_string(&settings_path).unwrap_or_default())
                } else { None };
                let proposed = serde_json::to_string_pretty(&settings).unwrap_or_default();

                diffs.push(ConfigDiffEntry {
                    file_path: settings_path.clone(),
                    change_type: if current.is_some() { ChangeType::Modify } else { ChangeType::Add },
                    current_content: current,
                    proposed_content: proposed,
                    merged_content: None,
                });
                }
            }
        }
        } // end if !is_global (hooks)

        // -- Plugins: build proposed settings (project-scoped, skipped for global deploy)
        if !is_global {
        {
            let plugins: Vec<_> = capabilities.iter().filter_map(|c| {
                if let UniversalCapability::Plugin(p) = c { Some(p) } else { None }
            }).collect();

            if !plugins.is_empty() {
                let mut settings = self.read_settings_at(&settings_path)?;
                let plugins_obj = settings
                    .as_object_mut()
                    .ok_or("Settings must be an object")?
                    .entry("plugins")
                    .or_insert(json!({}));

                for plugin in &plugins {
                    plugins_obj[&plugin.id.name] = json!({
                        "install_command": plugin.install_command,
                        "config": plugin.config,
                    });
                }

                let current = if settings_path.exists() {
                    Some(fs::read_to_string(&settings_path).unwrap_or_default())
                } else { None };
                let proposed = serde_json::to_string_pretty(&settings).unwrap_or_default();

                diffs.push(ConfigDiffEntry {
                    file_path: settings_path,
                    change_type: if current.is_some() { ChangeType::Modify } else { ChangeType::Add },
                    current_content: current,
                    proposed_content: proposed,
                    merged_content: None,
                });
            }
        }
        } // end if !is_global (plugins)

        // -- Custom: each custom capability with claude-code adapter config (project-scoped)
        if !is_global {
        for cap in capabilities {
            if let UniversalCapability::Custom(custom) = cap {
                if let Some(config) = custom.adapter_configs.get("claude-code") {
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
        } // end if !is_global (custom)

        for agent in agents {
            let artifact = agent.id.artifact_name(&agent.name);
            let path = self.agents_dir(project_path).join(format!("{}.md", artifact));
            let current = if path.exists() {
                Some(fs::read_to_string(&path).unwrap_or_default())
            } else {
                None
            };

            let proposed = generate_agent_md(agent);

            diffs.push(ConfigDiffEntry {
                file_path: path,
                change_type: if current.is_some() {
                    ChangeType::Modify
                } else {
                    ChangeType::Add
                },
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
        options: Option<&serde_json::Value>,
    ) -> Result<DeployResult, String> {
        let mut all_files = vec![];
        let settings_target = options
            .and_then(|o| o.get("claude_settings_target"))
            .and_then(|v| v.as_str());
        // "user" target means global deploy — skip project-scoped items
        let is_global = settings_target == Some("user");
        let settings_path = self.settings_path_for_target(project_path, settings_target);
        let mut settings = self.read_settings_at(&settings_path)?;

        let mcp_files = if is_global {
            let global_mcp = Self::global_mcp_path();
            self.deploy_mcp_servers_to_settings(capabilities, &global_mcp)?
        } else {
            self.deploy_mcp_servers(project_path, capabilities)?
        };
        all_files.extend(mcp_files);

        // Deploy MCP tool filters to settings (allowedTools / disallowedTools)
        {
            let mcp_servers: Vec<_> = capabilities.iter().filter_map(|c| {
                if let UniversalCapability::Mcp(mcp) = c { Some(mcp) } else { None }
            }).collect();

            let mut has_tool_filters = false;
            for mcp in &mcp_servers {
                if mcp.always_allow.is_some() || mcp.disabled_tools.is_some() {
                    has_tool_filters = true;
                    let server_key = mcp.id.artifact_name(&mcp.name);
                    let settings_obj = settings.as_object_mut().ok_or("Settings must be an object")?;

                    if let Some(ref tools) = mcp.always_allow {
                        let allowed = settings_obj
                            .entry("allowedTools")
                            .or_insert(json!([]));
                        if let Some(arr) = allowed.as_array_mut() {
                            for tool in tools {
                                let entry = format!("mcp__{}__{}",
                                    server_key.replace('-', "_"), tool);
                                if !arr.iter().any(|v| v.as_str() == Some(&entry)) {
                                    arr.push(json!(entry));
                                }
                            }
                        }
                    }

                    if let Some(ref tools) = mcp.disabled_tools {
                        let disallowed = settings_obj
                            .entry("disallowedTools")
                            .or_insert(json!([]));
                        if let Some(arr) = disallowed.as_array_mut() {
                            for tool in tools {
                                let entry = format!("mcp__{}__{}",
                                    server_key.replace('-', "_"), tool);
                                if !arr.iter().any(|v| v.as_str() == Some(&entry)) {
                                    arr.push(json!(entry));
                                }
                            }
                        }
                    }
                }
            }

            if has_tool_filters {
                let content = serde_json::to_string_pretty(&settings)
                    .map_err(|e| format!("Failed to serialize settings: {}", e))?;
                self.write_file_atomic(&settings_path, &content)?;
                all_files.push(settings_path.clone());
            }
        }

        let rule_files = self.deploy_rules(project_path, capabilities)?;
        all_files.extend(rule_files);

        if !is_global {
            let skill_files = self.deploy_skills(project_path, capabilities)?;
            all_files.extend(skill_files);

            let hook_files = self.deploy_hooks(project_path, capabilities, &mut settings, &settings_path)?;
            all_files.extend(hook_files);

            let plugin_files = self.deploy_plugins(project_path, capabilities, &mut settings, &settings_path)?;
            all_files.extend(plugin_files);

            let custom_files = self.deploy_custom(project_path, capabilities)?;
            all_files.extend(custom_files);

            let agent_files = self.deploy_agents_internal(project_path, agents)?;
            all_files.extend(agent_files);
        }

        if !is_global {
            // Write capability manifest to CLAUDE.md and AGENTS.md
            if let Ok(items) = crate::utils::project_inventory::collect_installed_items(project_path) {
                let manifest = crate::utils::manifest::build_capability_manifest(&items);
                if !manifest.is_empty() {
                    // Update CLAUDE.md
                    let claude_md_path = project_path.join("CLAUDE.md");
                    let existing = std::fs::read_to_string(&claude_md_path).unwrap_or_default();
                    let updated = crate::utils::manifest::replace_manifest_section(&existing, &manifest);
                    if let Ok(()) = crate::utils::paths::atomic_write_str(&claude_md_path, &updated) {
                        all_files.push(claude_md_path);
                    }
                    // Update AGENTS.md
                    if let Ok(agents_path) = crate::utils::manifest::write_agents_md(project_path, &items) {
                        all_files.push(agents_path);
                    }
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

        if !capability_ids.is_empty() {
            // Remove MCPs from .mcp.json
            let mcp_path = self.mcp_config_path(project_path);
            if mcp_path.exists() {
                let content = fs::read_to_string(&mcp_path)
                    .map_err(|e| format!("Failed to read .mcp.json: {}", e))?;
                if let Ok(mut config) = serde_json::from_str::<Value>(&content) {
                    if let Some(servers) = config.get_mut("mcpServers").and_then(|s| s.as_object_mut()) {
                        for id in capability_ids {
                            let key = id.artifact_name(&id.name);
                            servers.remove(&key);
                        }
                    }
                    let new_content = serde_json::to_string_pretty(&config)
                        .map_err(|e| format!("Failed to serialize .mcp.json: {}", e))?;
                    self.write_file_atomic(&mcp_path, &new_content)?;
                    removed_files.push(mcp_path);
                }
            }

            // Remove rules from CLAUDE.md by ID
            let claude_md = self.claude_md_path(project_path);
            if claude_md.exists() {
                if let Ok(content) = fs::read_to_string(&claude_md) {
                    let mut new_content = content.clone();
                    for id in capability_ids {
                        new_content = crate::utils::rule_block::remove_rule(&new_content, &id.to_string());
                    }
                    if new_content != content {
                        self.write_file_atomic(&claude_md, &new_content)?;
                        removed_files.push(claude_md);
                    }
                }
            }

            // Remove skill directories
            let skills_dir = self.skills_dir(project_path);
            if skills_dir.exists() {
                for id in capability_ids {
                    let skill_dir = skills_dir.join(&id.name);
                    if skill_dir.exists() && skill_dir.is_dir() {
                        let _ = fs::remove_dir_all(&skill_dir);
                        removed_files.push(skill_dir);
                    }
                }
            }
        }

        // Remove agents
        for agent_id in agent_ids {
            let artifact = agent_id.artifact_name(&agent_id.name);
            let agent_path = self.agents_dir(project_path).join(format!("{}.md", artifact));
            if agent_path.exists() {
                fs::remove_file(&agent_path)
                    .map_err(|e| format!("Failed to remove agent file: {}", e))?;
                removed_files.push(agent_path);
            }
        }

        // Rebuild manifests after removal
        let _ = crate::utils::manifest::rebuild_all_manifests(project_path);

        Ok(RemoveResult::success(removed_files))
    }

    fn managed_paths(&self, project_path: &Path) -> Vec<PathBuf> {
        vec![
            self.mcp_config_path(project_path),
            self.settings_path(project_path),
            self.claude_md_path(project_path),
            self.skills_dir(project_path),
            self.agents_dir(project_path),
            self.agent_memory_dir(project_path),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AgentColor, AgentModel, EnvVariable, McpServer, Rule, Skill, SkillFile, ToolAccess, Visibility};
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
            compatible_agents: vec!["claude-code".to_string()],
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
            compatible_agents: vec!["claude-code".to_string()],
            scope: "project".to_string(),
            content: "# Test Rule\nAlways be helpful.".to_string(),
            env: std::collections::HashMap::new(),
        })
    }

    fn create_test_skill() -> UniversalCapability {
        UniversalCapability::Skill(Skill {
            id: CompositeId::new("community", "test-skill").unwrap(),
            name: "Test Skill".to_string(),
            description: "Test skill".to_string(),
            version: "1.0.0".to_string(),
            author: "test".to_string(),
            visibility: Visibility::Public,
            tags: vec![],
            scope: String::new(),
            files: vec![SkillFile {
                path: "SKILL.md".to_string(),
                content: "# Test Skill\nDo the thing.".to_string(),
            }],
            env: std::collections::HashMap::new(),
            compatible_agents: vec!["claude-code".to_string()],
            allowed_tools: None,
            model: None,
            context: None,
            agent: None,
            argument_hint: None,
            license: None,
        })
    }

    fn create_test_agent() -> AgentDefinition {
        AgentDefinition {
            id: CompositeId::new("test", "test-agent").unwrap(),
            name: "Test Agent".to_string(),
            description: "A test agent".to_string(),
            version: "1.0.0".to_string(),
            author: "test".to_string(),
            visibility: Visibility::Private,
            tags: vec![],
            model: AgentModel::Sonnet,
            color: AgentColor::Blue,
            memory: MemoryScope::None,
            tools: vec![ToolAccess::All],
            required_capabilities: vec![],
            prompt: "You are a test agent.".to_string(),
            examples: vec![],
        }
    }

    #[test]
    fn test_adapter_id_and_name() {
        let adapter = ClaudeCodeAdapter::new();
        assert_eq!(adapter.id(), "claude-code");
        assert_eq!(adapter.name(), "Claude Code");
    }

    #[test]
    fn test_adapter_capabilities() {
        let adapter = ClaudeCodeAdapter::new();
        let caps = adapter.capabilities();
        assert!(caps.mcp);
        assert!(caps.rules);
        assert!(caps.skills);
        assert!(caps.hooks);
        assert!(caps.plugins);
        assert!(caps.agents);
    }

    #[test]
    fn test_detect_with_claude_dir() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = ClaudeCodeAdapter::new();

        assert!(!adapter.detect(temp_dir.path()));

        fs::create_dir(temp_dir.path().join(".claude")).unwrap();
        assert!(adapter.detect(temp_dir.path()));
    }

    #[test]
    fn test_detect_with_claude_md() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = ClaudeCodeAdapter::new();

        assert!(!adapter.detect(temp_dir.path()));

        fs::write(temp_dir.path().join("CLAUDE.md"), "# Rules").unwrap();
        assert!(adapter.detect(temp_dir.path()));
    }

    #[test]
    fn test_deploy_mcp_server() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = ClaudeCodeAdapter::new();
        let mcp = create_test_mcp();

        let result = adapter.deploy(
            temp_dir.path(),
            &[mcp],
            &[],
            DeployStrategy::Merge,
            None,
        ).unwrap();

        assert!(result.success);
        
        let mcp_path = temp_dir.path().join(".mcp.json");
        assert!(mcp_path.exists());

        let content = fs::read_to_string(&mcp_path).unwrap();
        assert!(content.contains("mcpServers"));
        assert!(content.contains("test-mcp"));
    }

    #[test]
    fn test_deploy_rule() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = ClaudeCodeAdapter::new();
        let rule = create_test_rule();

        let result = adapter.deploy(
            temp_dir.path(),
            &[rule],
            &[],
            DeployStrategy::Merge,
            None,
        ).unwrap();

        assert!(result.success);
        
        let claude_md = temp_dir.path().join("CLAUDE.md");
        assert!(claude_md.exists());

        let content = fs::read_to_string(&claude_md).unwrap();
        assert!(content.contains("Test Rule"));
        assert!(content.contains("Always be helpful"));
    }

    #[test]
    fn test_deploy_skill() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = ClaudeCodeAdapter::new();
        let skill = create_test_skill();

        let result = adapter.deploy(
            temp_dir.path(),
            &[skill],
            &[],
            DeployStrategy::Merge,
            None,
        ).unwrap();

        assert!(result.success);

        let skill_id = CompositeId::new("community", "test-skill").unwrap();
        let artifact = skill_id.artifact_name("Test Skill");
        let skill_file = temp_dir.path()
            .join(".claude")
            .join("skills")
            .join(&artifact)
            .join("SKILL.md");
        assert!(skill_file.exists());

        let content = fs::read_to_string(&skill_file).unwrap();
        assert!(content.starts_with("---\n"));
        assert!(content.contains("name: Test Skill"));
        assert!(content.contains("description:"));
        assert!(content.contains("Test Skill\nDo the thing."));
    }

    #[test]
    fn test_deploy_agent() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = ClaudeCodeAdapter::new();
        let agent = create_test_agent();
        let artifact = agent.id.artifact_name(&agent.name);

        let result = adapter.deploy(
            temp_dir.path(),
            &[],
            &[agent],
            DeployStrategy::Merge,
            None,
        ).unwrap();

        assert!(result.success);
        let agent_file = temp_dir.path().join(".claude").join("agents").join(format!("{}.md", artifact));
        assert!(agent_file.exists());

        let content = fs::read_to_string(&agent_file).unwrap();
        assert!(content.contains("---"));
        assert!(content.contains("name: test-agent"));
        assert!(content.contains("model: sonnet"));
        assert!(content.contains("You are a test agent"));
    }

    #[test]
    fn test_deploy_agent_with_memory() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = ClaudeCodeAdapter::new();
        let mut agent = create_test_agent();
        agent.memory = MemoryScope::Project;
        let artifact = agent.id.artifact_name(&agent.name);

        let result = adapter.deploy(
            temp_dir.path(),
            &[],
            &[agent],
            DeployStrategy::Merge,
            None,
        ).unwrap();

        assert!(result.success);
        let memory_dir = temp_dir.path().join(".claude").join("agent-memory").join(&artifact);
        assert!(memory_dir.exists());
    }

    #[test]
    fn test_remove_agent() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = ClaudeCodeAdapter::new();
        let agent = create_test_agent();
        let artifact = agent.id.artifact_name(&agent.name);

        adapter.deploy(temp_dir.path(), &[], &[agent.clone()], DeployStrategy::Merge, None).unwrap();
        let agent_file = temp_dir.path().join(".claude").join("agents").join(format!("{}.md", artifact));
        assert!(agent_file.exists());

        let result = adapter.remove(
            temp_dir.path(),
            &[],
            &[agent.id],
        ).unwrap();

        assert!(result.success);
        assert!(!agent_file.exists());
    }

    #[test]
    fn test_managed_paths() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = ClaudeCodeAdapter::new();
        
        let paths = adapter.managed_paths(temp_dir.path());
        assert_eq!(paths.len(), 6);
        assert!(paths.iter().any(|p| p.ends_with(".mcp.json")));
        assert!(paths.iter().any(|p| p.ends_with("settings.json")));
        assert!(paths.iter().any(|p| p.ends_with("CLAUDE.md")));
        assert!(paths.iter().any(|p| p.ends_with("skills")));
        assert!(paths.iter().any(|p| p.ends_with("agents")));
        assert!(paths.iter().any(|p| p.ends_with("agent-memory")));
    }

    #[test]
    fn test_diff_generates_entries() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = ClaudeCodeAdapter::new();
        let mcp = create_test_mcp();
        let agent = create_test_agent();

        let diffs = adapter.diff(temp_dir.path(), &[mcp], &[agent], None).unwrap();
        
        assert!(!diffs.is_empty());
        assert!(diffs.iter().any(|d| d.change_type == ChangeType::Add));
    }

    #[test]
    fn test_deploy_mcp_includes_type_and_env() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = ClaudeCodeAdapter::new();
        let mcp = create_test_mcp();

        adapter.deploy(temp_dir.path(), &[mcp], &[], DeployStrategy::Merge, None).unwrap();

        let mcp_path = temp_dir.path().join(".mcp.json");
        let content = fs::read_to_string(&mcp_path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        let server = &parsed["mcpServers"]["test-mcp"];
        assert_eq!(server["type"], "stdio");
        assert!(server["env"].is_object());
    }

    #[test]
    fn test_deploy_mcp_empty_env_still_present() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = ClaudeCodeAdapter::new();
        let mcp = create_test_mcp();

        adapter.deploy(temp_dir.path(), &[mcp], &[], DeployStrategy::Merge, None).unwrap();

        let mcp_path = temp_dir.path().join(".mcp.json");
        let content = fs::read_to_string(&mcp_path).unwrap();
        assert!(content.contains("\"env\""));
        assert!(content.contains("\"type\""));
    }
}
