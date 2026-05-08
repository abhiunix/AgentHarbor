use std::fs;
use std::path::PathBuf;

use crate::models::UniversalCapability;

fn get_custom_registry_path() -> PathBuf {
    crate::utils::paths::app_data_dir()
        .join("registry")
        .join("custom")
}

fn get_capability_subdir(capability: &UniversalCapability) -> &'static str {
    match capability {
        UniversalCapability::Mcp(_) => "mcps",
        UniversalCapability::Rule(_) => "rules",
        UniversalCapability::Skill(_) => "skills",
        UniversalCapability::Hook(_) => "hooks",
        UniversalCapability::Plugin(_) => "plugins",
        UniversalCapability::Custom(_) => "customs",
    }
}

fn capability_filename(id: &str) -> String {
    id.replace('/', "_") + ".json"
}

#[tauri::command]
pub fn save_custom_capability(
    original_id: Option<String>,
    capability: UniversalCapability,
) -> Result<UniversalCapability, String> {
    if let Some(ref oid) = original_id {
        let current_id = capability.id().to_string();
        if oid != &current_id {
            return Err(format!(
                "ID cannot be changed: original {} vs incoming {}",
                oid, current_id
            ));
        }
    }

    let custom_path = get_custom_registry_path();
    let subdir = get_capability_subdir(&capability);
    let dir_path = custom_path.join("capabilities").join(subdir);
    
    fs::create_dir_all(&dir_path)
        .map_err(|e| format!("Failed to create directory: {}", e))?;
    
    let filename = capability_filename(&capability.id().to_string());
    let file_path = dir_path.join(&filename);
    
    let json = serde_json::to_string_pretty(&capability)
        .map_err(|e| format!("Failed to serialize capability: {}", e))?;
    
    crate::utils::paths::atomic_write_str(&file_path, &json)?;
    
    Ok(capability)
}

#[tauri::command]
pub fn delete_custom_capability(id: String, capability_type: String) -> Result<(), String> {
    let custom_path = get_custom_registry_path();
    
    let subdir = match capability_type.as_str() {
        "mcp" => "mcps",
        "rule" => "rules",
        "skill" => "skills",
        "hook" => "hooks",
        "plugin" => "plugins",
        "custom" => "customs",
        _ => return Err(format!("Unknown capability type: {}", capability_type)),
    };
    
    let filename = capability_filename(&id);
    let file_path = custom_path.join("capabilities").join(subdir).join(&filename);
    
    if file_path.exists() {
        fs::remove_file(&file_path)
            .map_err(|e| format!("Failed to delete file: {}", e))?;
    }
    
    Ok(())
}

#[tauri::command]
pub fn get_custom_capabilities_dir() -> String {
    get_custom_registry_path().to_string_lossy().to_string()
}

#[tauri::command]
pub fn list_custom_capabilities() -> Vec<String> {
    let custom_path = get_custom_registry_path();
    let caps_path = custom_path.join("capabilities");
    let mut ids = Vec::new();
    
    if !caps_path.exists() {
        return ids;
    }
    
    let subdirs = ["mcps", "rules", "skills", "hooks", "plugins", "customs"];
    for subdir in &subdirs {
        let subdir_path = caps_path.join(subdir);
        if let Ok(entries) = fs::read_dir(&subdir_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "json").unwrap_or(false) {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(cap) = serde_json::from_str::<UniversalCapability>(&content) {
                            ids.push(cap.id().to_string());
                        }
                    }
                }
            }
        }
    }
    
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::capability::{Rule, Visibility};
    use crate::models::CompositeId;
    use std::str::FromStr;

    #[test]
    fn test_capability_filename() {
        assert_eq!(capability_filename("user/my-mcp"), "user_my-mcp.json");
        assert_eq!(capability_filename("community/test"), "community_test.json");
    }

    #[test]
    fn test_save_rejects_id_mutation_on_update() {
        let id_b = CompositeId::from_str("user/bbb").unwrap();
        let rule = UniversalCapability::Rule(Rule {
            id: id_b.clone(),
            name: "Rule".to_string(),
            description: "d".to_string(),
            version: "1.0".to_string(),
            author: "user".to_string(),
            visibility: Visibility::Private,
            tags: vec![],
            scope: "global".to_string(),
            content: "content".to_string(),
            env: std::collections::HashMap::new(),
            compatible_agents: vec![],
            category: None,
            author_github: None,
            source_info: None,
            stats: None,
        });
        let result = save_custom_capability(Some("user/aaa".to_string()), rule);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("ID cannot be changed"));
    }
}
