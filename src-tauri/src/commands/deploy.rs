use crate::adapters::{AdapterRegistry, ConfigDiffEntry, DeployResult, DeployStrategy};
use crate::models::{AgentDefinition, UniversalCapability};
use crate::registry::{get_bundled_registry_path, get_community_registry_path, load_agents, load_capabilities};
use crate::utils::drift;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffEntry {
    pub file_path: String,
    pub change_type: String,
    pub current_content: Option<String>,
    pub proposed_content: String,
}

impl From<ConfigDiffEntry> for DiffEntry {
    fn from(entry: ConfigDiffEntry) -> Self {
        use crate::utils::paths::normalize_line_endings;
        Self {
            file_path: entry.file_path.to_string_lossy().to_string(),
            change_type: match entry.change_type {
                crate::adapters::ChangeType::Add => "add".to_string(),
                crate::adapters::ChangeType::Modify => "modify".to_string(),
                crate::adapters::ChangeType::Remove => "remove".to_string(),
            },
            current_content: entry.current_content.map(|c| normalize_line_endings(&c)),
            proposed_content: normalize_line_endings(&entry.proposed_content),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployResultResponse {
    pub success: bool,
    pub files_written: Vec<String>,
    pub errors: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

impl From<DeployResult> for DeployResultResponse {
    fn from(result: DeployResult) -> Self {
        Self {
            success: result.success,
            files_written: result.files_written.iter().map(|p| p.to_string_lossy().to_string()).collect(),
            errors: result.errors,
            warnings: vec![],
        }
    }
}

fn get_all_registry_paths() -> Vec<PathBuf> {
    let mut dirs = vec![get_bundled_registry_path()];
    
    let community_path = get_community_registry_path();
    if community_path.exists() {
        dirs.push(community_path);
    }
    
    let custom_path = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.agentharbor.app")
        .join("registry")
        .join("custom");
    if custom_path.exists() {
        dirs.push(custom_path);
    }
    
    dirs
}

fn get_capabilities_by_ids(ids: &[String]) -> Vec<UniversalCapability> {
    let registry_paths = get_all_registry_paths();
    let all_caps = load_capabilities(&registry_paths).items;
    
    all_caps
        .into_iter()
        .filter(|cap| {
            let cap_id = cap.id();
            ids.contains(&cap_id.to_string())
        })
        .collect()
}

fn get_agents_by_ids(ids: &[String]) -> Vec<AgentDefinition> {
    let registry_paths = get_all_registry_paths();
    let all_agents = load_agents(&registry_paths).items;
    
    all_agents
        .into_iter()
        .filter(|agent| ids.contains(&agent.id.to_string()))
        .collect()
}

/// Collect env key -> value from all capabilities (for .env merge).
/// Tries EnvVariable.value first, falls back to label if it's not a placeholder.
fn collect_env_from_capabilities(capabilities: &[UniversalCapability]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for cap in capabilities {
        let env_map = match cap {
            UniversalCapability::Mcp(m) => &m.env,
            UniversalCapability::Rule(r) => &r.env,
            UniversalCapability::Skill(s) => &s.env,
            UniversalCapability::Hook(h) => &h.env,
            UniversalCapability::Plugin(p) => &p.env,
            UniversalCapability::Custom(c) => &c.env,
        };
        for (k, v) in env_map {
            if !k.trim().is_empty() {
                let key = k.trim().to_string();
                // Prefer explicit value field
                let val = if let Some(ref val) = v.value {
                    if !val.is_empty() { val.clone() } else { String::new() }
                } else {
                    // Fallback: use label if it's not a placeholder like ${KEY}
                    let label = &v.label;
                    if !label.is_empty() && !label.starts_with("${") && !label.contains("${") {
                        label.clone()
                    } else {
                        String::new()
                    }
                };
                out.insert(key, val);
            }
        }
    }
    out
}

/// Parse existing .env: lines like KEY=value (and KEY= for empty). Returns map key -> value.
fn parse_env_file(content: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let key = k.trim().to_string();
            let val = v.trim().to_string();
            out.insert(key, val);
        }
    }
    out
}

/// Merge new env into existing .env content and return new content. New keys appended; existing keys updated in place when possible.
fn merge_env_into_content(existing: &str, new: &HashMap<String, String>) -> String {
    let mut parsed = parse_env_file(existing);
    for (k, v) in new {
        parsed.insert(k.clone(), v.clone());
    }
    let mut lines: Vec<String> = Vec::new();
    let mut emitted = std::collections::HashSet::new();
    for line in existing.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            lines.push(line.to_string());
            continue;
        }
        if let Some((k, _)) = line.split_once('=') {
            let key = k.trim().to_string();
            if let Some(val) = parsed.get(&key) {
                emitted.insert(key.clone());
                lines.push(format!("{}={}", key, val));
            } else {
                lines.push(line.to_string());
            }
        }
    }
    for (k, v) in &parsed {
        if !emitted.contains(k) {
            lines.push(format!("{}={}", k, v));
        }
    }
    lines.join("\n") + "\n"
}

/// Write merged env vars to project_path/.env (create if missing).
fn merge_env_into_project(project_path: &Path, capabilities: &[UniversalCapability]) -> Result<(), String> {
    let new_vars = collect_env_from_capabilities(capabilities);
    if new_vars.is_empty() {
        return Ok(());
    }
    let env_path = project_path.join(".env");
    let existing = if env_path.exists() {
        fs::read_to_string(&env_path).map_err(|e| format!("Failed to read .env: {}", e))?
    } else {
        String::new()
    };
    let new_content = merge_env_into_content(&existing, &new_vars);
    if let Some(parent) = env_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create parent for .env: {}", e))?;
    }
    fs::write(&env_path, new_content).map_err(|e| format!("Failed to write .env: {}", e))?;
    Ok(())
}

#[tauri::command]
pub fn preview_deploy(
    project_path: String,
    adapter_id: String,
    capability_ids: Vec<String>,
    agent_ids: Vec<String>,
    claude_settings_target: Option<String>,
    global_deploy: Option<bool>,
) -> Result<Vec<DiffEntry>, String> {
    let is_global = global_deploy.unwrap_or(false);
    let registry = AdapterRegistry::new();
    let adapter = registry
        .get(&adapter_id)
        .ok_or_else(|| format!("Adapter '{}' not found", adapter_id))?;

    let path = if is_global {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
    } else {
        PathBuf::from(&project_path)
    };

    let capabilities = get_capabilities_by_ids(&capability_ids);
    // Agents are project-scoped — exclude from global deploys
    let agents = if is_global { vec![] } else { get_agents_by_ids(&agent_ids) };

    let options = {
        let mut opts = serde_json::Map::new();
        if is_global {
            opts.insert("global".to_string(), json!(true));
        }
        // For Claude Code global deploy, always use user (global) settings target
        let cc_target = if adapter_id == "claude-code" {
            if is_global { Some("user".to_string()) } else { claude_settings_target.clone() }
        } else {
            None
        };
        if let Some(ref t) = cc_target {
            opts.insert("claude_settings_target".to_string(), json!(t));
        }
        if opts.is_empty() { None } else { Some(Value::Object(opts)) }
    };

    let mut diffs: Vec<DiffEntry> = adapter.diff(&path, &capabilities, &agents, options.as_ref())?
        .into_iter()
        .map(DiffEntry::from)
        .collect();

    // Skip .env diff for global deploys (project-specific)
    if !is_global {
        let new_env = collect_env_from_capabilities(&capabilities);
        if !new_env.is_empty() {
            let env_path = path.join(".env");
            let current = if env_path.exists() {
                fs::read_to_string(&env_path).ok()
            } else {
                None
            };
            let proposed = merge_env_into_content(current.as_deref().unwrap_or(""), &new_env);
            diffs.push(DiffEntry {
                file_path: env_path.to_string_lossy().to_string(),
                change_type: if current.is_some() { "modify".to_string() } else { "add".to_string() },
                current_content: current,
                proposed_content: proposed,
            });
        }
    }

    Ok(diffs)
}

#[tauri::command]
pub fn execute_deploy(
    project_path: String,
    adapter_id: String,
    capability_ids: Vec<String>,
    agent_ids: Vec<String>,
    strategies: HashMap<String, String>,
    claude_settings_target: Option<String>,
    global_deploy: Option<bool>,
) -> Result<DeployResultResponse, String> {
    let is_global = global_deploy.unwrap_or(false);
    let registry = AdapterRegistry::new();
    let adapter = registry
        .get(&adapter_id)
        .ok_or_else(|| format!("Adapter '{}' not found", adapter_id))?;

    let path = if is_global {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
    } else {
        PathBuf::from(&project_path)
    };

    let capabilities = get_capabilities_by_ids(&capability_ids);
    let agents = if is_global { vec![] } else { get_agents_by_ids(&agent_ids) };

    let strategy = if strategies.values().any(|s| s == "overwrite") {
        DeployStrategy::Overwrite
    } else if strategies.values().any(|s| s == "skip") {
        DeployStrategy::Skip
    } else {
        DeployStrategy::Merge
    };

    let options = {
        let mut opts = serde_json::Map::new();
        if is_global {
            opts.insert("global".to_string(), json!(true));
        }
        let cc_target = if adapter_id == "claude-code" {
            if is_global { Some("user".to_string()) } else { claude_settings_target.clone() }
        } else {
            None
        };
        if let Some(ref t) = cc_target {
            opts.insert("claude_settings_target".to_string(), json!(t));
        }
        if opts.is_empty() { None } else { Some(Value::Object(opts)) }
    };

    let diffs = adapter.diff(&path, &capabilities, &agents, options.as_ref())?;
    let result = adapter.deploy(&path, &capabilities, &agents, strategy, options.as_ref())?;

    let mut response = DeployResultResponse::from(result);

    if response.success && !is_global {
        let mut deployed_files: HashMap<String, String> = HashMap::new();
        for diff in &diffs {
            let relative_path = match diff.file_path.strip_prefix(&path) {
                Ok(rel) => rel.to_string_lossy().to_string(),
                Err(_) => {
                    eprintln!("[execute_deploy] Warning: file {} is outside project root", diff.file_path.display());
                    diff.file_path.to_string_lossy().to_string()
                }
            };
            deployed_files.insert(relative_path, diff.proposed_content.clone());
        }

        if let Err(e) = merge_env_into_project(&path, &capabilities) {
            response.warnings.push(format!("Failed to merge .env: {}", e));
        }

        if let Err(e) = drift::save_deploy_state(&project_path, &deployed_files) {
            response.warnings.push(format!("Failed to save drift state: {}", e));
        }
    }

    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_preview_deploy_empty() {
        let temp_dir = TempDir::new().unwrap();
        let result = preview_deploy(
            temp_dir.path().to_string_lossy().to_string(),
            "claude-code".to_string(),
            vec![],
            vec![],
            None,
            None,
        );
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_preview_deploy_invalid_adapter() {
        let temp_dir = TempDir::new().unwrap();
        let result = preview_deploy(
            temp_dir.path().to_string_lossy().to_string(),
            "invalid".to_string(),
            vec![],
            vec![],
            None,
            None,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_execute_deploy_empty() {
        let temp_dir = TempDir::new().unwrap();
        let result = execute_deploy(
            temp_dir.path().to_string_lossy().to_string(),
            "claude-code".to_string(),
            vec![],
            vec![],
            HashMap::new(),
            None,
            None,
        );
        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.success);
    }

    #[test]
    fn test_collect_env_with_value_field() {
        use crate::models::{McpServer, EnvVariable, Visibility, CompositeId};
        let mcp = UniversalCapability::Mcp(McpServer {
            id: CompositeId::new("test", "test-mcp").unwrap(),
            name: "Test".to_string(),
            description: "Test".to_string(),
            version: "1.0.0".to_string(),
            author: "test".to_string(),
            visibility: Visibility::Private,
            tags: vec![],
            transport: "stdio".to_string(),
            command: "npx".to_string(),
            args: vec![],
            url: String::new(),
            env: {
                let mut m = HashMap::new();
                m.insert("API_KEY".to_string(), EnvVariable {
                    var_type: "secret".to_string(),
                    label: "${API_KEY}".to_string(), // placeholder
                    required: false,
                    value: Some("my-secret-key-123".to_string()), // actual value
                });
                m
            },
            compatible_agents: vec![],
            disabled: None,
            always_allow: None,
            disabled_tools: None,
            tool_list: None,
            category: None,
            author_github: None,
            source_info: None,
            stats: None,
        });

        let env = collect_env_from_capabilities(&[mcp]);
        assert_eq!(env.get("API_KEY").unwrap(), "my-secret-key-123");
    }

    #[test]
    fn test_collect_env_fallback_to_label() {
        use crate::models::{McpServer, EnvVariable, Visibility, CompositeId};
        let mcp = UniversalCapability::Mcp(McpServer {
            id: CompositeId::new("test", "test-mcp").unwrap(),
            name: "Test".to_string(),
            description: "Test".to_string(),
            version: "1.0.0".to_string(),
            author: "test".to_string(),
            visibility: Visibility::Private,
            tags: vec![],
            transport: "stdio".to_string(),
            command: "npx".to_string(),
            args: vec![],
            url: String::new(),
            env: {
                let mut m = HashMap::new();
                // value is None, label is not a placeholder — should use label
                m.insert("API_KEY".to_string(), EnvVariable {
                    var_type: "secret".to_string(),
                    label: "my-actual-key".to_string(),
                    required: false,
                    value: None,
                });
                m
            },
            compatible_agents: vec![],
            disabled: None,
            always_allow: None,
            disabled_tools: None,
            tool_list: None,
            category: None,
            author_github: None,
            source_info: None,
            stats: None,
        });

        let env = collect_env_from_capabilities(&[mcp]);
        assert_eq!(env.get("API_KEY").unwrap(), "my-actual-key");
    }

    #[test]
    fn test_collect_env_placeholder_label_gives_empty() {
        use crate::models::{McpServer, EnvVariable, Visibility, CompositeId};
        let mcp = UniversalCapability::Mcp(McpServer {
            id: CompositeId::new("test", "test-mcp").unwrap(),
            name: "Test".to_string(),
            description: "Test".to_string(),
            version: "1.0.0".to_string(),
            author: "test".to_string(),
            visibility: Visibility::Private,
            tags: vec![],
            transport: "stdio".to_string(),
            command: "npx".to_string(),
            args: vec![],
            url: String::new(),
            env: {
                let mut m = HashMap::new();
                // value is None, label is a placeholder — should be empty
                m.insert("API_KEY".to_string(), EnvVariable {
                    var_type: "secret".to_string(),
                    label: "${API_KEY}".to_string(),
                    required: false,
                    value: None,
                });
                m
            },
            compatible_agents: vec![],
            disabled: None,
            always_allow: None,
            disabled_tools: None,
            tool_list: None,
            category: None,
            author_github: None,
            source_info: None,
            stats: None,
        });

        let env = collect_env_from_capabilities(&[mcp]);
        assert_eq!(env.get("API_KEY").unwrap(), "");
    }

    #[test]
    fn test_merge_env_into_content() {
        let existing = "EXISTING_KEY=old_value\n";
        let mut new_vars = HashMap::new();
        new_vars.insert("EXISTING_KEY".to_string(), "new_value".to_string());
        new_vars.insert("NEW_KEY".to_string(), "new_key_value".to_string());
        let result = merge_env_into_content(existing, &new_vars);
        assert!(result.contains("EXISTING_KEY=new_value"));
        assert!(result.contains("NEW_KEY=new_key_value"));
    }
}
