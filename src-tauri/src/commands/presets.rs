use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub capability_ids: Vec<String>,
    pub tags: Vec<String>,
    pub is_bundled: bool,
}

fn get_presets_file_path() -> PathBuf {
    crate::utils::paths::app_data_dir().join("presets.json")
}

fn get_bundled_presets() -> Vec<Preset> {
    vec![
        Preset {
            id: "bundled/full-stack-web".to_string(),
            name: "Full-Stack Web".to_string(),
            description: "Complete setup for full-stack web development with React, TypeScript, and Git workflows.".to_string(),
            capability_ids: vec![
                "community/github-mcp".to_string(),
                "community/filesystem-mcp".to_string(),
                "community/ts-strict-style".to_string(),
                "community/react-best-practices".to_string(),
                "community/react-component-gen".to_string(),
                "community/pre-commit-lint".to_string(),
            ],
            tags: vec!["web".to_string(), "react".to_string(), "typescript".to_string()],
            is_bundled: true,
        },
        Preset {
            id: "bundled/data-science".to_string(),
            name: "Data Science".to_string(),
            description: "Tools for data science workflows with database access and Python best practices.".to_string(),
            capability_ids: vec![
                "community/postgres-mcp".to_string(),
                "community/filesystem-mcp".to_string(),
                "community/python-pep8".to_string(),
                "community/api-scaffold".to_string(),
            ],
            tags: vec!["data".to_string(), "python".to_string(), "database".to_string()],
            is_bundled: true,
        },
    ]
}

fn load_user_presets() -> Vec<Preset> {
    let path = get_presets_file_path();
    if !path.exists() {
        return vec![];
    }

    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => vec![],
    }
}

fn save_user_presets(presets: &[Preset]) -> Result<(), String> {
    let path = get_presets_file_path();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    let user_presets: Vec<_> = presets.iter().filter(|p| !p.is_bundled).collect();
    let content = serde_json::to_string_pretty(&user_presets)
        .map_err(|e| format!("Failed to serialize presets: {}", e))?;

    let temp_path = path.with_extension("tmp");
    fs::write(&temp_path, &content)
        .map_err(|e| format!("Failed to write temp file: {}", e))?;
    fs::rename(&temp_path, &path)
        .map_err(|e| format!("Failed to rename temp file: {}", e))?;

    Ok(())
}

#[tauri::command]
pub fn get_presets() -> Vec<Preset> {
    let mut presets = get_bundled_presets();
    presets.extend(load_user_presets());
    presets
}

#[tauri::command]
pub fn save_preset(preset: Preset) -> Result<Preset, String> {
    let mut user_presets = load_user_presets();

    if let Some(existing) = user_presets.iter_mut().find(|p| p.id == preset.id) {
        *existing = preset.clone();
    } else {
        user_presets.push(preset.clone());
    }

    save_user_presets(&user_presets)?;
    Ok(preset)
}

#[tauri::command]
pub fn update_preset(preset: Preset) -> Result<Preset, String> {
    if preset.is_bundled {
        return Err("Cannot modify bundled presets".to_string());
    }

    let mut user_presets = load_user_presets();

    if let Some(existing) = user_presets.iter_mut().find(|p| p.id == preset.id) {
        *existing = preset.clone();
        save_user_presets(&user_presets)?;
        Ok(preset)
    } else {
        Err(format!("Preset '{}' not found", preset.id))
    }
}

#[tauri::command]
pub fn delete_preset(id: String) -> Result<(), String> {
    let mut user_presets = load_user_presets();
    let initial_len = user_presets.len();
    user_presets.retain(|p| p.id != id);

    if user_presets.len() == initial_len {
        return Err(format!("Preset '{}' not found", id));
    }

    save_user_presets(&user_presets)?;
    Ok(())
}

#[tauri::command]
pub fn add_capability_to_preset(preset_id: String, capability_id: String) -> Result<Preset, String> {
    let mut user_presets = load_user_presets();

    if let Some(preset) = user_presets.iter_mut().find(|p| p.id == preset_id) {
        if preset.is_bundled {
            return Err("Cannot modify bundled presets".to_string());
        }
        if !preset.capability_ids.contains(&capability_id) {
            preset.capability_ids.push(capability_id);
        }
        let updated = preset.clone();
        save_user_presets(&user_presets)?;
        Ok(updated)
    } else {
        Err(format!("Preset '{}' not found", preset_id))
    }
}

#[tauri::command]
pub fn remove_capability_from_preset(preset_id: String, capability_id: String) -> Result<Preset, String> {
    let mut user_presets = load_user_presets();

    if let Some(preset) = user_presets.iter_mut().find(|p| p.id == preset_id) {
        if preset.is_bundled {
            return Err("Cannot modify bundled presets".to_string());
        }
        preset.capability_ids.retain(|c| c != &capability_id);
        let updated = preset.clone();
        save_user_presets(&user_presets)?;
        Ok(updated)
    } else {
        Err(format!("Preset '{}' not found", preset_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bundled_presets() {
        let presets = get_bundled_presets();
        assert_eq!(presets.len(), 2);
        assert!(presets.iter().all(|p| p.is_bundled));
    }

    #[test]
    fn test_full_stack_web_preset() {
        let presets = get_bundled_presets();
        let fsweb = presets.iter().find(|p| p.id == "bundled/full-stack-web");
        assert!(fsweb.is_some());
        let preset = fsweb.unwrap();
        assert_eq!(preset.name, "Full-Stack Web");
        assert!(preset.capability_ids.contains(&"community/github-mcp".to_string()));
    }

    #[test]
    fn test_data_science_preset() {
        let presets = get_bundled_presets();
        let ds = presets.iter().find(|p| p.id == "bundled/data-science");
        assert!(ds.is_some());
        let preset = ds.unwrap();
        assert_eq!(preset.name, "Data Science");
        assert!(preset.capability_ids.contains(&"community/postgres-mcp".to_string()));
    }
}
