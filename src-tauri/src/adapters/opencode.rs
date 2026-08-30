//! OpenCode deploy adapter.
//!
//! Paths (xdg-basedir semantics, same on Windows — `%USERPROFILE%\.config\opencode`
//! etc., not `%APPDATA%`, per docs/opencode-adapter-research.md):
//! - Global config: `~/.config/opencode/opencode.json` (or legacy `.jsonc`).
//! - Global data: `~/.local/share/opencode/`.
//! - Project scope: same file/dir names directly under the project root
//!   (`<project>/opencode.json`, `<project>/AGENTS.md`, `<project>/skill/…`,
//!   `<project>/agent/…`), matching OpenCode's own project-file conventions.
//!
//! JSONC hazard: OpenCode merges `.jsonc` *after* `.json` at global scope, so a
//! parallel `opencode.json` written by us would be silently overridden by an
//! existing `opencode.jsonc`. `diff()` refuses (with a rename instruction)
//! whenever `.jsonc` exists at the target scope and the selection needs the
//! JSON file (MCP/custom); rules/skills/agents are unaffected and proceed.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::adapters::traits::*;
use crate::models::{AgentDefinition, CompositeId, Skill, UniversalCapability};

pub struct OpencodeAdapter;

impl OpencodeAdapter {
    pub fn new() -> Self {
        Self
    }

    fn config_dir() -> PathBuf {
        dirs::home_dir().unwrap_or_default().join(".config").join("opencode")
    }

    fn data_dir() -> PathBuf {
        dirs::home_dir().unwrap_or_default().join(".local").join("share").join("opencode")
    }

    /// Root directory for config/rule/skill/agent artifacts at this scope:
    /// the project root when deploying per-project, `~/.config/opencode` when
    /// deploying globally (`options: {"global": true}`).
    fn scope_dir(&self, project_path: &Path, is_global: bool) -> PathBuf {
        if is_global {
            Self::config_dir()
        } else {
            project_path.to_path_buf()
        }
    }

    fn agents_md_path(&self, project_path: &Path, is_global: bool) -> PathBuf {
        self.scope_dir(project_path, is_global).join("AGENTS.md")
    }

    fn skill_dir(&self, project_path: &Path, is_global: bool) -> PathBuf {
        self.scope_dir(project_path, is_global).join("skill")
    }

    fn agent_dir(&self, project_path: &Path, is_global: bool) -> PathBuf {
        self.scope_dir(project_path, is_global).join("agent")
    }

    fn config_json_path(&self, project_path: &Path, is_global: bool) -> PathBuf {
        self.scope_dir(project_path, is_global).join("opencode.json")
    }

    fn config_jsonc_path(&self, project_path: &Path, is_global: bool) -> PathBuf {
        self.scope_dir(project_path, is_global).join("opencode.jsonc")
    }

    fn write_file_atomic(&self, path: &Path, content: &str) -> Result<(), String> {
        crate::utils::paths::atomic_write_str(path, content)
    }

    /// Dedup check: OpenCode also reads skills from `.claude/skills/` natively,
    /// so skip deploying a skill under `skill/` when the same artifact already
    /// exists there (avoid double-deploying the same skill).
    fn claude_skill_exists(&self, project_path: &Path, is_global: bool, artifact: &str) -> bool {
        let claude_dir = if is_global {
            dirs::home_dir().unwrap_or_default().join(".claude")
        } else {
            project_path.join(".claude")
        };
        claude_dir.join("skills").join(artifact).exists()
    }

    /// True when a `.jsonc` config exists at this scope — see module doc.
    fn jsonc_conflict(&self, project_path: &Path, is_global: bool) -> bool {
        self.config_jsonc_path(project_path, is_global).exists()
    }

    /// Read the current config at this scope for merging. `.json` is
    /// authoritative when present; a `.jsonc` is read tolerantly (comments and
    /// trailing commas stripped) purely so `read_config()` can report already-
    /// installed items — `diff()`/`deploy()` never write there (`jsonc_conflict`
    /// refuses first). Falls back to a freshly-seeded config with `$schema`.
    fn read_merge_base(&self, project_path: &Path, is_global: bool) -> Value {
        let json_path = self.config_json_path(project_path, is_global);
        if json_path.exists() {
            if let Ok(content) = fs::read_to_string(&json_path) {
                if let Ok(v) = serde_json::from_str::<Value>(&content) {
                    return v;
                }
            }
        }
        let jsonc_path = self.config_jsonc_path(project_path, is_global);
        if jsonc_path.exists() {
            if let Ok(content) = fs::read_to_string(&jsonc_path) {
                let stripped = strip_jsonc_comments(&content);
                if let Ok(v) = serde_json::from_str::<Value>(&stripped) {
                    return v;
                }
            }
        }
        json!({ "$schema": "https://opencode.ai/config.json" })
    }
}

impl Default for OpencodeAdapter {
    fn default() -> Self {
        Self::new()
    }
}

fn is_global_option(options: Option<&Value>) -> bool {
    options.and_then(|o| o.get("global")).and_then(|v| v.as_bool()).unwrap_or(false)
}

/// Artifact filename/dirname derivation shared by `deploy()` and `remove()` so
/// a capability removed later resolves to the same on-disk name a prior
/// deploy wrote (avoids codex.rs's algorithm-divergence bug, where deploy used
/// `id.artifact_name(name)` but remove used a different ad hoc transform).
fn artifact_name(id: &CompositeId, display_name: &str) -> String {
    id.artifact_name(display_name)
}

/// `remove()` only has a `CompositeId` (no display name), so it can't
/// reproduce a private item's exact slug. Try both the shared helper (using
/// `id.name` as a stand-in — correct for community items, where
/// `artifact_name` ignores its display-name argument) and the raw `id.name`
/// (covers legacy/community dirs named directly after the id).
///
/// `AgentAdapter::remove()` isn't wired into any command yet (true for every
/// adapter in this codebase today — see docs/creating-an-adapter.md), so this
/// helper is only reachable from tests; `dead_code` would otherwise flag it.
#[allow(dead_code)]
fn removal_candidates(id: &CompositeId) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for candidate in [artifact_name(id, &id.name), id.name.clone()] {
        if seen.insert(candidate.clone()) {
            out.push(candidate);
        }
    }
    out
}

/// Strip `//` and `/* */` comments and trailing commas from JSONC so it can
/// be parsed with `serde_json`. Comment-like sequences inside string literals
/// are left untouched.
fn strip_jsonc_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escape = false;

    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }

        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '/' if chars.peek() == Some(&'/') => {
                chars.next();
                for nc in chars.by_ref() {
                    if nc == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut prev = ' ';
                for nc in chars.by_ref() {
                    if prev == '*' && nc == '/' {
                        break;
                    }
                    prev = nc;
                }
            }
            _ => out.push(c),
        }
    }

    strip_trailing_commas(&out)
}

/// Remove a trailing comma immediately before `}` or `]` (ignoring
/// whitespace), so lenient JSONC-with-trailing-commas parses as strict JSON.
fn strip_trailing_commas(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut in_string = false;
    let mut escape = false;
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }

        if c == '"' {
            in_string = true;
            out.push(c);
            i += 1;
            continue;
        }

        if c == ',' {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if j < chars.len() && (chars[j] == '}' || chars[j] == ']') {
                i += 1;
                continue;
            }
        }

        out.push(c);
        i += 1;
    }

    out
}

/// Translate a universal MCP server into OpenCode's `mcp.<name>` shape:
/// local = `{"type":"local","command":[cmd,...args],"environment":{...}}`,
/// remote = `{"type":"remote","url":...}`. `command` is one array (no
/// separate `args`), and the env key is `environment` (not `env`).
fn build_mcp_entry(mcp: &crate::models::McpServer) -> Value {
    let transport = if mcp.transport.is_empty() { "stdio" } else { mcp.transport.as_str() };
    if transport == "stdio" {
        let mut command = vec![mcp.command.clone()];
        command.extend(mcp.args.iter().cloned());
        let mut entry = json!({
            "type": "local",
            "command": command,
        });
        if !mcp.env.is_empty() {
            let env_map: HashMap<String, String> = mcp
                .env
                .iter()
                .map(|(k, v)| (k.clone(), crate::adapters::claude_code::resolve_env_value(k, v)))
                .collect();
            entry["environment"] = json!(env_map);
        }
        entry
    } else {
        json!({
            "type": "remote",
            "url": mcp.url,
        })
    }
}

/// Build an OpenCode SKILL.md frontmatter block.
fn build_opencode_skill_frontmatter(skill: &Skill) -> String {
    let mut fm = String::from("---\n");
    fm.push_str(&format!("name: {}\n", skill.name));
    fm.push_str(&format!("description: \"{}\"\n", skill.description.replace('"', "\\\"")));
    if let Some(ref license) = skill.license {
        fm.push_str(&format!("license: {}\n", license));
    }
    fm.push_str("---\n");
    fm
}

/// Build an OpenCode agent markdown file: `description` (required), free-string
/// `model` pass-through when set (`AgentDefinition.model: Option<String>`), body = prompt.
fn generate_opencode_agent_md(agent: &AgentDefinition) -> String {
    let mut lines = Vec::new();
    lines.push("---".to_string());
    lines.push(format!(
        "description: \"{}\"",
        agent.description.replace('\\', "\\\\").replace('"', "\\\"")
    ));
    if let Some(model) = crate::models::normalize_model(agent.model.clone()) {
        lines.push(format!("model: {}", model));
    }
    lines.push("---".to_string());
    lines.push(String::new());
    lines.push(agent.prompt.clone());
    lines.join("\n")
}

impl AgentAdapter for OpencodeAdapter {
    fn id(&self) -> &str {
        "opencode"
    }

    fn name(&self) -> &str {
        "OpenCode"
    }

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            mcp: true,
            rules: true,
            skills: true,
            hooks: false,
            plugins: false,
            agents: true,
            custom: true,
        }
    }

    fn detect(&self, project_path: &Path) -> bool {
        Self::config_dir().exists()
            || Self::data_dir().exists()
            || project_path.join(".opencode").exists()
            || project_path.join("opencode.json").exists()
            || project_path.join("opencode.jsonc").exists()
            || project_path.join("AGENTS.md").exists()
    }

    fn read_config(&self, project_path: &Path) -> Result<AgentConfig, String> {
        let mut config = AgentConfig::default();

        let merged = self.read_merge_base(project_path, false);
        if let Some(mcp) = merged.get("mcp").and_then(|v| v.as_object()) {
            config.mcp_servers = mcp.keys().cloned().collect();
        }

        let skill_dir = self.skill_dir(project_path, false);
        if skill_dir.exists() {
            if let Ok(entries) = fs::read_dir(&skill_dir) {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        if let Some(name) = entry.file_name().to_str() {
                            config.skills.push(name.to_string());
                        }
                    }
                }
            }
        }

        let agent_dir = self.agent_dir(project_path, false);
        if agent_dir.exists() {
            if let Ok(entries) = fs::read_dir(&agent_dir) {
                for entry in entries.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        if let Some(stem) = name.strip_suffix(".md") {
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
        let is_global = is_global_option(options);
        let mut entries = Vec::new();

        let mcp_servers: Vec<_> = capabilities
            .iter()
            .filter_map(|c| if let UniversalCapability::Mcp(m) = c { Some(m) } else { None })
            .collect();
        let customs: Vec<_> = capabilities
            .iter()
            .filter_map(|c| if let UniversalCapability::Custom(c2) = c { Some(c2) } else { None })
            .collect();

        if (!mcp_servers.is_empty() || !customs.is_empty()) && self.jsonc_conflict(project_path, is_global) {
            let jsonc_path = self.config_jsonc_path(project_path, is_global);
            let json_path = self.config_json_path(project_path, is_global);
            return Err(format!(
                "{} exists, but AgentHarbor writes {}. OpenCode merges .jsonc after .json, \
                 so a parallel opencode.json here would be silently overridden. Rename {} to \
                 opencode.json (or move its contents there) before deploying MCP servers or custom files.",
                jsonc_path.display(),
                json_path.display(),
                jsonc_path.display(),
            ));
        }

        // -- MCP -> opencode.json ("mcp" key)
        if !mcp_servers.is_empty() {
            let json_path = self.config_json_path(project_path, is_global);
            let mut config = self.read_merge_base(project_path, is_global);
            let servers = config
                .as_object_mut()
                .ok_or("Config must be an object")?
                .entry("mcp")
                .or_insert(json!({}));
            for mcp in &mcp_servers {
                let key = artifact_name(&mcp.id, &mcp.name);
                servers[&key] = build_mcp_entry(mcp);
            }

            let current = if json_path.exists() {
                Some(fs::read_to_string(&json_path).unwrap_or_default())
            } else {
                None
            };
            let proposed = serde_json::to_string_pretty(&config).unwrap_or_default();

            entries.push(ConfigDiffEntry {
                file_path: json_path,
                change_type: if current.is_some() { ChangeType::Modify } else { ChangeType::Add },
                current_content: current,
                proposed_content: proposed,
                merged_content: None,
            });
        }

        // -- Custom -> adapter_configs["opencode"]
        for custom in &customs {
            if let Some(config) = custom.adapter_configs.get("opencode") {
                let files: Vec<Value> = if let Some(arr) = config.get("files").and_then(|v| v.as_array()) {
                    arr.clone()
                } else {
                    vec![config.clone()]
                };
                for file in files {
                    let deploy_path = file.get("deploy_path").and_then(|v| v.as_str()).unwrap_or("");
                    let content = file.get("content").and_then(|v| v.as_str()).unwrap_or("");
                    if deploy_path.is_empty() {
                        continue;
                    }
                    let full_path = self.scope_dir(project_path, is_global).join(deploy_path);
                    let current = if full_path.exists() {
                        Some(crate::utils::paths::normalize_line_endings(
                            &crate::utils::paths::read_with_sharing(&full_path).unwrap_or_default(),
                        ))
                    } else {
                        None
                    };
                    entries.push(ConfigDiffEntry {
                        file_path: full_path,
                        change_type: if current.is_some() { ChangeType::Modify } else { ChangeType::Add },
                        current_content: current,
                        proposed_content: crate::utils::paths::normalize_line_endings(content),
                        merged_content: None,
                    });
                }
            }
        }

        // -- Rules -> AGENTS.md
        let rules: Vec<_> = capabilities
            .iter()
            .filter_map(|c| if let UniversalCapability::Rule(r) = c { Some(r) } else { None })
            .collect();
        if !rules.is_empty() {
            let path = self.agents_md_path(project_path, is_global);
            let current = if path.exists() {
                Some(crate::utils::paths::normalize_line_endings(
                    &crate::utils::paths::read_with_sharing(&path).unwrap_or_default(),
                ))
            } else {
                None
            };
            let mut proposed = current.clone().unwrap_or_default();
            for rule in &rules {
                proposed = crate::utils::rule_block::inject_rule(&proposed, &rule.id.to_string(), &rule.name, &rule.content);
            }
            entries.push(ConfigDiffEntry {
                file_path: path,
                change_type: if current.is_some() { ChangeType::Modify } else { ChangeType::Add },
                current_content: current,
                proposed_content: proposed,
                merged_content: None,
            });
        }

        // -- Skills -> skill/<artifact>/SKILL.md (dedup vs .claude/skills)
        let skills: Vec<&Skill> = capabilities
            .iter()
            .filter_map(|c| if let UniversalCapability::Skill(s) = c { Some(s) } else { None })
            .collect();
        let skill_dir = self.skill_dir(project_path, is_global);
        for skill in &skills {
            let artifact = artifact_name(&skill.id, &skill.name);
            if self.claude_skill_exists(project_path, is_global, &artifact) {
                continue;
            }

            let folder = skill_dir.join(&artifact);
            let skill_md_path = folder.join("SKILL.md");
            let frontmatter = build_opencode_skill_frontmatter(skill);
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
            let proposed = format!("{}\n{}", frontmatter, body);
            let current = if skill_md_path.exists() {
                Some(fs::read_to_string(&skill_md_path).unwrap_or_default())
            } else {
                None
            };
            entries.push(ConfigDiffEntry {
                file_path: skill_md_path,
                change_type: if current.is_some() { ChangeType::Modify } else { ChangeType::Add },
                current_content: current,
                proposed_content: proposed,
                merged_content: None,
            });

            for file in &skill.files {
                let lower = file.path.to_lowercase();
                if lower == "skill.md" || lower.ends_with("/skill.md") {
                    continue;
                }
                let file_path = folder.join(&file.path);
                let current = if file_path.exists() {
                    Some(fs::read_to_string(&file_path).unwrap_or_default())
                } else {
                    None
                };
                entries.push(ConfigDiffEntry {
                    file_path,
                    change_type: if current.is_some() { ChangeType::Modify } else { ChangeType::Add },
                    current_content: current,
                    proposed_content: file.content.clone(),
                    merged_content: None,
                });
            }
        }

        // -- Agents -> agent/<artifact>.md
        let agent_dir = self.agent_dir(project_path, is_global);
        for agent in agents {
            let artifact = artifact_name(&agent.id, &agent.name);
            let path = agent_dir.join(format!("{}.md", artifact));
            let proposed = generate_opencode_agent_md(agent);
            let current = if path.exists() {
                Some(fs::read_to_string(&path).unwrap_or_default())
            } else {
                None
            };
            entries.push(ConfigDiffEntry {
                file_path: path,
                change_type: if current.is_some() { ChangeType::Modify } else { ChangeType::Add },
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
        agents: &[AgentDefinition],
        _strategy: DeployStrategy,
        options: Option<&serde_json::Value>,
    ) -> Result<DeployResult, String> {
        // Deploy writes exactly what diff() proposed, by construction — drift
        // is recorded from the diff's proposed_content, so any divergence
        // here would show as immediate phantom drift.
        let entries = self.diff(project_path, capabilities, agents, options)?;

        let mut files = Vec::new();
        let mut errors = Vec::new();
        for entry in entries {
            match self.write_file_atomic(&entry.file_path, &entry.proposed_content) {
                Ok(()) => files.push(entry.file_path),
                Err(e) => errors.push(e),
            }
        }

        if errors.is_empty() {
            Ok(DeployResult::success(files))
        } else {
            Ok(DeployResult {
                success: !files.is_empty(),
                files_written: files,
                errors,
            })
        }
    }

    fn remove(
        &self,
        project_path: &Path,
        capability_ids: &[CompositeId],
        agent_ids: &[CompositeId],
    ) -> Result<RemoveResult, String> {
        let mut removed = Vec::new();

        // The trait doesn't carry `options`, so scope (global vs project) is
        // unknown here — probe both; a non-existent path is simply skipped.
        for is_global in [false, true] {
            let skill_dir = self.skill_dir(project_path, is_global);
            for id in capability_ids {
                for candidate in removal_candidates(id) {
                    let folder = skill_dir.join(&candidate);
                    if folder.exists() && fs::remove_dir_all(&folder).is_ok() {
                        removed.push(folder);
                    }
                }
            }

            let agent_dir = self.agent_dir(project_path, is_global);
            for id in agent_ids {
                for candidate in removal_candidates(id) {
                    let path = agent_dir.join(format!("{}.md", candidate));
                    if path.exists() && fs::remove_file(&path).is_ok() {
                        removed.push(path);
                    }
                }
            }

            let agents_md = self.agents_md_path(project_path, is_global);
            if agents_md.exists() {
                if let Ok(content) = fs::read_to_string(&agents_md) {
                    let mut new_content = content.clone();
                    for id in capability_ids {
                        new_content = crate::utils::rule_block::remove_rule(&new_content, &id.to_string());
                    }
                    if new_content != content && self.write_file_atomic(&agents_md, &new_content).is_ok() {
                        removed.push(agents_md);
                    }
                }
            }

            // MCP keys — only from opencode.json; a .jsonc here was never
            // written by us (diff() refuses when it's present), so never
            // touch it.
            let json_path = self.config_json_path(project_path, is_global);
            if json_path.exists() {
                if let Ok(content) = fs::read_to_string(&json_path) {
                    if let Ok(mut config) = serde_json::from_str::<Value>(&content) {
                        let mut changed = false;
                        if let Some(mcp) = config.get_mut("mcp").and_then(|v| v.as_object_mut()) {
                            for id in capability_ids {
                                for candidate in removal_candidates(id) {
                                    if mcp.remove(&candidate).is_some() {
                                        changed = true;
                                    }
                                }
                            }
                        }
                        if changed {
                            let new_content = serde_json::to_string_pretty(&config).unwrap_or_default();
                            if self.write_file_atomic(&json_path, &new_content).is_ok() {
                                removed.push(json_path);
                            }
                        }
                    }
                }
            }
        }

        Ok(RemoveResult::success(removed))
    }

    fn managed_paths(&self, project_path: &Path) -> Vec<PathBuf> {
        vec![
            self.config_json_path(project_path, false),
            self.agents_md_path(project_path, false),
            self.skill_dir(project_path, false),
            self.agent_dir(project_path, false),
            Self::config_dir(),
            Self::data_dir(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AgentColor, MemoryScope, McpServer, Rule, SkillFile, ToolAccess, Visibility};
    use tempfile::TempDir;

    fn create_test_agent() -> AgentDefinition {
        AgentDefinition {
            artifact: None,
            id: CompositeId::new("test", "test-agent").unwrap(),
            name: "Test Agent".to_string(),
            description: "A test agent".to_string(),
            version: "1.0.0".to_string(),
            author: "test".to_string(),
            visibility: Visibility::Private,
            tags: vec![],
            model: Some("sonnet".to_string()),
            color: AgentColor::Blue,
            memory: MemoryScope::None,
            tools: vec![ToolAccess::All],
            required_capabilities: vec![],
            prompt: "You are a test agent.".to_string(),
            examples: vec![],
        }
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
            env: HashMap::new(),
            compatible_agents: vec!["opencode".to_string()],
            allowed_tools: None,
            model: None,
            context: None,
            agent: None,
            argument_hint: None,
            license: None,
            category: None,
            author_github: None,
            source_info: None,
            stats: None,
        })
    }

    fn create_test_mcp() -> UniversalCapability {
        UniversalCapability::Mcp(McpServer {
            id: CompositeId::new("community", "test-mcp").unwrap(),
            name: "Test MCP".to_string(),
            description: "Test MCP server".to_string(),
            version: "1.0.0".to_string(),
            author: "test".to_string(),
            visibility: Visibility::Public,
            tags: vec![],
            compatible_agents: vec!["opencode".to_string()],
            disabled: None,
            always_allow: None,
            disabled_tools: None,
            tool_list: None,
            transport: "stdio".to_string(),
            command: "npx".to_string(),
            args: vec!["-y".to_string(), "@test/mcp".to_string()],
            url: String::new(),
            env: HashMap::new(),
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
            compatible_agents: vec!["opencode".to_string()],
            scope: "project".to_string(),
            content: "Always be helpful.".to_string(),
            env: HashMap::new(),
            category: None,
            author_github: None,
            source_info: None,
            stats: None,
        })
    }

    // ── strip_jsonc_comments ──────────────────────────────────────────────────

    #[test]
    fn strips_line_and_block_comments() {
        let input = "{\n  // a comment\n  \"a\": 1, /* inline */ \"b\": 2\n}";
        let stripped = strip_jsonc_comments(input);
        let parsed: Value = serde_json::from_str(&stripped).unwrap();
        assert_eq!(parsed["a"], 1);
        assert_eq!(parsed["b"], 2);
    }

    #[test]
    fn strips_trailing_commas() {
        let input = "{\"a\": 1, \"b\": [1, 2, 3,],}";
        let stripped = strip_jsonc_comments(input);
        let parsed: Value = serde_json::from_str(&stripped).unwrap();
        assert_eq!(parsed["a"], 1);
        assert_eq!(parsed["b"][2], 3);
    }

    #[test]
    fn leaves_comment_like_sequences_inside_strings_alone() {
        let input = r#"{"url": "https://example.com//path", "note": "trailing, comma, style"}"#;
        let stripped = strip_jsonc_comments(input);
        let parsed: Value = serde_json::from_str(&stripped).unwrap();
        assert_eq!(parsed["url"], "https://example.com//path");
    }

    // ── MCP translation ───────────────────────────────────────────────────────

    #[test]
    fn mcp_local_translates_to_command_array_and_environment_key() {
        let mcp = McpServer {
            id: CompositeId::new("community", "test-mcp").unwrap(),
            name: "Test MCP".into(),
            description: String::new(),
            version: "1.0.0".into(),
            author: "test".into(),
            visibility: Visibility::Public,
            tags: vec![],
            compatible_agents: vec![],
            disabled: None,
            always_allow: None,
            disabled_tools: None,
            tool_list: None,
            transport: "stdio".into(),
            command: "bun".into(),
            args: vec!["x".into(), "server".into()],
            url: String::new(),
            env: HashMap::new(),
            category: None,
            author_github: None,
            source_info: None,
            stats: None,
        };
        let entry = build_mcp_entry(&mcp);
        assert_eq!(entry["type"], "local");
        assert_eq!(entry["command"], json!(["bun", "x", "server"]));
        assert!(entry.get("environment").is_none());
    }

    #[test]
    fn mcp_remote_translates_to_url() {
        let mut mcp_cap = create_test_mcp();
        if let UniversalCapability::Mcp(ref mut m) = mcp_cap {
            m.transport = "sse".into();
            m.url = "https://example.com/mcp".into();
        }
        let UniversalCapability::Mcp(mcp) = mcp_cap else { unreachable!() };
        let entry = build_mcp_entry(&mcp);
        assert_eq!(entry["type"], "remote");
        assert_eq!(entry["url"], "https://example.com/mcp");
    }

    // ── opencode.json merge preserves $schema + foreign keys ─────────────────

    #[test]
    fn deploy_seeds_schema_key_on_new_config() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = OpencodeAdapter::new();
        let result = adapter.deploy(temp_dir.path(), &[create_test_mcp()], &[], DeployStrategy::Merge, None).unwrap();
        assert!(result.success);
        let content = fs::read_to_string(temp_dir.path().join("opencode.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["$schema"], "https://opencode.ai/config.json");
        assert!(parsed["mcp"]["test-mcp"].is_object());
    }

    #[test]
    fn merge_preserves_unrelated_top_level_keys() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(
            temp_dir.path().join("opencode.json"),
            r#"{"$schema": "https://opencode.ai/config.json", "permission": {"bash": "ask"}}"#,
        )
        .unwrap();
        let adapter = OpencodeAdapter::new();
        adapter.deploy(temp_dir.path(), &[create_test_mcp()], &[], DeployStrategy::Merge, None).unwrap();
        let content = fs::read_to_string(temp_dir.path().join("opencode.json")).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["permission"]["bash"], "ask");
        assert!(parsed["mcp"]["test-mcp"].is_object());
    }

    // ── JSONC read-tolerance + write-refusal ──────────────────────────────────

    #[test]
    fn read_config_tolerates_jsonc_with_comments() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(
            temp_dir.path().join("opencode.jsonc"),
            "{\n  // schema\n  \"$schema\": \"https://opencode.ai/config.json\",\n  \"mcp\": {\"existing\": {\"type\": \"local\", \"command\": [\"x\"],},},\n}",
        )
        .unwrap();
        let adapter = OpencodeAdapter::new();
        let config = adapter.read_config(temp_dir.path()).unwrap();
        assert!(config.mcp_servers.contains(&"existing".to_string()));
    }

    #[test]
    fn diff_refuses_mcp_when_jsonc_exists_at_scope() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("opencode.jsonc"), "{\"$schema\": \"https://opencode.ai/config.json\"}").unwrap();
        let adapter = OpencodeAdapter::new();
        let result = adapter.diff(temp_dir.path(), &[create_test_mcp()], &[], None);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("opencode.jsonc"));
        assert!(msg.contains("opencode.json"));
    }

    #[test]
    fn diff_allows_rules_and_skills_even_with_jsonc_present() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("opencode.jsonc"), "{}").unwrap();
        let adapter = OpencodeAdapter::new();
        let result = adapter.diff(temp_dir.path(), &[create_test_rule(), create_test_skill()], &[], None);
        assert!(result.is_ok());
        assert!(!result.unwrap().is_empty());
    }

    // ── Rule inject/remove roundtrip ──────────────────────────────────────────

    #[test]
    fn rule_deploy_then_remove_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = OpencodeAdapter::new();
        let rule_cap = create_test_rule();
        let rule_id = rule_cap.id().clone();

        adapter.deploy(temp_dir.path(), &[rule_cap], &[], DeployStrategy::Merge, None).unwrap();
        let agents_md = temp_dir.path().join("AGENTS.md");
        assert!(agents_md.exists());
        let content = fs::read_to_string(&agents_md).unwrap();
        assert!(content.contains("Always be helpful."));

        adapter.remove(temp_dir.path(), &[rule_id], &[]).unwrap();
        let content_after = fs::read_to_string(&agents_md).unwrap();
        assert!(!content_after.contains("Always be helpful."));
    }

    // ── Skill dedup skip ───────────────────────────────────────────────────────

    #[test]
    fn skill_deploy_skipped_when_claude_skill_already_exists() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = OpencodeAdapter::new();
        let skill_cap = create_test_skill();
        let UniversalCapability::Skill(ref skill) = skill_cap else { unreachable!() };
        let artifact = artifact_name(&skill.id, &skill.name);

        fs::create_dir_all(temp_dir.path().join(".claude").join("skills").join(&artifact)).unwrap();

        let diffs = adapter.diff(temp_dir.path(), &[skill_cap], &[], None).unwrap();
        assert!(diffs.iter().all(|d| !d.file_path.to_string_lossy().contains("skill")));
    }

    #[test]
    fn skill_deploy_proceeds_when_no_claude_skill_exists() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = OpencodeAdapter::new();
        let diffs = adapter.diff(temp_dir.path(), &[create_test_skill()], &[], None).unwrap();
        let skill_md = diffs.iter().find(|d| d.file_path.ends_with("SKILL.md"));
        assert!(skill_md.is_some());
        assert!(skill_md.unwrap().file_path.starts_with(temp_dir.path().join("skill")));
    }

    // ── Deploy/remove artifact symmetry ────────────────────────────────────────

    #[test]
    fn agent_deploy_then_remove_symmetry() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = OpencodeAdapter::new();
        let agent = create_test_agent();
        let agent_id = agent.id.clone();

        let result = adapter.deploy(temp_dir.path(), &[], &[agent], DeployStrategy::Merge, None).unwrap();
        assert!(result.success);
        assert_eq!(result.files_written.len(), 1);
        let agent_path = &result.files_written[0];
        assert!(agent_path.exists());

        adapter.remove(temp_dir.path(), &[], &[agent_id]).unwrap();
        assert!(!agent_path.exists());
    }

    #[test]
    fn community_skill_deploy_then_remove_symmetry() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = OpencodeAdapter::new();
        let skill_cap = create_test_skill();
        let skill_id = skill_cap.id().clone();

        let result = adapter.deploy(temp_dir.path(), &[skill_cap], &[], DeployStrategy::Merge, None).unwrap();
        assert!(result.success);
        let skill_folder = temp_dir.path().join("skill").join("test-skill");
        assert!(skill_folder.exists());

        adapter.remove(temp_dir.path(), &[skill_id], &[]).unwrap();
        assert!(!skill_folder.exists());
    }

    #[test]
    fn mcp_deploy_then_remove_symmetry() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = OpencodeAdapter::new();
        let mcp_cap = create_test_mcp();
        let mcp_id = mcp_cap.id().clone();

        adapter.deploy(temp_dir.path(), &[mcp_cap], &[], DeployStrategy::Merge, None).unwrap();
        let json_path = temp_dir.path().join("opencode.json");
        let content = fs::read_to_string(&json_path).unwrap();
        assert!(content.contains("test-mcp"));

        adapter.remove(temp_dir.path(), &[mcp_id], &[]).unwrap();
        let content_after = fs::read_to_string(&json_path).unwrap();
        assert!(!content_after.contains("test-mcp"));
    }

    // ── Global vs project targets ──────────────────────────────────────────────

    #[test]
    fn global_option_deploys_under_config_dir_home_override() {
        // `scope_dir` resolves `Self::config_dir()` from `dirs::home_dir()`,
        // which isn't overridable per-test; instead verify the project-scope
        // path stays under project_path, and that global mode is a distinct
        // path (not project_path) for the same capability.
        let temp_dir = TempDir::new().unwrap();
        let adapter = OpencodeAdapter::new();
        let rule_cap = create_test_rule();

        let project_diffs = adapter.diff(temp_dir.path(), std::slice::from_ref(&rule_cap), &[], None).unwrap();
        let project_path = &project_diffs[0].file_path;
        assert!(project_path.starts_with(temp_dir.path()));

        let global_opts = json!({ "global": true });
        let global_diffs = adapter.diff(temp_dir.path(), &[rule_cap], &[], Some(&global_opts)).unwrap();
        let global_path = &global_diffs[0].file_path;
        assert!(!global_path.starts_with(temp_dir.path()));
        assert!(global_path.to_string_lossy().contains(".config"));
        assert!(global_path.to_string_lossy().contains("opencode"));
    }

    #[test]
    fn test_adapter_id_and_name() {
        let adapter = OpencodeAdapter::new();
        assert_eq!(adapter.id(), "opencode");
        assert_eq!(adapter.name(), "OpenCode");
    }

    #[test]
    fn test_adapter_capabilities() {
        let adapter = OpencodeAdapter::new();
        let caps = adapter.capabilities();
        assert!(caps.mcp);
        assert!(caps.rules);
        assert!(caps.skills);
        assert!(caps.agents);
        assert!(caps.custom);
        assert!(!caps.hooks);
        assert!(!caps.plugins);
    }

    #[test]
    fn test_detect_with_project_markers() {
        let temp_dir = TempDir::new().unwrap();
        let adapter = OpencodeAdapter::new();
        // Home-dir global paths may or may not exist on the test machine, so
        // only assert the *positive* project-marker case here.
        fs::write(temp_dir.path().join("opencode.json"), "{}").unwrap();
        assert!(adapter.detect(temp_dir.path()));
    }
}
