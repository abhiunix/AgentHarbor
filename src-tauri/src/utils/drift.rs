use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftInfo {
    pub has_drift: bool,
    pub files: Vec<DriftFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DriftChangeType {
    Add,
    Modify,
    Remove,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftFile {
    pub path: String,
    pub relative_path: String,
    pub change_type: DriftChangeType,
    pub expected_hash: Option<String>,
    pub current_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployState {
    pub files: HashMap<String, FileState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileState {
    pub hash: String,
    pub content: String,
}

fn get_deploy_state_path(project_path: &str) -> PathBuf {
    PathBuf::from(project_path)
        .join(".agentharbor")
        .join("deploy-state.json")
}

fn compute_hash(content: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let normalized = crate::utils::paths::normalize_line_endings(content);
    let mut hasher = DefaultHasher::new();
    normalized.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

pub fn save_deploy_state(project_path: &str, files: &HashMap<String, String>) -> Result<(), String> {
    let state_path = get_deploy_state_path(project_path);
    
    if let Some(parent) = state_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    
    let mut file_states = HashMap::new();
    for (path, content) in files {
        file_states.insert(path.clone(), FileState {
            hash: compute_hash(content),
            content: content.clone(),
        });
    }
    
    let state = DeployState { files: file_states };
    let json = serde_json::to_string_pretty(&state).map_err(|e| e.to_string())?;
    fs::write(&state_path, json).map_err(|e| e.to_string())?;
    
    Ok(())
}

pub fn load_deploy_state(project_path: &str) -> Option<DeployState> {
    let state_path = get_deploy_state_path(project_path);
    if !state_path.exists() {
        return None;
    }
    
    fs::read_to_string(&state_path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
}

fn resolve_path(project: &Path, file_path: &str) -> PathBuf {
    let p = PathBuf::from(file_path);
    if p.is_absolute() {
        p
    } else {
        project.join(file_path)
    }
}

pub fn detect_drift(project_path: &str) -> DriftInfo {
    let state = match load_deploy_state(project_path) {
        Some(s) => s,
        None => {
            return DriftInfo {
                has_drift: false,
                files: vec![],
            };
        }
    };
    
    let project = PathBuf::from(project_path);
    let mut drift_files = vec![];
    
    for (file_path, file_state) in &state.files {
        let full_path = resolve_path(&project, file_path);
        let relative_path = file_path.clone();
        
        if !full_path.exists() {
            drift_files.push(DriftFile {
                path: full_path.to_string_lossy().to_string(),
                relative_path,
                change_type: DriftChangeType::Remove,
                expected_hash: Some(file_state.hash.clone()),
                current_hash: None,
            });
        } else if let Ok(current_content) = fs::read_to_string(&full_path) {
            let current_hash = compute_hash(&current_content);
            if current_hash != file_state.hash {
                drift_files.push(DriftFile {
                    path: full_path.to_string_lossy().to_string(),
                    relative_path,
                    change_type: DriftChangeType::Modify,
                    expected_hash: Some(file_state.hash.clone()),
                    current_hash: Some(current_hash),
                });
            }
        }
    }
    
    DriftInfo {
        has_drift: !drift_files.is_empty(),
        files: drift_files,
    }
}

pub fn accept_drift(project_path: &str) -> Result<(), String> {
    let state = match load_deploy_state(project_path) {
        Some(s) => s,
        None => return Ok(()),
    };
    
    let project = PathBuf::from(project_path);
    let mut new_files = HashMap::new();
    
    for (file_path, _) in state.files {
        let full_path = resolve_path(&project, &file_path);
        if let Ok(content) = fs::read_to_string(&full_path) {
            new_files.insert(file_path, content);
        }
    }
    
    save_deploy_state(project_path, &new_files)?;
    Ok(())
}

pub fn restore_drift(project_path: &str) -> Result<(), String> {
    let state = match load_deploy_state(project_path) {
        Some(s) => s,
        None => return Err("No deploy state found".to_string()),
    };
    
    let project = PathBuf::from(project_path);
    
    for (file_path, file_state) in state.files {
        let full_path = resolve_path(&project, &file_path);
        
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        
        fs::write(&full_path, &file_state.content).map_err(|e| e.to_string())?;
    }
    
    Ok(())
}

pub fn get_drift_diff(project_path: &str, file_path: &str) -> Option<(String, String)> {
    let state = load_deploy_state(project_path)?;
    let file_state = state.files.get(file_path)?;
    
    let project = PathBuf::from(project_path);
    let full_path = resolve_path(&project, file_path);
    
    let current_content = fs::read_to_string(&full_path).unwrap_or_default();
    
    Some((file_state.content.clone(), current_content))
}
