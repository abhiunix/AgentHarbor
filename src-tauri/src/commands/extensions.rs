use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudePlugin {
    pub name: String,
    pub version: String,
    pub scope: String,
    pub project_path: Option<String>,
    pub installed_at: String,
    pub last_updated: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorExtension {
    pub id: String,
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub installed_timestamp: Option<u64>,
    pub source: String,
    pub is_builtin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorPlugin {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
struct InstalledPluginsFile {
    #[serde(default)]
    plugins: HashMap<String, Vec<PluginInstallRecord>>,
}

#[derive(Debug, Clone, Deserialize)]
struct PluginInstallRecord {
    #[serde(default)]
    scope: String,
    #[serde(rename = "projectPath")]
    project_path: Option<String>,
    #[serde(default)]
    version: String,
    #[serde(rename = "installedAt", default)]
    installed_at: String,
    #[serde(rename = "lastUpdated", default)]
    last_updated: String,
}

fn read_enabled_plugins() -> HashMap<String, bool> {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return HashMap::new(),
    };
    let path = home.join(".claude").join("settings.json");
    if !path.exists() { return HashMap::new(); }
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    let json: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return HashMap::new(),
    };
    let ep = match json.get("enabledPlugins") {
        Some(v) => v,
        None => return HashMap::new(),
    };
    if let Some(obj) = ep.as_object() {
        obj.iter()
            .filter_map(|(k, v)| v.as_bool().map(|b| (k.clone(), b)))
            .collect()
    } else {
        HashMap::new()
    }
}

#[tauri::command]
pub fn list_claude_plugins() -> Result<Vec<ClaudePlugin>, String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let path = home.join(".claude").join("plugins").join("installed_plugins.json");
    if !path.exists() { return Ok(vec![]); }

    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let file: InstalledPluginsFile = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse installed_plugins.json: {}", e))?;

    let enabled = read_enabled_plugins();
    let mut plugins = Vec::new();

    for (name, records) in &file.plugins {
        for record in records {
            let is_enabled = enabled.get(name).copied().unwrap_or(false);
            plugins.push(ClaudePlugin {
                name: name.clone(),
                version: record.version.clone(),
                scope: record.scope.clone(),
                project_path: record.project_path.clone(),
                installed_at: record.installed_at.clone(),
                last_updated: record.last_updated.clone(),
                enabled: is_enabled,
            });
        }
    }

    plugins.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(plugins)
}

#[tauri::command]
pub fn list_cursor_extensions() -> Result<Vec<CursorExtension>, String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let path = home.join(".cursor").join("extensions").join("extensions.json");
    if !path.exists() { return Ok(vec![]); }

    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let arr: Vec<serde_json::Value> = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse extensions.json: {}", e))?;

    let mut extensions = Vec::new();
    for item in &arr {
        let identifier = item.get("identifier").unwrap_or(&serde_json::Value::Null);
        let full_id = identifier.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let version = item.get("version").and_then(|v| v.as_str()).unwrap_or("").to_string();

        let metadata = item.get("metadata").unwrap_or(&serde_json::Value::Null);
        let installed_ts = metadata.get("installedTimestamp").and_then(|v| v.as_u64());
        let source = metadata.get("source").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let is_builtin = metadata.get("isBuiltin").and_then(|v| v.as_bool()).unwrap_or(false);
        let publisher_display = metadata.get("publisherDisplayName")
            .and_then(|v| v.as_str()).unwrap_or("").to_string();

        let publisher = if !publisher_display.is_empty() {
            publisher_display
        } else {
            full_id.split('.').next().unwrap_or("").to_string()
        };

        let name = full_id.split('.').nth(1).unwrap_or(&full_id).to_string();

        extensions.push(CursorExtension {
            id: full_id,
            name,
            version,
            publisher,
            installed_timestamp: installed_ts,
            source,
            is_builtin,
        });
    }

    extensions.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(extensions)
}

#[tauri::command]
pub fn list_cursor_plugins() -> Result<Vec<CursorPlugin>, String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let cache_dir = home.join(".cursor").join("plugins").join("cache");
    if !cache_dir.exists() { return Ok(vec![]); }

    let mut plugins = Vec::new();
    let entries = std::fs::read_dir(&cache_dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() { continue; }

        let dir_entries = match std::fs::read_dir(&path) { Ok(e) => e, Err(_) => continue };
        for sub in dir_entries.flatten() {
            let sub_path = sub.path();
            if !sub_path.is_dir() { continue; }

            let plugin_json = sub_path.join(".cursor-plugin").join("plugin.json");
            let name = if plugin_json.exists() {
                let content = std::fs::read_to_string(&plugin_json).unwrap_or_default();
                let json: serde_json::Value = serde_json::from_str(&content).unwrap_or_default();
                json.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string()
            } else {
                path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string()
            };

            if name.is_empty() { continue; }
            plugins.push(CursorPlugin {
                name,
                path: sub_path.to_string_lossy().to_string(),
            });
        }
    }

    plugins.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(plugins)
}

#[tauri::command]
pub fn toggle_claude_plugin(plugin_name: String, enabled: bool) -> Result<(), String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let settings_path = home.join(".claude").join("settings.json");

    let content = if settings_path.exists() {
        std::fs::read_to_string(&settings_path).map_err(|e| e.to_string())?
    } else {
        "{}".to_string()
    };

    let mut json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse settings.json: {}", e))?;

    let obj = json.as_object_mut().ok_or("settings.json is not an object")?;
    let ep = obj.entry("enabledPlugins")
        .or_insert_with(|| serde_json::json!({}));
    if let Some(ep_obj) = ep.as_object_mut() {
        ep_obj.insert(plugin_name, serde_json::Value::Bool(enabled));
    }

    let output = serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?;
    crate::utils::paths::atomic_write_str(&settings_path, &output)?;

    Ok(())
}
