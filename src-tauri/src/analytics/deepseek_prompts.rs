//! DeepSeek Prompt History — its own sidebar section (Kimi-parity layout).
//! DeepSeek Harness (`dsh`) session logs are zstd-compressed JSONL under
//! `sessions/<workspace-slug>/session-<uuid>/session.jsonl.zstd`. Each
//! `user/message` event's `data.content[]` text parts are concatenated into
//! the prompt content; the workspace is resolved via `deepseek_v2`'s
//! session_projcache/workspace.json metadata (falling back to the first
//! `session` event's `cwd` for sessions dsh hasn't indexed yet).
//!
//! NOTE: dsh has no resume-by-id CLI, so — unlike Kimi — there is no
//! `start_session`/`build_resume_command` command pair here.

use crate::analytics::deepseek_v2::{
    decode_session_events, discover_dsh_sessions, dsh_root, load_session_metadata, resolve_session_workspace,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekPromptEntry {
    pub display: String,
    pub timestamp: String,
    pub timestamp_ms: u64,
    pub project: Option<String>,
    pub project_name: Option<String>,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekPromptHistoryPage {
    pub entries: Vec<DeepSeekPromptEntry>,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekPromptHistoryStats {
    pub total: u64,
}

/// Concatenate `type: "text"` parts of a `content` array (reasoning/tool
/// parts are skipped).
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

/// Parse a decoded session's `user/message` events into prompt entries.
fn parse_session_prompts(
    events: &[serde_json::Value],
    session_id: &str,
    project_path: Option<&str>,
    project_name: Option<&str>,
) -> Vec<DeepSeekPromptEntry> {
    let mut out = Vec::new();
    for event in events {
        if event.get("type").and_then(|t| t.as_str()) != Some("user/message") {
            continue;
        }
        let Some(time_ms) = event.get("time").and_then(|t| t.as_i64()) else { continue };
        let display = event
            .get("data")
            .and_then(|d| d.get("content"))
            .map(extract_text_parts)
            .unwrap_or_default();
        if display.trim().is_empty() {
            continue;
        }

        let timestamp = chrono::DateTime::from_timestamp_millis(time_ms).map(|dt| dt.to_rfc3339()).unwrap_or_default();

        out.push(DeepSeekPromptEntry {
            display,
            timestamp,
            timestamp_ms: time_ms.max(0) as u64,
            project: project_path.map(String::from),
            project_name: project_name.map(String::from),
            session_id: Some(session_id.to_string()),
        });
    }
    out
}

/// Decode every session log under `sessions/` and collect every prompt,
/// newest first. Corrupt sessions (bad zstd/JSON) are skipped, never panic.
fn read_all_deepseek_prompts() -> Vec<DeepSeekPromptEntry> {
    let Some(root) = dsh_root() else { return vec![] };
    let metadata = load_session_metadata(&root);
    let mut out = Vec::new();

    for sf in discover_dsh_sessions(&root) {
        let Some(events) = decode_session_events(&sf.log_path) else { continue };
        let (project_path, project_name) = resolve_session_workspace(&sf.session_id, &events, &metadata);
        out.extend(parse_session_prompts(
            &events,
            &sf.session_id,
            project_path.as_deref(),
            project_name.as_deref(),
        ));
    }

    out.sort_by(|a, b| b.timestamp_ms.cmp(&a.timestamp_ms));
    out
}

// ── Tauri Commands ───────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_deepseek_prompt_history(limit: usize, offset: usize) -> Result<DeepSeekPromptHistoryPage, String> {
    let all = read_all_deepseek_prompts();
    let total = all.len() as u64;
    let entries = all.into_iter().skip(offset).take(limit).collect();
    Ok(DeepSeekPromptHistoryPage { entries, total })
}

#[tauri::command]
pub fn search_deepseek_prompt_history(query: String) -> Result<Vec<DeepSeekPromptEntry>, String> {
    let all = read_all_deepseek_prompts();
    let q = query.to_lowercase();
    let results: Vec<DeepSeekPromptEntry> =
        all.into_iter().filter(|e| e.display.to_lowercase().contains(&q)).take(200).collect();
    Ok(results)
}

#[tauri::command]
pub fn get_deepseek_prompt_stats() -> Result<DeepSeekPromptHistoryStats, String> {
    let all = read_all_deepseek_prompts();
    Ok(DeepSeekPromptHistoryStats { total: all.len() as u64 })
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
{"type": "user/message", "time": 1500, "data": {"content": [{"type": "text", "text": "first prompt"}], "source": {"kind": "user"}, "role": "user", "id": "u1"}}
{"type": "assistant/message", "time": 1600, "data": {"turn": 1, "step": 1, "message": {"role": "assistant", "content": [{"type": "reasoning", "text": "thinking"}, {"type": "text", "text": "reply"}]}}}
{"type": "user/message", "time": 2000, "data": {"content": [{"type": "text", "text": "second"}, {"type": "text", "text": "prompt"}], "role": "user"}}
{"type": "user/message", "time": 2500, "data": {"content": [{"type": "image", "data": "..."}], "role": "user"}}
"#;

    #[test]
    fn parse_session_prompts_extracts_user_message_text_and_skips_non_text() {
        let events: Vec<serde_json::Value> =
            SESSION_FIXTURE.lines().filter(|l| !l.trim().is_empty()).map(|l| serde_json::from_str(l).unwrap()).collect();
        let entries = parse_session_prompts(&events, "session-abc", Some("/tmp/proj"), Some("proj"));

        // The third user/message has no text parts, so it's skipped.
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].display, "first prompt");
        assert_eq!(entries[0].timestamp_ms, 1500);
        assert_eq!(entries[0].session_id.as_deref(), Some("session-abc"));
        assert_eq!(entries[0].project.as_deref(), Some("/tmp/proj"));
        assert_eq!(entries[0].project_name.as_deref(), Some("proj"));

        assert_eq!(entries[1].display, "second\nprompt");
        assert_eq!(entries[1].timestamp_ms, 2000);
    }

    #[test]
    fn entries_sort_newest_first_across_sessions() {
        let mut entries = [
            DeepSeekPromptEntry {
                display: "a".into(),
                timestamp: String::new(),
                timestamp_ms: 100,
                project: None,
                project_name: None,
                session_id: None,
            },
            DeepSeekPromptEntry {
                display: "b".into(),
                timestamp: String::new(),
                timestamp_ms: 300,
                project: None,
                project_name: None,
                session_id: None,
            },
            DeepSeekPromptEntry {
                display: "c".into(),
                timestamp: String::new(),
                timestamp_ms: 200,
                project: None,
                project_name: None,
                session_id: None,
            },
        ];
        entries.sort_by(|a, b| b.timestamp_ms.cmp(&a.timestamp_ms));
        assert_eq!(entries.iter().map(|e| e.display.as_str()).collect::<Vec<_>>(), ["b", "c", "a"]);
    }

    #[test]
    fn decode_session_events_round_trips_zstd_jsonl() {
        let tmp = tempfile::tempdir().unwrap();
        let log_path = write_session_log(&tmp.path().join("ws-slug").join("session-abc"), SESSION_FIXTURE);

        let events = decode_session_events(&log_path).expect("decode fixture");
        assert_eq!(events.len(), 5);
        assert_eq!(events[0].get("type").and_then(|t| t.as_str()), Some("session"));
    }

    #[test]
    fn decode_session_events_returns_none_for_corrupt_file_without_panicking() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("ws-slug").join("session-bad");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.jsonl.zstd");
        std::fs::write(&path, b"not zstd data at all").unwrap();

        assert!(decode_session_events(&path).is_none());
    }

    #[test]
    fn discover_and_decode_skips_corrupt_session_and_reads_valid_ones() {
        let tmp = tempfile::tempdir().unwrap();
        let sessions_root = tmp.path().join("sessions");

        write_session_log(&sessions_root.join("proj-slug").join("session-good"), SESSION_FIXTURE);

        let bad_dir = sessions_root.join("proj-slug").join("session-bad");
        std::fs::create_dir_all(&bad_dir).unwrap();
        std::fs::write(bad_dir.join("session.jsonl.zstd"), b"garbage").unwrap();

        let metadata = std::collections::HashMap::new();
        let mut all = Vec::new();
        for sf in discover_dsh_sessions(tmp.path()) {
            let Some(events) = decode_session_events(&sf.log_path) else { continue };
            let (project_path, project_name) = resolve_session_workspace(&sf.session_id, &events, &metadata);
            all.extend(parse_session_prompts(&events, &sf.session_id, project_path.as_deref(), project_name.as_deref()));
        }
        all.sort_by(|a, b| b.timestamp_ms.cmp(&a.timestamp_ms));

        // The corrupt session is skipped entirely (no panic); the good one decodes fine.
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].display, "second\nprompt");
        assert_eq!(all[1].display, "first prompt");
        // No session_projcache.json entry — falls back to the `session` event's cwd.
        assert_eq!(all[0].project.as_deref(), Some("/tmp/proj"));
        assert_eq!(all[0].project_name.as_deref(), Some("proj"));
    }
}
