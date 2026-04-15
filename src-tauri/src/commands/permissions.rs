use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudePermissions {
    pub allow: Vec<String>,
    pub deny: Vec<String>,
    pub enabled_plugins: HashMap<String, bool>,
    pub skip_dangerous_mode: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRestriction {
    pub key: String,
    pub allowed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorPermissions {
    pub allow: Vec<String>,
    pub deny: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeProjectFilePermissions {
    pub allow: Vec<String>,
    pub deny: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeProjectPermissions {
    pub settings: ClaudeProjectFilePermissions,
    pub settings_local: ClaudeProjectFilePermissions,
}

fn read_permissions_from_json(json: &serde_json::Value) -> (Vec<String>, Vec<String>) {
    let perms = json.get("permissions");
    let allow: Vec<String> = perms
        .and_then(|p| p.get("allow"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let deny: Vec<String> = perms
        .and_then(|p| p.get("deny"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    (allow, deny)
}

/// Returns JSON content for project settings.json containing only the permissions part from global.
fn get_global_claude_settings_content() -> String {
    let default_json = r#"{"permissions":{"allow":[],"deny":[]},"skipDangerousModePermissionPrompt":false}"#;
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return default_json.to_string(),
    };
    let path = home.join(".claude").join("settings.json");
    if !path.exists() {
        return default_json.to_string();
    }
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return default_json.to_string(),
    };
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return default_json.to_string();
    }
    let json: serde_json::Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => return default_json.to_string(),
    };
    let (allow, deny) = read_permissions_from_json(&json);
    let skip_dangerous = json
        .get("skipDangerousModePermissionPrompt")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let out = serde_json::json!({
        "permissions": { "allow": allow, "deny": deny },
        "skipDangerousModePermissionPrompt": skip_dangerous
    });
    serde_json::to_string_pretty(&out).unwrap_or_else(|_| default_json.to_string())
}

/// Ensures project .claude/settings.json exists and is valid; if missing or invalid, copies from global.
fn ensure_project_settings_json(settings_path: &Path) -> Result<(), String> {
    let need_init = if !settings_path.exists() {
        true
    } else {
        let content = std::fs::read_to_string(settings_path).unwrap_or_default();
        let trimmed = content.trim();
        trimmed.is_empty() || serde_json::from_str::<serde_json::Value>(trimmed).is_err()
    };
    if need_init {
        if let Some(parent) = settings_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let global_content = get_global_claude_settings_content();
        crate::utils::paths::atomic_write_str(settings_path, &global_content)?;
    }
    Ok(())
}

#[tauri::command]
pub fn get_claude_permissions() -> Result<ClaudePermissions, String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let path = home.join(".claude").join("settings.json");

    if !path.exists() {
        return Ok(ClaudePermissions {
            allow: vec![],
            deny: vec![],
            enabled_plugins: HashMap::new(),
            skip_dangerous_mode: false,
        });
    }

    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse settings.json: {}", e))?;

    let perms = json.get("permissions");
    let allow: Vec<String> = perms
        .and_then(|p| p.get("allow"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let deny: Vec<String> = perms
        .and_then(|p| p.get("deny"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let enabled_plugins: HashMap<String, bool> = json
        .get("enabledPlugins")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_bool().map(|b| (k.clone(), b)))
                .collect()
        })
        .unwrap_or_default();

    let skip_dangerous_mode = json
        .get("skipDangerousModePermissionPrompt")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    Ok(ClaudePermissions { allow, deny, enabled_plugins, skip_dangerous_mode })
}

#[tauri::command]
pub fn update_claude_permissions(
    allow: Vec<String>,
    deny: Vec<String>,
    skip_dangerous_mode: bool,
) -> Result<(), String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let path = home.join(".claude").join("settings.json");

    let content = if path.exists() {
        std::fs::read_to_string(&path).map_err(|e| e.to_string())?
    } else {
        "{}".to_string()
    };

    let mut json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse: {}", e))?;

    let obj = json.as_object_mut().ok_or("Not an object")?;
    let perms = obj.entry("permissions").or_insert_with(|| serde_json::json!({}));
    if let Some(p) = perms.as_object_mut() {
        p.insert("allow".to_string(), serde_json::json!(allow));
        p.insert("deny".to_string(), serde_json::json!(deny));
    }
    obj.insert(
        "skipDangerousModePermissionPrompt".to_string(),
        serde_json::json!(skip_dangerous_mode),
    );

    let out = serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?;
    crate::utils::paths::atomic_write_str(&path, &out)?;
    Ok(())
}

#[tauri::command]
pub fn get_claude_policy() -> Result<Vec<PolicyRestriction>, String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let path = home.join(".claude").join("policy-limits.json");
    if !path.exists() { return Ok(vec![]); }

    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse policy-limits.json: {}", e))?;

    let mut policies = Vec::new();
    if let Some(restrictions) = json.get("restrictions").and_then(|v| v.as_object()) {
        for (key, value) in restrictions {
            let allowed = value.get("allowed").and_then(|v| v.as_bool()).unwrap_or(false);
            policies.push(PolicyRestriction { key: key.clone(), allowed });
        }
    }
    policies.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(policies)
}

#[tauri::command]
pub fn update_claude_policy(key: String, allowed: bool) -> Result<(), String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let path = home.join(".claude").join("policy-limits.json");

    let content = if path.exists() {
        std::fs::read_to_string(&path).map_err(|e| e.to_string())?
    } else { r#"{"restrictions":{}}"#.to_string() };

    let mut json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse: {}", e))?;

    let obj = json.as_object_mut().ok_or("Not an object")?;
    let restrictions = obj.entry("restrictions").or_insert_with(|| serde_json::json!({}));
    if let Some(r) = restrictions.as_object_mut() {
        r.insert(key, serde_json::json!({"allowed": allowed}));
    }

    let out = serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?;
    crate::utils::paths::atomic_write_str(&path, &out)?;
    Ok(())
}

#[tauri::command]
pub fn get_claude_project_permissions(project_path: String) -> Result<ClaudeProjectPermissions, String> {
    if project_path.contains("..") {
        return Err("Invalid project path".to_string());
    }
    let base = Path::new(&project_path);
    let settings_path = base.join(".claude").join("settings.json");
    let settings_local_path = base.join(".claude").join("settings.local.json");

    ensure_project_settings_json(&settings_path)?;

    let (settings_allow, settings_deny) = {
        let content = std::fs::read_to_string(&settings_path).map_err(|e| e.to_string())?;
        let json: serde_json::Value = serde_json::from_str(content.trim())
            .map_err(|e| format!("Failed to parse settings.json: {}", e))?;
        read_permissions_from_json(&json)
    };

    let (local_allow, local_deny) = if settings_local_path.exists() {
        let content = std::fs::read_to_string(&settings_local_path).unwrap_or_default();
        let trimmed = content.trim();
        if trimmed.is_empty() {
            (vec![], vec![])
        } else {
            match serde_json::from_str::<serde_json::Value>(trimmed) {
                Ok(json) => read_permissions_from_json(&json),
                Err(_) => (vec![], vec![]),
            }
        }
    } else {
        (vec![], vec![])
    };

    Ok(ClaudeProjectPermissions {
        settings: ClaudeProjectFilePermissions { allow: settings_allow, deny: settings_deny },
        settings_local: ClaudeProjectFilePermissions { allow: local_allow, deny: local_deny },
    })
}

#[tauri::command]
pub fn update_claude_project_permissions(
    project_path: String,
    file: String,
    allow: Vec<String>,
    deny: Vec<String>,
) -> Result<(), String> {
    if project_path.contains("..") {
        return Err("Invalid project path".to_string());
    }
    let base = Path::new(&project_path);
    let path = match file.as_str() {
        "settings" => base.join(".claude").join("settings.json"),
        "settings_local" => base.join(".claude").join("settings.local.json"),
        _ => return Err("file must be 'settings' or 'settings_local'".to_string()),
    };

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let content = if path.exists() {
        std::fs::read_to_string(&path).map_err(|e| e.to_string())?
    } else {
        "{}".to_string()
    };

    let mut json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse: {}", e))?;

    let obj = json.as_object_mut().ok_or("Not an object")?;
    let perms = obj.entry("permissions").or_insert_with(|| serde_json::json!({}));
    if let Some(p) = perms.as_object_mut() {
        p.insert("allow".to_string(), serde_json::json!(allow));
        p.insert("deny".to_string(), serde_json::json!(deny));
    }

    let out = serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?;
    crate::utils::paths::atomic_write_str(&path, &out)?;
    Ok(())
}

#[tauri::command]
pub fn get_cursor_permissions() -> Result<CursorPermissions, String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let path = home.join(".cursor").join("cli-config.json");
    if !path.exists() {
        return Ok(CursorPermissions { allow: vec![], deny: vec![] });
    }

    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse cli-config.json: {}", e))?;

    let perms = json.get("permissions");
    let allow: Vec<String> = perms
        .and_then(|p| p.get("allow"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let deny: Vec<String> = perms
        .and_then(|p| p.get("deny"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    Ok(CursorPermissions { allow, deny })
}

#[tauri::command]
pub fn update_cursor_permissions(allow: Vec<String>, deny: Vec<String>) -> Result<(), String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let path = home.join(".cursor").join("cli-config.json");

    let content = if path.exists() {
        std::fs::read_to_string(&path).map_err(|e| e.to_string())?
    } else { r#"{"version":1,"permissions":{"allow":[],"deny":[]}}"#.to_string() };

    let mut json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse: {}", e))?;

    let obj = json.as_object_mut().ok_or("Not an object")?;
    let perms = obj.entry("permissions").or_insert_with(|| serde_json::json!({}));
    if let Some(p) = perms.as_object_mut() {
        p.insert("allow".to_string(), serde_json::json!(allow));
        p.insert("deny".to_string(), serde_json::json!(deny));
    }

    let out = serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?;
    crate::utils::paths::atomic_write_str(&path, &out)?;
    Ok(())
}
