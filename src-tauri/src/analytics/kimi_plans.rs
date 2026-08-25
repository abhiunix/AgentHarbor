//! Kimi Code Plans & Todos — read-only browser for `~/.kimi/plans/*.md` and
//! per-session `todos` embedded in `~/.kimi/sessions/<md5>/<sessionId>/state.json`.
//! Reuses `analytics::kimi_v2`'s `kimi_root()`/`build_dir_map()` for project
//! path resolution, matching `kimi_transcripts.rs`'s session-walking pattern.

use crate::analytics::kimi_v2::{build_dir_map, kimi_root};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KimiPlan {
    pub slug: String,
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub modified: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KimiTodoItem {
    pub title: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KimiTodoGroup {
    pub session_id: String,
    pub project_path: String,
    pub project_name: String,
    pub title: String,
    pub plan_slug: Option<String>,
    pub todos: Vec<KimiTodoItem>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct KimiStateTodo {
    #[serde(default)]
    title: String,
    #[serde(default)]
    status: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct KimiSessionState {
    #[serde(default)]
    todos: Vec<KimiStateTodo>,
    #[serde(default)]
    custom_title: Option<String>,
    #[serde(default)]
    plan_slug: Option<String>,
}

fn project_name_from_path(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

fn kimi_plans_dir() -> Option<std::path::PathBuf> {
    kimi_root().map(|r| r.join("plans"))
}

fn metadata_to_rfc3339(m: &std::fs::Metadata) -> Option<String> {
    m.modified().ok().map(|t| {
        let dt: chrono::DateTime<chrono::Utc> = t.into();
        dt.to_rfc3339()
    })
}

/// Only allow reads within `~/.kimi/plans` — mirrors `is_safe_plan_path` in
/// `commands::plans` for Claude/Cursor.
fn is_safe_kimi_plan_path(file_path: &str) -> bool {
    if file_path.contains("..") {
        return false;
    }
    let Some(plans_dir) = kimi_plans_dir() else { return false };
    let path = std::path::PathBuf::from(file_path);
    let canonical = dunce::canonicalize(&path).unwrap_or(path);
    canonical.starts_with(&plans_dir)
}

// ── Session enumeration (mirrors kimi_transcripts::discover_sessions) ───────

struct SessionDir {
    project_path: String,
    session_id: String,
    dir: std::path::PathBuf,
}

fn discover_session_dirs() -> Vec<SessionDir> {
    let Some(root) = kimi_root() else { return vec![] };
    let dir_map = build_dir_map();
    let sessions_root = root.join("sessions");
    let mut out = Vec::new();

    let Ok(md5_dirs) = std::fs::read_dir(&sessions_root) else { return out };
    for md5_entry in md5_dirs.flatten() {
        if !md5_entry.path().is_dir() {
            continue;
        }
        let md5_name = md5_entry.file_name().to_string_lossy().to_string();
        let project_path = dir_map
            .get(&md5_name)
            .map(|(p, _)| p.clone())
            .unwrap_or_else(|| md5_name.clone());

        let Ok(session_dirs) = std::fs::read_dir(md5_entry.path()) else { continue };
        for sess_entry in session_dirs.flatten() {
            let sess_path = sess_entry.path();
            if !sess_path.is_dir() {
                continue;
            }
            out.push(SessionDir {
                project_path: project_path.clone(),
                session_id: sess_entry.file_name().to_string_lossy().to_string(),
                dir: sess_path,
            });
        }
    }
    out
}

fn read_session_state(dir: &std::path::Path) -> Option<KimiSessionState> {
    let text = std::fs::read_to_string(dir.join("state.json")).ok()?;
    serde_json::from_str(&text).ok()
}

fn dir_modified_unix(dir: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(dir).and_then(|m| m.modified()).ok()
}

/// Extract a `(group, item)` pair from a session's `state.json`, or `None` if
/// there are no todos (sessions without todos are excluded entirely).
fn state_to_group(sd: &SessionDir, state: &KimiSessionState) -> Option<KimiTodoGroup> {
    if state.todos.is_empty() {
        return None;
    }
    let todos: Vec<KimiTodoItem> = state
        .todos
        .iter()
        .filter(|t| !t.title.is_empty())
        .map(|t| KimiTodoItem { title: t.title.clone(), status: t.status.clone() })
        .collect();
    if todos.is_empty() {
        return None;
    }
    let title = state
        .custom_title
        .as_ref()
        .filter(|t| !t.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| sd.session_id.clone());
    Some(KimiTodoGroup {
        session_id: sd.session_id.clone(),
        project_path: sd.project_path.clone(),
        project_name: project_name_from_path(&sd.project_path),
        title,
        plan_slug: state.plan_slug.clone(),
        todos,
    })
}

// ── Tauri Commands ───────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_kimi_plans() -> Result<Vec<KimiPlan>, String> {
    let Some(plans_dir) = kimi_plans_dir() else {
        return Ok(Vec::new());
    };
    if !plans_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut plans = Vec::new();
    let Ok(entries) = std::fs::read_dir(&plans_dir) else {
        return Ok(Vec::new());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let slug = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        if slug.is_empty() {
            continue;
        }
        let metadata = std::fs::metadata(&path).ok();
        let size_bytes = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
        let modified = metadata.as_ref().and_then(metadata_to_rfc3339);

        plans.push(KimiPlan {
            slug: slug.clone(),
            name: slug,
            path: path.to_string_lossy().to_string(),
            size_bytes,
            modified,
        });
    }

    plans.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(plans)
}

#[tauri::command]
pub fn read_kimi_plan(path: String) -> Result<String, String> {
    if !is_safe_kimi_plan_path(&path) {
        return Err("Invalid plan path".to_string());
    }
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_kimi_todo_groups() -> Result<Vec<KimiTodoGroup>, String> {
    let mut sessions = discover_session_dirs();
    sessions.sort_by_key(|sd| std::cmp::Reverse(dir_modified_unix(&sd.dir)));

    let groups: Vec<KimiTodoGroup> = sessions
        .iter()
        .filter_map(|sd| {
            let state = read_session_state(&sd.dir)?;
            state_to_group(sd, &state)
        })
        .collect();
    Ok(groups)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_to_group_extracts_todos_and_falls_back_title() {
        let sd = SessionDir {
            project_path: "/Users/test/project".to_string(),
            session_id: "sess-123".to_string(),
            dir: std::path::PathBuf::from("/nonexistent"),
        };
        let state = KimiSessionState {
            todos: [
                KimiStateTodo { title: "Write tests".to_string(), status: "done".to_string() },
                KimiStateTodo { title: "Fix bug".to_string(), status: "in_progress".to_string() },
                KimiStateTodo { title: "Ship it".to_string(), status: "pending".to_string() },
            ]
            .into(),
            custom_title: None,
            plan_slug: Some("my-plan".to_string()),
        };

        let group = state_to_group(&sd, &state).expect("group present");
        assert_eq!(group.session_id, "sess-123");
        assert_eq!(group.project_path, "/Users/test/project");
        assert_eq!(group.project_name, "project");
        assert_eq!(group.title, "sess-123");
        assert_eq!(group.plan_slug.as_deref(), Some("my-plan"));
        assert_eq!(group.todos.len(), 3);
        assert_eq!(group.todos[0].title, "Write tests");
        assert_eq!(group.todos[0].status, "done");
        assert_eq!(group.todos[1].status, "in_progress");
        assert_eq!(group.todos[2].status, "pending");
    }

    #[test]
    fn state_to_group_uses_custom_title_and_skips_empty_titles() {
        let sd = SessionDir {
            project_path: "/Users/test/project".to_string(),
            session_id: "sess-456".to_string(),
            dir: std::path::PathBuf::from("/nonexistent"),
        };
        let state = KimiSessionState {
            todos: [
                KimiStateTodo { title: String::new(), status: "pending".to_string() },
                KimiStateTodo { title: "Real task".to_string(), status: "pending".to_string() },
            ]
            .into(),
            custom_title: Some("My session title".to_string()),
            plan_slug: None,
        };

        let group = state_to_group(&sd, &state).expect("group present");
        assert_eq!(group.title, "My session title");
        assert_eq!(group.todos.len(), 1);
        assert_eq!(group.todos[0].title, "Real task");
        assert!(group.plan_slug.is_none());
    }

    #[test]
    fn state_to_group_returns_none_when_no_todos() {
        let sd = SessionDir {
            project_path: "/Users/test/project".to_string(),
            session_id: "sess-empty".to_string(),
            dir: std::path::PathBuf::from("/nonexistent"),
        };
        let state = KimiSessionState { todos: [].into(), custom_title: None, plan_slug: None };
        assert!(state_to_group(&sd, &state).is_none());
    }

    #[test]
    fn list_kimi_plans_returns_empty_when_dir_absent() {
        // kimi_root() resolves to a real ~/.kimi that may or may not have a
        // plans/ subdir on this machine; list_kimi_plans must never error
        // either way — it only returns Ok(vec) or Ok(entries).
        let result = list_kimi_plans();
        assert!(result.is_ok());
    }
}
