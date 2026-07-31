use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const VALID_EVENTS: &[&str] = &[
    "beforeShellExecution",
    "beforeMCPExecution",
    "beforeReadFile",
    "afterFileEdit",
    "stop",
];

const VALID_ACTIONS: &[&str] = &["allow", "deny", "ask", "run"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorHook {
    pub event: String,
    pub command: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorHooksConfig {
    pub hooks: Vec<CursorHook>,
    pub source_path: String,
    pub is_global: bool,
}

/// Internal file format matching `.cursor/hooks.json`
#[derive(Debug, Clone, Serialize, Deserialize)]
struct HooksFile {
    hooks: Vec<CursorHook>,
}

fn validate_event(event: &str) -> Result<(), String> {
    if VALID_EVENTS.contains(&event) {
        Ok(())
    } else {
        Err(format!(
            "Invalid event '{}'. Valid events: {}",
            event,
            VALID_EVENTS.join(", ")
        ))
    }
}

fn validate_action(action: &str) -> Result<(), String> {
    if VALID_ACTIONS.contains(&action) {
        Ok(())
    } else {
        Err(format!(
            "Invalid action '{}'. Valid actions: {}",
            action,
            VALID_ACTIONS.join(", ")
        ))
    }
}

fn resolve_hooks_path(project_path: Option<&str>) -> Result<(PathBuf, bool), String> {
    match project_path {
        Some(p) => {
            if p.contains("..") {
                return Err("Invalid project path".to_string());
            }
            let path = PathBuf::from(p).join(".cursor").join("hooks.json");
            Ok((path, false))
        }
        None => {
            let home = dirs::home_dir().ok_or("Could not determine home directory")?;
            let path = home.join(".cursor").join("hooks.json");
            Ok((path, true))
        }
    }
}

fn read_hooks_file(path: &Path) -> Result<Vec<CursorHook>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path).map_err(|e| format!("Failed to read hooks file: {}", e))?;
    let hooks_file: HooksFile =
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse hooks file: {}", e))?;
    Ok(hooks_file.hooks)
}

fn write_hooks_file(path: &Path, hooks: &[CursorHook]) -> Result<(), String> {
    let hooks_file = HooksFile {
        hooks: hooks.to_vec(),
    };
    let json = serde_json::to_string_pretty(&hooks_file)
        .map_err(|e| format!("Failed to serialize hooks: {}", e))?;

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    crate::utils::paths::atomic_write_str(path, &json)?;

    Ok(())
}

#[tauri::command]
pub fn list_cursor_hooks(project_path: Option<String>) -> Result<CursorHooksConfig, String> {
    let (path, is_global) = resolve_hooks_path(project_path.as_deref())?;
    let hooks = read_hooks_file(&path)?;
    Ok(CursorHooksConfig {
        hooks,
        source_path: path.to_string_lossy().to_string(),
        is_global,
    })
}

#[tauri::command]
pub fn save_cursor_hooks(project_path: Option<String>, hooks_json: String) -> Result<(), String> {
    let (path, _is_global) = resolve_hooks_path(project_path.as_deref())?;
    let hooks_file: HooksFile = serde_json::from_str(&hooks_json)
        .map_err(|e| format!("Invalid hooks JSON: {}", e))?;

    // Validate all hooks
    for hook in &hooks_file.hooks {
        validate_event(&hook.event)?;
        validate_action(&hook.action)?;
    }

    write_hooks_file(&path, &hooks_file.hooks)
}

#[tauri::command]
pub fn add_cursor_hook(
    project_path: Option<String>,
    event: String,
    command: String,
    action: String,
) -> Result<(), String> {
    validate_event(&event)?;
    validate_action(&action)?;

    if command.trim().is_empty() {
        return Err("Command cannot be empty".to_string());
    }

    let (path, _is_global) = resolve_hooks_path(project_path.as_deref())?;
    let mut hooks = read_hooks_file(&path)?;

    hooks.push(CursorHook {
        event,
        command,
        action,
    });

    write_hooks_file(&path, &hooks)
}

#[tauri::command]
pub fn remove_cursor_hook(project_path: Option<String>, index: usize) -> Result<(), String> {
    let (path, _is_global) = resolve_hooks_path(project_path.as_deref())?;
    let mut hooks = read_hooks_file(&path)?;

    if index >= hooks.len() {
        return Err(format!(
            "Index {} out of range. Only {} hooks exist.",
            index,
            hooks.len()
        ));
    }

    hooks.remove(index);
    write_hooks_file(&path, &hooks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_project() -> TempDir {
        let dir = TempDir::new().unwrap();
        let cursor_dir = dir.path().join(".cursor");
        fs::create_dir_all(&cursor_dir).unwrap();
        dir
    }

    #[test]
    fn test_validate_event_valid() {
        assert!(validate_event("beforeShellExecution").is_ok());
        assert!(validate_event("afterFileEdit").is_ok());
        assert!(validate_event("stop").is_ok());
    }

    #[test]
    fn test_validate_event_invalid() {
        assert!(validate_event("onSave").is_err());
        assert!(validate_event("").is_err());
    }

    #[test]
    fn test_validate_action_valid() {
        assert!(validate_action("allow").is_ok());
        assert!(validate_action("deny").is_ok());
        assert!(validate_action("ask").is_ok());
        assert!(validate_action("run").is_ok());
    }

    #[test]
    fn test_validate_action_invalid() {
        assert!(validate_action("execute").is_err());
        assert!(validate_action("").is_err());
    }

    #[test]
    fn test_read_hooks_file_missing() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.json");
        let hooks = read_hooks_file(&path).unwrap();
        assert!(hooks.is_empty());
    }

    #[test]
    fn test_read_hooks_file_valid() {
        let dir = setup_project();
        let path = dir.path().join(".cursor").join("hooks.json");
        fs::write(
            &path,
            r#"{"hooks":[{"event":"afterFileEdit","command":"prettier --write ${filePath}","action":"run"}]}"#,
        )
        .unwrap();

        let hooks = read_hooks_file(&path).unwrap();
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0].event, "afterFileEdit");
        assert_eq!(hooks[0].action, "run");
    }

    #[test]
    fn test_write_and_read_roundtrip() {
        let dir = setup_project();
        let path = dir.path().join(".cursor").join("hooks.json");

        let hooks = vec![
            CursorHook {
                event: "beforeShellExecution".to_string(),
                command: "echo 'check'".to_string(),
                action: "allow".to_string(),
            },
            CursorHook {
                event: "afterFileEdit".to_string(),
                command: "prettier --write ${filePath}".to_string(),
                action: "run".to_string(),
            },
        ];

        write_hooks_file(&path, &hooks).unwrap();
        let loaded = read_hooks_file(&path).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].event, "beforeShellExecution");
        assert_eq!(loaded[1].command, "prettier --write ${filePath}");
    }

    #[test]
    fn test_resolve_hooks_path_project() {
        let (path, is_global) = resolve_hooks_path(Some("/tmp/myproject")).unwrap();
        assert_eq!(path, PathBuf::from("/tmp/myproject/.cursor/hooks.json"));
        assert!(!is_global);
    }

    #[test]
    fn test_resolve_hooks_path_global() {
        let (path, is_global) = resolve_hooks_path(None).unwrap();
        assert!(path.to_string_lossy().contains(".cursor/hooks.json"));
        assert!(is_global);
    }

    #[test]
    fn test_resolve_hooks_path_rejects_traversal() {
        let result = resolve_hooks_path(Some("/tmp/../etc"));
        assert!(result.is_err());
    }

    #[test]
    fn test_list_cursor_hooks_empty() {
        let dir = setup_project();
        let config = list_cursor_hooks(Some(dir.path().to_string_lossy().to_string())).unwrap();
        assert!(config.hooks.is_empty());
        assert!(!config.is_global);
    }

    #[test]
    fn test_add_and_remove_cursor_hook() {
        let dir = setup_project();
        let project = dir.path().to_string_lossy().to_string();

        add_cursor_hook(
            Some(project.clone()),
            "afterFileEdit".to_string(),
            "prettier --write ${filePath}".to_string(),
            "run".to_string(),
        )
        .unwrap();

        let config = list_cursor_hooks(Some(project.clone())).unwrap();
        assert_eq!(config.hooks.len(), 1);

        remove_cursor_hook(Some(project.clone()), 0).unwrap();

        let config = list_cursor_hooks(Some(project)).unwrap();
        assert!(config.hooks.is_empty());
    }

    #[test]
    fn test_add_cursor_hook_invalid_event() {
        let dir = setup_project();
        let result = add_cursor_hook(
            Some(dir.path().to_string_lossy().to_string()),
            "badEvent".to_string(),
            "echo hi".to_string(),
            "allow".to_string(),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid event"));
    }

    #[test]
    fn test_add_cursor_hook_invalid_action() {
        let dir = setup_project();
        let result = add_cursor_hook(
            Some(dir.path().to_string_lossy().to_string()),
            "afterFileEdit".to_string(),
            "echo hi".to_string(),
            "badAction".to_string(),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid action"));
    }

    #[test]
    fn test_add_cursor_hook_empty_command() {
        let dir = setup_project();
        let result = add_cursor_hook(
            Some(dir.path().to_string_lossy().to_string()),
            "afterFileEdit".to_string(),
            "  ".to_string(),
            "run".to_string(),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Command cannot be empty"));
    }

    #[test]
    fn test_remove_cursor_hook_out_of_bounds() {
        let dir = setup_project();
        let result = remove_cursor_hook(Some(dir.path().to_string_lossy().to_string()), 5);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("out of range"));
    }

    #[test]
    fn test_save_cursor_hooks_validates() {
        let dir = setup_project();
        let bad_json = r#"{"hooks":[{"event":"badEvent","command":"echo","action":"allow"}]}"#;
        let result = save_cursor_hooks(Some(dir.path().to_string_lossy().to_string()), bad_json.to_string());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid event"));
    }

    #[test]
    fn test_save_cursor_hooks_valid() {
        let dir = setup_project();
        let project = dir.path().to_string_lossy().to_string();
        let json = r#"{"hooks":[{"event":"afterFileEdit","command":"prettier --write ${filePath}","action":"run"}]}"#;
        save_cursor_hooks(Some(project.clone()), json.to_string()).unwrap();

        let config = list_cursor_hooks(Some(project)).unwrap();
        assert_eq!(config.hooks.len(), 1);
        assert_eq!(config.hooks[0].event, "afterFileEdit");
    }
}
