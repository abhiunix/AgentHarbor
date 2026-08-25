//! Kimi Code Transcripts — read-only session browser/reader for Kimi Code CLI
//! local session files under `~/.kimi/sessions/<md5(project_path)>/<sessionId>/`.
//! Mirrors `commands::transcripts` (Claude/Cursor) in shape, but is view-only:
//! no edit/replace/backup affordances, since Kimi's on-disk format (context +
//! wire, both derived from a live session) isn't safe to hand-edit like a flat
//! Claude/Cursor JSONL transcript.
//!
//! Reuses `analytics::kimi_v2`'s `kimi.json` work_dirs → project-path
//! resolution (`build_dir_map`/`md5_dir`) and its home-dir helper
//! (`kimi_root`).

use crate::analytics::kimi_v2::{build_dir_map, kimi_root, md5_dir};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KimiTranscriptSession {
    pub session_id: String,
    pub project_path: String,
    pub project_name: String,
    pub title: String,
    pub message_count: u64,
    pub first_activity: Option<String>,
    pub last_activity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KimiTranscriptMessage {
    pub role: String,
    pub content: String,
    pub timestamp: Option<String>,
}

fn project_name_from_path(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

fn unix_to_rfc3339(ts: f64) -> Option<String> {
    chrono::DateTime::<chrono::Utc>::from_timestamp(ts as i64, 0).map(|dt| dt.to_rfc3339())
}

/// Extract the visible text from a `context.jsonl` `content` value: a plain
/// string, or an array of parts where only `type: "text"` parts are rendered
/// — `think` blocks, tool_calls, and other structured parts are skipped.
fn extract_text(content: &serde_json::Value) -> String {
    if let Some(s) = content.as_str() {
        return s.to_string();
    }
    if let Some(arr) = content.as_array() {
        let texts: Vec<String> = arr
            .iter()
            .filter_map(|item| {
                if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                    item.get("text").and_then(|t| t.as_str()).map(String::from)
                } else {
                    None
                }
            })
            .collect();
        return texts.join("\n\n");
    }
    String::new()
}

// ── wire.jsonl ───────────────────────────────────────────────────────────────

/// Ordered turn-boundary timestamps from `wire.jsonl`, used to best-effort
/// align user/assistant messages parsed from `context.jsonl` (which itself
/// carries no timestamps). `first`/`last` cover the whole file, independent
/// of turn boundaries, for session first/last-activity.
#[derive(Default, Debug, PartialEq)]
struct WireActivity {
    turn_begins: Vec<f64>,
    turn_ends: Vec<f64>,
    first: Option<f64>,
    last: Option<f64>,
}

fn parse_wire_activity(content: &str) -> WireActivity {
    let mut w = WireActivity::default();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(obj) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        let Some(ts) = obj.get("timestamp").and_then(|t| t.as_f64()) else { continue };
        w.first = Some(w.first.map_or(ts, |cur: f64| cur.min(ts)));
        w.last = Some(w.last.map_or(ts, |cur: f64| cur.max(ts)));
        match obj
            .get("message")
            .and_then(|m| m.get("type"))
            .and_then(|t| t.as_str())
        {
            Some("TurnBegin") => w.turn_begins.push(ts),
            Some("TurnEnd") => w.turn_ends.push(ts),
            _ => {}
        }
    }
    w
}

// ── context.jsonl ────────────────────────────────────────────────────────────

/// Parse `context.jsonl` into ordered display messages, best-effort
/// timestamped from `wire`: the Nth user message gets the Nth `TurnBegin`
/// timestamp, the Nth assistant message gets the Nth `TurnEnd` timestamp.
/// `_checkpoint`/`_usage`/`tool` lines are internal bookkeeping and dropped;
/// `_system_prompt` collapses into a single "system" entry; assistant steps
/// that resolve to no visible text (pure tool orchestration) are skipped.
fn parse_context_messages(content: &str, wire: &WireActivity) -> Vec<KimiTranscriptMessage> {
    let mut out = Vec::new();
    let mut user_idx = 0usize;
    let mut assistant_idx = 0usize;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(obj) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        let role = obj.get("role").and_then(|r| r.as_str()).unwrap_or("");
        let content_val = obj.get("content");

        match role {
            "_system_prompt" => {
                let text = content_val.map(extract_text).unwrap_or_default();
                if !text.is_empty() {
                    out.push(KimiTranscriptMessage {
                        role: "system".to_string(),
                        content: text,
                        timestamp: None,
                    });
                }
            }
            "user" => {
                let text = content_val.map(extract_text).unwrap_or_default();
                let ts = wire.turn_begins.get(user_idx).copied();
                user_idx += 1;
                if !text.is_empty() {
                    out.push(KimiTranscriptMessage {
                        role: "user".to_string(),
                        content: text,
                        timestamp: ts.and_then(unix_to_rfc3339),
                    });
                }
            }
            "assistant" => {
                let text = content_val.map(extract_text).unwrap_or_default();
                let ts = wire.turn_ends.get(assistant_idx).copied();
                assistant_idx += 1;
                if !text.is_empty() {
                    out.push(KimiTranscriptMessage {
                        role: "assistant".to_string(),
                        content: text,
                        timestamp: ts.and_then(unix_to_rfc3339),
                    });
                }
            }
            // "_checkpoint", "_usage", "tool", and anything else are internal
            // bookkeeping / tool plumbing — not shown in the read-only viewer.
            _ => {}
        }
    }
    out
}

fn read_custom_title(dir: &std::path::Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join("state.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    json.get("custom_title")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
}

// ── Session enumeration ──────────────────────────────────────────────────────

struct SessionFile {
    project_path: String,
    session_id: String,
    dir: std::path::PathBuf,
}

fn discover_sessions() -> Vec<SessionFile> {
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
            out.push(SessionFile {
                project_path: project_path.clone(),
                session_id: sess_entry.file_name().to_string_lossy().to_string(),
                dir: sess_path,
            });
        }
    }
    out
}

fn resolve_session_dir(session_id: &str, project_path: Option<&str>) -> Result<std::path::PathBuf, String> {
    let root = kimi_root().ok_or_else(|| "Kimi home directory not found".to_string())?;
    if let Some(p) = project_path {
        let dir = root.join("sessions").join(md5_dir(p)).join(session_id);
        if dir.is_dir() {
            return Ok(dir);
        }
    }
    discover_sessions()
        .into_iter()
        .find(|sf| sf.session_id == session_id)
        .map(|sf| sf.dir)
        .ok_or_else(|| "Session not found".to_string())
}

struct KimiSessionData {
    messages: Vec<KimiTranscriptMessage>,
    custom_title: Option<String>,
    first_activity: Option<String>,
    last_activity: Option<String>,
}

fn load_session(sf: &SessionFile) -> KimiSessionData {
    let wire_content = std::fs::read_to_string(sf.dir.join("wire.jsonl")).unwrap_or_default();
    let wire = parse_wire_activity(&wire_content);
    let context_content = std::fs::read_to_string(sf.dir.join("context.jsonl")).unwrap_or_default();
    let messages = parse_context_messages(&context_content, &wire);
    KimiSessionData {
        messages,
        custom_title: read_custom_title(&sf.dir),
        first_activity: wire.first.and_then(unix_to_rfc3339),
        last_activity: wire.last.and_then(unix_to_rfc3339),
    }
}

/// `custom_title` if set, else the first user prompt (truncated), else the
/// session id.
fn session_title(data: &KimiSessionData, session_id: &str) -> String {
    if let Some(t) = &data.custom_title {
        return t.clone();
    }
    if let Some(first_user) = data.messages.iter().find(|m| m.role == "user") {
        let text = first_user.content.trim();
        if !text.is_empty() {
            let truncated: String = text.chars().take(80).collect();
            return if text.chars().count() > 80 {
                format!("{}…", truncated)
            } else {
                truncated
            };
        }
    }
    session_id.to_string()
}

fn to_session_summary(sf: &SessionFile, data: &KimiSessionData) -> KimiTranscriptSession {
    KimiTranscriptSession {
        session_id: sf.session_id.clone(),
        project_path: sf.project_path.clone(),
        project_name: project_name_from_path(&sf.project_path),
        title: session_title(data, &sf.session_id),
        message_count: data.messages.iter().filter(|m| m.role != "system").count() as u64,
        first_activity: data.first_activity.clone(),
        last_activity: data.last_activity.clone(),
    }
}

// ── Tauri Commands ───────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_kimi_transcript_sessions() -> Result<Vec<KimiTranscriptSession>, String> {
    let mut sessions: Vec<KimiTranscriptSession> = discover_sessions()
        .iter()
        .map(|sf| {
            let data = load_session(sf);
            to_session_summary(sf, &data)
        })
        .collect();
    sessions.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));
    Ok(sessions)
}

#[tauri::command]
pub fn read_kimi_transcript(
    session_id: String,
    project_path: Option<String>,
) -> Result<Vec<KimiTranscriptMessage>, String> {
    let dir = resolve_session_dir(&session_id, project_path.as_deref())?;
    let wire_content = std::fs::read_to_string(dir.join("wire.jsonl")).unwrap_or_default();
    let wire = parse_wire_activity(&wire_content);
    let context_content = std::fs::read_to_string(dir.join("context.jsonl"))
        .map_err(|e| format!("Failed to read transcript: {}", e))?;
    Ok(parse_context_messages(&context_content, &wire))
}

#[tauri::command]
pub fn search_kimi_transcripts(query: String) -> Result<Vec<KimiTranscriptSession>, String> {
    if query.trim().is_empty() {
        return list_kimi_transcript_sessions();
    }
    let q = query.to_lowercase();

    let mut sessions: Vec<KimiTranscriptSession> = discover_sessions()
        .iter()
        .filter_map(|sf| {
            let data = load_session(sf);
            let matches = data
                .messages
                .iter()
                .any(|m| m.role != "system" && m.content.to_lowercase().contains(&q));
            if !matches {
                return None;
            }
            Some(to_session_summary(sf, &data))
        })
        .collect();
    sessions.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));
    Ok(sessions)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const CONTEXT_FIXTURE: &str = r#"{"role": "_system_prompt", "content": "You are Kimi."}
{"role": "_checkpoint", "id": 0}
{"role": "user", "content": "hi yo"}
{"role": "_usage", "token_count": 100}
{"role": "assistant", "content": [{"type": "think", "think": "greet back"}, {"type": "text", "text": "Yo! What's up?"}]}
{"role": "_usage", "token_count": 150}
{"role": "user", "content": "eit"}
{"role": "tool", "content": [{"type": "text", "text": "tool noise"}], "tool_call_id": "X"}
{"role": "assistant", "content": "Hey! Need anything?"}
"#;

    const WIRE_FIXTURE: &str = r#"{"type": "metadata", "protocol_version": "1.10"}
{"timestamp": 100.0, "message": {"type": "TurnBegin", "payload": {"user_input": [{"type": "text", "text": "hi yo"}]}}}
{"timestamp": 101.0, "message": {"type": "TurnEnd", "payload": {}}}
{"timestamp": 200.0, "message": {"type": "TurnBegin", "payload": {"user_input": [{"type": "text", "text": "eit"}]}}}
{"timestamp": 201.0, "message": {"type": "TurnEnd", "payload": {}}}
"#;

    #[test]
    fn parse_context_messages_orders_and_normalizes_roles() {
        let wire = parse_wire_activity(WIRE_FIXTURE);
        let messages = parse_context_messages(CONTEXT_FIXTURE, &wire);

        // system, user, assistant, user, assistant — tool/_usage/_checkpoint dropped.
        let roles: Vec<&str> = messages.iter().map(|m| m.role.as_str()).collect();
        assert_eq!(roles, ["system", "user", "assistant", "user", "assistant"]);

        assert_eq!(messages[0].content, "You are Kimi.");
        assert_eq!(messages[1].content, "hi yo");
        assert_eq!(messages[2].content, "Yo! What's up?");
        assert_eq!(messages[3].content, "eit");
        assert_eq!(messages[4].content, "Hey! Need anything?");
    }

    #[test]
    fn parse_context_messages_attaches_best_effort_timestamps() {
        let wire = parse_wire_activity(WIRE_FIXTURE);
        let messages = parse_context_messages(CONTEXT_FIXTURE, &wire);

        assert_eq!(messages[0].timestamp, None); // system prompt: no timestamp
        assert_eq!(
            messages[1].timestamp.as_deref(),
            Some("1970-01-01T00:01:40+00:00")
        ); // 1st user <- 1st TurnBegin (100.0)
        assert_eq!(
            messages[2].timestamp.as_deref(),
            Some("1970-01-01T00:01:41+00:00")
        ); // 1st assistant <- 1st TurnEnd (101.0)
        assert_eq!(
            messages[3].timestamp.as_deref(),
            Some("1970-01-01T00:03:20+00:00")
        ); // 2nd user <- 2nd TurnBegin (200.0)
        assert_eq!(
            messages[4].timestamp.as_deref(),
            Some("1970-01-01T00:03:21+00:00")
        ); // 2nd assistant <- 2nd TurnEnd (201.0)
    }

    #[test]
    fn parse_wire_activity_tracks_first_and_last_regardless_of_type() {
        let wire = parse_wire_activity(WIRE_FIXTURE);
        assert_eq!(wire.turn_begins, [100.0, 200.0]);
        assert_eq!(wire.turn_ends, [101.0, 201.0]);
        assert_eq!(wire.first, Some(100.0));
        assert_eq!(wire.last, Some(201.0));
    }

    #[test]
    fn session_summary_uses_custom_title_and_excludes_system_from_count() {
        let sf = SessionFile {
            project_path: "/Users/test/project".to_string(),
            session_id: "abc123".to_string(),
            dir: std::path::PathBuf::from("/nonexistent"),
        };
        let wire = parse_wire_activity(WIRE_FIXTURE);
        let messages = parse_context_messages(CONTEXT_FIXTURE, &wire);
        let data = KimiSessionData {
            messages,
            custom_title: Some("My session".to_string()),
            first_activity: wire.first.and_then(unix_to_rfc3339),
            last_activity: wire.last.and_then(unix_to_rfc3339),
        };

        let summary = to_session_summary(&sf, &data);
        assert_eq!(summary.session_id, "abc123");
        assert_eq!(summary.project_path, "/Users/test/project");
        assert_eq!(summary.project_name, "project");
        assert_eq!(summary.title, "My session");
        assert_eq!(summary.message_count, 4); // 2 user + 2 assistant; system excluded
        assert_eq!(
            summary.last_activity.as_deref(),
            Some("1970-01-01T00:03:21+00:00")
        );
    }

    #[test]
    fn session_title_falls_back_to_first_user_message_then_session_id() {
        let wire = WireActivity::default();
        let messages = parse_context_messages(CONTEXT_FIXTURE, &wire);
        let data = KimiSessionData {
            messages,
            custom_title: None,
            first_activity: None,
            last_activity: None,
        };
        assert_eq!(session_title(&data, "abc123"), "hi yo");

        let empty_data = KimiSessionData {
            messages: Vec::new(),
            custom_title: None,
            first_activity: None,
            last_activity: None,
        };
        assert_eq!(session_title(&empty_data, "abc123"), "abc123");
    }

    #[test]
    fn extract_text_skips_non_text_parts_and_handles_plain_strings() {
        let structured: serde_json::Value = serde_json::from_str(
            r#"[{"type": "think", "think": "hmm"}, {"type": "text", "text": "hello"}]"#,
        )
        .unwrap();
        assert_eq!(extract_text(&structured), "hello");

        let plain = serde_json::Value::String("plain text".to_string());
        assert_eq!(extract_text(&plain), "plain text");
    }
}
