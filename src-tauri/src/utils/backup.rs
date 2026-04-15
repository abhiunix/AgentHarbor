use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    pub backup_id: String,
    pub project_path: String,
    pub adapter_id: String,
    pub timestamp: u64,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInfo {
    pub backup_id: String,
    pub adapter_id: String,
    pub timestamp: u64,
    pub file_count: usize,
}

fn get_backups_dir() -> PathBuf {
    crate::utils::paths::app_data_dir().join("backups")
}

fn generate_project_hash(project_path: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    let mut hasher = DefaultHasher::new();
    project_path.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn generate_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn get_backup_path(project_path: &str, timestamp: u64) -> PathBuf {
    let hash = generate_project_hash(project_path);
    get_backups_dir().join(&hash).join(timestamp.to_string())
}

pub fn create_backup(
    project_path: &str,
    adapter_id: &str,
    files: &[PathBuf],
) -> Result<String, String> {
    let timestamp = generate_timestamp();
    let backup_dir = get_backup_path(project_path, timestamp);

    fs::create_dir_all(&backup_dir)
        .map_err(|e| format!("Failed to create backup directory: {}", e))?;

    let project_root = Path::new(project_path);
    let mut backed_up_files = vec![];

    for file_path in files {
        if file_path.exists() {
            let stored_path = match file_path.strip_prefix(project_root) {
                Ok(rel) => rel.to_string_lossy().to_string(),
                Err(_) => file_path.to_string_lossy().to_string(),
            };

            let safe_backup_name = stored_path
                .replace('/', "_")
                .replace('\\', "_");
            let backup_file_path = backup_dir.join(&safe_backup_name);

            if let Some(parent) = backup_file_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create backup subdirectory: {}", e))?;
            }

            fs::copy(file_path, &backup_file_path)
                .map_err(|e| format!("Failed to copy file to backup: {}", e))?;

            backed_up_files.push(stored_path);
        }
    }

    let manifest = BackupManifest {
        backup_id: format!("{}_{}", generate_project_hash(project_path), timestamp),
        project_path: project_path.to_string(),
        adapter_id: adapter_id.to_string(),
        timestamp,
        files: backed_up_files,
    };

    let manifest_path = backup_dir.join("manifest.json");
    let manifest_content = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("Failed to serialize manifest: {}", e))?;

    crate::utils::paths::atomic_write_str(&manifest_path, &manifest_content)?;

    Ok(manifest.backup_id)
}

pub fn restore_backup(backup_id: &str) -> Result<Vec<PathBuf>, String> {
    let (project_hash, timestamp_str) = backup_id.rsplit_once('_')
        .ok_or_else(|| "Invalid backup ID format".to_string())?;

    let timestamp: u64 = timestamp_str
        .parse()
        .map_err(|_| "Invalid timestamp in backup ID".to_string())?;

    let backup_dir = get_backups_dir()
        .join(project_hash)
        .join(timestamp.to_string());

    let manifest_path = backup_dir.join("manifest.json");
    if !manifest_path.exists() {
        return Err("Backup manifest not found".to_string());
    }

    let manifest_content = fs::read_to_string(&manifest_path)
        .map_err(|e| format!("Failed to read manifest: {}", e))?;
    let manifest: BackupManifest = serde_json::from_str(&manifest_content)
        .map_err(|e| format!("Failed to parse manifest: {}", e))?;

    let project_root = Path::new(&manifest.project_path);
    let mut restored_files = vec![];

    for stored_path in &manifest.files {
        let safe_backup_name = stored_path
            .replace('/', "_")
            .replace('\\', "_");
        let backup_file = backup_dir.join(&safe_backup_name);

        // Path traversal guard: reject paths containing ".."
        if stored_path.contains("..") {
            return Err(format!("Path traversal detected in backup manifest: {}", stored_path));
        }

        let original_file = {
            let p = Path::new(stored_path);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                project_root.join(stored_path)
            }
        };

        if backup_file.exists() {
            if let Some(parent) = original_file.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create directory: {}", e))?;
            }

            fs::copy(&backup_file, &original_file)
                .map_err(|e| format!("Failed to restore file: {}", e))?;

            restored_files.push(original_file);
        }
    }

    Ok(restored_files)
}

pub fn list_backups(project_path: &str) -> Result<Vec<BackupInfo>, String> {
    let hash = generate_project_hash(project_path);
    let project_backups_dir = get_backups_dir().join(&hash);

    if !project_backups_dir.exists() {
        return Ok(vec![]);
    }

    let mut backups = vec![];

    let entries = fs::read_dir(&project_backups_dir)
        .map_err(|e| format!("Failed to read backups directory: {}", e))?;

    for entry in entries.flatten() {
        let manifest_path = entry.path().join("manifest.json");
        if manifest_path.exists() {
            if let Ok(content) = fs::read_to_string(&manifest_path) {
                if let Ok(manifest) = serde_json::from_str::<BackupManifest>(&content) {
                    backups.push(BackupInfo {
                        backup_id: manifest.backup_id,
                        adapter_id: manifest.adapter_id,
                        timestamp: manifest.timestamp,
                        file_count: manifest.files.len(),
                    });
                }
            }
        }
    }

    backups.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(backups)
}

pub fn cleanup_old_backups(retention_days: u64) -> Result<usize, String> {
    let cutoff = generate_timestamp().saturating_sub(retention_days * 24 * 60 * 60);
    let backups_dir = get_backups_dir();

    if !backups_dir.exists() {
        return Ok(0);
    }

    let mut removed_count = 0;

    let project_dirs = fs::read_dir(&backups_dir)
        .map_err(|e| format!("Failed to read backups directory: {}", e))?;

    for project_entry in project_dirs.flatten() {
        if !project_entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }

        let backup_dirs = fs::read_dir(project_entry.path())
            .map_err(|e| format!("Failed to read project backup directory: {}", e))?;

        for backup_entry in backup_dirs.flatten() {
            let manifest_path = backup_entry.path().join("manifest.json");
            if manifest_path.exists() {
                if let Ok(content) = fs::read_to_string(&manifest_path) {
                    if let Ok(manifest) = serde_json::from_str::<BackupManifest>(&content) {
                        if manifest.timestamp < cutoff {
                            if fs::remove_dir_all(backup_entry.path()).is_ok() {
                                removed_count += 1;
                            }
                        }
                    }
                }
            }
        }

        if fs::read_dir(project_entry.path()).map(|mut e| e.next().is_none()).unwrap_or(false) {
            let _ = fs::remove_dir(project_entry.path());
        }
    }

    Ok(removed_count)
}

pub fn delete_backup(backup_id: &str) -> Result<(), String> {
    let (project_hash, timestamp_str) = backup_id.rsplit_once('_')
        .ok_or_else(|| "Invalid backup ID format".to_string())?;

    let timestamp: u64 = timestamp_str
        .parse()
        .map_err(|_| "Invalid timestamp in backup ID".to_string())?;

    let backup_dir = get_backups_dir()
        .join(project_hash)
        .join(timestamp.to_string());

    if backup_dir.exists() {
        fs::remove_dir_all(&backup_dir)
            .map_err(|e| format!("Failed to delete backup: {}", e))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_generate_project_hash() {
        let hash1 = generate_project_hash("/path/to/project");
        let hash2 = generate_project_hash("/path/to/project");
        let hash3 = generate_project_hash("/path/to/other");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_eq!(hash1.len(), 16);
    }

    #[test]
    fn test_backup_manifest_serialization() {
        let manifest = BackupManifest {
            backup_id: "test_123".to_string(),
            project_path: "/path/to/project".to_string(),
            adapter_id: "claude-code".to_string(),
            timestamp: 1234567890,
            files: vec!["file1.txt".to_string(), "file2.txt".to_string()],
        };

        let json = serde_json::to_string(&manifest).unwrap();
        let parsed: BackupManifest = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.backup_id, manifest.backup_id);
        assert_eq!(parsed.files.len(), 2);
    }

    #[test]
    fn test_backup_info_serialization() {
        let info = BackupInfo {
            backup_id: "test_123".to_string(),
            adapter_id: "cursor".to_string(),
            timestamp: 1234567890,
            file_count: 5,
        };

        let json = serde_json::to_string(&info).unwrap();
        let parsed: BackupInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.backup_id, info.backup_id);
        assert_eq!(parsed.file_count, 5);
    }
}
