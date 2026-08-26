//! DeepSeek Transcripts — read-only session browser/reader for DeepSeek
//! Harness (`dsh`) session logs under
//! `sessions/<workspace-slug>/session-<uuid>/session.jsonl.zstd` (zstd-
//! compressed JSONL). Mirrors `kimi_transcripts.rs` in shape, but is
//! view-only for the same reason: dsh's session log is a live wire format,
//! not a flat, hand-editable transcript.
//!
//! Reuses `analytics::deepseek_v2`'s zstd decode/discovery helpers and its
//! session_projcache/workspace.json metadata resolution.

use crate::analytics::deepseek_v2::{
    decode_session_events, discover_dsh_sessions, dsh_root, load_session_metadata, resolve_session_workspace,
    DshSessionFile, DshSessionMeta,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekTranscriptSession {
    pub session_id: String,
    pub workspace_path: String,
    pub workspace_name: String,
    pub title: String,
    pub message_count: u64,
    pub first_activity: Option<String>,
    pub last_activity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekTranscriptMessage {
    pub role: String,
    pub content: String,
    pub timestamp: Option<String>,
}

/// Concatenate `type: "text"` parts of a `content` array (reasoning/tool
/// parts are skipped — the reader stays conversation-focused).
fn extract_text_parts(content: &serde_json::Value) -> String {
    content
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter(|item| item.get("type").and_then(|t| t.as_str()) == Some("text"))
                .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn ms_to_rfc3339(ms: i64) -> Option<String> {
    chrono::DateTime::from_timestamp_millis(ms).map(|dt| dt.to_rfc3339())
}

/// Parse a decoded session's coalesced `user/message` / `assistant/message`
/// events into ordered display messages. `*-chunks` streaming deltas and
/// `tool/call` / `tool/result` events are ignored.
fn parse_session_messages(events: &[serde_json::Value]) -> Vec<DeepSeekTranscriptMessage> {
    let mut out = Vec::new();
    for event in events {
        let time_ms = event.get("time").and_then(|t| t.as_i64());
        match event.get("type").and_then(|t| t.as_str()) {
            Some("user/message") => {
                let text = event.get("data").and_then(|d| d.get("content")).map(extract_text_parts).unwrap_or_default();
                if !text.is_empty() {
                    out.push(DeepSeekTranscriptMessage {
                        role: "user".to_string(),
                        content: text,
                        timestamp: time_ms.and_then(ms_to_rfc3339),
                    });
                }
            }
            Some("assistant/message") => {
                let text = event
                    .get("data")
                    .and_then(|d| d.get("message"))
                    .and_then(|m| m.get("content"))
                    .map(extract_text_parts)
                    .unwrap_or_default();
                if !text.is_empty() {
                    out.push(DeepSeekTranscriptMessage {
                        role: "assistant".to_string(),
                        content: text,
                        timestamp: time_ms.and_then(ms_to_rfc3339),
                    });
                }
            }
            _ => {}
        }
    }
    out
}

/// First/last `time` seen across all events, independent of message role —
/// used for session first/last-activity.
fn session_activity(events: &[serde_json::Value]) -> (Option<i64>, Option<i64>) {
    let mut first = None;
    let mut last = None;
    for event in events {
        if let Some(t) = event.get("time").and_then(|v| v.as_i64()) {
            first = Some(first.map_or(t, |cur: i64| cur.min(t)));
            last = Some(last.map_or(t, |cur: i64| cur.max(t)));
        }
    }
    (first, last)
}

/// The most recent `session/title` event's title, if the session was renamed.
fn latest_title_event(events: &[serde_json::Value]) -> Option<String> {
    events.iter().rev().find_map(|e| {
        if e.get("type").and_then(|t| t.as_str()) == Some("session/title") {
            e.get("data").and_then(|d| d.get("title")).and_then(|t| t.as_str()).map(String::from)
        } else {
            None
        }
    })
}

/// A `session/title` event (an explicit rename) wins over the cached
/// projcache title, which in turn wins over the bare session id.
fn session_title(meta: Option<&DshSessionMeta>, events: &[serde_json::Value], session_id: &str) -> String {
    if let Some(title) = latest_title_event(events) {
        return title;
    }
    if let Some(t) = meta.and_then(|m| m.title.as_ref()) {
        let trimmed = t.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    session_id.to_string()
}

fn to_session_summary(
    sf: &DshSessionFile,
    events: &[serde_json::Value],
    metadata: &HashMap<String, DshSessionMeta>,
) -> DeepSeekTranscriptSession {
    let (workspace_path, workspace_name) = resolve_session_workspace(&sf.session_id, events, metadata);
    let (first_ms, last_ms) = session_activity(events);

    DeepSeekTranscriptSession {
        session_id: sf.session_id.clone(),
        workspace_path: workspace_path.unwrap_or_else(|| "unknown".to_string()),
        workspace_name: workspace_name.unwrap_or_else(|| "unknown".to_string()),
        title: session_title(metadata.get(&sf.session_id), events, &sf.session_id),
        message_count: parse_session_messages(events).len() as u64,
        first_activity: first_ms.and_then(ms_to_rfc3339),
        last_activity: last_ms.and_then(ms_to_rfc3339),
    }
}

/// Find a session's log path, preferring one matching the `workspace_path`
/// hint (when given) before falling back to a plain session-id scan.
fn resolve_session_log(
    root: &std::path::Path,
    session_id: &str,
    workspace_path: Option<&str>,
    metadata: &HashMap<String, DshSessionMeta>,
) -> Result<std::path::PathBuf, String> {
    let sessions = discover_dsh_sessions(root);
    if let Some(hint) = workspace_path {
        if let Some(sf) = sessions.iter().find(|sf| {
            sf.session_id == session_id && metadata.get(&sf.session_id).map(|m| m.workspace_path.as_str()) == Some(hint)
        }) {
            return Ok(sf.log_path.clone());
        }
    }
    sessions.into_iter().find(|sf| sf.session_id == session_id).map(|sf| sf.log_path).ok_or_else(|| "Session not found".to_string())
}

// ── Tauri Commands ───────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_deepseek_transcript_sessions() -> Result<Vec<DeepSeekTranscriptSession>, String> {
    let Some(root) = dsh_root() else { return Ok(vec![]) };
    let metadata = load_session_metadata(&root);
    let mut sessions: Vec<DeepSeekTranscriptSession> = discover_dsh_sessions(&root)
        .iter()
        .filter_map(|sf| {
            let events = decode_session_events(&sf.log_path)?;
            Some(to_session_summary(sf, &events, &metadata))
        })
        .collect();
    sessions.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));
    Ok(sessions)
}

#[tauri::command]
pub fn read_deepseek_transcript(
    session_id: String,
    workspace_path: Option<String>,
) -> Result<Vec<DeepSeekTranscriptMessage>, String> {
    let root = dsh_root().ok_or_else(|| "DeepSeek Harness home directory not found".to_string())?;
    let metadata = load_session_metadata(&root);
    let log_path = resolve_session_log(&root, &session_id, workspace_path.as_deref(), &metadata)?;
    let events = decode_session_events(&log_path).ok_or_else(|| "Failed to read transcript".to_string())?;
    Ok(parse_session_messages(&events))
}

#[tauri::command]
pub fn search_deepseek_transcripts(query: String) -> Result<Vec<DeepSeekTranscriptSession>, String> {
    if query.trim().is_empty() {
        return list_deepseek_transcript_sessions();
    }
    let Some(root) = dsh_root() else { return Ok(vec![]) };
    let metadata = load_session_metadata(&root);
    let q = query.to_lowercase();

    let mut sessions: Vec<DeepSeekTranscriptSession> = discover_dsh_sessions(&root)
        .iter()
        .filter_map(|sf| {
            let events = decode_session_events(&sf.log_path)?;
            let messages = parse_session_messages(&events);
            let matches = messages.iter().any(|m| m.content.to_lowercase().contains(&q));
            if !matches {
                return None;
            }
            Some(to_session_summary(sf, &events, &metadata))
        })
        .collect();
    sessions.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));
    Ok(sessions)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn compress(text: &str) -> Vec<u8> {
        zstd::encode_all(text.as_bytes(), 0).expect("compress fixture")
    }

    fn write_session_log(dir: &std::path::Path, content: &str) -> std::path::PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join("session.jsonl.zstd");
        std::fs::write(&path, compress(content)).unwrap();
        path
    }

    const SESSION_FIXTURE: &str = r#"{"type": "session", "time": 1000, "data": {"id": "session-abc", "createdAt": 1000, "cwd": "/tmp/proj", "agentPreset": "default"}}
{"type": "user/message", "time": 1500, "data": {"content": [{"type": "text", "text": "hi there"}], "role": "user"}}
{"type": "assistant/chunk", "time": 1550, "data": {"delta": "h"}}
{"type": "assistant/message", "time": 1600, "data": {"turn": 1, "step": 1, "message": {"role": "assistant", "content": [{"type": "reasoning", "text": "thinking"}, {"type": "text", "text": "hello!"}]}}}
{"type": "tool/call", "time": 1650, "data": {"name": "ls"}}
{"type": "tool/result", "time": 1660, "data": {"ok": true}}
{"type": "user/message", "time": 2000, "data": {"content": [{"type": "text", "text": "second question"}], "role": "user"}}
{"type": "assistant/message", "time": 2100, "data": {"turn": 2, "step": 1, "message": {"role": "assistant", "content": [{"type": "text", "text": "second answer"}]}}}
"#;

    fn fixture_events() -> Vec<serde_json::Value> {
        SESSION_FIXTURE.lines().filter(|l| !l.trim().is_empty()).map(|l| serde_json::from_str(l).unwrap()).collect()
    }

    #[test]
    fn parse_session_messages_orders_and_normalizes_roles_ignoring_chunks_and_tools() {
        let events = fixture_events();
        let messages = parse_session_messages(&events);

        let roles: Vec<&str> = messages.iter().map(|m| m.role.as_str()).collect();
        assert_eq!(roles, ["user", "assistant", "user", "assistant"]);

        assert_eq!(messages[0].content, "hi there");
        assert_eq!(messages[1].content, "hello!");
        assert_eq!(messages[2].content, "second question");
        assert_eq!(messages[3].content, "second answer");
    }

    #[test]
    fn parse_session_messages_attaches_event_timestamps() {
        let events = fixture_events();
        let messages = parse_session_messages(&events);

        assert_eq!(messages[0].timestamp.as_deref(), Some("1970-01-01T00:00:01.500+00:00"));
        assert_eq!(messages[1].timestamp.as_deref(), Some("1970-01-01T00:00:01.600+00:00"));
    }

    #[test]
    fn session_activity_tracks_first_and_last_regardless_of_type() {
        let events = fixture_events();
        let (first, last) = session_activity(&events);
        assert_eq!(first, Some(1000));
        assert_eq!(last, Some(2100));
    }

    #[test]
    fn session_title_falls_back_from_rename_event_to_projcache_to_session_id() {
        let events = fixture_events();

        // No rename event, no projcache metadata: falls back to the session id.
        assert_eq!(session_title(None, &events, "session-abc"), "session-abc");

        // projcache metadata present: used when there's no rename event.
        let meta = DshSessionMeta {
            workspace_path: "/tmp/proj".to_string(),
            workspace_name: "proj".to_string(),
            title: Some("Cached title".to_string()),
        };
        assert_eq!(session_title(Some(&meta), &events, "session-abc"), "Cached title");

        // An explicit rename event wins over the cached title.
        let mut renamed = events.clone();
        renamed.push(serde_json::json!({"type": "session/title", "time": 3000, "data": {"title": "Renamed"}}));
        assert_eq!(session_title(Some(&meta), &renamed, "session-abc"), "Renamed");
    }

    #[test]
    fn to_session_summary_resolves_workspace_via_session_event_fallback() {
        let tmp = tempfile::tempdir().unwrap();
        let log_path = write_session_log(&tmp.path().join("proj-slug").join("session-abc"), SESSION_FIXTURE);
        let sf = DshSessionFile { session_id: "session-abc".to_string(), log_path };
        let events = fixture_events();
        let metadata = HashMap::new();

        let summary = to_session_summary(&sf, &events, &metadata);
        assert_eq!(summary.session_id, "session-abc");
        assert_eq!(summary.workspace_path, "/tmp/proj");
        assert_eq!(summary.workspace_name, "proj");
        assert_eq!(summary.title, "session-abc");
        assert_eq!(summary.message_count, 4);
        assert_eq!(summary.first_activity.as_deref(), Some("1970-01-01T00:00:01+00:00"));
    }

    #[test]
    fn newest_first_sort_orders_sessions_by_last_activity() {
        let mut sessions = [
            DeepSeekTranscriptSession {
                session_id: "old".to_string(),
                workspace_path: "unknown".to_string(),
                workspace_name: "unknown".to_string(),
                title: "old".to_string(),
                message_count: 1,
                first_activity: Some("2026-01-01T00:00:00+00:00".to_string()),
                last_activity: Some("2026-01-01T00:00:00+00:00".to_string()),
            },
            DeepSeekTranscriptSession {
                session_id: "new".to_string(),
                workspace_path: "unknown".to_string(),
                workspace_name: "unknown".to_string(),
                title: "new".to_string(),
                message_count: 1,
                first_activity: Some("2026-02-01T00:00:00+00:00".to_string()),
                last_activity: Some("2026-02-01T00:00:00+00:00".to_string()),
            },
        ];
        sessions.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));
        assert_eq!(sessions[0].session_id, "new");
        assert_eq!(sessions[1].session_id, "old");
    }

    #[test]
    fn decode_session_events_returns_none_for_corrupt_file_without_panicking() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("proj-slug").join("session-bad");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.jsonl.zstd");
        std::fs::write(&path, b"not zstd data at all").unwrap();

        assert!(decode_session_events(&path).is_none());
    }

    #[test]
    fn list_sessions_skips_corrupt_session_and_reads_valid_one() {
        let tmp = tempfile::tempdir().unwrap();
        let sessions_root = tmp.path().join("sessions");
        write_session_log(&sessions_root.join("proj-slug").join("session-good"), SESSION_FIXTURE);

        let bad_dir = sessions_root.join("proj-slug").join("session-bad");
        std::fs::create_dir_all(&bad_dir).unwrap();
        std::fs::write(bad_dir.join("session.jsonl.zstd"), b"garbage").unwrap();

        let metadata: HashMap<String, DshSessionMeta> = HashMap::new();
        let sessions: Vec<DeepSeekTranscriptSession> = discover_dsh_sessions(tmp.path())
            .iter()
            .filter_map(|sf| {
                let events = decode_session_events(&sf.log_path)?;
                Some(to_session_summary(sf, &events, &metadata))
            })
            .collect();

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "session-good");
    }
}
