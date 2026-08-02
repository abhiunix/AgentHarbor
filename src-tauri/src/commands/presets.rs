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
    load_user_presets()
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
    fn no_presets_without_user_file() {
        // Bundled sample presets were removed: their hardcoded slug IDs
        // (community/github-mcp, ...) predate the registry's uuid/hash ID
        // scheme and resolved to nothing.
        assert!(get_presets().is_empty() || get_presets().iter().all(|p| !p.is_bundled));
    }
}
