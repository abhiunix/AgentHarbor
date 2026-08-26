//! DeepSeek Harness (`dsh`) Plans & Todos — read-only browser for the
//! per-session `todos` and `plan` projections embedded in
//! `storages/session_projcache.json` (`tables.sessions.<id>.rows`).
//! Unlike Kimi, dsh has no `plans/*.md` files — a session's "plan" is the
//! `plan` projection's `{ active, wanted, running }` state.
//! Reuses `analytics::deepseek_v2`'s `dsh_root()` and `load_session_metadata()`
//! for workspace path/title resolution.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::analytics::deepseek_v2::{dsh_root, load_session_metadata, DshSessionMeta};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekTodoItem {
    pub title: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekTodoGroup {
    pub session_id: String,
    pub workspace_path: String,
    pub workspace_name: String,
    pub title: String,
    pub todos: Vec<DeepSeekTodoItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekPlan {
    pub session_id: String,
    pub workspace_path: String,
    pub workspace_name: String,
    pub title: String,
    pub active: bool,
    pub wanted: Option<String>,
    pub running: Option<String>,
}

// ── storages/session_projcache.json (todos + plan rows only) ────────────────

#[derive(Debug, Clone, Deserialize)]
struct SessionCacheFile {
    tables: SessionCacheTables,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct SessionCacheTables {
    #[serde(default)]
    sessions: HashMap<String, SessionEntryRaw>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct SessionEntryRaw {
    #[serde(default)]
    identity: IdentityRaw,
    #[serde(default)]
    rows: RowsRaw,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct IdentityRaw {
    #[serde(default, rename = "createdAt")]
    created_at: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RowsRaw {
    #[serde(default)]
    todos: Option<ValWrap<Option<Vec<serde_json::Value>>>>,
    #[serde(default)]
    plan: Option<ValWrap<PlanVal>>,
}

#[derive(Debug, Clone, Deserialize)]
struct ValWrap<T> {
    val: T,
}

#[derive(Debug, Clone, Deserialize)]
struct PlanVal {
    #[serde(default)]
    active: bool,
    #[serde(default)]
    wanted: Option<serde_json::Value>,
    #[serde(default)]
    running: Option<serde_json::Value>,
}

fn parse_session_cache_raw(content: &str) -> HashMap<String, SessionEntryRaw> {
    serde_json::from_str::<SessionCacheFile>(content)
        .map(|f| f.tables.sessions)
        .unwrap_or_default()
}

fn read_session_cache_raw(root: &std::path::Path) -> HashMap<String, SessionEntryRaw> {
    std::fs::read_to_string(root.join("storages").join("session_projcache.json"))
        .map(|text| parse_session_cache_raw(&text))
        .unwrap_or_default()
}

// ── Todo extraction (defensive: content/title/text + status/state) ─────────

/// Extract a todo item from a raw JSON value defensively — the confirmed
/// shape (from the dsh-tool-todo harness source) is `{ content, status }`,
/// but `title`/`text` and `state` are accepted as fallbacks in case a future
/// harness version renames the fields.
fn todo_item_from_value(v: &serde_json::Value) -> Option<DeepSeekTodoItem> {
    let obj = v.as_object()?;
    let title = obj
        .get("content")
        .or_else(|| obj.get("title"))
        .or_else(|| obj.get("text"))
        .and_then(|t| t.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())?;
    let status = obj.get("status").or_else(|| obj.get("state")).and_then(|s| s.as_str()).unwrap_or("pending");
    Some(DeepSeekTodoItem { title: title.to_string(), status: status.to_string() })
}

fn extract_todos(entry: &SessionEntryRaw) -> Vec<DeepSeekTodoItem> {
    entry
        .rows
        .todos
        .as_ref()
        .and_then(|w| w.val.as_ref())
        .map(|items| items.iter().filter_map(todo_item_from_value).collect())
        .unwrap_or_default()
}

/// A session's plan is worth showing when it's active, or a selection is
/// pending (`wanted`) or in flight (`running`) — `Option::is_some` already
/// treats JSON `null` as absent, so no extra null-checking is needed.
fn session_has_plan(plan: &PlanVal) -> bool {
    plan.active || plan.wanted.is_some() || plan.running.is_some()
}

/// Render a `wanted`/`running` value as display text: `null` is absent,
/// strings pass through, and anything else (bool, object) is JSON-stringified
/// best-effort.
fn value_to_display_text(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::Null => None,
        serde_json::Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
        }
        other => Some(other.to_string()),
    }
}

fn session_title(sid: &str, meta: Option<&DshSessionMeta>) -> String {
    meta.and_then(|m| m.title.as_ref())
        .filter(|t| !t.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| sid.to_string())
}

// ── Build lists ──────────────────────────────────────────────────────────────

fn build_todo_groups(
    raw: &HashMap<String, SessionEntryRaw>,
    metadata: &HashMap<String, DshSessionMeta>,
) -> Vec<DeepSeekTodoGroup> {
    let mut sessions: Vec<(&String, &SessionEntryRaw)> = raw.iter().collect();
    sessions.sort_by_key(|(_, e)| std::cmp::Reverse(e.identity.created_at));

    sessions
        .into_iter()
        .filter_map(|(sid, entry)| {
            let todos = extract_todos(entry);
            if todos.is_empty() {
                return None;
            }
            let meta = metadata.get(sid);
            Some(DeepSeekTodoGroup {
                session_id: sid.clone(),
                workspace_path: meta.map(|m| m.workspace_path.clone()).unwrap_or_default(),
                workspace_name: meta.map(|m| m.workspace_name.clone()).unwrap_or_default(),
                title: session_title(sid, meta),
                todos,
            })
        })
        .collect()
}

fn build_plans(
    raw: &HashMap<String, SessionEntryRaw>,
    metadata: &HashMap<String, DshSessionMeta>,
) -> Vec<DeepSeekPlan> {
    let mut sessions: Vec<(&String, &SessionEntryRaw)> = raw.iter().collect();
    sessions.sort_by_key(|(_, e)| std::cmp::Reverse(e.identity.created_at));

    sessions
        .into_iter()
        .filter_map(|(sid, entry)| {
            let plan = entry.rows.plan.as_ref().map(|w| &w.val)?;
            if !session_has_plan(plan) {
                return None;
            }
            let meta = metadata.get(sid);
            Some(DeepSeekPlan {
                session_id: sid.clone(),
                workspace_path: meta.map(|m| m.workspace_path.clone()).unwrap_or_default(),
                workspace_name: meta.map(|m| m.workspace_name.clone()).unwrap_or_default(),
                title: session_title(sid, meta),
                active: plan.active,
                wanted: plan.wanted.as_ref().and_then(value_to_display_text),
                running: plan.running.as_ref().and_then(value_to_display_text),
            })
        })
        .collect()
}

// ── Tauri Commands ───────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_deepseek_todo_groups() -> Vec<DeepSeekTodoGroup> {
    let Some(root) = dsh_root() else { return Vec::new() };
    let metadata = load_session_metadata(&root);
    let raw = read_session_cache_raw(&root);
    build_todo_groups(&raw, &metadata)
}

#[tauri::command]
pub fn list_deepseek_plans() -> Vec<DeepSeekPlan> {
    let Some(root) = dsh_root() else { return Vec::new() };
    let metadata = load_session_metadata(&root);
    let raw = read_session_cache_raw(&root);
    build_plans(&raw, &metadata)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::deepseek_v2::workspace_name_from_path;

    const RAW_FIXTURE: &str = r#"{
        "tables": {
            "sessions": {
                "session-1": {
                    "identity": { "createdAt": 2000 },
                    "rows": {
                        "todos": { "val": [
                            { "content": "Write tests", "status": "completed" },
                            { "content": "Ship it", "status": "pending" }
                        ]},
                        "plan": { "val": { "active": true, "wanted": null, "running": null } }
                    }
                },
                "session-2": {
                    "identity": { "createdAt": 1000 },
                    "rows": {
                        "todos": { "val": null },
                        "plan": { "val": { "active": false, "wanted": null, "running": null } }
                    }
                }
            }
        }
    }"#;

    fn meta(path: &str, title: Option<&str>) -> DshSessionMeta {
        DshSessionMeta {
            workspace_path: path.to_string(),
            workspace_name: workspace_name_from_path(path),
            title: title.map(|t| t.to_string()),
        }
    }

    #[test]
    fn todo_item_from_value_prefers_content_and_status() {
        let v: serde_json::Value =
            serde_json::from_str(r#"{"content":"Do thing","status":"in_progress"}"#).unwrap();
        let item = todo_item_from_value(&v).expect("item");
        assert_eq!(item.title, "Do thing");
        assert_eq!(item.status, "in_progress");
    }

    #[test]
    fn todo_item_from_value_falls_back_to_title_text_and_state() {
        let v: serde_json::Value = serde_json::from_str(r#"{"title":"Fallback title","state":"pending"}"#).unwrap();
        let item = todo_item_from_value(&v).expect("item");
        assert_eq!(item.title, "Fallback title");
        assert_eq!(item.status, "pending");

        let v2: serde_json::Value = serde_json::from_str(r#"{"text":"Text fallback"}"#).unwrap();
        let item2 = todo_item_from_value(&v2).expect("item2");
        assert_eq!(item2.title, "Text fallback");
        assert_eq!(item2.status, "pending");
    }

    #[test]
    fn todo_item_from_value_rejects_missing_or_empty_title() {
        let v: serde_json::Value = serde_json::from_str(r#"{"status":"pending"}"#).unwrap();
        assert!(todo_item_from_value(&v).is_none());

        let v2: serde_json::Value = serde_json::from_str(r#"{"content":"   "}"#).unwrap();
        assert!(todo_item_from_value(&v2).is_none());
    }

    #[test]
    fn session_has_plan_true_when_active_or_selection_pending() {
        let active = PlanVal { active: true, wanted: None, running: None };
        assert!(session_has_plan(&active));

        let wanted_v: serde_json::Value = serde_json::from_str("true").unwrap();
        let wanted = PlanVal { active: false, wanted: Some(wanted_v), running: None };
        assert!(session_has_plan(&wanted));

        let idle = PlanVal { active: false, wanted: None, running: None };
        assert!(!session_has_plan(&idle));
    }

    #[test]
    fn value_to_display_text_handles_strings_objects_and_null() {
        assert_eq!(value_to_display_text(&serde_json::Value::Null), None);

        let s: serde_json::Value = serde_json::from_str(r#""hello""#).unwrap();
        assert_eq!(value_to_display_text(&s), Some("hello".to_string()));

        let obj: serde_json::Value = serde_json::from_str(r#"{"commandId":"c1","wanted":true}"#).unwrap();
        assert_eq!(value_to_display_text(&obj), Some(obj.to_string()));
    }

    #[test]
    fn build_todo_groups_extracts_only_sessions_with_todos_newest_first() {
        let raw = parse_session_cache_raw(RAW_FIXTURE);
        let metadata = HashMap::from([
            ("session-1".to_string(), meta("/proj/alpha", Some("Alpha session"))),
            ("session-2".to_string(), meta("/proj/beta", None)),
        ]);

        let groups = build_todo_groups(&raw, &metadata);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].session_id, "session-1");
        assert_eq!(groups[0].title, "Alpha session");
        assert_eq!(groups[0].workspace_name, "alpha");
        assert_eq!(groups[0].todos.len(), 2);
        assert_eq!(groups[0].todos[0].status, "completed");
        assert_eq!(groups[0].todos[1].status, "pending");
    }

    #[test]
    fn build_todo_groups_returns_empty_when_no_session_has_todos() {
        let raw = parse_session_cache_raw(
            r#"{"tables":{"sessions":{"s":{"identity":{"createdAt":1},"rows":{"todos":{"val":null}}}}}}"#,
        );
        assert!(build_todo_groups(&raw, &HashMap::new()).is_empty());
    }

    #[test]
    fn build_plans_extracts_active_plan_only_and_falls_back_title() {
        let raw = parse_session_cache_raw(RAW_FIXTURE);
        let metadata = HashMap::from([("session-1".to_string(), meta("/proj/alpha", None))]);

        let plans = build_plans(&raw, &metadata);
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].session_id, "session-1");
        assert_eq!(plans[0].title, "session-1");
        assert!(plans[0].active);
        assert!(plans[0].wanted.is_none());
        assert!(plans[0].running.is_none());
    }

    #[test]
    fn build_plans_returns_empty_when_no_session_has_plan() {
        let raw =
            parse_session_cache_raw(r#"{"tables":{"sessions":{"s":{"identity":{"createdAt":1},"rows":{}}}}}"#);
        assert!(build_plans(&raw, &HashMap::new()).is_empty());
    }

    #[test]
    fn list_functions_do_not_panic_on_real_environment() {
        let _ = list_deepseek_todo_groups();
        let _ = list_deepseek_plans();
    }
}
