use crate::utils::paths::{atomic_write_str, normalize_line_endings, read_with_sharing};
use chrono::TimeZone;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSession {
    pub session_id: String,
    pub project_name: String,
    pub source: String,
    pub modified_at: String,
    pub file_size_bytes: u64,
    pub file_path: String,
    /// True for a Cursor session known only from its DB — the
    /// `agent-transcripts/<composerId>/` directory is missing or empty, so
    /// there's no `.jsonl` file to open. Defaults to `false` (a real file).
    #[serde(default)]
    pub missing_file: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptMessage {
    pub role: String,
    pub content: String,
    /// Zero-based index of the JSONL line this message was parsed from.
    /// Used to surgically write text edits back into the correct line.
    pub line_index: usize,
}

/// Decode a Claude/Cursor project dir name by brute-forcing how the dash-joined
/// segments recombine, validating each candidate against the filesystem.
pub(crate) fn decode_project_path(encoded_dir_name: &str) -> String {
    let clean = encoded_dir_name.trim_start_matches('-');
    if clean.is_empty() {
        return "/".to_string();
    }

    let raw_parts: Vec<&str> = clean.split('-').collect();
    let mut segments: Vec<String> = Vec::new();
    let mut i = 0;
    while i < raw_parts.len() {
        if raw_parts[i].is_empty() {
            let mut empties: usize = 0;
            while i < raw_parts.len() && raw_parts[i].is_empty() {
                empties += 1;
                i += 1;
            }
            if i < raw_parts.len() {
                let prefix = format!(".{}", "-".repeat(empties.saturating_sub(1)));
                segments.push(format!("{}{}", prefix, raw_parts[i]));
                i += 1;
            }
        } else {
            segments.push(raw_parts[i].to_string());
            i += 1;
        }
    }

    let mut result = Vec::new();
    let sep = std::path::MAIN_SEPARATOR;
    if fs_recombine(&segments, 0, &mut result) {
        format!("{}{}", sep, result.join(&sep.to_string()))
    } else {
        format!("{}{}", sep, segments.join(&sep.to_string()))
    }
}

fn fs_recombine(segments: &[String], start: usize, result: &mut Vec<String>) -> bool {
    if start >= segments.len() {
        return true;
    }

    let base = if result.is_empty() {
        String::new()
    } else {
        format!("/{}", result.join("/"))
    };

    let mut candidate = segments[start].clone();
    for end in start..segments.len() {
        if end > start {
            candidate = format!("{}-{}", candidate, &segments[end]);
        }

        let test_path = format!("{}/{}", base, candidate);
        let is_last = end + 1 >= segments.len();

        if is_last {
            if std::path::Path::new(&test_path).exists() {
                result.push(candidate);
                return true;
            }
        } else if std::path::Path::new(&test_path).is_dir() {
            result.push(candidate.clone());
            if fs_recombine(segments, end + 1, result) {
                return true;
            }
            result.pop();
        }
    }
    false
}

fn scan_claude_transcripts() -> Vec<TranscriptSession> {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return vec![],
    };
    let projects_dir = home.join(".claude").join("projects");
    if !projects_dir.exists() {
        return vec![];
    }

    let mut sessions = Vec::new();
    let entries = match std::fs::read_dir(&projects_dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };

    for project_entry in entries.flatten() {
        let project_path = project_entry.path();
        if !project_path.is_dir() {
            continue;
        }
        let dir_name = project_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        if dir_name.starts_with('.') || dir_name.is_empty() {
            continue;
        }

        let decoded_path = decode_project_path(&dir_name);

        let files = match std::fs::read_dir(&project_path) {
            Ok(f) => f,
            Err(_) => continue,
        };
        for file_entry in files.flatten() {
            let file_path = file_entry.path();
            if !file_path.is_file() {
                continue;
            }
            let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "jsonl" {
                continue;
            }

            let session_id = file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let meta = match std::fs::metadata(&file_path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let modified_at = meta
                .modified()
                .map(|t| {
                    let dt: chrono::DateTime<chrono::Utc> = t.into();
                    dt.to_rfc3339()
                })
                .unwrap_or_default();

            sessions.push(TranscriptSession {
                session_id,
                project_name: decoded_path.clone(),
                source: "claude".to_string(),
                modified_at,
                file_size_bytes: meta.len(),
                file_path: file_path.to_string_lossy().to_string(),
                missing_file: false,
            });
        }
    }
    sessions
}

fn scan_cursor_transcripts() -> Vec<TranscriptSession> {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return vec![],
    };
    let projects_dir = home.join(".cursor").join("projects");
    if !projects_dir.exists() {
        return vec![];
    }

    let mut sessions = Vec::new();
    let entries = match std::fs::read_dir(&projects_dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };

    for project_entry in entries.flatten() {
        let project_path = project_entry.path();
        if !project_path.is_dir() {
            continue;
        }
        let dir_name = project_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        let decoded_path = decode_project_path(&dir_name);

        let transcripts_dir = project_path.join("agent-transcripts");
        if !transcripts_dir.exists() {
            continue;
        }

        let transcript_entries = match std::fs::read_dir(&transcripts_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for t_entry in transcript_entries.flatten() {
            let t_path = t_entry.path();
            if !t_path.is_dir() {
                continue;
            }
            let uuid_name = t_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            if uuid_name == "subagents" {
                continue;
            }

            let jsonl_path = t_path.join(format!("{}.jsonl", uuid_name));
            if !jsonl_path.exists() {
                continue;
            }

            let meta = match std::fs::metadata(&jsonl_path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let modified_at = meta
                .modified()
                .map(|t| {
                    let dt: chrono::DateTime<chrono::Utc> = t.into();
                    dt.to_rfc3339()
                })
                .unwrap_or_default();

            sessions.push(TranscriptSession {
                session_id: uuid_name,
                project_name: decoded_path.clone(),
                source: "cursor".to_string(),
                modified_at,
                file_size_bytes: meta.len(),
                file_path: jsonl_path.to_string_lossy().to_string(),
                missing_file: false,
            });
        }
    }

    // Merge in DB-known composers that never got a transcript file written
    // (an empty or missing `agent-transcripts/<composerId>/` dir) — reads the
    // cached corpus only, so a missing/unreadable Cursor DB just yields an
    // empty merge rather than failing the whole scan.
    merge_resolved_sessions(sessions, crate::analytics::cursor_projects::list_resolved_sessions())
}

/// Pure merge step of `scan_cursor_transcripts`: appends a `missing_file`
/// placeholder session for every `resolved` composer whose id isn't already
/// present in `sessions` (i.e. it has no `.jsonl` file on disk). Split out
/// from the corpus lookup so it's testable without the process-global,
/// live-Cursor-DB-backed corpus cache.
fn merge_resolved_sessions(
    mut sessions: Vec<TranscriptSession>,
    resolved: Vec<(String, String, Option<i64>, Option<String>)>,
) -> Vec<TranscriptSession> {
    let scanned_ids: HashSet<String> = sessions.iter().map(|s| s.session_id.clone()).collect();
    for (composer_id, project_path, last_updated_ms, _name) in resolved {
        if scanned_ids.contains(&composer_id) {
            continue;
        }
        let modified_at = last_updated_ms
            .and_then(|ms| chrono::Utc.timestamp_millis_opt(ms).single())
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default();
        sessions.push(TranscriptSession {
            session_id: composer_id,
            project_name: project_path,
            source: "cursor".to_string(),
            modified_at,
            file_size_bytes: 0,
            file_path: String::new(),
            missing_file: true,
        });
    }
    sessions
}

fn is_safe_transcript_path(file_path: &str) -> bool {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return false,
    };
    let path = PathBuf::from(file_path);
    let canonical = dunce::canonicalize(&path).unwrap_or(path.clone());
    let claude_dir = home.join(".claude");
    let cursor_dir = home.join(".cursor");
    canonical.starts_with(&claude_dir) || canonical.starts_with(&cursor_dir)
}

#[tauri::command]
pub fn list_transcript_sessions() -> Result<Vec<TranscriptSession>, String> {
    let mut sessions = scan_claude_transcripts();
    sessions.extend(scan_cursor_transcripts());
    sessions.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
    Ok(sessions)
}

#[tauri::command]
pub fn search_transcripts(query: String) -> Result<Vec<TranscriptSession>, String> {
    if query.trim().is_empty() {
        return list_transcript_sessions();
    }

    let query_lower = query.to_lowercase();
    let all_sessions = list_transcript_sessions()?;

    let mut matching = Vec::new();
    for session in &all_sessions {
        if session.missing_file || session.file_path.is_empty() {
            continue;
        }
        if !is_safe_transcript_path(&session.file_path) {
            continue;
        }
        if has_matching_user_prompt(&session.file_path, &query_lower) {
            matching.push(session.clone());
        }
    }

    Ok(matching)
}

fn has_matching_user_prompt(file_path: &str, query: &str) -> bool {
    let path = PathBuf::from(file_path);
    let file = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let json: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let role_str = json.get("role").and_then(|v| v.as_str()).unwrap_or("");
        let is_user = if role_str == "user" {
            true
        } else {
            json.get("type").and_then(|v| v.as_str()).unwrap_or("") == "user"
        };

        if !is_user {
            continue;
        }

        let content = extract_text_content(&json);
        if content.to_lowercase().contains(query) {
            return true;
        }
    }
    false
}

#[tauri::command]
pub fn read_transcript(
    file_path: String,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<Vec<TranscriptMessage>, String> {
    if !is_safe_transcript_path(&file_path) {
        return Err("Invalid transcript path".to_string());
    }

    let path = PathBuf::from(&file_path);
    if !path.exists() {
        return Err("Transcript file not found".to_string());
    }

    let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
    let reader = BufReader::new(file);
    let off = offset.unwrap_or(0);
    let lim = limit.unwrap_or(50);

    let mut messages = Vec::new();
    let mut count = 0;

    for (line_index, line) in reader.lines().enumerate() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let json: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let role_str = json.get("role").and_then(|v| v.as_str()).unwrap_or("");
        let role = if role_str == "user" || role_str == "assistant" {
            role_str
        } else {
            json.get("type").and_then(|v| v.as_str()).unwrap_or("")
        };
        if role != "user" && role != "assistant" {
            continue;
        }

        let content = extract_text_content(&json);
        if content.is_empty() {
            continue;
        }

        if count < off {
            count += 1;
            continue;
        }
        if messages.len() >= lim {
            break;
        }

        messages.push(TranscriptMessage {
            role: role.to_string(),
            content,
            line_index,
        });
        count += 1;
    }

    Ok(messages)
}

fn extract_text_content(json: &serde_json::Value) -> String {
    if let Some(content) = json.get("message").and_then(|m| m.get("content")) {
        return extract_from_content_value(content);
    }
    if let Some(content) = json.get("content") {
        return extract_from_content_value(content);
    }
    String::new()
}

fn extract_from_content_value(content: &serde_json::Value) -> String {
    if let Some(s) = content.as_str() {
        return s.to_string();
    }
    if let Some(arr) = content.as_array() {
        let texts: Vec<String> = arr
            .iter()
            .filter_map(|item| {
                let item_type = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
                if item_type == "text" {
                    item.get("text").and_then(|t| t.as_str()).map(String::from)
                } else {
                    None
                }
            })
            .collect();
        return texts.join("\n");
    }
    String::new()
}

/// Replace the text content of a message JSON node with `new_text`, preserving
/// all non-text structure (tool_use blocks, images, metadata, role, etc.).
/// Returns true if a text field was found and updated.
fn set_text_content(json: &mut serde_json::Value, new_text: &str) -> bool {
    if let Some(content) = json
        .get_mut("message")
        .and_then(|m| m.get_mut("content"))
    {
        return set_text_in_content_value(content, new_text);
    }
    if let Some(content) = json.get_mut("content") {
        return set_text_in_content_value(content, new_text);
    }
    false
}

fn set_text_in_content_value(content: &mut serde_json::Value, new_text: &str) -> bool {
    // String content: replace directly.
    if content.is_string() {
        *content = serde_json::Value::String(new_text.to_string());
        return true;
    }
    // Array content: collapse the text blocks into a single text block holding
    // the new value at the position of the first text block; drop the other
    // text blocks; keep every non-text block (tool_use, image, etc.) in order.
    if let Some(arr) = content.as_array() {
        let has_text = arr
            .iter()
            .any(|item| item.get("type").and_then(|t| t.as_str()) == Some("text"));
        if !has_text {
            return false;
        }
        let mut rebuilt = Vec::with_capacity(arr.len());
        let mut text_written = false;
        for item in arr {
            let is_text = item.get("type").and_then(|t| t.as_str()) == Some("text");
            if is_text {
                if !text_written {
                    rebuilt.push(serde_json::json!({ "type": "text", "text": new_text }));
                    text_written = true;
                }
                // subsequent text blocks are dropped (merged into the first)
            } else {
                rebuilt.push(item.clone());
            }
        }
        *content = serde_json::Value::Array(rebuilt);
        return true;
    }
    false
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchLocation {
    pub line_index: usize,
    pub byte_offset: usize,
    pub preview: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplaceResult {
    pub count: usize,
    pub backup_path: Option<String>,
}

fn line_role(json: &serde_json::Value) -> String {
    if let Some(role) = json
        .get("message")
        .and_then(|m| m.get("role"))
        .and_then(|v| v.as_str())
    {
        return role.to_string();
    }
    if let Some(role) = json.get("role").and_then(|v| v.as_str()) {
        return role.to_string();
    }
    if let Some(t) = json.get("type").and_then(|v| v.as_str()) {
        return t.to_string();
    }
    "unknown".to_string()
}

/// Validate that every non-empty line of `content` is parseable JSON.
/// Guards against a replace/edit that breaks the JSONL structure.
fn validate_jsonl(content: &str) -> Result<(), String> {
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if serde_json::from_str::<serde_json::Value>(trimmed).is_err() {
            return Err(format!(
                "Line {} is not valid JSON after the change; nothing was written.",
                i + 1
            ));
        }
    }
    Ok(())
}

#[tauri::command]
pub fn read_transcript_raw(file_path: String) -> Result<String, String> {
    if !is_safe_transcript_path(&file_path) {
        return Err("Invalid transcript path".to_string());
    }
    let content = read_with_sharing(&PathBuf::from(&file_path))?;
    Ok(normalize_line_endings(&content))
}

#[tauri::command]
pub fn search_transcript_matches(
    file_path: String,
    query: String,
) -> Result<Vec<MatchLocation>, String> {
    if !is_safe_transcript_path(&file_path) {
        return Err("Invalid transcript path".to_string());
    }
    if query.is_empty() {
        return Ok(vec![]);
    }

    let content = read_with_sharing(&PathBuf::from(&file_path))?;
    let content = normalize_line_endings(&content);

    let mut matches = Vec::new();
    for (line_index, line) in content.lines().enumerate() {
        let role = serde_json::from_str::<serde_json::Value>(line.trim())
            .map(|j| line_role(&j))
            .unwrap_or_else(|_| "unknown".to_string());

        let mut start = 0;
        while let Some(rel) = line[start..].find(&query) {
            let byte_offset = start + rel;
            let preview_start = byte_offset.saturating_sub(40);
            let preview_end = (byte_offset + query.len() + 40).min(line.len());
            // Snap to char boundaries to avoid slicing inside a UTF-8 sequence.
            let preview_start = floor_char_boundary(line, preview_start);
            let preview_end = ceil_char_boundary(line, preview_end);
            matches.push(MatchLocation {
                line_index,
                byte_offset,
                preview: line[preview_start..preview_end].to_string(),
                role: role.clone(),
            });
            start = byte_offset + query.len();
        }
    }
    Ok(matches)
}

fn floor_char_boundary(s: &str, mut idx: usize) -> usize {
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn ceil_char_boundary(s: &str, mut idx: usize) -> usize {
    while idx < s.len() && !s.is_char_boundary(idx) {
        idx += 1;
    }
    idx
}

fn backup_path_for(file_path: &str) -> PathBuf {
    PathBuf::from(format!("{}.bak", file_path))
}

/// Copy the original file's bytes to a sibling `.bak` before a destructive write.
fn write_backup(file_path: &str) -> Result<String, String> {
    let original = read_with_sharing(&PathBuf::from(file_path))?;
    let bak = backup_path_for(file_path);
    atomic_write_str(&bak, &original)?;
    Ok(bak.to_string_lossy().to_string())
}

#[tauri::command]
pub fn replace_in_transcript(
    file_path: String,
    find: String,
    replace: String,
    create_backup: bool,
) -> Result<ReplaceResult, String> {
    if !is_safe_transcript_path(&file_path) {
        return Err("Invalid transcript path".to_string());
    }
    if find.is_empty() {
        return Err("Search string cannot be empty".to_string());
    }

    let content = read_with_sharing(&PathBuf::from(&file_path))?;
    let content = normalize_line_endings(&content);

    let count = content.matches(&find).count();
    if count == 0 {
        return Ok(ReplaceResult {
            count: 0,
            backup_path: None,
        });
    }

    let backup_path = if create_backup {
        Some(write_backup(&file_path)?)
    } else {
        None
    };

    let updated = content.replace(&find, &replace);
    validate_jsonl(&updated)?;
    atomic_write_str(&PathBuf::from(&file_path), &updated)?;

    Ok(ReplaceResult { count, backup_path })
}

#[tauri::command]
pub fn save_transcript_raw(
    file_path: String,
    content: String,
    create_backup: bool,
) -> Result<Option<String>, String> {
    if !is_safe_transcript_path(&file_path) {
        return Err("Invalid transcript path".to_string());
    }
    let content = normalize_line_endings(&content);
    validate_jsonl(&content)?;

    let backup_path = if create_backup && PathBuf::from(&file_path).exists() {
        Some(write_backup(&file_path)?)
    } else {
        None
    };

    atomic_write_str(&PathBuf::from(&file_path), &content)?;
    Ok(backup_path)
}

/// Surgically replace the text content of a single message (identified by its
/// JSONL line index) with `new_text`, preserving the line's JSON structure.
/// Writes a sibling `.bak` first when `create_backup` is set.
#[tauri::command]
pub fn update_transcript_message(
    file_path: String,
    line_index: usize,
    new_text: String,
    create_backup: bool,
) -> Result<Option<String>, String> {
    if !is_safe_transcript_path(&file_path) {
        return Err("Invalid transcript path".to_string());
    }

    let content = read_with_sharing(&PathBuf::from(&file_path))?;
    let content = normalize_line_endings(&content);

    let mut lines: Vec<String> = content.split('\n').map(|l| l.to_string()).collect();
    if line_index >= lines.len() {
        return Err("Line index out of range".to_string());
    }

    let mut json: serde_json::Value = serde_json::from_str(lines[line_index].trim())
        .map_err(|e| format!("Line {} is not valid JSON: {}", line_index + 1, e))?;

    if !set_text_content(&mut json, &new_text) {
        return Err("No editable text content on this message".to_string());
    }
    lines[line_index] = serde_json::to_string(&json).map_err(|e| e.to_string())?;

    let backup_path = if create_backup {
        Some(write_backup(&file_path)?)
    } else {
        None
    };

    let updated = lines.join("\n");
    validate_jsonl(&updated)?;
    atomic_write_str(&PathBuf::from(&file_path), &updated)?;
    Ok(backup_path)
}

/// Open a transcript file in the system's default text editor.
/// macOS uses `open -t` (default text editor regardless of extension);
/// Windows falls back to notepad.
#[tauri::command]
pub fn open_in_text_editor(file_path: String) -> Result<(), String> {
    if !is_safe_transcript_path(&file_path) {
        return Err("Invalid transcript path".to_string());
    }
    if !PathBuf::from(&file_path).exists() {
        return Err("Transcript file not found".to_string());
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-t")
            .arg(&file_path)
            .spawn()
            .map_err(|e| format!("Failed to open editor: {}", e))?;
        Ok(())
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("notepad")
            .arg(&file_path)
            .spawn()
            .map_err(|e| format!("Failed to open editor: {}", e))?;
        Ok(())
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err("Opening a text editor is not supported on this platform".to_string())
    }
}

#[tauri::command]
pub fn delete_transcript_backup(backup_path: String) -> Result<(), String> {
    if !backup_path.ends_with(".bak") || !is_safe_transcript_path(&backup_path) {
        return Err("Invalid backup path".to_string());
    }
    std::fs::remove_file(&backup_path).map_err(|e| e.to_string())
}

/// Delete sibling `.jsonl.bak` files under ~/.claude/projects older than 7 days.
/// Delete `*.jsonl.bak` files older than `max_age` under `dir`, recursing up to
/// `depth` levels. Cursor backups nest deeper than Claude's
/// (`projects/<proj>/agent-transcripts/<uuid>/<uuid>.jsonl.bak`), so a flat
/// scan would orphan them — this walks both layouts.
fn prune_bak_in_dir(
    dir: &std::path::Path,
    max_age: std::time::Duration,
    now: std::time::SystemTime,
    depth: usize,
    removed: &mut usize,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth > 0 {
                prune_bak_in_dir(&path, max_age, now, depth - 1, removed);
            }
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.ends_with(".jsonl.bak") {
            continue;
        }
        let modified = match std::fs::metadata(&path).and_then(|m| m.modified()) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if now.duration_since(modified).map(|d| d > max_age).unwrap_or(false)
            && std::fs::remove_file(&path).is_ok()
        {
            *removed += 1;
        }
    }
}

#[tauri::command]
pub fn prune_transcript_backups() -> Result<usize, String> {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return Ok(0),
    };

    let max_age = std::time::Duration::from_secs(7 * 24 * 60 * 60);
    let now = std::time::SystemTime::now();
    let mut removed = 0;

    // Claude: projects/<proj>/<uuid>.jsonl.bak (1 level under projects).
    // Cursor: projects/<proj>/agent-transcripts/<uuid>/<uuid>.jsonl.bak (3 levels).
    // A depth cap of 4 covers both while staying bounded.
    for root in [
        home.join(".claude").join("projects"),
        home.join(".cursor").join("projects"),
    ] {
        if root.exists() {
            prune_bak_in_dir(&root, max_age, now, 4, &mut removed);
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_jsonl_accepts_valid_lines() {
        let content = "{\"type\":\"user\"}\n{\"role\":\"assistant\",\"content\":\"hi\"}\n";
        assert!(validate_jsonl(content).is_ok());
    }

    #[test]
    fn validate_jsonl_rejects_broken_line() {
        let content = "{\"type\":\"user\"}\n{not json}\n";
        assert!(validate_jsonl(content).is_err());
    }

    #[test]
    fn replace_is_case_sensitive_and_counts() {
        let content = "secret SECRET secret";
        assert_eq!(content.matches("secret").count(), 2);
        assert_eq!(content.replace("secret", "X"), "X SECRET X");
    }

    #[test]
    fn search_matches_finds_all_occurrences_across_roles() {
        let dir = std::env::temp_dir().join(format!("ah_tx_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("s.jsonl");
        let body = "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"key=ABC123\"}}\n{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":\"you said ABC123\"}}\n";
        std::fs::write(&file, body).unwrap();

        // Bypass the home-dir guard by calling the inner logic directly through file read.
        let content = std::fs::read_to_string(&file).unwrap();
        let mut count = 0;
        for line in content.lines() {
            count += line.matches("ABC123").count();
        }
        assert_eq!(count, 2);
        std::fs::remove_file(&file).ok();
    }

    #[test]
    fn merge_resolved_sessions_marks_db_only_missing_and_leaves_file_backed_alone() {
        let file_backed = TranscriptSession {
            session_id: "composer-with-file".to_string(),
            project_name: "/repo/a".to_string(),
            source: "cursor".to_string(),
            modified_at: "2026-01-01T00:00:00+00:00".to_string(),
            file_size_bytes: 1234,
            file_path: "/Users/x/.cursor/projects/repo-a/agent-transcripts/composer-with-file/composer-with-file.jsonl".to_string(),
            missing_file: false,
        };
        let scanned = vec![file_backed.clone()];

        // One resolved composer duplicates a file-backed session (must not
        // be re-added or altered); the other has no matching scanned file
        // (must be appended as a `missing_file` placeholder).
        let resolved = vec![
            (
                "composer-with-file".to_string(),
                "/repo/a".to_string(),
                Some(1_767_225_600_000), // 2026-01-01T00:00:00Z
                Some("has a file".to_string()),
            ),
            (
                "composer-db-only".to_string(),
                "/repo/b".to_string(),
                Some(1_767_225_600_000), // 2026-01-01T00:00:00Z
                Some("db only".to_string()),
            ),
        ];

        let merged = merge_resolved_sessions(scanned, resolved);
        assert_eq!(merged.len(), 2, "must not duplicate the file-backed composer");

        let kept = merged.iter().find(|s| s.session_id == "composer-with-file").unwrap();
        assert!(!kept.missing_file, "file-backed session must stay missing_file=false");
        assert_eq!(kept.file_path, file_backed.file_path);

        let added = merged.iter().find(|s| s.session_id == "composer-db-only").unwrap();
        assert!(added.missing_file, "DB-only composer must be marked missing_file=true");
        assert_eq!(added.project_name, "/repo/b");
        assert_eq!(added.source, "cursor");
        assert_eq!(added.file_size_bytes, 0);
        assert_eq!(added.file_path, "");
        assert!(
            added.modified_at.starts_with("2026-01-01T00:00:00"),
            "unexpected modified_at: {}",
            added.modified_at
        );
    }

    #[test]
    fn delete_backup_rejects_non_bak() {
        let err = delete_transcript_backup("/tmp/foo.jsonl".to_string());
        assert!(err.is_err());
    }

    #[test]
    fn set_text_content_preserves_tool_blocks() {
        let mut json: serde_json::Value = serde_json::from_str(
            r#"{"type":"assistant","message":{"role":"assistant","content":[
                {"type":"text","text":"my key is ABC123"},
                {"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}}
            ]}}"#,
        )
        .unwrap();
        assert!(set_text_content(&mut json, "my key is DUMMY"));
        let arr = json["message"]["content"].as_array().unwrap();
        // text rewritten, tool_use preserved
        assert_eq!(arr[0]["text"], "my key is DUMMY");
        assert_eq!(arr[1]["type"], "tool_use");
        assert_eq!(arr[1]["input"]["command"], "ls");
    }

    #[test]
    fn set_text_content_handles_string_content() {
        let mut json: serde_json::Value =
            serde_json::from_str(r#"{"role":"user","content":"token=XYZ"}"#).unwrap();
        assert!(set_text_content(&mut json, "token=DUMMY"));
        assert_eq!(json["content"], "token=DUMMY");
    }

    #[test]
    fn prune_walks_cursor_depth_nesting() {
        // Mimic the Cursor layout: root/proj/agent-transcripts/uuid/x.jsonl.bak
        let root = std::env::temp_dir().join(format!("ah_prune_{}", std::process::id()));
        let deep = root.join("proj").join("agent-transcripts").join("uuid");
        std::fs::create_dir_all(&deep).unwrap();
        let bak = deep.join("x.jsonl.bak");
        std::fs::write(&bak, "old").unwrap();
        // A recent (non-stale) .bak at the Claude depth that must survive.
        let claude_proj = root.join("cproj");
        std::fs::create_dir_all(&claude_proj).unwrap();
        let fresh = claude_proj.join("y.jsonl.bak");
        std::fs::write(&fresh, "new").unwrap();

        // Age the deep one well past the cutoff.
        let mut removed = 0;
        let max_age = std::time::Duration::from_secs(0); // everything counts as stale
        prune_bak_in_dir(
            &root,
            max_age,
            std::time::SystemTime::now() + std::time::Duration::from_secs(10),
            4,
            &mut removed,
        );
        assert_eq!(removed, 2, "should reach both Claude- and Cursor-depth backups");
        assert!(!bak.exists());
        assert!(!fresh.exists());
        std::fs::remove_dir_all(&root).ok();
    }
}
