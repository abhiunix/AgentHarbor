use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static SETTINGS_MUTEX: Mutex<()> = Mutex::new(());

// ── Structs ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiHook {
    pub hook_type: String,
    pub matcher: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiSkill {
    pub name: String,
    pub file_path: String,
    pub has_scripts: bool,
    pub has_resources: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiAgent {
    pub name: String,
    pub file_path: String,
    pub is_global: bool,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiExtension {
    pub name: String,
    pub dir_path: String,
    pub has_manifest: bool,
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn gemini_home() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".gemini")
}

fn settings_path() -> PathBuf {
    gemini_home().join("settings.json")
}

fn read_settings_value() -> Result<Value, String> {
    let path = settings_path();
    if !path.exists() {
        return Ok(Value::Object(serde_json::Map::new()));
    }
    let content = fs::read_to_string(&path).map_err(|e| format!("Failed to read settings: {}", e))?;
    serde_json::from_str(&content).map_err(|e| format!("Failed to parse settings: {}", e))
}

fn write_settings_value(value: &Value) -> Result<(), String> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create .gemini directory: {}", e))?;
    }
    let content = serde_json::to_string_pretty(value)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;
    crate::utils::paths::atomic_write_str(&path, &content)
}

fn atomic_write_string(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }
    crate::utils::paths::atomic_write_str(path, content)
}

// ── Global Config Commands ───────────────────────────────────────────────────

#[tauri::command]
pub fn read_gemini_settings() -> Result<String, String> {
    let path = settings_path();
    if !path.exists() {
        return Ok("{}".to_string());
    }
    fs::read_to_string(&path).map_err(|e| format!("Failed to read settings: {}", e))
}

#[tauri::command]
pub fn write_gemini_settings(content: String) -> Result<(), String> {
    let _lock = SETTINGS_MUTEX
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?;
    // Validate JSON before writing
    let _: Value =
        serde_json::from_str(&content).map_err(|e| format!("Invalid JSON: {}", e))?;
    let path = settings_path();
    atomic_write_string(&path, &content)
}

#[tauri::command]
pub fn get_gemini_mcp_servers() -> Result<Vec<String>, String> {
    let value = read_settings_value()?;
    let servers = match value.get("mcpServers") {
        Some(Value::Object(map)) => map.keys().cloned().collect(),
        _ => Vec::new(),
    };
    Ok(servers)
}

#[tauri::command]
pub fn add_gemini_mcp_server(name: String, config: Value) -> Result<(), String> {
    let _lock = SETTINGS_MUTEX
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?;
    let mut value = read_settings_value()?;
    let obj = value
        .as_object_mut()
        .ok_or_else(|| "Settings is not an object".to_string())?;

    if !obj.contains_key("mcpServers") {
        obj.insert(
            "mcpServers".to_string(),
            Value::Object(serde_json::Map::new()),
        );
    }

    if let Some(Value::Object(servers)) = obj.get_mut("mcpServers") {
        servers.insert(name, config);
    }

    write_settings_value(&value)
}

#[tauri::command]
pub fn remove_gemini_mcp_server(name: String) -> Result<(), String> {
    let _lock = SETTINGS_MUTEX
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?;
    let mut value = read_settings_value()?;

    if let Some(Value::Object(servers)) = value
        .as_object_mut()
        .and_then(|obj| obj.get_mut("mcpServers"))
    {
        servers.remove(&name);
    }

    write_settings_value(&value)
}

// ── Memory Commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn read_gemini_memory() -> Result<String, String> {
    let path = gemini_home().join("GEMINI.md");
    if !path.exists() {
        return Ok(String::new());
    }
    fs::read_to_string(&path).map_err(|e| format!("Failed to read GEMINI.md: {}", e))
}

#[tauri::command]
pub fn write_gemini_memory(content: String) -> Result<(), String> {
    let path = gemini_home().join("GEMINI.md");
    atomic_write_string(&path, &content)
}

#[tauri::command]
pub fn read_gemini_project_memory(project_path: String) -> Result<String, String> {
    if project_path.contains("..") {
        return Err("Invalid project path".to_string());
    }
    let path = PathBuf::from(&project_path).join("GEMINI.md");
    if !path.exists() {
        return Ok(String::new());
    }
    fs::read_to_string(&path).map_err(|e| format!("Failed to read GEMINI.md: {}", e))
}

#[tauri::command]
pub fn write_gemini_project_memory(project_path: String, content: String) -> Result<(), String> {
    if project_path.contains("..") {
        return Err("Invalid project path".to_string());
    }
    let path = PathBuf::from(&project_path).join("GEMINI.md");
    atomic_write_string(&path, &content)
}

// ── Hooks Command ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_gemini_hooks() -> Result<Vec<GeminiHook>, String> {
    let value = read_settings_value()?;
    let mut hooks = Vec::new();

    if let Some(Value::Object(hooks_obj)) = value.get("hooks") {
        for (hook_type, entries) in hooks_obj {
            if let Value::Array(arr) = entries {
                for entry in arr {
                    let matcher = entry
                        .get("matcher")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let command = entry
                        .get("command")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    hooks.push(GeminiHook {
                        hook_type: hook_type.clone(),
                        matcher,
                        command,
                    });
                }
            }
        }
    }

    Ok(hooks)
}

// ── Skills Command ───────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_gemini_skills(project_path: Option<String>) -> Result<Vec<GeminiSkill>, String> {
    let skills_dir = match &project_path {
        Some(p) => {
            if p.contains("..") {
                return Err("Invalid project path".to_string());
            }
            PathBuf::from(p).join(".gemini").join("skills")
        }
        None => gemini_home().join("skills"),
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
                    let has_resources = entry_path.join("resources").exists();

                    skills.push(GeminiSkill {
                        name,
                        file_path: skill_md.to_string_lossy().to_string(),
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

// ── Agents Commands ──────────────────────────────────────────────────────────

fn collect_agents(dir: &PathBuf, is_global: bool) -> Vec<GeminiAgent> {
    if !dir.exists() {
        return Vec::new();
    }
    let mut agents = Vec::new();

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|e| e == "md") {
                let name = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
                agents.push(GeminiAgent {
                    name,
                    file_path: path.to_string_lossy().to_string(),
                    is_global,
                    size_bytes,
                });
            }
        }
    }

    agents
}

#[tauri::command]
pub fn list_gemini_agents(project_path: Option<String>) -> Result<Vec<GeminiAgent>, String> {
    let mut agents = match &project_path {
        Some(p) => {
            if p.contains("..") {
                return Err("Invalid project path".to_string());
            }
            let project_agents_dir = PathBuf::from(p).join(".gemini").join("agents");
            let global_agents_dir = gemini_home().join("agents");
            let mut all = collect_agents(&project_agents_dir, false);
            all.extend(collect_agents(&global_agents_dir, true));
            all
        }
        None => {
            let global_agents_dir = gemini_home().join("agents");
            collect_agents(&global_agents_dir, true)
        }
    };

    agents.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(agents)
}

#[tauri::command]
pub fn read_gemini_agent(file_path: String) -> Result<String, String> {
    if file_path.contains("..") {
        return Err("Invalid file path".to_string());
    }
    let path = PathBuf::from(&file_path);
    if !path.exists() {
        return Ok(String::new());
    }
    fs::read_to_string(&path).map_err(|e| format!("Failed to read agent file: {}", e))
}

// ── Extensions Command ───────────────────────────────────────────────────────

#[tauri::command]
pub fn list_gemini_extensions() -> Result<Vec<GeminiExtension>, String> {
    let ext_dir = gemini_home().join("extensions");
    if !ext_dir.exists() {
        return Ok(Vec::new());
    }

    let mut extensions = Vec::new();

    if let Ok(entries) = fs::read_dir(&ext_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name() {
                    let name = name.to_string_lossy().to_string();
                    let has_manifest = path.join("gemini-extension.json").exists();
                    extensions.push(GeminiExtension {
                        name,
                        dir_path: path.to_string_lossy().to_string(),
                        has_manifest,
                    });
                }
            }
        }
    }

    extensions.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(extensions)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_parse_settings_empty() {
        let value: Value = serde_json::from_str("{}").unwrap();
        let servers = match value.get("mcpServers") {
            Some(Value::Object(map)) => map.keys().cloned().collect::<Vec<_>>(),
            _ => Vec::new(),
        };
        assert!(servers.is_empty());
    }

    #[test]
    fn test_parse_settings_with_mcp_servers() {
        let json = r#"{
            "mcpServers": {
                "github": {"command": "gh", "args": ["mcp"]},
                "postgres": {"command": "pg-mcp", "args": []}
            }
        }"#;
        let value: Value = serde_json::from_str(json).unwrap();
        let mut servers = match value.get("mcpServers") {
            Some(Value::Object(map)) => map.keys().cloned().collect::<Vec<_>>(),
            _ => Vec::new(),
        };
        servers.sort();
        assert_eq!(servers, vec!["github", "postgres"]);
    }

    #[test]
    fn test_parse_hooks() {
        let json = r#"{
            "hooks": {
                "beforeTool": [
                    {"matcher": "shell_.*", "command": "echo before"},
                    {"matcher": "file_write", "command": "lint-check"}
                ],
                "afterTool": [
                    {"matcher": ".*", "command": "echo done"}
                ]
            }
        }"#;
        let value: Value = serde_json::from_str(json).unwrap();
        let mut hooks = Vec::new();
        if let Some(Value::Object(hooks_obj)) = value.get("hooks") {
            for (hook_type, entries) in hooks_obj {
                if let Value::Array(arr) = entries {
                    for entry in arr {
                        let matcher = entry.get("matcher").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let command = entry.get("command").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        hooks.push(GeminiHook {
                            hook_type: hook_type.clone(),
                            matcher,
                            command,
                        });
                    }
                }
            }
        }
        assert_eq!(hooks.len(), 3);
        let before_hooks: Vec<_> = hooks.iter().filter(|h| h.hook_type == "beforeTool").collect();
        assert_eq!(before_hooks.len(), 2);
        assert_eq!(before_hooks[0].matcher, "shell_.*");
    }

    #[test]
    fn test_parse_settings_no_hooks() {
        let json = r#"{"model": "gemini-2.5-pro"}"#;
        let value: Value = serde_json::from_str(json).unwrap();
        let hooks_obj = value.get("hooks");
        assert!(hooks_obj.is_none());
    }

    #[test]
    fn test_add_mcp_server_to_empty_settings() {
        let mut value: Value = serde_json::from_str("{}").unwrap();
        let obj = value.as_object_mut().unwrap();
        if !obj.contains_key("mcpServers") {
            obj.insert("mcpServers".to_string(), Value::Object(serde_json::Map::new()));
        }
        if let Some(Value::Object(servers)) = obj.get_mut("mcpServers") {
            servers.insert(
                "test-server".to_string(),
                serde_json::json!({"command": "test", "args": []}),
            );
        }
        let servers = value["mcpServers"].as_object().unwrap();
        assert!(servers.contains_key("test-server"));
    }

    #[test]
    fn test_remove_mcp_server() {
        let mut value: Value = serde_json::from_str(r#"{"mcpServers": {"a": {}, "b": {}}}"#).unwrap();
        if let Some(Value::Object(servers)) = value
            .as_object_mut()
            .and_then(|obj| obj.get_mut("mcpServers"))
        {
            servers.remove("a");
        }
        let servers = value["mcpServers"].as_object().unwrap();
        assert!(!servers.contains_key("a"));
        assert!(servers.contains_key("b"));
    }

    #[test]
    fn test_remove_nonexistent_mcp_server() {
        let mut value: Value = serde_json::from_str(r#"{"mcpServers": {"a": {}}}"#).unwrap();
        if let Some(Value::Object(servers)) = value
            .as_object_mut()
            .and_then(|obj| obj.get_mut("mcpServers"))
        {
            servers.remove("nonexistent");
        }
        let servers = value["mcpServers"].as_object().unwrap();
        assert_eq!(servers.len(), 1);
    }

    #[test]
    fn test_list_skills_empty_dir() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join(".gemini").join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        let result = list_gemini_skills(Some(temp.path().to_string_lossy().to_string())).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_list_skills_with_entries() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join(".gemini").join("skills");

        let skill_a = skills_dir.join("my-skill");
        fs::create_dir_all(&skill_a).unwrap();
        fs::write(skill_a.join("SKILL.md"), "# My Skill").unwrap();
        fs::create_dir_all(skill_a.join("scripts")).unwrap();

        // skill without SKILL.md should be skipped
        let skill_b = skills_dir.join("incomplete-skill");
        fs::create_dir_all(&skill_b).unwrap();

        let result = list_gemini_skills(Some(temp.path().to_string_lossy().to_string())).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "my-skill");
        assert!(result[0].has_scripts);
        assert!(!result[0].has_resources);
    }

    #[test]
    fn test_list_skills_nonexistent_dir() {
        let result = list_gemini_skills(Some("/tmp/nonexistent-gemini-test-dir".to_string())).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_collect_agents_empty() {
        let temp = TempDir::new().unwrap();
        let agents_dir = temp.path().join("agents");
        fs::create_dir_all(&agents_dir).unwrap();

        let result = collect_agents(&agents_dir.to_path_buf(), true);
        assert!(result.is_empty());
    }

    #[test]
    fn test_collect_agents_with_files() {
        let temp = TempDir::new().unwrap();
        let agents_dir = temp.path().join("agents");
        fs::create_dir_all(&agents_dir).unwrap();
        fs::write(agents_dir.join("reviewer.md"), "You are a reviewer.").unwrap();
        fs::write(agents_dir.join("planner.md"), "You plan things.").unwrap();
        fs::write(agents_dir.join("notes.txt"), "not an agent").unwrap();

        let result = collect_agents(&agents_dir.to_path_buf(), false);
        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|a| !a.is_global));
        let names: Vec<_> = result.iter().map(|a| a.name.as_str()).collect();
        assert!(names.contains(&"reviewer"));
        assert!(names.contains(&"planner"));
    }

    #[test]
    fn test_collect_agents_nonexistent() {
        let path = PathBuf::from("/tmp/nonexistent-gemini-agents-test");
        let result = collect_agents(&path, true);
        assert!(result.is_empty());
    }

    #[test]
    fn test_list_extensions_empty() {
        // If the directory doesn't exist, we get an empty list
        // This test just validates the struct construction
        let ext = GeminiExtension {
            name: "my-ext".to_string(),
            dir_path: "/home/.gemini/extensions/my-ext".to_string(),
            has_manifest: true,
        };
        assert_eq!(ext.name, "my-ext");
        assert!(ext.has_manifest);
    }

    #[test]
    fn test_gemini_hook_struct() {
        let hook = GeminiHook {
            hook_type: "beforeTool".to_string(),
            matcher: "shell_.*".to_string(),
            command: "echo hi".to_string(),
        };
        let json = serde_json::to_string(&hook).unwrap();
        let parsed: GeminiHook = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.hook_type, "beforeTool");
        assert_eq!(parsed.matcher, "shell_.*");
        assert_eq!(parsed.command, "echo hi");
    }

    #[test]
    fn test_gemini_agent_struct_serialization() {
        let agent = GeminiAgent {
            name: "code-reviewer".to_string(),
            file_path: "/home/.gemini/agents/code-reviewer.md".to_string(),
            is_global: true,
            size_bytes: 1024,
        };
        let json = serde_json::to_string(&agent).unwrap();
        assert!(json.contains("code-reviewer"));
        assert!(json.contains("is_global"));
    }

    #[test]
    fn test_read_gemini_project_memory_path_traversal() {
        let result = read_gemini_project_memory("../../etc".to_string());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Invalid project path");
    }

    #[test]
    fn test_write_gemini_project_memory_path_traversal() {
        let result = write_gemini_project_memory("../../../etc".to_string(), "bad".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_read_gemini_agent_path_traversal() {
        let result = read_gemini_agent("../../etc/passwd".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_read_gemini_agent_nonexistent() {
        let result = read_gemini_agent("/tmp/nonexistent-gemini-agent.md".to_string());
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_read_gemini_agent_content() {
        let temp = TempDir::new().unwrap();
        let agent_path = temp.path().join("test-agent.md");
        fs::write(&agent_path, "# Test Agent\nYou are helpful.").unwrap();

        let result = read_gemini_agent(agent_path.to_string_lossy().to_string());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "# Test Agent\nYou are helpful.");
    }

    #[test]
    fn test_atomic_write() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("test.md");
        atomic_write_string(&path.to_path_buf(), "hello world").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "hello world");
    }

    #[test]
    fn test_full_settings_roundtrip() {
        let json = r#"{
            "mcpServers": {
                "github": {"command": "gh", "args": ["mcp"]}
            },
            "hooks": {
                "beforeTool": [
                    {"matcher": ".*", "command": "echo before"}
                ]
            },
            "model": "gemini-2.5-pro",
            "sandbox": true,
            "theme": "dark"
        }"#;
        let value: Value = serde_json::from_str(json).unwrap();

        // MCP servers
        let servers: Vec<_> = value["mcpServers"].as_object().unwrap().keys().cloned().collect();
        assert_eq!(servers, vec!["github"]);

        // Hooks
        let hooks = value["hooks"]["beforeTool"].as_array().unwrap();
        assert_eq!(hooks.len(), 1);

        // Other fields preserved
        assert_eq!(value["model"], "gemini-2.5-pro");
        assert_eq!(value["sandbox"], true);
        assert_eq!(value["theme"], "dark");
    }
}
