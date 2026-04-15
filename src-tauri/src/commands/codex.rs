use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

fn codex_home() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".codex")
}

// ── Skill struct ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexSkill {
    pub name: String,
    pub file_path: String,
    pub has_scripts: bool,
    pub has_resources: bool,
}

// ── List skills ─────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_codex_skills(project_path: Option<String>) -> Result<Vec<CodexSkill>, String> {
    let skills_dir = match &project_path {
        Some(p) => {
            if p.contains("..") {
                return Err("Invalid project path".to_string());
            }
            PathBuf::from(p).join(".codex").join("skills")
        }
        None => codex_home().join("skills"),
    };

    if !skills_dir.exists() {
        return Ok(Vec::new());
    }

    let mut skills = Vec::new();

    if let Ok(entries) = fs::read_dir(&skills_dir) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if entry_path.is_dir() {
                if let Some(name) = entry_path.file_name() {
                    let name = name.to_string_lossy().to_string();
                    let skill_md = entry_path.join("SKILL.md");
                    if !skill_md.exists() {
                        continue;
                    }
                    let has_scripts = entry_path.join("scripts").exists();
                    let has_resources = entry_path.join("references").exists()
                        || entry_path.join("resources").exists();

                    skills.push(CodexSkill {
                        name,
                        file_path: entry_path.to_string_lossy().to_string(),
                        has_scripts,
                        has_resources,
                    });
                }
            }
        }
    }

    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

// ── Read skill file ─────────────────────────────────────────────────────────

#[tauri::command]
pub fn read_codex_skill_file(file_path: String) -> Result<String, String> {
    if file_path.contains("..") {
        return Err("Invalid file path".to_string());
    }
    fs::read_to_string(&file_path)
        .map_err(|e| format!("Failed to read file: {}", e))
}

// ── Config read/write ───────────────────────────────────────────────────────

fn config_path() -> PathBuf {
    codex_home().join("config.toml")
}

#[tauri::command]
pub fn read_codex_config() -> Result<String, String> {
    let path = config_path();
    if !path.exists() {
        return Ok(String::new());
    }
    fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read config: {}", e))
}

#[tauri::command]
pub fn write_codex_config(content: String) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }
    crate::utils::paths::atomic_write_str(&path, &content)
}
