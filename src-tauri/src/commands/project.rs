use crate::adapters::{AdapterRegistry, AdapterCapabilities};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri_plugin_dialog::{DialogExt, FileDialogBuilder};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentProject {
    pub name: String,
    pub path: String,
    pub last_opened: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedAdapter {
    pub id: String,
    pub name: String,
    pub capabilities: AdapterCapabilities,
}

fn get_projects_file_path() -> PathBuf {
    crate::utils::paths::app_data_dir().join("projects.json")
}

fn load_recent_projects_from_file() -> Vec<RecentProject> {
    let path = get_projects_file_path();
    if !path.exists() {
        return vec![];
    }

    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => vec![],
    }
}

fn save_recent_projects_to_file(projects: &[RecentProject]) -> Result<(), String> {
    let path = get_projects_file_path();
    
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    let content = serde_json::to_string_pretty(projects)
        .map_err(|e| format!("Failed to serialize projects: {}", e))?;

    let temp_path = path.with_extension("tmp");
    fs::write(&temp_path, &content)
        .map_err(|e| format!("Failed to write temp file: {}", e))?;
    fs::rename(&temp_path, &path)
        .map_err(|e| format!("Failed to rename temp file: {}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn select_project_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use std::sync::mpsc;
    
    let (tx, rx) = mpsc::channel();
    
    app.dialog()
        .file()
        .pick_folder(move |folder| {
            let _ = tx.send(folder);
        });
    
    match rx.recv() {
        Ok(Some(path)) => Ok(Some(path.to_string())),
        Ok(None) => Ok(None),
        Err(_) => Ok(None),
    }
}

#[tauri::command]
pub fn detect_adapters(project_path: String) -> Vec<DetectedAdapter> {
    let registry = AdapterRegistry::new();
    let path = PathBuf::from(&project_path);

    registry
        .all()
        .iter()
        .filter(|adapter| adapter.detect(&path))
        .map(|adapter| DetectedAdapter {
            id: adapter.id().to_string(),
            name: adapter.name().to_string(),
            capabilities: adapter.capabilities(),
        })
        .collect()
}

#[tauri::command]
pub fn get_recent_projects() -> Vec<RecentProject> {
    load_recent_projects_from_file()
}

#[tauri::command]
pub fn add_recent_project(project_path: String) -> Result<Vec<RecentProject>, String> {
    let mut projects = load_recent_projects_from_file();

    projects.retain(|p| p.path != project_path);

    let name = PathBuf::from(&project_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Unknown")
        .to_string();

    let new_project = RecentProject {
        name,
        path: project_path,
        last_opened: chrono::Utc::now().to_rfc3339(),
    };

    projects.insert(0, new_project);

    if projects.len() > 20 {
        projects.truncate(20);
    }

    save_recent_projects_to_file(&projects)?;

    Ok(projects)
}

#[tauri::command]
pub fn remove_recent_project(project_path: String) -> Result<Vec<RecentProject>, String> {
    let mut projects = load_recent_projects_from_file();
    projects.retain(|p| p.path != project_path);
    save_recent_projects_to_file(&projects)?;
    Ok(projects)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfigInfo {
    pub mcp_servers: Vec<String>,
    pub has_config: bool,
}

fn get_global_config_path(adapter_id: &str) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    match adapter_id {
        "claude-code" => Some(home.join(".claude.json")),
        "cursor" => Some(home.join(".cursor").join("mcp.json")),
        "windsurf" => Some(home.join(".codeium").join("windsurf").join("mcp_config.json")),
        "codex" => Some(home.join(".codex").join("config.toml")),
        _ => None,
    }
}

#[tauri::command]
pub fn get_global_config(adapter_id: String) -> Result<GlobalConfigInfo, String> {
    let config_path = get_global_config_path(&adapter_id)
        .ok_or_else(|| format!("Unknown adapter: {}", adapter_id))?;

    // Codex uses TOML, not JSON — return empty MCP list (no MCP support)
    if adapter_id == "codex" {
        return Ok(GlobalConfigInfo {
            mcp_servers: vec![],
            has_config: config_path.exists(),
        });
    }

    if !config_path.exists() {
        return Ok(GlobalConfigInfo {
            mcp_servers: vec![],
            has_config: false,
        });
    }

    let content = fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read config: {}", e))?;

    let json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse config: {}", e))?;

    let mcp_servers = json
        .get("mcpServers")
        .and_then(|v| v.as_object())
        .map(|obj| obj.keys().cloned().collect())
        .unwrap_or_default();

    Ok(GlobalConfigInfo {
        mcp_servers,
        has_config: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_projects_file_path() {
        let path = get_projects_file_path();
        assert!(path.to_string_lossy().contains("com.agentharbor.app"));
        assert!(path.to_string_lossy().ends_with("projects.json"));
    }

    #[test]
    fn test_detect_adapters_empty_dir() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let adapters = detect_adapters(temp_dir.path().to_string_lossy().to_string());
        assert!(adapters.is_empty());
    }

    #[test]
    fn test_detect_adapters_claude_project() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        fs::create_dir(temp_dir.path().join(".claude")).unwrap();
        
        let adapters = detect_adapters(temp_dir.path().to_string_lossy().to_string());
        assert_eq!(adapters.len(), 1);
        assert_eq!(adapters[0].id, "claude-code");
    }
}
