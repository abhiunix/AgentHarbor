use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindsurfRuleEntry {
    pub name: String,
    pub file_path: String,
    pub is_legacy: bool,
    pub size_bytes: u64,
    pub modified_at: String,
}

fn validate_project_path(project_path: &str) -> Result<PathBuf, String> {
    if project_path.contains("..") {
        return Err("Invalid project path: path traversal not allowed".to_string());
    }
    let path = PathBuf::from(project_path);
    if !path.exists() {
        return Err(format!("Project path does not exist: {}", project_path));
    }
    Ok(path)
}

fn validate_file_name(file_name: &str) -> Result<(), String> {
    if file_name.contains("..") || file_name.contains('/') || file_name.contains('\\') {
        return Err("Invalid file name: must not contain path separators or traversal".to_string());
    }
    if file_name.is_empty() {
        return Err("File name must not be empty".to_string());
    }
    Ok(())
}

fn metadata_to_iso8601(metadata: &fs::Metadata) -> String {
    metadata
        .modified()
        .ok()
        .map(|t| {
            let dt: DateTime<Utc> = t.into();
            dt.to_rfc3339()
        })
        .unwrap_or_default()
}

fn entry_from_path(path: &Path, name: &str, is_legacy: bool) -> Option<WindsurfRuleEntry> {
    let metadata = fs::metadata(path).ok()?;
    Some(WindsurfRuleEntry {
        name: name.to_string(),
        file_path: path.to_string_lossy().to_string(),
        is_legacy,
        size_bytes: metadata.len(),
        modified_at: metadata_to_iso8601(&metadata),
    })
}

fn atomic_write(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory {}: {}", parent.display(), e))?;
    }
    crate::utils::paths::atomic_write_str(path, content)
}

#[tauri::command]
pub fn list_windsurf_rules(project_path: String) -> Result<Vec<WindsurfRuleEntry>, String> {
    let path = validate_project_path(&project_path)?;
    let mut entries = Vec::new();

    // Check legacy .windsurfrules file
    let legacy_path = path.join(".windsurfrules");
    if legacy_path.is_file() {
        if let Some(entry) = entry_from_path(&legacy_path, ".windsurfrules", true) {
            entries.push(entry);
        }
    }

    // Check .windsurf/rules/ directory
    let rules_dir = path.join(".windsurf").join("rules");
    if rules_dir.is_dir() {
        if let Ok(dir_entries) = fs::read_dir(&rules_dir) {
            for dir_entry in dir_entries.flatten() {
                let entry_path = dir_entry.path();
                if entry_path.is_file() {
                    if let Some(name) = entry_path.file_name() {
                        let name_str = name.to_string_lossy().to_string();
                        if let Some(entry) = entry_from_path(&entry_path, &name_str, false) {
                            entries.push(entry);
                        }
                    }
                }
            }
        }
    }

    entries.sort_by(|a, b| {
        // Legacy first, then alphabetical
        a.is_legacy
            .cmp(&b.is_legacy)
            .reverse()
            .then_with(|| a.name.cmp(&b.name))
    });

    Ok(entries)
}

#[tauri::command]
pub fn read_windsurf_rules_file(
    project_path: String,
    file_name: String,
) -> Result<String, String> {
    let path = validate_project_path(&project_path)?;
    validate_file_name(&file_name)?;

    let file_path = path.join(".windsurf").join("rules").join(&file_name);
    if !file_path.is_file() {
        return Err(format!("Rule file not found: {}", file_name));
    }

    fs::read_to_string(&file_path).map_err(|e| format!("Failed to read {}: {}", file_name, e))
}

#[tauri::command]
pub fn write_windsurf_rules_file(
    project_path: String,
    file_name: String,
    content: String,
) -> Result<(), String> {
    let path = validate_project_path(&project_path)?;
    validate_file_name(&file_name)?;

    let file_path = path.join(".windsurf").join("rules").join(&file_name);
    atomic_write(&file_path, &content)
}

#[tauri::command]
pub fn delete_windsurf_rule(project_path: String, file_name: String) -> Result<(), String> {
    let path = validate_project_path(&project_path)?;
    validate_file_name(&file_name)?;

    if file_name == ".windsurfrules" {
        return Err("Cannot delete the legacy .windsurfrules file".to_string());
    }

    let file_path = path.join(".windsurf").join("rules").join(&file_name);
    if !file_path.exists() {
        return Ok(());
    }

    fs::remove_file(&file_path).map_err(|e| format!("Failed to delete {}: {}", file_name, e))
}

#[tauri::command]
pub fn read_windsurf_legacy_rules(project_path: String) -> Result<String, String> {
    let path = validate_project_path(&project_path)?;
    let legacy_path = path.join(".windsurfrules");

    if !legacy_path.is_file() {
        return Ok(String::new());
    }

    fs::read_to_string(&legacy_path)
        .map_err(|e| format!("Failed to read .windsurfrules: {}", e))
}

#[tauri::command]
pub fn write_windsurf_legacy_rules(
    project_path: String,
    content: String,
) -> Result<(), String> {
    let path = validate_project_path(&project_path)?;
    let legacy_path = path.join(".windsurfrules");
    atomic_write(&legacy_path, &content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_temp_project() -> TempDir {
        TempDir::new().unwrap()
    }

    #[test]
    fn test_list_windsurf_rules_empty_project() {
        let temp = setup_temp_project();
        let result = list_windsurf_rules(temp.path().to_string_lossy().to_string());
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_list_windsurf_rules_legacy_only() {
        let temp = setup_temp_project();
        let legacy = temp.path().join(".windsurfrules");
        fs::write(&legacy, "legacy rules content").unwrap();

        let result = list_windsurf_rules(temp.path().to_string_lossy().to_string()).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, ".windsurfrules");
        assert!(result[0].is_legacy);
        assert_eq!(result[0].size_bytes, 20);
    }

    #[test]
    fn test_list_windsurf_rules_directory_only() {
        let temp = setup_temp_project();
        let rules_dir = temp.path().join(".windsurf").join("rules");
        fs::create_dir_all(&rules_dir).unwrap();
        fs::write(rules_dir.join("coding-style.md"), "# Style").unwrap();
        fs::write(rules_dir.join("testing.md"), "# Testing").unwrap();

        let result = list_windsurf_rules(temp.path().to_string_lossy().to_string()).unwrap();
        assert_eq!(result.len(), 2);
        assert!(!result[0].is_legacy);
        assert!(!result[1].is_legacy);
        // Should be alphabetically sorted
        assert_eq!(result[0].name, "coding-style.md");
        assert_eq!(result[1].name, "testing.md");
    }

    #[test]
    fn test_list_windsurf_rules_both_sources() {
        let temp = setup_temp_project();
        let legacy = temp.path().join(".windsurfrules");
        fs::write(&legacy, "legacy").unwrap();
        let rules_dir = temp.path().join(".windsurf").join("rules");
        fs::create_dir_all(&rules_dir).unwrap();
        fs::write(rules_dir.join("style.md"), "# Style").unwrap();

        let result = list_windsurf_rules(temp.path().to_string_lossy().to_string()).unwrap();
        assert_eq!(result.len(), 2);
        // Legacy should come first
        assert!(result[0].is_legacy);
        assert!(!result[1].is_legacy);
    }

    #[test]
    fn test_read_windsurf_rules_file() {
        let temp = setup_temp_project();
        let rules_dir = temp.path().join(".windsurf").join("rules");
        fs::create_dir_all(&rules_dir).unwrap();
        fs::write(rules_dir.join("test.md"), "# Test Content").unwrap();

        let result = read_windsurf_rules_file(
            temp.path().to_string_lossy().to_string(),
            "test.md".to_string(),
        );
        assert_eq!(result.unwrap(), "# Test Content");
    }

    #[test]
    fn test_read_windsurf_rules_file_not_found() {
        let temp = setup_temp_project();
        let result = read_windsurf_rules_file(
            temp.path().to_string_lossy().to_string(),
            "nonexistent.md".to_string(),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_write_windsurf_rules_file_creates_dirs() {
        let temp = setup_temp_project();
        let result = write_windsurf_rules_file(
            temp.path().to_string_lossy().to_string(),
            "new-rule.md".to_string(),
            "# New Rule".to_string(),
        );
        assert!(result.is_ok());

        let content = fs::read_to_string(
            temp.path().join(".windsurf").join("rules").join("new-rule.md"),
        )
        .unwrap();
        assert_eq!(content, "# New Rule");
    }

    #[test]
    fn test_write_windsurf_rules_file_overwrites() {
        let temp = setup_temp_project();
        let rules_dir = temp.path().join(".windsurf").join("rules");
        fs::create_dir_all(&rules_dir).unwrap();
        fs::write(rules_dir.join("rule.md"), "old content").unwrap();

        write_windsurf_rules_file(
            temp.path().to_string_lossy().to_string(),
            "rule.md".to_string(),
            "new content".to_string(),
        )
        .unwrap();

        let content = fs::read_to_string(rules_dir.join("rule.md")).unwrap();
        assert_eq!(content, "new content");
    }

    #[test]
    fn test_delete_windsurf_rule() {
        let temp = setup_temp_project();
        let rules_dir = temp.path().join(".windsurf").join("rules");
        fs::create_dir_all(&rules_dir).unwrap();
        fs::write(rules_dir.join("delete-me.md"), "bye").unwrap();

        let result = delete_windsurf_rule(
            temp.path().to_string_lossy().to_string(),
            "delete-me.md".to_string(),
        );
        assert!(result.is_ok());
        assert!(!rules_dir.join("delete-me.md").exists());
    }

    #[test]
    fn test_delete_windsurf_rule_nonexistent_is_ok() {
        let temp = setup_temp_project();
        let result = delete_windsurf_rule(
            temp.path().to_string_lossy().to_string(),
            "nonexistent.md".to_string(),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_delete_windsurf_rule_blocks_legacy() {
        let temp = setup_temp_project();
        let result = delete_windsurf_rule(
            temp.path().to_string_lossy().to_string(),
            ".windsurfrules".to_string(),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Cannot delete"));
    }

    #[test]
    fn test_read_windsurf_legacy_rules() {
        let temp = setup_temp_project();
        fs::write(temp.path().join(".windsurfrules"), "legacy content").unwrap();

        let result =
            read_windsurf_legacy_rules(temp.path().to_string_lossy().to_string()).unwrap();
        assert_eq!(result, "legacy content");
    }

    #[test]
    fn test_read_windsurf_legacy_rules_missing() {
        let temp = setup_temp_project();
        let result =
            read_windsurf_legacy_rules(temp.path().to_string_lossy().to_string()).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_write_windsurf_legacy_rules() {
        let temp = setup_temp_project();
        write_windsurf_legacy_rules(
            temp.path().to_string_lossy().to_string(),
            "new legacy content".to_string(),
        )
        .unwrap();

        let content = fs::read_to_string(temp.path().join(".windsurfrules")).unwrap();
        assert_eq!(content, "new legacy content");
    }

    #[test]
    fn test_validate_file_name_rejects_traversal() {
        assert!(validate_file_name("../etc/passwd").is_err());
        assert!(validate_file_name("foo/bar.md").is_err());
        assert!(validate_file_name("foo\\bar.md").is_err());
        assert!(validate_file_name("").is_err());
    }

    #[test]
    fn test_validate_file_name_accepts_valid() {
        assert!(validate_file_name("coding-style.md").is_ok());
        assert!(validate_file_name("my_rule.md").is_ok());
        assert!(validate_file_name(".windsurfrules").is_ok());
    }

    #[test]
    fn test_validate_project_path_rejects_traversal() {
        let result = validate_project_path("/some/../path");
        assert!(result.is_err());
    }

    #[test]
    fn test_list_rules_ignores_subdirectories() {
        let temp = setup_temp_project();
        let rules_dir = temp.path().join(".windsurf").join("rules");
        fs::create_dir_all(&rules_dir).unwrap();
        fs::write(rules_dir.join("real-rule.md"), "content").unwrap();
        fs::create_dir_all(rules_dir.join("subdir")).unwrap();

        let result = list_windsurf_rules(temp.path().to_string_lossy().to_string()).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "real-rule.md");
    }

    #[test]
    fn test_modified_at_is_iso8601() {
        let temp = setup_temp_project();
        fs::write(temp.path().join(".windsurfrules"), "x").unwrap();

        let result = list_windsurf_rules(temp.path().to_string_lossy().to_string()).unwrap();
        assert_eq!(result.len(), 1);
        // Should contain 'T' separator typical of ISO 8601
        assert!(result[0].modified_at.contains('T'));
    }
}
