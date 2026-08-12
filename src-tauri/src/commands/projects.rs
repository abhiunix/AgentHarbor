use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::utils::project_inventory::{
    collect_installed_items, stats_from_installed_items, InstalledItem,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDeployment {
    pub adapter: String,
    pub capability_ids: Vec<String>,
    pub agent_ids: Vec<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub path: String,
    pub name: String,
    pub last_deployed: Option<String>,
    pub deployments: Vec<ProjectDeployment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectsData {
    pub projects: Vec<Project>,
}

impl Default for ProjectsData {
    fn default() -> Self {
        Self { projects: vec![] }
    }
}

fn get_projects_file_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.agentharbor.app")
        .join("projects.json")
}

fn load_projects_data() -> ProjectsData {
    let path = get_projects_file_path();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(data) = serde_json::from_str(&content) {
                return data;
            }
        }
    }
    ProjectsData::default()
}

/// Returns paths of all tracked projects (for discovery and sidebar).
pub fn get_tracked_project_paths() -> Vec<String> {
    load_projects_data()
        .projects
        .into_iter()
        .map(|p| p.path)
        .collect()
}

fn save_projects_data(data: &ProjectsData) -> Result<(), String> {
    let path = get_projects_file_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    crate::utils::paths::atomic_write_str(&path, &content)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub path: String,
    pub name: String,
    pub last_deployed: Option<String>,
    pub deployments_count: usize,
    pub detected_adapters: Vec<String>,
    pub capabilities_count: usize,
    pub agents_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDetail {
    pub path: String,
    pub name: String,
    pub last_deployed: Option<String>,
    pub deployments: Vec<ProjectDeployment>,
    pub detected_adapters: Vec<AdapterStatus>,
    pub deployed_capabilities: Vec<String>,
    pub deployed_agents: Vec<String>,
    #[serde(default)]
    pub is_tracked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterStatus {
    pub id: String,
    pub name: String,
    pub has_config: bool,
    pub config_path: String,
}

fn detect_project_adapters(project_path: &str) -> Vec<AdapterStatus> {
    let path = PathBuf::from(project_path);
    let mut adapters = vec![];

    let claude_dir = path.join(".claude");
    let claude_settings = claude_dir.join("settings.json");
    let claude_settings_local = claude_dir.join("settings.local.json");
    let mcp_root = path.join(".mcp.json");
    let claude_md = path.join("CLAUDE.md");
    let claude_configured = (claude_dir.is_dir())
        || claude_settings.exists()
        || claude_settings_local.exists()
        || mcp_root.exists()
        || claude_md.exists();
    let claude_config_path = if claude_settings.exists() {
        claude_settings.to_string_lossy().to_string()
    } else if claude_settings_local.exists() {
        claude_settings_local.to_string_lossy().to_string()
    } else if mcp_root.exists() {
        mcp_root.to_string_lossy().to_string()
    } else if claude_md.exists() {
        claude_md.to_string_lossy().to_string()
    } else {
        claude_dir.to_string_lossy().to_string()
    };
    adapters.push(AdapterStatus {
        id: "claude-code".to_string(),
        name: "Claude Code".to_string(),
        has_config: claude_configured,
        config_path: claude_config_path,
    });

    let cursor_dir = path.join(".cursor");
    let cursor_mcp = cursor_dir.join("mcp.json");
    let cursorrules = path.join(".cursorrules");
    let cursor_configured = cursor_mcp.exists() || cursor_dir.is_dir() || cursorrules.exists();
    let cursor_config_path = if cursor_mcp.exists() {
        cursor_mcp.to_string_lossy().to_string()
    } else if cursorrules.exists() {
        cursorrules.to_string_lossy().to_string()
    } else {
        cursor_dir.to_string_lossy().to_string()
    };
    adapters.push(AdapterStatus {
        id: "cursor".to_string(),
        name: "Cursor".to_string(),
        has_config: cursor_configured,
        config_path: cursor_config_path,
    });

    let windsurf_dir = path.join(".windsurf");
    let windsurf_mcp = windsurf_dir.join("mcp_config.json");
    let windsurfrules = path.join(".windsurfrules");
    let windsurf_configured =
        windsurf_mcp.exists() || windsurf_dir.is_dir() || windsurfrules.exists();
    let windsurf_config_path = if windsurf_mcp.exists() {
        windsurf_mcp.to_string_lossy().to_string()
    } else if windsurfrules.exists() {
        windsurfrules.to_string_lossy().to_string()
    } else {
        windsurf_dir.to_string_lossy().to_string()
    };
    adapters.push(AdapterStatus {
        id: "windsurf".to_string(),
        name: "Windsurf".to_string(),
        has_config: windsurf_configured,
        config_path: windsurf_config_path,
    });

    adapters
}

fn count_deployed_items(project: &Project) -> (usize, usize) {
    let mut cap_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut agent_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    for deployment in &project.deployments {
        for id in &deployment.capability_ids {
            cap_ids.insert(id.clone());
        }
        for id in &deployment.agent_ids {
            agent_ids.insert(id.clone());
        }
    }

    (cap_ids.len(), agent_ids.len())
}

fn get_all_deployed_ids(project: &Project) -> (Vec<String>, Vec<String>) {
    let mut cap_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut agent_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    for deployment in &project.deployments {
        for id in &deployment.capability_ids {
            cap_ids.insert(id.clone());
        }
        for id in &deployment.agent_ids {
            agent_ids.insert(id.clone());
        }
    }

    (cap_ids.into_iter().collect(), agent_ids.into_iter().collect())
}


#[tauri::command]
pub fn get_all_projects() -> Vec<ProjectInfo> {
    let data = load_projects_data();
    
    data.projects
        .iter()
        .map(|project| {
            let adapters = detect_project_adapters(&project.path);
            let detected = adapters
                .iter()
                .filter(|a| a.has_config)
                .map(|a| a.id.clone())
                .collect();
            let (hist_caps, hist_agents) = count_deployed_items(project);
            let pb = PathBuf::from(&project.path);
            let (fs_caps, fs_agents) = collect_installed_items(pb.as_path())
                .map(|items| stats_from_installed_items(&items))
                .unwrap_or((0, 0));
            
            ProjectInfo {
                path: project.path.clone(),
                name: project.name.clone(),
                last_deployed: project.last_deployed.clone(),
                deployments_count: project.deployments.len(),
                detected_adapters: detected,
                capabilities_count: hist_caps.max(fs_caps),
                agents_count: hist_agents.max(fs_agents),
            }
        })
        .collect()
}

#[tauri::command]
pub fn get_project_detail(project_path: String) -> Option<ProjectDetail> {
    let data = load_projects_data();
    let adapters = detect_project_adapters(&project_path);
    let name = PathBuf::from(&project_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Unknown".to_string());

    if let Some(project) = data.projects.iter().find(|p| p.path == project_path) {
        let (caps, agents) = get_all_deployed_ids(project);
        return Some(ProjectDetail {
            path: project.path.clone(),
            name: project.name.clone(),
            last_deployed: project.last_deployed.clone(),
            deployments: project.deployments.clone(),
            detected_adapters: adapters,
            deployed_capabilities: caps,
            deployed_agents: agents,
            is_tracked: true,
        });
    }

    Some(ProjectDetail {
        path: project_path.clone(),
        name,
        last_deployed: None,
        deployments: vec![],
        detected_adapters: adapters,
        deployed_capabilities: vec![],
        deployed_agents: vec![],
        is_tracked: false,
    })
}

#[tauri::command]
pub fn add_project(project_path: String, name: String) -> Result<ProjectInfo, String> {
    let mut data = load_projects_data();
    
    if data.projects.iter().any(|p| p.path == project_path) {
        return Err("Project already exists".to_string());
    }
    
    let project = Project {
        path: project_path.clone(),
        name: name.clone(),
        last_deployed: None,
        deployments: vec![],
    };
    
    data.projects.push(project);
    save_projects_data(&data)?;
    
    let adapters = detect_project_adapters(&project_path);
    let detected = adapters
        .iter()
        .filter(|a| a.has_config)
        .map(|a| a.id.clone())
        .collect();
    
    Ok(ProjectInfo {
        path: project_path,
        name,
        last_deployed: None,
        deployments_count: 0,
        detected_adapters: detected,
        capabilities_count: 0,
        agents_count: 0,
    })
}

#[tauri::command]
pub fn remove_project(project_path: String) -> Result<(), String> {
    let mut data = load_projects_data();
    data.projects.retain(|p| p.path != project_path);
    save_projects_data(&data)?;
    Ok(())
}

#[tauri::command]
pub fn record_deployment(
    project_path: String,
    adapter: String,
    capability_ids: Vec<String>,
    agent_ids: Vec<String>,
) -> Result<(), String> {
    let mut data = load_projects_data();
    
    let project = data
        .projects
        .iter_mut()
        .find(|p| p.path == project_path);
    
    let now = chrono::Utc::now().to_rfc3339();
    
    match project {
        Some(p) => {
            p.last_deployed = Some(now.clone());
            p.deployments.push(ProjectDeployment {
                adapter,
                capability_ids,
                agent_ids,
                timestamp: now,
            });
            
            const MAX_DEPLOYMENTS: usize = 50;
            if p.deployments.len() > MAX_DEPLOYMENTS {
                let excess = p.deployments.len() - MAX_DEPLOYMENTS;
                p.deployments.drain(0..excess);
            }
        }
        None => {
            let name = PathBuf::from(&project_path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Unknown".to_string());
            
            data.projects.push(Project {
                path: project_path.clone(),
                name,
                last_deployed: Some(now.clone()),
                deployments: vec![ProjectDeployment {
                    adapter,
                    capability_ids,
                    agent_ids,
                    timestamp: now,
                }],
            });
        }
    }
    
    save_projects_data(&data)?;
    Ok(())
}

#[tauri::command]
pub fn open_project_in_finder(project_path: String) -> Result<(), String> {
    crate::utils::platform::open_in_file_manager(&project_path)
}

#[tauri::command]
pub fn open_project_in_cursor(project_path: String) -> Result<(), String> {
    crate::utils::platform::open_in_ide("cursor", &project_path)
}

#[tauri::command]
pub fn open_project_in_vscode(project_path: String) -> Result<(), String> {
    crate::utils::platform::open_in_ide("code", &project_path)
}

#[tauri::command]
pub fn open_project_in_terminal(project_path: String) -> Result<(), String> {
    crate::utils::platform::open_in_terminal(&project_path)
}

/// Launch a fresh Claude Code session in the given project directory.
#[tauri::command]
pub fn start_claude_in_project(project_path: String) -> Result<(), String> {
    let quoted = format!("'{}'", project_path.replace('\'', "'\\''"));
    crate::utils::platform::launch_in_terminal(&format!("cd {} && claude", quoted))
}


#[tauri::command]
pub fn get_project_installed_items(project_path: String) -> Result<Vec<InstalledItem>, String> {
    collect_installed_items(Path::new(&project_path))
}

#[tauri::command]
pub fn remove_project_item(
    project_path: String,
    item_name: String,
    item_type: String,
    adapter_id: String,
) -> Result<(), String> {
    let path = PathBuf::from(&project_path);

    match item_type.as_str() {
        "mcp" => {
            let config_path = match adapter_id.as_str() {
                "claude-code" => path.join(".mcp.json"),
                "cursor" => path.join(".cursor").join("mcp.json"),
                "windsurf" => path.join(".windsurf").join("mcp_config.json"),
                _ => return Err(format!("Unknown adapter: {}", adapter_id)),
            };

            if !config_path.exists() {
                return Err("Config file not found".to_string());
            }

            let content = fs::read_to_string(&config_path)
                .map_err(|e| format!("Failed to read config: {}", e))?;
            let mut json: Value = serde_json::from_str(&content)
                .map_err(|e| format!("Failed to parse config: {}", e))?;

            if let Some(servers) = json.get_mut("mcpServers").and_then(|v| v.as_object_mut()) {
                servers.remove(&item_name);
            }

            let new_content = serde_json::to_string_pretty(&json)
                .map_err(|e| format!("Failed to serialize config: {}", e))?;
            crate::utils::paths::atomic_write_str(&config_path, &new_content)?;
        }
        "hook" | "plugin" => {
            let section = if item_type == "hook" { "hooks" } else { "plugins" };
            let settings_candidates = [
                path.join(".claude").join("settings.json"),
                path.join(".claude").join("settings.local.json"),
            ];
            let mut updated = false;
            for settings_path in &settings_candidates {
                if !settings_path.exists() {
                    continue;
                }
                let content = match fs::read_to_string(settings_path) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let mut json: Value = match serde_json::from_str(&content) {
                    Ok(j) => j,
                    Err(_) => continue,
                };
                let had_key = json
                    .get(section)
                    .and_then(|v| v.as_object())
                    .map(|o| o.contains_key(&item_name))
                    .unwrap_or(false);
                if !had_key {
                    continue;
                }
                if let Some(obj) = json.get_mut(section).and_then(|v| v.as_object_mut()) {
                    obj.remove(&item_name);
                }
                let new_content = serde_json::to_string_pretty(&json)
                    .map_err(|e| format!("Failed to serialize settings: {}", e))?;
                crate::utils::paths::atomic_write_str(settings_path, &new_content)?;
                updated = true;
                break;
            }
            if !updated {
                return Err("Settings file not found or key missing".to_string());
            }
        }
        "skill" => {
            let skill_path = match adapter_id.as_str() {
                "cursor" => path.join(".cursor").join("skills").join(&item_name),
                "windsurf" => path.join(".windsurf").join("skills").join(&item_name),
                "gemini" => path.join(".gemini").join("skills").join(&item_name),
                _ => path.join(".claude").join("skills").join(&item_name),
            };
            if skill_path.exists() {
                if skill_path.is_dir() {
                    fs::remove_dir_all(&skill_path)
                        .map_err(|e| format!("Failed to remove skill directory: {}", e))?;
                } else {
                    fs::remove_file(&skill_path)
                        .map_err(|e| format!("Failed to remove skill file: {}", e))?;
                }
            }
        }
        "rule" => {
            let rules_dir = match adapter_id.as_str() {
                "cursor" => path.join(".cursor").join("rules"),
                "windsurf" => path.join(".windsurf").join("rules"),
                "claude-code" => path.join(".claude").join("rules"),
                _ => return Err(format!("Unknown adapter for rule: {}", adapter_id)),
            };
            let mut removed = false;
            for ext in [".mdc", ".md"] {
                let rule_path = rules_dir.join(format!("{}{}", item_name, ext));
                if rule_path.exists() {
                    fs::remove_file(&rule_path)
                        .map_err(|e| format!("Failed to remove rule file: {}", e))?;
                    removed = true;
                    break;
                }
            }
            if !removed {
                return Err("Rule file not found".to_string());
            }
        }
        "agent" => {
            let agent_path = match adapter_id.as_str() {
                "claude-code" => path.join(".claude").join("agents").join(format!("{}.md", item_name)),
                "cursor" => path.join(".cursor").join("agents").join(format!("{}.md", item_name)),
                "gemini" => path.join(".gemini").join("agents").join(format!("{}.md", item_name)),
                "shared" | _ => path.join("agents").join(format!("{}.md", item_name)),
            };
            if !agent_path.exists() {
                return Err("Agent file not found".to_string());
            }
            fs::remove_file(&agent_path)
                .map_err(|e| format!("Failed to remove agent file: {}", e))?;
        }
        _ => return Err(format!("Unknown item type: {}", item_type)),
    }

    // Rebuild manifests after item removal
    let _ = crate::utils::manifest::rebuild_all_manifests(std::path::Path::new(&project_path));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_project_adapters() {
        let adapters = detect_project_adapters("/tmp/nonexistent");
        assert_eq!(adapters.len(), 3);
        assert!(!adapters[0].has_config);
    }
}
