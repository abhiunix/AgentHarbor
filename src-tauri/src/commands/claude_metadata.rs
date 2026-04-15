//! Parse Claude Code local metadata files for analytics V2.
//! Reads: .claude.json, plugins, settings, commands, todos, plans, hooks, file-history

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── App Info (.claude.json) ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClaudeAppInfo {
    pub num_startups: u64,
    pub install_method: Option<String>,
    pub prompt_queue_use_count: u64,
    pub features: HashMap<String, bool>,
}

#[tauri::command]
pub fn get_claude_app_info() -> Result<ClaudeAppInfo, String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let path = home.join(".claude.json");
    if !path.exists() {
        return Ok(ClaudeAppInfo::default());
    }
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse .claude.json: {}", e))?;

    let num_startups = json.get("numStartups").and_then(|v| v.as_u64()).unwrap_or(0);
    let install_method = json.get("installMethod").and_then(|v| v.as_str()).map(String::from);
    let prompt_queue_use_count = json.get("promptQueueUseCount").and_then(|v| v.as_u64()).unwrap_or(0);

    let mut features = HashMap::new();
    if let Some(obj) = json.get("cachedGrowthBookFeatures").and_then(|v| v.as_object()) {
        for (key, val) in obj {
            if let Some(b) = val.as_bool() {
                features.insert(key.clone(), b);
            }
        }
    }

    Ok(ClaudeAppInfo {
        num_startups,
        install_method,
        prompt_queue_use_count,
        features,
    })
}

// ── Plugins ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPlugin {
    pub name: String,
    pub scope: Option<String>,
    pub version: Option<String>,
    pub install_path: Option<String>,
    pub installed_at: Option<String>,
    pub updated_at: Option<String>,
}

#[tauri::command]
pub fn get_claude_installed_plugins() -> Result<Vec<InstalledPlugin>, String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let path = home.join(".claude").join("plugins").join("installed_plugins.json");
    if !path.exists() {
        return Ok(vec![]);
    }
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse installed_plugins.json: {}", e))?;

    let mut plugins = Vec::new();
    if let Some(arr) = json.as_array() {
        for item in arr {
            plugins.push(InstalledPlugin {
                name: item.get("name").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
                scope: item.get("scope").and_then(|v| v.as_str()).map(String::from),
                version: item.get("version").and_then(|v| v.as_str()).map(String::from),
                install_path: item.get("installPath").and_then(|v| v.as_str()).map(String::from),
                installed_at: item.get("installedAt").and_then(|v| v.as_str()).map(String::from),
                updated_at: item.get("updatedAt").and_then(|v| v.as_str()).map(String::from),
            });
        }
    } else if let Some(obj) = json.as_object() {
        // Some versions use object keyed by name
        for (name, item) in obj {
            plugins.push(InstalledPlugin {
                name: name.clone(),
                scope: item.get("scope").and_then(|v| v.as_str()).map(String::from),
                version: item.get("version").and_then(|v| v.as_str()).map(String::from),
                install_path: item.get("installPath").and_then(|v| v.as_str()).map(String::from),
                installed_at: item.get("installedAt").and_then(|v| v.as_str()).map(String::from),
                updated_at: item.get("updatedAt").and_then(|v| v.as_str()).map(String::from),
            });
        }
    }

    Ok(plugins)
}

// ── Settings Info ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClaudeSettingsInfo {
    pub enabled_plugins: Vec<String>,
    pub allow_permissions: Vec<String>,
    pub deny_permissions: Vec<String>,
}

#[tauri::command]
pub fn get_claude_settings_info() -> Result<ClaudeSettingsInfo, String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let path = home.join(".claude").join("settings.json");
    if !path.exists() {
        return Ok(ClaudeSettingsInfo::default());
    }
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse settings.json: {}", e))?;

    let enabled_plugins = json.get("enabledPlugins")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let allow_permissions = json.get("permissions")
        .and_then(|p| p.get("allow"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let deny_permissions = json.get("permissions")
        .and_then(|p| p.get("deny"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    Ok(ClaudeSettingsInfo {
        enabled_plugins,
        allow_permissions,
        deny_permissions,
    })
}

// ── Custom Commands ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomCommand {
    pub name: String,
    pub file_name: String,
}

#[tauri::command]
pub fn get_claude_custom_commands() -> Result<Vec<CustomCommand>, String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let commands_dir = home.join(".claude").join("commands");
    if !commands_dir.exists() {
        return Ok(vec![]);
    }
    let mut commands = Vec::new();
    for entry in std::fs::read_dir(&commands_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("md") {
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
            let name = file_name.trim_end_matches(".md").to_string();
            commands.push(CustomCommand { name, file_name });
        }
    }
    commands.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(commands)
}

// ── Todos Summary ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TodosSummary {
    pub total_sessions_with_todos: u32,
    pub total_todos: u32,
    pub completed_todos: u32,
    pub pending_todos: u32,
}

#[tauri::command]
pub fn get_claude_todos_summary() -> Result<TodosSummary, String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let todos_dir = home.join(".claude").join("todos");
    if !todos_dir.exists() {
        return Ok(TodosSummary::default());
    }

    let mut summary = TodosSummary::default();
    for entry in walkdir::WalkDir::new(&todos_dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let json: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(arr) = json.as_array() {
            summary.total_sessions_with_todos += 1;
            for item in arr {
                summary.total_todos += 1;
                let status = item.get("status").and_then(|v| v.as_str()).unwrap_or("");
                if status == "completed" || status == "done" {
                    summary.completed_todos += 1;
                } else {
                    summary.pending_todos += 1;
                }
            }
        }
    }
    Ok(summary)
}

// ── Plans Summary ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlansSummary {
    pub total_plans: u32,
    pub plan_names: Vec<String>,
}

#[tauri::command]
pub fn get_claude_plans_summary() -> Result<PlansSummary, String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let plans_dir = home.join(".claude").join("plans");
    if !plans_dir.exists() {
        return Ok(PlansSummary::default());
    }
    let mut summary = PlansSummary::default();
    for entry in std::fs::read_dir(&plans_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("md") {
            summary.total_plans += 1;
            if let Some(name) = path.file_stem().and_then(|n| n.to_str()) {
                summary.plan_names.push(name.to_string());
            }
        }
    }
    Ok(summary)
}

// ── Hooks Summary ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HooksSummary {
    pub total_hook_executions: u32,
}

#[tauri::command]
pub fn get_claude_hooks_summary() -> Result<HooksSummary, String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let hooks_dir = home.join(".claude").join("hooks-data");
    if !hooks_dir.exists() {
        return Ok(HooksSummary::default());
    }
    let mut count = 0u32;
    for entry in std::fs::read_dir(&hooks_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.path().is_file() && entry.path().extension().and_then(|e| e.to_str()) == Some("json") {
            count += 1;
        }
    }
    Ok(HooksSummary { total_hook_executions: count })
}

// ── File History Stats ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileHistoryStats {
    pub total_sessions: u32,
    pub total_checkpoints: u32,
}

#[tauri::command]
pub fn get_claude_file_history_stats() -> Result<FileHistoryStats, String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let history_dir = home.join(".claude").join("file-history");
    if !history_dir.exists() {
        return Ok(FileHistoryStats::default());
    }
    let mut stats = FileHistoryStats::default();
    for entry in std::fs::read_dir(&history_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.path().is_dir() {
            stats.total_sessions += 1;
            // Count files in each session dir
            if let Ok(files) = std::fs::read_dir(entry.path()) {
                for file in files {
                    if let Ok(f) = file {
                        if f.path().is_file() {
                            stats.total_checkpoints += 1;
                        }
                    }
                }
            }
        }
    }
    Ok(stats)
}
