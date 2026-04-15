use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

fn home_dir() -> Option<PathBuf> {
    dirs::home_dir()
}

/// Path to Claude desktop app config (macOS: ~/Library/Application Support/Claude/claude_desktop_config.json)
fn claude_desktop_config_path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("Claude").join("claude_desktop_config.json"))
}

fn claude_settings_path() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".claude").join("settings.json"))
}

fn claude_memory_path() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".claude").join("CLAUDE.md"))
}

fn global_config_path(adapter_id: &str) -> Option<PathBuf> {
    let home = home_dir()?;
    match adapter_id {
        "claude-code" => Some(home.join(".claude.json")),
        "cursor" => Some(home.join(".cursor").join("mcp.json")),
        "windsurf" => Some(home.join(".codeium").join("windsurf").join("mcp_config.json")),
        "codex" => Some(home.join(".codex").join("config.toml")),
        _ => None,
    }
}

fn write_atomic(path: &PathBuf, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory: {}", e))?;
    }
    crate::utils::paths::atomic_write_str(path, content)
}

#[tauri::command]
pub fn read_claude_settings() -> Result<String, String> {
    let path = claude_settings_path().ok_or("Could not resolve path")?;
    if !path.exists() {
        return Ok("{}".to_string());
    }
    fs::read_to_string(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn write_claude_settings(content: String) -> Result<(), String> {
    let path = claude_settings_path().ok_or("Could not resolve path")?;
    serde_json::from_str::<Value>(&content).map_err(|e| format!("Invalid JSON: {}", e))?;
    write_atomic(&path, &content)
}

#[tauri::command]
pub fn read_claude_memory() -> Result<String, String> {
    let path = claude_memory_path().ok_or("Could not resolve path")?;
    if !path.exists() {
        return Ok(String::new());
    }
    fs::read_to_string(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn write_claude_memory(content: String) -> Result<(), String> {
    let path = claude_memory_path().ok_or("Could not resolve path")?;
    write_atomic(&path, &content)
}

#[tauri::command]
pub fn read_claude_desktop_config() -> Result<String, String> {
    let path = claude_desktop_config_path().ok_or("Could not resolve path")?;
    if !path.exists() {
        return Ok("{}".to_string());
    }
    fs::read_to_string(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn write_claude_desktop_config(content: String) -> Result<(), String> {
    let path = claude_desktop_config_path().ok_or("Could not resolve path")?;
    serde_json::from_str::<Value>(&content).map_err(|e| format!("Invalid JSON: {}", e))?;
    write_atomic(&path, &content)
}

#[tauri::command]
pub fn get_claude_desktop_config_path() -> Result<String, String> {
    claude_desktop_config_path()
        .and_then(|p| p.to_str().map(String::from))
        .ok_or_else(|| "Could not resolve path".to_string())
}

#[tauri::command]
pub fn read_global_config_raw(adapter_id: String) -> Result<String, String> {
    let path = global_config_path(&adapter_id).ok_or_else(|| format!("Unknown adapter: {}", adapter_id))?;
    if !path.exists() {
        return Ok(if adapter_id == "claude-code" {
            r#"{"mcpServers":{}}"#.to_string()
        } else {
            "{}".to_string()
        });
    }
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let json: Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    if adapter_id == "claude-code" {
        let mcp_only = json.get("mcpServers").cloned().unwrap_or(json!({}));
        let out = serde_json::json!({ "mcpServers": mcp_only });
        Ok(serde_json::to_string_pretty(&out).unwrap_or_else(|_| "{}".to_string()))
    } else {
        Ok(serde_json::to_string_pretty(&json).unwrap_or_else(|_| content))
    }
}

#[tauri::command]
pub fn write_global_config_raw(adapter_id: String, content: String) -> Result<(), String> {
    let path = global_config_path(&adapter_id).ok_or_else(|| format!("Unknown adapter: {}", adapter_id))?;
    let new_json: Value = serde_json::from_str(&content).map_err(|e| format!("Invalid JSON: {}", e))?;
    if adapter_id == "claude-code" {
        let mcp_servers = new_json.get("mcpServers").cloned().unwrap_or(json!({}));
        let mut full: Value = if path.exists() {
            let s = fs::read_to_string(&path).map_err(|e| e.to_string())?;
            serde_json::from_str(&s).unwrap_or(json!({}))
        } else {
            json!({})
        };
        full["mcpServers"] = mcp_servers;
        let out = serde_json::to_string_pretty(&full).map_err(|e| e.to_string())?;
        write_atomic(&path, &out)
    } else {
        write_atomic(&path, &content)
    }
}

#[tauri::command]
pub fn add_global_mcp_server(
    adapter_id: String,
    name: String,
    config: Value,
) -> Result<(), String> {
    let path = global_config_path(&adapter_id).ok_or_else(|| format!("Unknown adapter: {}", adapter_id))?;
    let mut json: Value = if path.exists() {
        let s = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str(&s).map_err(|e| e.to_string())?
    } else {
        json!({})
    };
    let obj = json.as_object_mut().ok_or("Invalid config")?;
    let mcp = obj.entry("mcpServers").or_insert_with(|| json!({}));
    let mcp_obj = mcp.as_object_mut().ok_or("mcpServers must be an object")?;
    mcp_obj.insert(name, config);
    let out = serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?;
    write_atomic(&path, &out)
}

#[tauri::command]
pub fn remove_global_mcp_server(adapter_id: String, name: String) -> Result<(), String> {
    let path = global_config_path(&adapter_id).ok_or_else(|| format!("Unknown adapter: {}", adapter_id))?;
    if !path.exists() {
        return Ok(());
    }
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut json: Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    if let Some(mcp) = json.get_mut("mcpServers").and_then(|v| v.as_object_mut()) {
        mcp.remove(&name);
    }
    let out = serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?;
    write_atomic(&path, &out)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredCapability {
    pub name: String,
    pub description: String,
    pub transport: String,
    pub command: String,
    pub args: Vec<String>,
    pub url: String,
    #[serde(default)]
    pub env: HashMap<String, Value>,
    pub source: String,
    pub adapter_id: String,
    #[serde(default)]
    pub adapter_ids: Vec<String>,
}

fn parse_mcp_entry(name: &str, v: &Value, source: &str, adapter_id: &str) -> Option<DiscoveredCapability> {
    let obj = v.as_object()?;
    let transport = obj
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("stdio")
        .to_string();
    let command = obj.get("command").and_then(|c| c.as_str()).unwrap_or("").to_string();
    let args: Vec<String> = obj
        .get("args")
        .and_then(|a| a.as_array())
        .map(|arr| arr.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let url = obj.get("url").and_then(|u| u.as_str()).unwrap_or("").to_string();
    let env: HashMap<String, Value> = obj
        .get("env")
        .and_then(|e| e.as_object())
        .map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();
    Some(DiscoveredCapability {
        name: name.to_string(),
        description: source.to_string(),
        transport,
        command,
        args,
        url,
        env,
        source: source.to_string(),
        adapter_id: adapter_id.to_string(),
        adapter_ids: vec![adapter_id.to_string()],
    })
}

fn collect_mcp_servers_from_value(
    root: &Value,
    source: &str,
    adapter_id: &str,
    seen: &mut std::collections::HashMap<String, usize>,
    out: &mut Vec<DiscoveredCapability>,
) {
    let mcp = root.get("mcpServers").and_then(|v| v.as_object());
    let mcp = match mcp {
        Some(m) => m,
        None => return,
    };
    for (name, config) in mcp {
        if let Some(cap) = parse_mcp_entry(name, config, source, adapter_id) {
            let key = format!(
                "{}|{}|{}|{}",
                source,
                cap.name.to_lowercase(),
                cap.command.to_lowercase(),
                cap.url.to_lowercase()
            );
            if let Some(&idx) = seen.get(&key) {
                if !out[idx].adapter_ids.contains(&adapter_id.to_string()) {
                    out[idx].adapter_ids.push(adapter_id.to_string());
                }
            } else {
                seen.insert(key, out.len());
                out.push(cap);
            }
        }
    }
}

#[tauri::command]
pub fn discover_capabilities() -> Result<Vec<DiscoveredCapability>, String> {
    let home = home_dir().ok_or("Could not determine home directory")?;
    let mut seen = HashMap::new();
    let mut out = Vec::new();

    let claude_json_path = home.join(".claude.json");
    let source_str = |p: &std::path::Path| p.to_string_lossy().into_owned();
    if claude_json_path.exists() {
        let source = source_str(&claude_json_path);
        let content = fs::read_to_string(&claude_json_path).map_err(|e| e.to_string())?;
        if let Ok(json) = serde_json::from_str::<Value>(&content) {
            collect_mcp_servers_from_value(&json, &source, "claude-code", &mut seen, &mut out);
            if let Some(projects) = json.get("projects").and_then(|p| p.as_object()) {
                for (_project_key, project_val) in projects {
                    if let Some(project_obj) = project_val.as_object() {
                        if let Some(mcp) = project_obj.get("mcpServers") {
                            let wrapper = json!({ "mcpServers": mcp });
                            collect_mcp_servers_from_value(
                                &wrapper,
                                &source,
                                "claude-code",
                                &mut seen,
                                &mut out,
                            );
                        }
                    }
                }
            }
        }
    }

    for (adapter_id, path) in [
        ("cursor", home.join(".cursor").join("mcp.json")),
        (
            "windsurf",
            home.join(".codeium").join("windsurf").join("mcp_config.json"),
        ),
    ] {
        if path.exists() {
            let source = source_str(&path);
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(json) = serde_json::from_str::<Value>(&content) {
                    collect_mcp_servers_from_value(&json, &source, adapter_id, &mut seen, &mut out);
                }
            }
        }
    }

    if claude_json_path.exists() {
        let content = fs::read_to_string(&claude_json_path).map_err(|e| e.to_string())?;
        if let Ok(json) = serde_json::from_str::<Value>(&content) {
            if let Some(projects) = json.get("projects").and_then(|p| p.as_object()) {
                for (project_path_str, _) in projects {
                    let mcp_path = PathBuf::from(project_path_str).join(".mcp.json");
                    if mcp_path.exists() {
                        let source = source_str(&mcp_path);
                        if let Ok(content) = fs::read_to_string(&mcp_path) {
                            if let Ok(proj_json) = serde_json::from_str::<Value>(&content) {
                                collect_mcp_servers_from_value(
                                    &proj_json,
                                    &source,
                                    "claude-code",
                                    &mut seen,
                                    &mut out,
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    // Discover from tracked projects' adapter config files (project-wise capabilities).
    for project_path in crate::commands::projects::get_tracked_project_paths() {
        let base = PathBuf::from(&project_path);
        let configs: [(&str, PathBuf); 4] = [
            ("claude-code", base.join(".mcp.json")),
            ("claude-code", base.join(".claude").join("settings.json")),
            ("cursor", base.join(".cursor").join("mcp.json")),
            ("windsurf", base.join(".windsurf").join("mcp_config.json")),
        ];
        for (adapter_id, path) in configs {
            if path.exists() {
                let source = source_str(&path);
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(json) = serde_json::from_str::<Value>(&content) {
                        collect_mcp_servers_from_value(&json, &source, adapter_id, &mut seen, &mut out);
                    }
                }
            }
        }
    }

    Ok(out)
}
