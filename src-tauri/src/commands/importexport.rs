use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::models::{AgentDefinition, UniversalCapability, Visibility};
use crate::registry::{load_agents, load_capabilities, get_bundled_registry_path};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportData {
    pub version: String,
    pub exported_at: String,
    pub capabilities: Vec<UniversalCapability>,
    pub agents: Vec<AgentDefinition>,
    pub presets: Vec<ExportedPreset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportedPreset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub capability_ids: Vec<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportResult {
    pub success: bool,
    pub message: String,
    pub capabilities_imported: usize,
    pub agents_imported: usize,
    pub presets_imported: usize,
    pub conflicts: Vec<ImportConflict>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportConflict {
    pub item_type: String,
    pub item_id: String,
    pub message: String,
}

fn get_custom_registry_path() -> PathBuf {
    crate::utils::paths::app_data_dir()
        .join("registry")
        .join("custom")
}

fn get_custom_agents_path() -> PathBuf {
    crate::utils::paths::app_data_dir().join("agents")
}

fn get_presets_path() -> PathBuf {
    crate::utils::paths::app_data_dir().join("presets.json")
}

fn load_all_presets() -> Vec<ExportedPreset> {
    let path = get_presets_path();
    if !path.exists() {
        return vec![];
    }
    
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(presets) = serde_json::from_str::<Vec<ExportedPreset>>(&content) {
            return presets;
        }
    }
    
    vec![]
}

fn save_presets(presets: &[ExportedPreset]) -> Result<(), String> {
    let path = get_presets_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string_pretty(presets).map_err(|e| e.to_string())?;
    fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn export_data(
    capability_ids: Vec<String>,
    agent_ids: Vec<String>,
    preset_ids: Vec<String>,
    output_path: String,
) -> Result<(), String> {
    let bundled_path = get_bundled_registry_path();
    let custom_path = get_custom_registry_path();

    let mut dirs = vec![bundled_path];
    if custom_path.exists() {
        dirs.push(custom_path);
    }

    let all_caps = load_capabilities(&dirs);
    let all_agents = load_agents(&dirs);
    let all_presets = load_all_presets();

    let caps: Vec<UniversalCapability> = if capability_ids.is_empty() {
        all_caps.items
            .into_iter()
            .filter(|c| c.is_private())
            .collect()
    } else {
        all_caps.items
            .into_iter()
            .filter(|c| capability_ids.contains(&c.id().to_string()) && c.is_private())
            .collect()
    };

    let agents: Vec<AgentDefinition> = if agent_ids.is_empty() {
        all_agents.items
            .into_iter()
            .filter(|a| a.visibility == Visibility::Private)
            .collect()
    } else {
        all_agents.items
            .into_iter()
            .filter(|a| agent_ids.contains(&a.id.to_string()) && a.visibility == Visibility::Private)
            .collect()
    };

    let presets: Vec<ExportedPreset> = if preset_ids.is_empty() {
        all_presets
    } else {
        all_presets
            .into_iter()
            .filter(|p| preset_ids.contains(&p.id))
            .collect()
    };

    let export = ExportData {
        version: "1.0".to_string(),
        exported_at: chrono::Utc::now().to_rfc3339(),
        capabilities: caps,
        agents,
        presets,
    };

    let json = serde_json::to_string_pretty(&export).map_err(|e| e.to_string())?;
    let path = std::path::Path::new(&output_path);
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, &json).map_err(|e| format!("Write failed: {}", e))?;
    fs::rename(&tmp_path, path).map_err(|e| format!("Rename failed: {}", e))?;

    Ok(())
}

#[tauri::command]
pub fn import_data(data: ExportData, rename_conflicts: bool) -> ImportResult {
    let mut caps_imported = 0;
    let mut agents_imported = 0;
    let mut presets_imported = 0;
    let mut conflicts = Vec::new();
    
    let custom_path = get_custom_registry_path();
    
    for cap in data.capabilities {
        let subdir = match &cap {
            UniversalCapability::Mcp(_) => "mcps",
            UniversalCapability::Rule(_) => "rules",
            UniversalCapability::Skill(_) => "skills",
            UniversalCapability::Hook(_) => "hooks",
            UniversalCapability::Plugin(_) => "plugins",
            UniversalCapability::Custom(_) => "customs",
        };
        
        let dir_path = custom_path.join("capabilities").join(subdir);
        if fs::create_dir_all(&dir_path).is_err() {
            conflicts.push(ImportConflict {
                item_type: "capability".to_string(),
                item_id: cap.id().to_string(),
                message: "Failed to create directory".to_string(),
            });
            continue;
        }
        
        let filename = cap.id().to_string().replace('/', "_") + ".json";
        let file_path = dir_path.join(&filename);
        
        let final_cap = if file_path.exists() {
            if rename_conflicts {
                let new_id = format!("{}-imported", cap.id());
                let new_filename = new_id.replace('/', "_") + ".json";
                let new_path = dir_path.join(&new_filename);
                
                conflicts.push(ImportConflict {
                    item_type: "capability".to_string(),
                    item_id: cap.id().to_string(),
                    message: format!("Renamed to {}", new_id),
                });
                
                (new_path, cap)
            } else {
                conflicts.push(ImportConflict {
                    item_type: "capability".to_string(),
                    item_id: cap.id().to_string(),
                    message: "Already exists, skipped".to_string(),
                });
                continue;
            }
        } else {
            (file_path, cap)
        };
        
        match serde_json::to_string_pretty(&final_cap.1) {
            Ok(json) => {
                if fs::write(&final_cap.0, json).is_ok() {
                    caps_imported += 1;
                }
            }
            Err(e) => {
                conflicts.push(ImportConflict {
                    item_type: "capability".to_string(),
                    item_id: final_cap.1.id().to_string(),
                    message: format!("Serialization failed: {}", e),
                });
            }
        }
    }
    
    let agents_path = get_custom_agents_path();
    if fs::create_dir_all(&agents_path).is_ok() {
        for agent in data.agents {
            let filename = agent.id.to_string().replace('/', "_") + ".json";
            let file_path = agents_path.join(&filename);
            
            if file_path.exists() {
                if rename_conflicts {
                    let new_id = format!("{}-imported", agent.id);
                    let new_filename = new_id.replace('/', "_") + ".json";
                    let new_path = agents_path.join(&new_filename);
                    
                    conflicts.push(ImportConflict {
                        item_type: "agent".to_string(),
                        item_id: agent.id.to_string(),
                        message: format!("Renamed to {}", new_id),
                    });
                    
                    if let Ok(json) = serde_json::to_string_pretty(&agent) {
                        if fs::write(&new_path, json).is_ok() {
                            agents_imported += 1;
                        }
                    }
                } else {
                    conflicts.push(ImportConflict {
                        item_type: "agent".to_string(),
                        item_id: agent.id.to_string(),
                        message: "Already exists, skipped".to_string(),
                    });
                }
            } else if let Ok(json) = serde_json::to_string_pretty(&agent) {
                if fs::write(&file_path, json).is_ok() {
                    agents_imported += 1;
                }
            }
        }
    }
    
    let mut existing_presets = load_all_presets();
    for preset in data.presets {
        if existing_presets.iter().any(|p| p.id == preset.id) {
            if rename_conflicts {
                let mut new_preset = preset.clone();
                new_preset.id = format!("{}-imported", preset.id);
                new_preset.name = format!("{} (Imported)", preset.name);
                
                conflicts.push(ImportConflict {
                    item_type: "preset".to_string(),
                    item_id: preset.id.clone(),
                    message: format!("Renamed to {}", new_preset.id),
                });
                
                existing_presets.push(new_preset);
                presets_imported += 1;
            } else {
                conflicts.push(ImportConflict {
                    item_type: "preset".to_string(),
                    item_id: preset.id.clone(),
                    message: "Already exists, skipped".to_string(),
                });
            }
        } else {
            existing_presets.push(preset);
            presets_imported += 1;
        }
    }
    
    if presets_imported > 0 {
        let _ = save_presets(&existing_presets);
    }
    
    let total = caps_imported + agents_imported + presets_imported;
    let message = if total > 0 {
        format!(
            "Imported {} capabilities, {} agents, {} presets",
            caps_imported, agents_imported, presets_imported
        )
    } else {
        "No items imported".to_string()
    };
    
    ImportResult {
        success: total > 0 && conflicts.iter().all(|c| c.message.starts_with("Renamed")),
        message,
        capabilities_imported: caps_imported,
        agents_imported,
        presets_imported,
        conflicts,
    }
}

#[tauri::command]
pub fn validate_import_data(json_string: String) -> Result<ExportData, String> {
    serde_json::from_str::<ExportData>(&json_string)
        .map_err(|e| format!("Invalid import file: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_data_serialization() {
        let export = ExportData {
            version: "1.0".to_string(),
            exported_at: "2026-02-21T10:00:00Z".to_string(),
            capabilities: vec![],
            agents: vec![],
            presets: vec![],
        };
        
        let json = serde_json::to_string(&export).unwrap();
        let parsed: ExportData = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, "1.0");
    }
}
