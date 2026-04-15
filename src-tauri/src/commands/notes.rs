use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

fn notes_root() -> PathBuf {
    crate::utils::paths::app_data_dir().join("private_notes")
}

/// Joins notes_root() with relative_path, creates root if missing, and validates
/// that the result is under notes_root(). Returns error if path escapes (e.g. "..").
fn resolve_notes_path(relative_path: &str) -> Result<PathBuf, String> {
    let root = notes_root();
    if !root.exists() {
        fs::create_dir_all(&root).map_err(|e| format!("Failed to create notes root: {}", e))?;
    }

    let normalized = relative_path.replace('\\', "/").trim_matches('/').to_string();
    if !normalized.is_empty() {
        for segment in normalized.split('/') {
            if segment == ".." || segment == "." {
                return Err("Invalid path: parent or current segments not allowed".to_string());
            }
        }
    }
    if relative_path.starts_with('/') {
        return Err("Invalid path: absolute not allowed".to_string());
    }

    let full = if normalized.is_empty() {
        root.clone()
    } else {
        root.join(&normalized)
    };
    if !full.starts_with(&root) {
        return Err("Path escapes notes root".to_string());
    }
    Ok(if full.exists() {
        dunce::canonicalize(&full).unwrap_or(full)
    } else {
        full
    })
}

fn root_canonical() -> Result<PathBuf, String> {
    let root = notes_root();
    if !root.exists() {
        fs::create_dir_all(&root).map_err(|e| format!("Failed to create notes root: {}", e))?;
    }
    dunce::canonicalize(&root).map_err(|e| format!("Failed to resolve root: {}", e))
}

fn to_relative_path(root: &Path, full: &Path) -> String {
    full.strip_prefix(root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| full.to_string_lossy().into_owned())
}

fn validate_folder_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Folder name cannot be empty".to_string());
    }
    if name.contains('/') || name.contains('\\') || name == "." || name == ".." {
        return Err("Folder name cannot contain path separators or . / ..".to_string());
    }
    Ok(())
}

fn validate_file_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("File name cannot be empty".to_string());
    }
    if name.contains('/') || name.contains('\\') || name == "." || name == ".." {
        return Err("File name cannot contain path separators or . / ..".to_string());
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotesEntry {
    pub name: String,
    pub relative_path: String,
    pub is_folder: bool,
}

#[tauri::command]
pub fn list_notes_entries(relative_path: String) -> Result<Vec<NotesEntry>, String> {
    let root = root_canonical()?;
    let dir_path = resolve_notes_path(&relative_path)?;

    if !dir_path.exists() {
        return Ok(vec![]);
    }
    if !dir_path.is_dir() {
        return Err("Path is not a directory".to_string());
    }

    let mut entries = Vec::new();
    for e in fs::read_dir(&dir_path).map_err(|e| format!("Failed to read directory: {}", e))? {
        let e = e.map_err(|e| format!("Failed to read entry: {}", e))?;
        let name = e
            .file_name()
            .to_string_lossy()
            .into_owned();
        let full = e.path();
        let rel = to_relative_path(&root, &full);
        let is_folder = full.is_dir();
        entries.push(NotesEntry {
            name,
            relative_path: rel,
            is_folder,
        });
    }
    entries.sort_by(|a, b| {
        let a_lower = a.name.to_lowercase();
        let b_lower = b.name.to_lowercase();
        if a.is_folder != b.is_folder {
            if a.is_folder {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            }
        } else {
            a_lower.cmp(&b_lower)
        }
    });
    Ok(entries)
}

#[tauri::command]
pub fn read_note_content(relative_path: String) -> Result<String, String> {
    let path = resolve_notes_path(&relative_path)?;
    if !path.exists() {
        return Err("File not found".to_string());
    }
    if path.is_dir() {
        return Err("Path is a directory, not a file".to_string());
    }
    fs::read_to_string(&path).map_err(|e| format!("Failed to read file: {}", e))
}

#[tauri::command]
pub fn write_note_content(relative_path: String, content: String) -> Result<(), String> {
    let path = resolve_notes_path(&relative_path)?;
    if path.is_dir() {
        return Err("Path is a directory".to_string());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create parent: {}", e))?;
    }
    crate::utils::paths::atomic_write_str(&path, &content)?;
    Ok(())
}

#[tauri::command]
pub fn create_notes_folder(parent_relative_path: String, name: String) -> Result<NotesEntry, String> {
    validate_folder_name(&name)?;
    let root = root_canonical()?;
    let parent = resolve_notes_path(&parent_relative_path)?;
    if !parent.exists() {
        fs::create_dir_all(&parent).map_err(|e| format!("Failed to create parent: {}", e))?;
    }
    if !parent.is_dir() {
        return Err("Parent is not a directory".to_string());
    }
    let new_dir = parent.join(&name);
    if new_dir.exists() {
        return Err("A file or folder with that name already exists".to_string());
    }
    fs::create_dir(&new_dir).map_err(|e| format!("Failed to create folder: {}", e))?;
    let relative_path = to_relative_path(&root, &new_dir);
    Ok(NotesEntry {
        name,
        relative_path,
        is_folder: true,
    })
}

#[tauri::command]
pub fn create_notes_file(parent_relative_path: String, name: String) -> Result<NotesEntry, String> {
    validate_file_name(&name)?;
    let root = root_canonical()?;
    let parent = resolve_notes_path(&parent_relative_path)?;
    if !parent.exists() {
        fs::create_dir_all(&parent).map_err(|e| format!("Failed to create parent: {}", e))?;
    }
    if !parent.is_dir() {
        return Err("Parent is not a directory".to_string());
    }
    let new_file = parent.join(&name);
    if new_file.exists() {
        return Err("A file or folder with that name already exists".to_string());
    }
    fs::write(&new_file, "").map_err(|e| format!("Failed to create file: {}", e))?;
    let relative_path = to_relative_path(&root, &new_file);
    Ok(NotesEntry {
        name,
        relative_path,
        is_folder: false,
    })
}

#[tauri::command]
pub fn rename_notes_entry(relative_path: String, new_name: String) -> Result<NotesEntry, String> {
    if relative_path.is_empty() {
        return Err("Cannot rename root".to_string());
    }
    validate_file_name(&new_name)?;
    let root = root_canonical()?;
    let src = resolve_notes_path(&relative_path)?;
    if !src.exists() {
        return Err("Entry not found".to_string());
    }
    let parent = src.parent().ok_or("Invalid path")?;
    let dest = parent.join(&new_name);
    if dest.exists() {
        return Err("A file or folder with that name already exists".to_string());
    }
    fs::rename(&src, &dest).map_err(|e| format!("Failed to rename: {}", e))?;
    let new_relative = to_relative_path(&root, &dest);
    Ok(NotesEntry {
        name: new_name,
        relative_path: new_relative,
        is_folder: dest.is_dir(),
    })
}

#[tauri::command]
pub fn delete_notes_entry(relative_path: String) -> Result<(), String> {
    if relative_path.is_empty() {
        return Err("Cannot delete root".to_string());
    }
    let path = resolve_notes_path(&relative_path)?;
    if !path.exists() {
        return Err("Entry not found".to_string());
    }
    if path.is_dir() {
        let has_children = fs::read_dir(&path)
            .map_err(|e| format!("Failed to read directory: {}", e))?
            .next()
            .is_some();
        if has_children {
            return Err("Directory is not empty. Delete its contents first.".to_string());
        }
    }
    if path.is_dir() {
        fs::remove_dir(&path).map_err(|e| format!("Failed to delete folder: {}", e))?;
    } else {
        fs::remove_file(&path).map_err(|e| format!("Failed to delete file: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
pub fn move_notes_entry(relative_path: String, new_parent_relative_path: String) -> Result<NotesEntry, String> {
    if relative_path.is_empty() {
        return Err("Cannot move root".to_string());
    }
    let root = root_canonical()?;
    let src = resolve_notes_path(&relative_path)?;
    if !src.exists() {
        return Err("Entry not found".to_string());
    }
    let new_parent = resolve_notes_path(&new_parent_relative_path)?;
    if !new_parent.exists() {
        fs::create_dir_all(&new_parent).map_err(|e| format!("Failed to create destination folder: {}", e))?;
    }
    if !new_parent.is_dir() {
        return Err("Destination is not a directory".to_string());
    }
    let entry_name = src
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Invalid entry name")?;
    let new_rel_normalized = new_parent_relative_path.replace('\\', "/").trim_matches('/').to_string();
    if new_rel_normalized == relative_path || new_rel_normalized.starts_with(&format!("{}/", relative_path)) {
        return Err("Cannot move into itself or into a descendant".to_string());
    }
    let dest = new_parent.join(entry_name);
    if dest.exists() {
        return Err("A file or folder with that name already exists in the destination".to_string());
    }
    fs::rename(&src, &dest).map_err(|e| format!("Failed to move: {}", e))?;
    let new_relative = to_relative_path(&root, &dest);
    Ok(NotesEntry {
        name: entry_name.to_string(),
        relative_path: new_relative,
        is_folder: dest.is_dir(),
    })
}
