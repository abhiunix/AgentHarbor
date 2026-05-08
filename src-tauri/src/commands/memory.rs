use std::fs;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

// ── Memory file listing (Claude Code project memory/*.md) ─────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFileEntry {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub modified_at: u64,
}

/// Derives the Claude Code project memory directory from an absolute project path.
/// Claude Code uses the path with '/' replaced by '-' as the project identifier.
fn project_memory_dir(project_path: &str) -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let id = project_path.replace('/', "-");
    home.join(".claude").join("projects").join(id).join("memory")
}

fn global_memory_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".claude").join("memory")
}

#[tauri::command]
pub fn list_memory_files(project_path: Option<String>) -> Vec<MemoryFileEntry> {
    let dir = match &project_path {
        Some(p) => project_memory_dir(p),
        None => global_memory_dir(),
    };
    if !dir.exists() {
        return vec![];
    }
    let mut entries = vec![];
    if let Ok(read) = fs::read_dir(&dir) {
        for entry in read.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            if let (Some(name), Ok(meta)) = (
                path.file_name().map(|n| n.to_string_lossy().to_string()),
                entry.metadata(),
            ) {
                let modified_at = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                entries.push(MemoryFileEntry {
                    name,
                    path: path.to_string_lossy().to_string(),
                    size_bytes: meta.len(),
                    modified_at,
                });
            }
        }
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

#[tauri::command]
pub fn read_memory_file(file_path: String) -> Result<String, String> {
    if file_path.contains("..") {
        return Err("Invalid path".to_string());
    }
    let path = PathBuf::from(&file_path);
    if !path.exists() {
        return Ok(String::new());
    }
    fs::read_to_string(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn write_memory_file(file_path: String, content: String) -> Result<(), String> {
    if file_path.contains("..") {
        return Err("Invalid path".to_string());
    }
    let path = PathBuf::from(&file_path);
    crate::utils::paths::atomic_write_str(&path, &content)
}

#[tauri::command]
pub fn create_memory_file(project_path: Option<String>, filename: String) -> Result<MemoryFileEntry, String> {
    if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
        return Err("Invalid filename".to_string());
    }
    let name = if filename.ends_with(".md") {
        filename.clone()
    } else {
        format!("{}.md", filename)
    };
    let dir = match &project_path {
        Some(p) => project_memory_dir(p),
        None => global_memory_dir(),
    };
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create directory: {}", e))?;
    let path = dir.join(&name);
    if path.exists() {
        return Err(format!("File '{}' already exists", name));
    }
    fs::write(&path, "").map_err(|e| e.to_string())?;
    Ok(MemoryFileEntry {
        name,
        path: path.to_string_lossy().to_string(),
        size_bytes: 0,
        modified_at: 0,
    })
}

#[tauri::command]
pub fn delete_memory_file(file_path: String) -> Result<(), String> {
    if file_path.contains("..") {
        return Err("Invalid path".to_string());
    }
    let path = PathBuf::from(&file_path);
    if !path.exists() {
        return Ok(());
    }
    fs::remove_file(&path).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMemory {
    pub agent_name: String,
    pub path: String,
    pub size_bytes: u64,
    pub file_count: usize,
}

fn get_dir_size(path: &PathBuf, max_depth: u32) -> (u64, usize) {
    if max_depth == 0 {
        return (0, 0);
    }
    let mut total_size = 0u64;
    let mut file_count = 0usize;

    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            // Skip symlinks to prevent infinite loops
            if entry.file_type().map(|t| t.is_symlink()).unwrap_or(false) {
                continue;
            }
            let entry_path = entry.path();
            if entry_path.is_dir() {
                let (sub_size, sub_count) = get_dir_size(&entry_path, max_depth - 1);
                total_size += sub_size;
                file_count += sub_count;
            } else if let Ok(metadata) = entry.metadata() {
                total_size += metadata.len();
                file_count += 1;
            }
        }
    }

    (total_size, file_count)
}

#[tauri::command]
pub fn read_project_memory(project_path: String) -> Result<String, String> {
    if project_path.contains("..") {
        return Err("Invalid project path".to_string());
    }
    let path = PathBuf::from(&project_path).join("CLAUDE.md");
    if !path.exists() {
        return Ok(String::new());
    }
    fs::read_to_string(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn write_project_memory(project_path: String, content: String) -> Result<(), String> {
    if project_path.contains("..") {
        return Err("Invalid project path".to_string());
    }
    let path = PathBuf::from(&project_path).join("CLAUDE.md");
    crate::utils::paths::atomic_write_str(&path, &content)
}

#[tauri::command]
pub fn list_agent_memory(project_path: String) -> Vec<AgentMemory> {
    let path = PathBuf::from(&project_path);
    let memory_dir = path.join(".claude").join("agent-memory");
    
    if !memory_dir.exists() {
        return vec![];
    }
    
    let mut memories = vec![];
    
    if let Ok(entries) = fs::read_dir(&memory_dir) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if entry_path.is_dir() {
                if let Some(name) = entry_path.file_name() {
                    let agent_name = name.to_string_lossy().to_string();
                    let (size_bytes, file_count) = get_dir_size(&entry_path, 20);
                    
                    memories.push(AgentMemory {
                        agent_name,
                        path: entry_path.to_string_lossy().to_string(),
                        size_bytes,
                        file_count,
                    });
                }
            }
        }
    }
    
    memories.sort_by(|a, b| a.agent_name.cmp(&b.agent_name));
    memories
}

#[tauri::command]
pub fn list_global_agent_memory() -> Vec<AgentMemory> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let memory_dir = home.join(".claude").join("agent-memory");
    
    if !memory_dir.exists() {
        return vec![];
    }
    
    let mut memories = vec![];
    
    if let Ok(entries) = fs::read_dir(&memory_dir) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if entry_path.is_dir() {
                if let Some(name) = entry_path.file_name() {
                    let agent_name = name.to_string_lossy().to_string();
                    let (size_bytes, file_count) = get_dir_size(&entry_path, 20);
                    
                    memories.push(AgentMemory {
                        agent_name,
                        path: entry_path.to_string_lossy().to_string(),
                        size_bytes,
                        file_count,
                    });
                }
            }
        }
    }
    
    memories.sort_by(|a, b| a.agent_name.cmp(&b.agent_name));
    memories
}

#[tauri::command]
pub fn clear_agent_memory(memory_path: String) -> Result<(), String> {
    let path = PathBuf::from(&memory_path);
    
    if !path.exists() {
        return Ok(());
    }
    
    if let Ok(entries) = fs::read_dir(&path) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if entry_path.is_dir() {
                fs::remove_dir_all(&entry_path)
                    .map_err(|e| format!("Failed to remove directory: {}", e))?;
            } else {
                fs::remove_file(&entry_path)
                    .map_err(|e| format!("Failed to remove file: {}", e))?;
            }
        }
    }
    
    Ok(())
}

#[tauri::command]
pub fn clear_all_agent_memory(project_path: String) -> Result<(), String> {
    let path = PathBuf::from(&project_path);
    let memory_dir = path.join(".claude").join("agent-memory");
    
    if !memory_dir.exists() {
        return Ok(());
    }
    
    if let Ok(entries) = fs::read_dir(&memory_dir) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if entry_path.is_dir() {
                if let Ok(sub_entries) = fs::read_dir(&entry_path) {
                    for sub_entry in sub_entries.flatten() {
                        let sub_path = sub_entry.path();
                        if sub_path.is_dir() {
                            fs::remove_dir_all(&sub_path)
                                .map_err(|e| format!("Failed to remove: {}", e))?;
                        } else {
                            fs::remove_file(&sub_path)
                                .map_err(|e| format!("Failed to remove: {}", e))?;
                        }
                    }
                }
            }
        }
    }
    
    Ok(())
}

#[tauri::command]
pub fn clear_all_global_memory() -> Result<(), String> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let memory_dir = home.join(".claude").join("agent-memory");
    
    if !memory_dir.exists() {
        return Ok(());
    }
    
    if let Ok(entries) = fs::read_dir(&memory_dir) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if entry_path.is_dir() {
                if let Ok(sub_entries) = fs::read_dir(&entry_path) {
                    for sub_entry in sub_entries.flatten() {
                        let sub_path = sub_entry.path();
                        if sub_path.is_dir() {
                            fs::remove_dir_all(&sub_path)
                                .map_err(|e| format!("Failed to remove: {}", e))?;
                        } else {
                            fs::remove_file(&sub_path)
                                .map_err(|e| format!("Failed to remove: {}", e))?;
                        }
                    }
                }
            }
        }
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_list_agent_memory_empty() {
        let temp = TempDir::new().unwrap();
        let result = list_agent_memory(temp.path().to_string_lossy().to_string());
        assert!(result.is_empty());
    }

    #[test]
    fn test_get_dir_size() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("test.txt");
        fs::write(&file_path, "hello").unwrap();
        
        let (size, count) = get_dir_size(&temp.path().to_path_buf(), 20);
        assert_eq!(count, 1);
        assert_eq!(size, 5);
    }
}
