//! Kimi Prompt History — its own sidebar section (Claude-parity layout).
//! Unlike `~/.kimi/user-history/*.jsonl` (content only, no timestamp/session),
//! prompts here are sourced from the session wire logs:
//!   sessions/<md5(project path)>/<sessionId>/wire.jsonl
//! Each `TurnBegin` message's `user_input[]` text parts are concatenated into
//! the prompt content; the `<md5>` directory is resolved to a project path via
//! `~/.kimi/kimi.json` `work_dirs[]` (reusing `kimi_v2`'s md5→path map).

use crate::analytics::kimi_v2::{build_dir_map, kimi_root};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KimiPromptEntry {
    pub display: String,
    pub timestamp: String,
    pub timestamp_ms: u64,
    pub project: Option<String>,
    pub project_name: Option<String>,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KimiPromptHistoryPage {
    pub entries: Vec<KimiPromptEntry>,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KimiPromptHistoryStats {
    pub total: u64,
}

/// Parse one `wire.jsonl`'s `TurnBegin` events into prompt entries.
fn parse_wire_prompts(
    content: &str,
    session_id: &str,
    project_path: Option<&str>,
    project_name: Option<&str>,
) -> Vec<KimiPromptEntry> {
    let mut out = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(obj) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        let Some(msg) = obj.get("message") else { continue };
        if msg.get("type").and_then(|t| t.as_str()) != Some("TurnBegin") {
            continue;
        }
        let Some(ts) = obj.get("timestamp").and_then(|t| t.as_f64()) else { continue };

        let display = msg
            .get("payload")
            .and_then(|p| p.get("user_input"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter(|item| item.get("type").and_then(|t| t.as_str()) == Some("text"))
                    .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();

        if display.trim().is_empty() {
            continue;
        }

        let ts_ms = (ts * 1000.0).round() as i64;
        let timestamp = chrono::DateTime::from_timestamp_millis(ts_ms)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default();

        out.push(KimiPromptEntry {
            display,
            timestamp,
            timestamp_ms: ts_ms.max(0) as u64,
            project: project_path.map(String::from),
            project_name: project_name.map(String::from),
            session_id: Some(session_id.to_string()),
        });
    }
    out
}

/// Walk `~/.kimi/sessions/<md5>/<sessionId>/wire.jsonl` and collect every
/// prompt, newest first.
fn read_all_kimi_prompts() -> Vec<KimiPromptEntry> {
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
        let project_name = std::path::Path::new(&project_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| project_path.clone());

        let Ok(session_dirs) = std::fs::read_dir(md5_entry.path()) else { continue };
        for sess_entry in session_dirs.flatten() {
            let sess_path = sess_entry.path();
            if !sess_path.is_dir() {
                continue;
            }
            let session_id = sess_entry.file_name().to_string_lossy().to_string();
            let Ok(text) = std::fs::read_to_string(sess_path.join("wire.jsonl")) else { continue };
            out.extend(parse_wire_prompts(
                &text,
                &session_id,
                Some(project_path.as_str()),
                Some(project_name.as_str()),
            ));
        }
    }

    out.sort_by(|a, b| b.timestamp_ms.cmp(&a.timestamp_ms));
    out
}

/// Build the `kimi --resume <session-id>` command for a session.
fn kimi_resume_command(session_id: &str, project: Option<&str>) -> String {
    let resume = format!("kimi --resume {}", session_id);
    match project {
        Some(p) if !p.is_empty() => format!("cd {} && {}", shell_quote(p), resume),
        _ => resume,
    }
}

/// Single-quote a path for POSIX shells, escaping embedded single quotes.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

// ── Tauri Commands ───────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_kimi_prompt_history(limit: usize, offset: usize) -> Result<KimiPromptHistoryPage, String> {
    let all = read_all_kimi_prompts();
    let total = all.len() as u64;
    let entries = all.into_iter().skip(offset).take(limit).collect();
    Ok(KimiPromptHistoryPage { entries, total })
}

#[tauri::command]
pub fn search_kimi_prompt_history(query: String) -> Result<Vec<KimiPromptEntry>, String> {
    let all = read_all_kimi_prompts();
    let q = query.to_lowercase();
    let results: Vec<KimiPromptEntry> = all
        .into_iter()
        .filter(|e| e.display.to_lowercase().contains(&q))
        .take(200)
        .collect();
    Ok(results)
}

#[tauri::command]
pub fn get_kimi_prompt_stats() -> Result<KimiPromptHistoryStats, String> {
    let all = read_all_kimi_prompts();
    Ok(KimiPromptHistoryStats { total: all.len() as u64 })
}

#[tauri::command]
pub fn build_kimi_resume_command(session_id: String, project: Option<String>) -> String {
    kimi_resume_command(&session_id, project.as_deref())
}

#[tauri::command]
pub fn start_kimi_session(session_id: String, project: Option<String>) -> Result<(), String> {
    let command = kimi_resume_command(&session_id, project.as_deref());
    crate::utils::platform::launch_in_terminal(&command)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_wire_prompts_extracts_turn_begin_text_and_skips_non_text() {
        let wire = r#"{"type": "metadata", "protocol_version": "1.10"}
{"timestamp": 100.0, "message": {"type": "TurnBegin", "payload": {"user_input": [{"type": "text", "text": "first prompt"}]}}}
{"timestamp": 101.0, "message": {"type": "StepBegin", "payload": {}}}
{"timestamp": 200.5, "message": {"type": "TurnBegin", "payload": {"user_input": [{"type": "text", "text": "second"}, {"type": "text", "text": "prompt"}]}}}
{"timestamp": 300.0, "message": {"type": "TurnBegin", "payload": {"user_input": [{"type": "image", "data": "..."}]}}}
"#;
        let entries = parse_wire_prompts(wire, "sess-1", Some("/tmp/proj"), Some("proj"));
        // The third TurnBegin has no text parts, so it's skipped.
        assert_eq!(entries.len(), 2);

        assert_eq!(entries[0].display, "first prompt");
        assert_eq!(entries[0].timestamp_ms, 100_000);
        assert_eq!(entries[0].session_id.as_deref(), Some("sess-1"));
        assert_eq!(entries[0].project.as_deref(), Some("/tmp/proj"));
        assert_eq!(entries[0].project_name.as_deref(), Some("proj"));

        assert_eq!(entries[1].display, "second\nprompt");
        assert_eq!(entries[1].timestamp_ms, 200_500);
    }

    #[test]
    fn parse_wire_prompts_skips_lines_without_timestamp_or_empty_text() {
        let wire = r#"{"message": {"type": "TurnBegin", "payload": {"user_input": [{"type": "text", "text": "no timestamp"}]}}}
{"timestamp": 50.0, "message": {"type": "TurnBegin", "payload": {"user_input": [{"type": "text", "text": "   "}]}}}
"#;
        let entries = parse_wire_prompts(wire, "sess-2", None, None);
        assert!(entries.is_empty());
    }

    #[test]
    fn entries_sort_newest_first_across_sessions() {
        let mut entries = [
            KimiPromptEntry {
                display: "a".into(),
                timestamp: String::new(),
                timestamp_ms: 100,
                project: None,
                project_name: None,
                session_id: None,
            },
            KimiPromptEntry {
                display: "b".into(),
                timestamp: String::new(),
                timestamp_ms: 300,
                project: None,
                project_name: None,
                session_id: None,
            },
            KimiPromptEntry {
                display: "c".into(),
                timestamp: String::new(),
                timestamp_ms: 200,
                project: None,
                project_name: None,
                session_id: None,
            },
        ];
        entries.sort_by(|a, b| b.timestamp_ms.cmp(&a.timestamp_ms));
        assert_eq!(
            entries.iter().map(|e| e.display.as_str()).collect::<Vec<_>>(),
            vec!["b", "c", "a"]
        );
    }

    #[test]
    fn kimi_resume_command_quotes_project_path() {
        assert_eq!(
            kimi_resume_command("sess-1", Some("/Users/a b/proj")),
            "cd '/Users/a b/proj' && kimi --resume sess-1"
        );
        assert_eq!(kimi_resume_command("sess-1", None), "kimi --resume sess-1");
    }
}
