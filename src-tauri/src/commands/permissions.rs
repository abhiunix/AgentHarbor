use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ClaudePermissions {
    pub allow: Vec<String>,
    pub deny: Vec<String>,
    pub additional_directories: Vec<String>,
    pub default_mode: Option<String>,
    pub enabled_plugins: HashMap<String, bool>,
    pub skip_dangerous_mode: bool,

    pub always_thinking_enabled: Option<bool>,
    pub auto_memory_enabled: Option<bool>,
    pub include_git_instructions: Option<bool>,
    pub disable_all_hooks: Option<bool>,
    pub disable_agent_view: Option<bool>,
    pub disable_skill_shell_execution: Option<bool>,
    pub disable_remote_control: Option<bool>,
    pub fast_mode_per_session_opt_in: Option<bool>,
    pub respect_gitignore: Option<bool>,
    pub show_thinking_summaries: Option<bool>,

    pub effort_level: Option<String>,
    pub model: Option<String>,
    pub auto_updates_channel: Option<String>,
    pub editor_mode: Option<String>,
    pub view_mode: Option<String>,
    pub default_shell: Option<String>,
    pub plans_directory: Option<String>,
    pub cleanup_period_days: Option<u32>,

    pub claude_md_excludes: Vec<String>,
    pub available_models: Vec<String>,
}

fn opt_str(json: &Value, key: &str) -> Option<String> {
    json.get(key).and_then(|v| v.as_str()).map(String::from)
}

fn opt_bool(json: &Value, key: &str) -> Option<bool> {
    json.get(key).and_then(|v| v.as_bool())
}

fn opt_u32(json: &Value, key: &str) -> Option<u32> {
    json.get(key).and_then(|v| v.as_u64()).map(|n| n as u32)
}

fn str_array(json: &Value, key: &str) -> Vec<String> {
    json.get(key)
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

fn set_or_remove_str(obj: &mut serde_json::Map<String, Value>, key: &str, val: &Option<String>) {
    match val {
        Some(s) if !s.is_empty() => { obj.insert(key.into(), Value::String(s.clone())); }
        _ => { obj.remove(key); }
    }
}

fn set_or_remove_bool(obj: &mut serde_json::Map<String, Value>, key: &str, val: Option<bool>) {
    match val {
        Some(b) => { obj.insert(key.into(), Value::Bool(b)); }
        None => { obj.remove(key); }
    }
}

fn set_or_remove_u32(obj: &mut serde_json::Map<String, Value>, key: &str, val: Option<u32>) {
    match val {
        Some(n) => { obj.insert(key.into(), serde_json::json!(n)); }
        None => { obj.remove(key); }
    }
}

fn set_or_remove_str_array(obj: &mut serde_json::Map<String, Value>, key: &str, val: &[String]) {
    if val.is_empty() {
        obj.remove(key);
    } else {
        obj.insert(key.into(), Value::Array(val.iter().map(|s| Value::String(s.clone())).collect()));
    }
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
        return Ok(ClaudePermissions::default());
    }

    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let json: Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse settings.json: {}", e))?;

    let perms_val = json.get("permissions").cloned().unwrap_or(Value::Null);
    let allow = str_array(&perms_val, "allow");
    let deny = str_array(&perms_val, "deny");
    let additional_directories = str_array(&perms_val, "additionalDirectories");
    let default_mode = opt_str(&perms_val, "defaultMode");

    let enabled_plugins: HashMap<String, bool> = json
        .get("enabledPlugins")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_bool().map(|b| (k.clone(), b)))
                .collect()
        })
        .unwrap_or_default();

    let skip_dangerous_mode = opt_bool(&json, "skipDangerousModePermissionPrompt").unwrap_or(false);

    Ok(ClaudePermissions {
        allow,
        deny,
        additional_directories,
        default_mode,
        enabled_plugins,
        skip_dangerous_mode,

        always_thinking_enabled: opt_bool(&json, "alwaysThinkingEnabled"),
        auto_memory_enabled: opt_bool(&json, "autoMemoryEnabled"),
        include_git_instructions: opt_bool(&json, "includeGitInstructions"),
        disable_all_hooks: opt_bool(&json, "disableAllHooks"),
        disable_agent_view: opt_bool(&json, "disableAgentView"),
        disable_skill_shell_execution: opt_bool(&json, "disableSkillShellExecution"),
        disable_remote_control: opt_bool(&json, "disableRemoteControl"),
        fast_mode_per_session_opt_in: opt_bool(&json, "fastModePerSessionOptIn"),
        respect_gitignore: opt_bool(&json, "respectGitignore"),
        show_thinking_summaries: opt_bool(&json, "showThinkingSummaries"),

        effort_level: opt_str(&json, "effortLevel"),
        model: opt_str(&json, "model"),
        auto_updates_channel: opt_str(&json, "autoUpdatesChannel"),
        editor_mode: opt_str(&json, "editorMode"),
        view_mode: opt_str(&json, "viewMode"),
        default_shell: opt_str(&json, "defaultShell"),
        plans_directory: opt_str(&json, "plansDirectory"),
        cleanup_period_days: opt_u32(&json, "cleanupPeriodDays"),

        claude_md_excludes: str_array(&json, "claudeMdExcludes"),
        available_models: str_array(&json, "availableModels"),
    })
}

#[tauri::command]
pub fn update_claude_permissions(payload: ClaudePermissions) -> Result<(), String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let path = home.join(".claude").join("settings.json");

    let content = if path.exists() {
        std::fs::read_to_string(&path).map_err(|e| e.to_string())?
    } else {
        "{}".to_string()
    };

    let mut json: Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse: {}", e))?;

    let obj = json.as_object_mut().ok_or("Not an object")?;

    let perms = obj.entry("permissions").or_insert_with(|| serde_json::json!({}));
    if let Some(p) = perms.as_object_mut() {
        p.insert("allow".to_string(), serde_json::json!(payload.allow));
        p.insert("deny".to_string(), serde_json::json!(payload.deny));
        set_or_remove_str_array(p, "additionalDirectories", &payload.additional_directories);
        set_or_remove_str(p, "defaultMode", &payload.default_mode);
    }

    obj.insert(
        "skipDangerousModePermissionPrompt".to_string(),
        Value::Bool(payload.skip_dangerous_mode),
    );

    set_or_remove_bool(obj, "alwaysThinkingEnabled", payload.always_thinking_enabled);
    set_or_remove_bool(obj, "autoMemoryEnabled", payload.auto_memory_enabled);
    set_or_remove_bool(obj, "includeGitInstructions", payload.include_git_instructions);
    set_or_remove_bool(obj, "disableAllHooks", payload.disable_all_hooks);
    set_or_remove_bool(obj, "disableAgentView", payload.disable_agent_view);
    set_or_remove_bool(obj, "disableSkillShellExecution", payload.disable_skill_shell_execution);
    set_or_remove_bool(obj, "disableRemoteControl", payload.disable_remote_control);
    set_or_remove_bool(obj, "fastModePerSessionOptIn", payload.fast_mode_per_session_opt_in);
    set_or_remove_bool(obj, "respectGitignore", payload.respect_gitignore);
    set_or_remove_bool(obj, "showThinkingSummaries", payload.show_thinking_summaries);

    set_or_remove_str(obj, "effortLevel", &payload.effort_level);
    set_or_remove_str(obj, "model", &payload.model);
    set_or_remove_str(obj, "autoUpdatesChannel", &payload.auto_updates_channel);
    set_or_remove_str(obj, "editorMode", &payload.editor_mode);
    set_or_remove_str(obj, "viewMode", &payload.view_mode);
    set_or_remove_str(obj, "defaultShell", &payload.default_shell);
    set_or_remove_str(obj, "plansDirectory", &payload.plans_directory);
    set_or_remove_u32(obj, "cleanupPeriodDays", payload.cleanup_period_days);

    set_or_remove_str_array(obj, "claudeMdExcludes", &payload.claude_md_excludes);
    set_or_remove_str_array(obj, "availableModels", &payload.available_models);

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
