use crate::utils::backup::{
    create_backup, restore_backup, list_backups, cleanup_old_backups, delete_backup,
    BackupInfo,
};
use std::path::PathBuf;

#[tauri::command]
pub fn create_project_backup(
    project_path: String,
    adapter_id: String,
    files: Vec<String>,
) -> Result<String, String> {
    let file_paths: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
    create_backup(&project_path, &adapter_id, &file_paths)
}

#[tauri::command]
pub fn restore_project_backup(backup_id: String) -> Result<Vec<String>, String> {
    let restored = restore_backup(&backup_id)?;
    Ok(restored.iter().map(|p| p.to_string_lossy().to_string()).collect())
}

#[tauri::command]
pub fn get_project_backups(project_path: String) -> Result<Vec<BackupInfo>, String> {
    list_backups(&project_path)
}

#[tauri::command]
pub fn cleanup_backups(retention_days: u64) -> Result<usize, String> {
    cleanup_old_backups(retention_days)
}

#[tauri::command]
pub fn delete_project_backup(backup_id: String) -> Result<(), String> {
    delete_backup(&backup_id)
}

#[tauri::command]
pub fn run_backup_cleanup_on_launch() -> Result<usize, String> {
    cleanup_old_backups(30)
}
