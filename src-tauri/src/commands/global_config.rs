use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

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

fn write_atomic(path: &Path, content: &str) -> Result<(), String> {
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

// Env keys managed by the Ollama switch.
// We deliberately do NOT set ANTHROPIC_AUTH_TOKEN alongside ANTHROPIC_API_KEY —
// Claude Code treats them as competing auth methods and warns when both are present.
// ANTHROPIC_API_KEY=ollama is the single correct override for local/proxy endpoints.
const OLLAMA_ENV_KEYS: [&str; 3] = [
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_MODEL",
];
// Also clean up ANTHROPIC_AUTH_TOKEN if a previous version wrote it.
const LEGACY_OLLAMA_KEYS: [&str; 1] = ["ANTHROPIC_AUTH_TOKEN"];

/// Read the file at `path`, mutate the top-level `env` object to reflect `cc`, write back atomically.
/// Preserves any unrelated env keys. Creates the file from `{}` if missing.
pub(crate) fn write_claude_settings_env_at(
    path: &std::path::Path,
    cc: &crate::commands::config::ClaudeCodeSettings,
) -> Result<(), String> {
    let mut root: Value = if path.exists() {
        let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
        if raw.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&raw)
                .map_err(|e| format!("Existing {} is malformed: {}", path.display(), e))?
        }
    } else {
        json!({})
    };

    let obj = root
        .as_object_mut()
        .ok_or_else(|| format!("{} is not a JSON object", path.display()))?;

    let env_entry = obj.entry("env".to_string()).or_insert_with(|| json!({}));
    let env_obj = env_entry
        .as_object_mut()
        .ok_or_else(|| "`env` field is not a JSON object".to_string())?;

    use crate::commands::config::ClaudeCodeProvider;
    match cc.provider {
        ClaudeCodeProvider::Ollama => {
            env_obj.insert(
                "ANTHROPIC_BASE_URL".to_string(),
                Value::String(cc.ollama_base_url.trim_end_matches('/').to_string()),
            );
            // Single auth override — do NOT also set ANTHROPIC_AUTH_TOKEN.
            // Claude Code warns and behaves unpredictably when both are present.
            env_obj.insert(
                "ANTHROPIC_API_KEY".to_string(),
                Value::String(if cc.ollama_auth_token.is_empty() {
                    "ollama".to_string()
                } else {
                    cc.ollama_auth_token.clone()
                }),
            );
            env_obj.insert(
                "ANTHROPIC_MODEL".to_string(),
                Value::String(cc.ollama_model.clone()),
            );
            // Remove legacy key written by earlier versions of this feature.
            for key in LEGACY_OLLAMA_KEYS.iter() {
                env_obj.remove(*key);
            }
        }
        ClaudeCodeProvider::Anthropic => {
            for key in OLLAMA_ENV_KEYS.iter() {
                env_obj.remove(*key);
            }
            for key in LEGACY_OLLAMA_KEYS.iter() {
                env_obj.remove(*key);
            }
        }
    }

    let serialised = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    write_atomic(path, &serialised)
}

pub(crate) fn mutate_claude_settings_env(
    cc: &crate::commands::config::ClaudeCodeSettings,
) -> Result<(), String> {
    let path = claude_settings_path().ok_or("Could not resolve ~/.claude/settings.json path")?;
    write_claude_settings_env_at(&path, cc)
}

#[tauri::command]
pub async fn test_ollama_connection(base_url: String) -> Result<bool, String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return Err("Base URL must start with http:// or https://".to_string());
    }
    let url = format!("{}/api/tags", trimmed);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .map_err(|e| e.to_string())?;
    match client.get(&url).send().await {
        Ok(r) if r.status().is_success() => Ok(true),
        Ok(r) => Err(format!("HTTP {}", r.status())),
        Err(e) => Err(e.to_string()),
    }
}

/// Open a new Terminal window and run `ollama launch claude --model <model>`.
/// Uses Ollama's native Claude Code integration (no separate proxy needed).
/// Clears our env overrides from settings.json first so that Ollama's own
/// ANTHROPIC_AUTH_TOKEN doesn't conflict with our ANTHROPIC_API_KEY.
#[tauri::command]
pub fn launch_claude_via_ollama(model: String) -> Result<(), String> {
    // Clear our env keys so Ollama can manage its own auth without conflict.
    if let Some(path) = claude_settings_path() {
        let clear = crate::commands::config::ClaudeCodeSettings {
            provider: crate::commands::config::ClaudeCodeProvider::Anthropic,
            ..Default::default()
        };
        let _ = write_claude_settings_env_at(&path, &clear);
    }
    crate::utils::platform::launch_in_terminal(&format!(
        "ollama launch claude --model {}",
        model.trim()
    ))
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
        Ok(serde_json::to_string_pretty(&json).unwrap_or(content))
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

#[cfg(test)]
mod claude_env_tests {
    use super::*;
    use crate::commands::config::{ClaudeCodeProvider, ClaudeCodeSettings};
    use tempfile::tempdir;

    fn ollama_cc() -> ClaudeCodeSettings {
        ClaudeCodeSettings {
            provider: ClaudeCodeProvider::Ollama,
            ollama_base_url: "http://localhost:11434".to_string(),
            ollama_model: "llama3.1:8b".to_string(),
            ollama_auth_token: "ollama".to_string(),
        }
    }

    fn anthropic_cc() -> ClaudeCodeSettings {
        ClaudeCodeSettings {
            provider: ClaudeCodeProvider::Anthropic,
            ollama_base_url: "http://localhost:11434".to_string(),
            ollama_model: "llama3.1:8b".to_string(),
            ollama_auth_token: "ollama".to_string(),
        }
    }

    #[test]
    fn creates_file_when_missing_and_writes_three_env_keys() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        assert!(!path.exists());

        write_claude_settings_env_at(&path, &ollama_cc()).unwrap();

        let raw = fs::read_to_string(&path).unwrap();
        let v: Value = serde_json::from_str(&raw).unwrap();
        let env = v.get("env").unwrap().as_object().unwrap();
        assert_eq!(env.get("ANTHROPIC_BASE_URL").unwrap(), "http://localhost:11434");
        assert_eq!(env.get("ANTHROPIC_API_KEY").unwrap(), "ollama");
        assert_eq!(env.get("ANTHROPIC_MODEL").unwrap(), "llama3.1:8b");
        // ANTHROPIC_AUTH_TOKEN must NOT be written — causes auth conflict warning in Claude Code
        assert!(env.get("ANTHROPIC_AUTH_TOKEN").is_none());
    }

    #[test]
    fn preserves_unrelated_env_keys_and_top_level_keys() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(
            &path,
            r#"{
              "env": { "MY_OTHER": "foo" },
              "permissions": { "allow": [] }
            }"#,
        )
        .unwrap();

        write_claude_settings_env_at(&path, &ollama_cc()).unwrap();

        let v: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let env = v.get("env").unwrap().as_object().unwrap();
        assert_eq!(env.get("MY_OTHER").unwrap(), "foo");
        assert_eq!(env.get("ANTHROPIC_MODEL").unwrap(), "llama3.1:8b");
        assert!(v.get("permissions").is_some());
    }

    #[test]
    fn switching_to_anthropic_removes_all_ollama_keys_and_keeps_others() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        write_claude_settings_env_at(&path, &ollama_cc()).unwrap();

        // Inject an unrelated key and a legacy ANTHROPIC_AUTH_TOKEN to verify cleanup
        let mut v: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        v["env"]["MY_OTHER"] = Value::String("foo".to_string());
        v["env"]["ANTHROPIC_AUTH_TOKEN"] = Value::String("legacy".to_string());
        fs::write(&path, serde_json::to_string(&v).unwrap()).unwrap();

        write_claude_settings_env_at(&path, &anthropic_cc()).unwrap();

        let v: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let env = v.get("env").unwrap().as_object().unwrap();
        assert!(env.get("ANTHROPIC_BASE_URL").is_none());
        assert!(env.get("ANTHROPIC_API_KEY").is_none());
        assert!(env.get("ANTHROPIC_AUTH_TOKEN").is_none(), "legacy key must be cleaned up");
        assert!(env.get("ANTHROPIC_MODEL").is_none());
        assert_eq!(env.get("MY_OTHER").unwrap(), "foo");
    }

    #[test]
    fn malformed_json_returns_structured_error() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, "{").unwrap();

        let err = write_claude_settings_env_at(&path, &ollama_cc()).unwrap_err();
        assert!(err.contains("malformed"), "got: {}", err);
        // File should not have been overwritten with the error
        assert_eq!(fs::read_to_string(&path).unwrap(), "{");
    }

    #[test]
    fn trailing_slash_in_base_url_is_stripped() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut cc = ollama_cc();
        cc.ollama_base_url = "http://localhost:11434/".to_string();
        write_claude_settings_env_at(&path, &cc).unwrap();

        let v: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["env"]["ANTHROPIC_BASE_URL"], "http://localhost:11434");
    }
}
