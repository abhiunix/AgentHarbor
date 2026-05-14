use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanEntry {
    pub name: String,
    pub source: String,
    pub file_path: String,
    pub overview: String,
    pub modified_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub content: String,
    pub status: String,
    pub source: String,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoStats {
    pub total: usize,
    pub pending: usize,
    pub in_progress: usize,
    pub completed: usize,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct CursorPlanFrontmatter {
    #[serde(default)]
    name: String,
    #[serde(default)]
    overview: String,
    #[serde(default)]
    todos: Vec<CursorPlanTodo>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct CursorPlanTodo {
    #[serde(default)]
    content: String,
    #[serde(default)]
    status: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct ClaudeTodoEntry {
    #[serde(default)]
    content: String,
    #[serde(default)]
    status: String,
}

fn get_modified_at(path: &std::path::Path) -> String {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|t| {
            let dt: chrono::DateTime<chrono::Utc> = t.into();
            dt.to_rfc3339()
        })
        .unwrap_or_default()
}

fn parse_cursor_frontmatter(content: &str) -> Option<CursorPlanFrontmatter> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") { return None; }
    let after_first = &trimmed[3..];
    let end_idx = after_first.find("\n---")?;
    let yaml_str = &after_first[..end_idx];
    serde_yaml::from_str(yaml_str).ok()
}

fn encode_claude_project_path(path: &str) -> String {
    // Normalize: strip leading / (Unix) or drive prefix like C:\ (Windows)
    let s = path.trim_start_matches('/');
    #[cfg(target_os = "windows")]
    let s = s.trim_start_matches(|c: char| c.is_ascii_alphabetic())
        .trim_start_matches(':')
        .trim_start_matches('\\');
    if s.is_empty() {
        return String::new();
    }
    let normalized = s.replace('\\', "/").replace('/', "-");
    format!("-{}", normalized)
}

pub(crate) fn is_safe_plan_path(file_path: &str) -> bool {
    if file_path.contains("..") {
        return false;
    }
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return false,
    };
    let path = PathBuf::from(file_path);
    let canonical = dunce::canonicalize(&path).unwrap_or(path.clone());
    let claude_plans = home.join(".claude").join("plans");
    let claude_projects = home.join(".claude").join("projects");
    let cursor_plans = home.join(".cursor").join("plans");
    if canonical.starts_with(&claude_plans) || canonical.starts_with(&cursor_plans) || canonical.starts_with(&claude_projects) {
        return true;
    }
    file_path.contains(".cursor/plans") || file_path.contains(".cursor\\plans")
        || file_path.contains(".claude/plans") || file_path.contains(".claude\\plans")
        || file_path.contains(".claude/projects")
}

#[tauri::command]
pub fn list_plans() -> Result<Vec<PlanEntry>, String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let mut plans = Vec::new();

    let claude_dir = home.join(".claude").join("plans");
    if claude_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&claude_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() { continue; }
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext != "md" { continue; }

                let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
                let content = std::fs::read_to_string(&path).unwrap_or_default();
                let overview: String = content.chars().take(200).collect();

                plans.push(PlanEntry {
                    name,
                    source: "claude".to_string(),
                    file_path: path.to_string_lossy().to_string(),
                    overview,
                    modified_at: get_modified_at(&path),
                });
            }
        }
    }

    let cursor_dir = home.join(".cursor").join("plans");
    if cursor_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&cursor_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() { continue; }
                let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !fname.ends_with(".plan.md") { continue; }

                let content = std::fs::read_to_string(&path).unwrap_or_default();
                let fm = parse_cursor_frontmatter(&content);
                let name = fm.as_ref().map(|f| f.name.clone()).unwrap_or_else(|| {
                    path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string()
                });
                let overview = fm.as_ref().map(|f| f.overview.clone()).unwrap_or_else(|| {
                    content.chars().take(200).collect()
                });

                plans.push(PlanEntry {
                    name,
                    source: "cursor".to_string(),
                    file_path: path.to_string_lossy().to_string(),
                    overview,
                    modified_at: get_modified_at(&path),
                });
            }
        }
    }

    plans.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
    Ok(plans)
}

#[tauri::command]
pub fn list_project_plans(project_path: String) -> Result<Vec<PlanEntry>, String> {
    if project_path.contains("..") {
        return Err("Invalid project path".to_string());
    }
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let base = PathBuf::from(&project_path);
    let mut plans = Vec::new();

    // Claude project-scoped plans: ~/.claude/projects/<encoded-path>/plans/
    let encoded = encode_claude_project_path(&project_path);
    if !encoded.is_empty() {
        let claude_project_plans = home.join(".claude").join("projects").join(&encoded).join("plans");
        if claude_project_plans.exists() {
            if let Ok(entries) = std::fs::read_dir(&claude_project_plans) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.is_file() { continue; }
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    if ext != "md" { continue; }

                    let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
                    let content = std::fs::read_to_string(&path).unwrap_or_default();
                    let overview: String = content.chars().take(200).collect();

                    plans.push(PlanEntry {
                        name,
                        source: "claude".to_string(),
                        file_path: path.to_string_lossy().to_string(),
                        overview,
                        modified_at: get_modified_at(&path),
                    });
                }
            }
        }
    }

    // Cursor project-scoped plans: <project>/.cursor/plans/
    let cursor_plans_dir = base.join(".cursor").join("plans");
    if cursor_plans_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&cursor_plans_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !fname.ends_with(".plan.md") {
                    continue;
                }

                let content = std::fs::read_to_string(&path).unwrap_or_default();
                let fm = parse_cursor_frontmatter(&content);
                let name = fm
                    .as_ref()
                    .map(|f| f.name.clone())
                    .unwrap_or_else(|| {
                        path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string()
                    });
                let overview = fm
                    .as_ref()
                    .map(|f| f.overview.clone())
                    .unwrap_or_else(|| content.chars().take(200).collect());

                plans.push(PlanEntry {
                    name,
                    source: "cursor".to_string(),
                    file_path: path.to_string_lossy().to_string(),
                    overview,
                    modified_at: get_modified_at(&path),
                });
            }
        }
    }

    plans.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
    Ok(plans)
}

#[tauri::command]
pub fn read_plan(file_path: String) -> Result<String, String> {
    if !is_safe_plan_path(&file_path) {
        return Err("Invalid plan path".to_string());
    }
    std::fs::read_to_string(&file_path).map_err(|e| e.to_string())
}

/// Delete a plan file from disk. The user is expected to have confirmed in
/// the UI. We do NOT enforce `is_safe_plan_path` here because the Debate
/// page also lists custom user-picked / project-scanned plans that live
/// outside `~/.claude/plans/` and `~/.cursor/plans/` — those need to be
/// deletable too. Sanity checks: must exist, must be a regular file, must
/// have a `.md` extension (so a misrouted call can't `rm` an arbitrary file).
#[tauri::command]
pub fn delete_plan_file(file_path: String) -> Result<(), String> {
    let path = std::path::Path::new(&file_path);
    if !path.exists() {
        return Err(format!("File does not exist: {}", file_path));
    }
    if !path.is_file() {
        return Err(format!("Not a regular file: {}", file_path));
    }
    if path.extension().and_then(|s| s.to_str()) != Some("md") {
        return Err(format!("Refusing to delete non-.md file: {}", file_path));
    }
    std::fs::remove_file(path)
        .map_err(|e| format!("Failed to delete {}: {}", file_path, e))?;
    Ok(())
}

// ── Hidden-from-Debate-page list (registry & custom plans the user has
//    chosen to hide without deleting from disk) ─────────────────────────────

fn debate_hidden_plans_path() -> std::path::PathBuf {
    crate::utils::paths::app_data_dir().join("debate_hidden_plans.json")
}

fn load_hidden_plans() -> std::collections::HashSet<String> {
    let p = debate_hidden_plans_path();
    if !p.exists() {
        return std::collections::HashSet::new();
    }
    std::fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_hidden_plans(set: &std::collections::HashSet<String>) -> Result<(), String> {
    let p = debate_hidden_plans_path();
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(set).map_err(|e| e.to_string())?;
    crate::utils::paths::atomic_write_str(&p, &json)
}

/// Returns the list of plan `file_path`s that the user has hidden from the
/// Debate page. The file is left on disk; just filtered out client-side.
#[tauri::command]
pub fn list_hidden_debate_plans() -> Vec<String> {
    let mut v: Vec<String> = load_hidden_plans().into_iter().collect();
    v.sort();
    v
}

/// Hide a plan from the Debate page without touching the file on disk.
/// Persistent across app restarts.
#[tauri::command]
pub fn hide_plan_from_debate(file_path: String) -> Result<(), String> {
    let mut set = load_hidden_plans();
    set.insert(file_path);
    save_hidden_plans(&set)
}

/// Un-hide a single previously-hidden plan path. No-op if not present.
#[tauri::command]
pub fn unhide_plan_from_debate(file_path: String) -> Result<(), String> {
    let mut set = load_hidden_plans();
    if set.remove(&file_path) {
        save_hidden_plans(&set)?;
    }
    Ok(())
}

/// Clear the entire hidden-plans list — equivalent to "show everything again".
#[tauri::command]
pub fn clear_hidden_debate_plans() -> Result<(), String> {
    save_hidden_plans(&std::collections::HashSet::new())
}

fn claude_project_session_ids(home: &std::path::Path, project_path: &str) -> std::collections::HashSet<String> {
    let encoded = encode_claude_project_path(project_path);
    if encoded.is_empty() {
        return std::collections::HashSet::new();
    }
    let project_dir = home.join(".claude").join("projects").join(&encoded);
    let mut ids = std::collections::HashSet::new();
    if let Ok(entries) = std::fs::read_dir(&project_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        ids.insert(stem.to_string());
                        let prefix: String = stem.split('-').take(5).collect::<Vec<_>>().join("-");
                        if !prefix.is_empty() {
                            ids.insert(prefix);
                        }
                    }
                }
        }
    }
    ids
}

#[tauri::command]
pub fn list_todos(project_path: Option<String>) -> Result<Vec<TodoItem>, String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let mut todos = Vec::new();

    let claude_project_sessions = project_path
        .as_ref()
        .map(|p| claude_project_session_ids(home.as_path(), p))
        .unwrap_or_default();

    let claude_dir = home.join(".claude").join("todos");
    if claude_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&claude_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext != "json" {
                    continue;
                }

                let fname = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
                let session_id = fname.split('-').take(5).collect::<Vec<_>>().join("-");

                if !claude_project_sessions.is_empty() && !claude_project_sessions.contains(&session_id) {
                    continue;
                }

                let content = match std::fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let items: Vec<ClaudeTodoEntry> = match serde_json::from_str(&content) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                for item in items {
                    if item.content.is_empty() {
                        continue;
                    }
                    todos.push(TodoItem {
                        content: item.content,
                        status: item.status,
                        source: "claude".to_string(),
                        session_id: Some(session_id.clone()),
                    });
                }
            }
        }
    }

    let cursor_dir = home.join(".cursor").join("plans");
    let cursor_include_global = project_path.is_none();
    if cursor_include_global && cursor_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&cursor_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !fname.ends_with(".plan.md") {
                    continue;
                }

                let content = match std::fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                if let Some(fm) = parse_cursor_frontmatter(&content) {
                    for todo in fm.todos {
                        if todo.content.is_empty() {
                            continue;
                        }
                        todos.push(TodoItem {
                            content: todo.content,
                            status: todo.status,
                            source: "cursor".to_string(),
                            session_id: None,
                        });
                    }
                }
            }
        }
    }

    if let Some(ref proj_path) = project_path {
        let base = PathBuf::from(proj_path);
        let project_cursor_plans = base.join(".cursor").join("plans");
        if project_cursor_plans.exists() {
            if let Ok(entries) = std::fs::read_dir(&project_cursor_plans) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.is_file() {
                        continue;
                    }
                    let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if !fname.ends_with(".plan.md") {
                        continue;
                    }

                    let content = match std::fs::read_to_string(&path) {
                        Ok(c) => c,
                        Err(_) => continue,
                    };
                    if let Some(fm) = parse_cursor_frontmatter(&content) {
                        for todo in fm.todos {
                            if todo.content.is_empty() {
                                continue;
                            }
                            todos.push(TodoItem {
                                content: todo.content,
                                status: todo.status,
                                source: "cursor".to_string(),
                                session_id: None,
                            });
                        }
                    }
                }
            }
        }
    }

    Ok(todos)
}

#[tauri::command]
pub fn get_todo_stats(project_path: Option<String>) -> Result<TodoStats, String> {
    let todos = list_todos(project_path)?;
    let total = todos.len();
    let pending = todos.iter().filter(|t| t.status == "pending").count();
    let in_progress = todos.iter().filter(|t| t.status == "in_progress").count();
    let completed = todos.iter().filter(|t| t.status == "completed").count();
    Ok(TodoStats { total, pending, in_progress, completed })
}
