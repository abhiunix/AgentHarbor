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

pub struct CursorAdapter;

impl CursorAdapter {
    pub fn new() -> Self {
        Self
    }

    fn mcp_config_path(&self, project_path: &Path) -> PathBuf {
        project_path.join(".cursor").join("mcp.json")
    }

    fn rules_dir(&self, project_path: &Path) -> PathBuf {
        project_path.join(".cursor").join("rules")
    }

    fn agents_dir(&self, project_path: &Path) -> PathBuf {
        project_path.join(".cursor").join("agents")
    }

    fn skills_dir(&self, project_path: &Path) -> PathBuf {
        project_path.join(".cursor").join("skills")
    }

    fn hooks_config_path(&self, project_path: &Path) -> PathBuf {
        project_path.join(".cursor").join("hooks.json")
    }

    fn global_mcp_path() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(".cursor").join("mcp.json")
    }

    fn read_mcp_config(&self, project_path: &Path) -> Result<Value, String> {
        let path = self.mcp_config_path(project_path);
        if !path.exists() {
            return Ok(json!({}));
        }
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read mcp.json: {}", e))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse mcp.json: {}", e))
    }

    fn write_file_atomic(&self, path: &Path, content: &str) -> Result<(), String> {
        crate::utils::paths::atomic_write_str(path, content)
    }

    fn deploy_mcp_servers(
        &self,
        project_path: &Path,
        capabilities: &[UniversalCapability],
        config: &mut Value,
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
                    .map(|(k, v)| (k.clone(), crate::adapters::claude_code::resolve_env_value(k, v)))
                    .collect();
                server_config["env"] = json!(env_map);
            }

            let key = mcp.id.artifact_name(&mcp.name);
            servers[&key] = server_config;
        }

        let mcp_path = self.mcp_config_path(project_path);
        let content = serde_json::to_string_pretty(config)
            .map_err(|e| format!("Failed to serialize mcp config: {}", e))?;
        self.write_file_atomic(&mcp_path, &content)?;

        Ok(vec![mcp_path])
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
            let file_name = format!("{}.mdc", artifact);
            let file_path = rules_dir.join(&file_name);

            let mut mdc_content = String::new();
            mdc_content.push_str("---\n");
            mdc_content.push_str(&format!(
                "description: \"{}\"\n",
                rule.description.replace('"', "\\\"")
            ));
            mdc_content.push_str("globs: \n");
            mdc_content.push_str("---\n\n");
            mdc_content.push_str(&rule.content);

            self.write_file_atomic(&file_path, &mdc_content)?;
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

            // Build Cursor-specific SKILL.md with frontmatter
            let frontmatter = crate::adapters::claude_code::build_cursor_skill_frontmatter(skill);

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

    fn map_hook_event(event: &str) -> Option<&'static str> {
        match event {
            // Pre/post tool use
            "PreToolUse" | "preToolUse" => Some("preToolUse"),
            "PostToolUse" | "postToolUse" | "file_save" | "afterFileEdit" => Some("afterFileEdit"),
            "PostToolUseFailure" | "postToolUseFailure" => Some("postToolUseFailure"),
            // Session lifecycle
            "SessionStart" | "sessionStart" => Some("sessionStart"),
            "SessionEnd" | "sessionEnd" => Some("sessionEnd"),
            // User prompt
            "UserPromptSubmit" | "userPromptSubmit" | "beforeSubmitPrompt" => Some("beforeSubmitPrompt"),
            // File changes
            "FileChanged" | "fileChanged" => Some("afterFileEdit"),
            // Shell execution
            "pre_command" | "beforeShellExecution" => Some("beforeShellExecution"),
            // Stop / notifications
            "Stop" | "stop" | "Notification" => Some("stop"),
            // Compaction
            "PreCompact" | "preCompact" => Some("preCompact"),
            // Subagents
            "SubagentStart" | "subagentStart" => Some("subagentStart"),
            "SubagentStop" | "subagentStop" => Some("subagentStop"),
            _ => None,
        }
    }

    fn deploy_hooks(
        &self,
        project_path: &Path,
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

        let mut written_files = vec![];

        // Hook adapter_configs.files (Custom-style): write each file as-is
        for hook in &hooks {
            if let Some(adapter_val) = hook.adapter_configs.get("cursor") {
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
            }
        }

        // Legacy: script files then merge into hooks.json
        for hook in &hooks {
            if let Some(adapter_val) = hook.adapter_configs.get("cursor") {
                if adapter_val.get("files").and_then(|v| v.as_array()).is_some() {
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
                    .get("cursor")
                    .and_then(|v| v.get("files").and_then(|v| v.as_array()))
                    .is_none()
            })
            .collect();

        if legacy_hooks.is_empty() {
            return Ok(written_files);
        }

        let hooks_path = legacy_hooks
            .iter()
            .find_map(|h| {
                h.adapter_configs.get("cursor").and_then(|v| v.get("deploy_path")).and_then(|v| v.as_str())
            })
            .map(|p| project_path.join(p))
            .unwrap_or_else(|| self.hooks_config_path(project_path));

        let mut hooks_config: Value = if hooks_path.exists() {
            let content = fs::read_to_string(&hooks_path)
                .map_err(|e| format!("Failed to read hooks.json: {}", e))?;
            serde_json::from_str(&content)
                .map_err(|e| format!("Failed to parse hooks.json: {}", e))?
        } else {
            json!({ "version": 1, "hooks": {} })
        };

        hooks_config["version"] = json!(1);

        // Merge adapter_configs entries directly (legacy only)
        for hook in &legacy_hooks {
            if let Some(adapter_val) = hook.adapter_configs.get("cursor") {
                if let Some(ver) = adapter_val.get("version") {
                    hooks_config["version"] = ver.clone();
                }
                if let Some(adapter_hooks) = adapter_val.get("hooks").and_then(|h| h.as_object()) {
                    let hooks_map = hooks_config
                        .as_object_mut()
                        .ok_or("hooks.json must be an object")?
                        .entry("hooks")
                        .or_insert(json!({}))
                        .as_object_mut()
                        .ok_or("hooks field must be an object")?;
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
            }
        }

        let mut events_deployed: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        let hooks_obj = hooks_config
            .as_object_mut()
            .ok_or("hooks.json must be an object")?
            .entry("hooks")
            .or_insert(json!({}));

        for hook in &hooks {
            if hook.adapter_configs.contains_key("cursor") {
                continue;
            }

            let cursor_event = match Self::map_hook_event(&hook.event) {
                Some(e) => e,
                None => continue,
            };

            let event_arr = hooks_obj
                .as_object_mut()
                .ok_or("hooks field must be an object")?
                .entry(cursor_event)
                .or_insert(json!([]));

            if !events_deployed.contains(cursor_event) {
                if let Some(arr) = event_arr.as_array_mut() {
                    arr.clear();
                }
                events_deployed.insert(cursor_event.to_string());
            }

            if let Some(arr) = event_arr.as_array_mut() {
                arr.push(json!({ "command": hook.command }));
            }
        }

        let content = serde_json::to_string_pretty(&hooks_config)
            .map_err(|e| format!("Failed to serialize hooks.json: {}", e))?;
        self.write_file_atomic(&hooks_path, &content)?;
        written_files.push(hooks_path);

        Ok(written_files)
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
            if let Some(config) = custom.adapter_configs.get("cursor") {
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
        }

        Ok(written_files)
    }
}

impl Default for CursorAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentAdapter for CursorAdapter {
    fn id(&self) -> &str {
        "cursor"
    }

    fn name(&self) -> &str {
        "Cursor"
    }

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            mcp: true,
            rules: true,
            skills: true,
            hooks: true,
            plugins: false,
            agents: true,
            custom: true,
        }
    }

    fn detect(&self, project_path: &Path) -> bool {
        let cursor_dir = project_path.join(".cursor");
        cursor_dir.exists()
    }

    fn read_config(&self, project_path: &Path) -> Result<AgentConfig, String> {
        let mut config = AgentConfig::default();

        let mcp_config = self.read_mcp_config(project_path)?;

        if let Some(mcp_servers) = mcp_config.get("mcpServers").and_then(|v| v.as_object()) {
            config.mcp_servers = mcp_servers.keys().cloned().collect();
        }

        let skills_dir = self.skills_dir(project_path);
        if skills_dir.exists() {
            if let Ok(entries) = fs::read_dir(&skills_dir) {
                for entry in entries.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        config.skills.push(name.to_string());
                    }
                }
            }
        }

        let hooks_path = self.hooks_config_path(project_path);
        if hooks_path.exists() {
            if let Ok(content) = fs::read_to_string(&hooks_path) {
                if let Ok(hooks_json) = serde_json::from_str::<Value>(&content) {
                    if let Some(hooks) = hooks_json.get("hooks").and_then(|h| h.as_object()) {
                        for event_name in hooks.keys() {
                            config.hooks.push(event_name.clone());
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
                            let stem = name.strip_suffix(".md").unwrap_or(name);
                            config.agents.push(stem.to_string());
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
        let is_global = options.and_then(|o| o.get("global")).and_then(|v| v.as_bool()).unwrap_or(false);
        let mut diffs = vec![];

        // -- MCP: build proposed .cursor/mcp.json
        {
            let mcp_servers: Vec<_> = capabilities.iter().filter_map(|c| {
                if let UniversalCapability::Mcp(mcp) = c { Some(mcp) } else { None }
            }).collect();

            if !mcp_servers.is_empty() {
                let mcp_path = self.mcp_config_path(project_path);
                let mut mcp_config = self.read_mcp_config(project_path)?;
                let servers = mcp_config.as_object_mut().ok_or("Config must be an object")?
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
                            .map(|(k, v)| (k.clone(), crate::adapters::claude_code::resolve_env_value(k, v))).collect();
                        server_config["env"] = json!(env_map);
                    }
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
        }

        // -- Rules: each as .mdc file (skipped in global deploy mode)
        if !is_global {
            for cap in capabilities {
                if let UniversalCapability::Rule(rule) = cap {
                    let artifact = rule.id.artifact_name(&rule.name);
                    let path = self.rules_dir(project_path).join(format!("{}.mdc", artifact));
                    let current = if path.exists() {
                        Some(fs::read_to_string(&path).unwrap_or_default())
                    } else { None };

                    let mut proposed = String::new();
                    proposed.push_str("---\n");
                    proposed.push_str(&format!("description: \"{}\"\n", rule.description.replace('"', "\\\"")));
                    proposed.push_str("globs: \n");
                    proposed.push_str("---\n\n");
                    proposed.push_str(&rule.content);

                    diffs.push(ConfigDiffEntry {
                        file_path: path,
                        change_type: if current.is_some() { ChangeType::Modify } else { ChangeType::Add },
                        current_content: current,
                        proposed_content: proposed,
                        merged_content: None,
                    });
                }
            }
        }

        // -- Skills: SKILL.md + supporting files (skipped in global deploy mode)
        if !is_global {
            for cap in capabilities {
                if let UniversalCapability::Skill(skill) = cap {
                    let artifact = skill.id.artifact_name(&skill.name);
                    let skill_folder = self.skills_dir(project_path).join(&artifact);

                    // SKILL.md with Cursor-specific frontmatter
                    let frontmatter = crate::adapters::claude_code::build_cursor_skill_frontmatter(skill);
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

        // -- Hooks: adapter_configs.files (Custom-style) + script files (legacy) + .cursor/hooks.json
        // Skipped in global deploy mode (hooks are project-scoped)
        if !is_global {
        {
            let hooks: Vec<_> = capabilities.iter().filter_map(|c| {
                if let UniversalCapability::Hook(h) = c { Some(h) } else { None }
            }).collect();

            if !hooks.is_empty() {
                // Diff entries for adapter_configs["cursor"].files (one entry per file)
                for hook in &hooks {
                    if let Some(adapter_val) = hook.adapter_configs.get("cursor") {
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
                        if let Some(scripts_arr) = adapter_val.get("scripts").and_then(|v| v.as_array()) {
                            for script in scripts_arr {
                                if let (Some(path), Some(content)) = (
                                    script.get("path").and_then(|v| v.as_str()),
                                    script.get("content").and_then(|v| v.as_str()),
                                ) {
                                    if !path.is_empty() && !content.is_empty() {
                                        let script_full = project_path.join(path);
                                        let current = if script_full.exists() {
                                            Some(fs::read_to_string(&script_full).unwrap_or_default())
                                        } else {
                                            None
                                        };
                                        diffs.push(ConfigDiffEntry {
                                            file_path: script_full,
                                            change_type: if current.is_some() { ChangeType::Modify } else { ChangeType::Add },
                                            current_content: current,
                                            proposed_content: content.to_string(),
                                            merged_content: None,
                                        });
                                    }
                                }
                            }
                        } else if let (Some(script_path), Some(script_content)) = (
                            adapter_val.get("script_path").and_then(|v| v.as_str()),
                            adapter_val.get("script_content").and_then(|v| v.as_str()),
                        ) {
                            if !script_path.is_empty() && !script_content.is_empty() {
                                let script_full = project_path.join(script_path);
                                let current = if script_full.exists() {
                                    Some(fs::read_to_string(&script_full).unwrap_or_default())
                                } else {
                                    None
                                };
                                diffs.push(ConfigDiffEntry {
                                    file_path: script_full,
                                    change_type: if current.is_some() { ChangeType::Modify } else { ChangeType::Add },
                                    current_content: current,
                                    proposed_content: script_content.to_string(),
                                    merged_content: None,
                                });
                            }
                        }
                    }
                }

                let legacy_hooks: Vec<_> = hooks.iter().filter(|h| {
                    h.adapter_configs.get("cursor").and_then(|v| v.get("files").and_then(|v| v.as_array())).is_none()
                }).collect();

                if !legacy_hooks.is_empty() {
                let hooks_path = legacy_hooks.iter().find_map(|h| {
                    h.adapter_configs.get("cursor").and_then(|v| v.get("deploy_path")).and_then(|v| v.as_str())
                }).map(|p| project_path.join(p)).unwrap_or_else(|| self.hooks_config_path(project_path));

                let mut hooks_config: Value = if hooks_path.exists() {
                    let content = fs::read_to_string(&hooks_path).unwrap_or_default();
                    serde_json::from_str(&content).unwrap_or(json!({ "version": 1, "hooks": {} }))
                } else {
                    json!({ "version": 1, "hooks": {} })
                };
                hooks_config["version"] = json!(1);

                for hook in &legacy_hooks {
                    if let Some(adapter_val) = hook.adapter_configs.get("cursor") {
                        if let Some(ver) = adapter_val.get("version") {
                            hooks_config["version"] = ver.clone();
                        }
                        if let Some(adapter_hooks) = adapter_val.get("hooks").and_then(|h| h.as_object()) {
                            let hooks_map = hooks_config.as_object_mut().ok_or("hooks.json must be an object")?
                                .entry("hooks").or_insert(json!({}))
                                .as_object_mut().ok_or("hooks field must be an object")?;
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
                    } else if let Some(cursor_event) = Self::map_hook_event(&hook.event) {
                        let hooks_obj = hooks_config.as_object_mut().ok_or("hooks.json must be an object")?
                            .entry("hooks").or_insert(json!({}));
                        let event_arr = hooks_obj.as_object_mut().ok_or("hooks field must be an object")?
                            .entry(cursor_event).or_insert(json!([]));
                        if let Some(arr) = event_arr.as_array_mut() {
                            arr.push(json!({ "command": hook.command }));
                        }
                    }
                }

                let current = if hooks_path.exists() {
                    Some(fs::read_to_string(&hooks_path).unwrap_or_default())
                } else { None };
                let proposed = serde_json::to_string_pretty(&hooks_config).unwrap_or_default();

                diffs.push(ConfigDiffEntry {
                    file_path: hooks_path,
                    change_type: if current.is_some() { ChangeType::Modify } else { ChangeType::Add },
                    current_content: current,
                    proposed_content: proposed,
                    merged_content: None,
                });
                }
            }
        }
        } // end if !is_global (hooks)

        // -- Custom: each custom capability with cursor adapter config
        // Skipped in global deploy mode (custom configs are project-scoped)
        if !is_global {
        for cap in capabilities {
            if let UniversalCapability::Custom(custom) = cap {
                if let Some(config) = custom.adapter_configs.get("cursor") {
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
        let is_global = options.and_then(|o| o.get("global")).and_then(|v| v.as_bool()).unwrap_or(false);
        let mut all_files = vec![];
        let mut mcp_config = self.read_mcp_config(project_path)?;

        let mcp_files = self.deploy_mcp_servers(project_path, capabilities, &mut mcp_config)?;
        all_files.extend(mcp_files);

        if !is_global {
            let rule_files = self.deploy_rules(project_path, capabilities)?;
            all_files.extend(rule_files);

            let skill_files = self.deploy_skills(project_path, capabilities)?;
            all_files.extend(skill_files);

            let hook_files = self.deploy_hooks(project_path, capabilities)?;
            all_files.extend(hook_files);

            let custom_files = self.deploy_custom(project_path, capabilities)?;
            all_files.extend(custom_files);

            let agent_files = self.deploy_agents_internal(project_path, agents)?;
            all_files.extend(agent_files);
        }

        if !is_global {
            if let Ok(items) = crate::utils::project_inventory::collect_installed_items(project_path) {
                let manifest = crate::utils::manifest::build_capability_manifest(&items);
                if !manifest.is_empty() {
                    // Write .cursor/rules/agentharbor-manifest.mdc
                    let mdc_dir = project_path.join(".cursor").join("rules");
                    let _ = std::fs::create_dir_all(&mdc_dir);
                    let mdc_path = mdc_dir.join("agentharbor-manifest.mdc");
                    let mdc_content = format!(
                        "---\ndescription: \"AgentHarbor deployed capabilities manifest\"\nglobs: \n---\n\n{}",
                        manifest
                    );
                    if let Ok(()) = crate::utils::paths::atomic_write_str(&mdc_path, &mdc_content) {
                        all_files.push(mdc_path);
                    }
                    // Update .cursorrules
                    let cursorrules_path = project_path.join(".cursorrules");
                    let existing = std::fs::read_to_string(&cursorrules_path).unwrap_or_default();
                    let updated = crate::utils::manifest::replace_manifest_section(&existing, &manifest);
                    if let Ok(()) = crate::utils::paths::atomic_write_str(&cursorrules_path, &updated) {
                        all_files.push(cursorrules_path);
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
            // Remove MCPs from .cursor/mcp.json
            let mcp_path = self.mcp_config_path(project_path);
            if mcp_path.exists() {
                let mut config = self.read_mcp_config(project_path)?;
                if let Some(servers) = config.get_mut("mcpServers").and_then(|s| s.as_object_mut()) {
                    for id in capability_ids {
                        let key = id.artifact_name(&id.name);
                        servers.remove(&key);
                    }
                }
                let new_content = serde_json::to_string_pretty(&config)
                    .map_err(|e| format!("Failed to serialize mcp.json: {}", e))?;
                self.write_file_atomic(&mcp_path, &new_content)?;
                removed_files.push(mcp_path);
            }

            // Remove rule files from .cursor/rules/
            let rules_dir = self.rules_dir(project_path);
            if rules_dir.exists() {
                for id in capability_ids {
                    let rule_file = rules_dir.join(format!("{}.mdc", id.name));
                    if rule_file.exists() {
                        let _ = fs::remove_file(&rule_file);
                        removed_files.push(rule_file);
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
            self.rules_dir(project_path),
            self.agents_dir(project_path),
            self.skills_dir(project_path),
            self.hooks_config_path(project_path),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AgentColor, AgentModel, Hook, McpServer, MemoryScope, Rule, ToolAccess, Visibility};
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
            compatible_agents: vec!["cursor".to_string()],
            disabled: None,
            always_allow: None,
            disabled_tools: None,
            tool_list: None,
            transport: "stdio".to_string(),
            command: "npx".to_string(),
            args: vec!["-y".to_string(), "@test/mcp".to_string()],
            url: String::new(),
            env: std::collections::HashMap::new(),
            category: None,
            author_github: None,
            source_info: None,
            stats: None,
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
            compatible_agents: vec!["cursor".to_string()],
            scope: "project".to_string(),
            content: "# Test Rule\nAlways be helpful.".to_string(),
            env: std::collections::HashMap::new(),
            category: None,
            author_github: None,
            source_info: None,
            stats: None,
        })
    }

    fn create_test_hook() -> UniversalCapability {
        UniversalCapability::Hook(Hook {
            id: CompositeId::new("community", "test-hook").unwrap(),
            name: "Test Hook".to_string(),
            description: "Test hook".to_string(),
            version: "1.0.0".to_string(),
            author: "test".to_string(),
            visibility: Visibility::Public,
            tags: vec![],
            compatible_agents: vec!["cursor".to_string()],
            event: "file_save".to_string(),
            matcher: "*.rs".to_string(),
            command: "cargo fmt".to_string(),
            timeout_ms: 5000,
            env: std::collections::HashMap::new(),
            adapter_configs: std::collections::HashMap::new(),
            category: None,
            author_github: None,
            source_info: None,
            stats: None,
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
        let adapter = CursorAdapter::new();
        assert_eq!(adapter.id(), "cursor");
        assert_eq!(adapter.name(), "Cursor");
    }

    #[test]
    fn test_adapter_capabilities() {
        let adapter = CursorAdapter::new();
        let caps = adapter.capabilities();
        assert!(caps.mcp);
        assert!(caps.rules);
        assert!(caps.skills);
        assert!(caps.hooks);
        assert!(!caps.plugins);
        assert!(caps.agents);
    }

    #[test]
    fn test_detect_with_cursor_dir() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = CursorAdapter::new();

        assert!(!adapter.detect(temp_dir.path()));

        fs::create_dir(temp_dir.path().join(".cursor")).unwrap();
        assert!(adapter.detect(temp_dir.path()));
    }

    #[test]
    fn test_deploy_mcp_server() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = CursorAdapter::new();
        let mcp = create_test_mcp();

        let result = adapter.deploy(
            temp_dir.path(),
            &[mcp],
            &[],
            DeployStrategy::Merge,
            None,
        ).unwrap();

        assert!(result.success);
        
        let mcp_path = temp_dir.path().join(".cursor").join("mcp.json");
        assert!(mcp_path.exists());

        let content = fs::read_to_string(&mcp_path).unwrap();
        assert!(content.contains("mcpServers"));
        assert!(content.contains("test-mcp"));
    }

    #[test]
    fn test_deploy_rule_as_mdc() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = CursorAdapter::new();
        let rule = create_test_rule();

        let result = adapter.deploy(
            temp_dir.path(),
            &[rule],
            &[],
            DeployStrategy::Merge,
            None,
        ).unwrap();

        assert!(result.success);

        let rules_dir = temp_dir.path().join(".cursor").join("rules");
        assert!(rules_dir.exists());

        let entries: Vec<_> = fs::read_dir(&rules_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy() != "agentharbor-manifest.mdc")
            .collect();
        assert_eq!(entries.len(), 1);

        let file_name = entries[0].file_name().to_string_lossy().to_string();
        assert!(file_name.ends_with(".mdc"));

        let content = fs::read_to_string(entries[0].path()).unwrap();
        assert!(content.starts_with("---\n"));
        assert!(content.contains("description:"));
        assert!(content.contains("globs:"));
        assert!(content.contains("Always be helpful"));
    }

    #[test]
    fn test_deploy_hook_as_json() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = CursorAdapter::new();
        let hook = create_test_hook();

        let result = adapter.deploy(
            temp_dir.path(),
            &[hook],
            &[],
            DeployStrategy::Merge,
            None,
        ).unwrap();

        assert!(result.success);

        let hooks_path = temp_dir.path().join(".cursor").join("hooks.json");
        assert!(hooks_path.exists());

        let content = fs::read_to_string(&hooks_path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["version"], 1);
        assert!(parsed["hooks"]["afterFileEdit"].is_array());
        assert!(parsed["hooks"]["afterFileEdit"][0]["command"]
            .as_str()
            .unwrap()
            .contains("cargo fmt"));
    }

    #[test]
    fn test_deploy_agent_to_agents_dir() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = CursorAdapter::new();
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
        let agent_file = temp_dir.path()
            .join(".cursor")
            .join("agents")
            .join(format!("{}.md", artifact));
        assert!(agent_file.exists());

        let content = fs::read_to_string(&agent_file).unwrap();
        assert!(content.starts_with("---\n"));
        assert!(content.contains("description:"));
        assert!(content.contains("You are a test agent."));
    }

    #[test]
    fn test_remove_agent() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = CursorAdapter::new();
        let agent = create_test_agent();
        let artifact = agent.id.artifact_name(&agent.name);

        adapter.deploy(temp_dir.path(), &[], std::slice::from_ref(&agent), DeployStrategy::Merge, None).unwrap();
        let agent_file = temp_dir.path()
            .join(".cursor")
            .join("agents")
            .join(format!("{}.md", artifact));
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
        let adapter = CursorAdapter::new();
        
        let paths = adapter.managed_paths(temp_dir.path());
        assert_eq!(paths.len(), 5);
        assert!(paths.iter().any(|p| p.ends_with("mcp.json")));
        assert!(paths.iter().any(|p| p.ends_with("rules")));
        assert!(paths.iter().any(|p| p.ends_with("agents")));
        assert!(paths.iter().any(|p| p.ends_with("skills")));
        assert!(paths.iter().any(|p| p.ends_with("hooks.json")));
    }

    #[test]
    fn test_diff_generates_entries() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = CursorAdapter::new();
        let mcp = create_test_mcp();
        let agent = create_test_agent();

        let diffs = adapter.diff(temp_dir.path(), &[mcp], &[agent], None).unwrap();
        
        assert!(!diffs.is_empty());
        assert!(diffs.iter().any(|d| d.change_type == ChangeType::Add));
    }

    #[test]
    fn test_hook_event_mapping() {
        assert_eq!(CursorAdapter::map_hook_event("file_save"), Some("afterFileEdit"));
        assert_eq!(CursorAdapter::map_hook_event("PostToolUse"), Some("afterFileEdit"));
        assert_eq!(CursorAdapter::map_hook_event("afterFileEdit"), Some("afterFileEdit"));
        assert_eq!(CursorAdapter::map_hook_event("stop"), Some("stop"));
        assert_eq!(CursorAdapter::map_hook_event("Notification"), Some("stop"));
        assert_eq!(CursorAdapter::map_hook_event("pre_command"), Some("beforeShellExecution"));
        assert_eq!(CursorAdapter::map_hook_event("beforeShellExecution"), Some("beforeShellExecution"));
        assert_eq!(CursorAdapter::map_hook_event("unknown_event"), None);
    }
}
