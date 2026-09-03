//! Read-only Codex history surfaces.
//!
//! Codex persists prompt history in `history.jsonl`, thread metadata in
//! `state_5.sqlite`, and complete rollouts below `sessions/` and
//! `archived_sessions/`. This module exposes only user-visible messages and
//! explicit `update_plan` calls. Developer messages, reasoning, tool calls,
//! tool results, shell snapshots, auth files, and raw memory inputs are never
//! returned.

use crate::utils::codex_paths::codex_home;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

const DEFAULT_PAGE_SIZE: usize = 100;
const MAX_PAGE_SIZE: usize = 200;
const MAX_PAGE_OFFSET: usize = 20_000;
const MAX_HISTORY_BYTES: u64 = 16 * 1024 * 1024;
const MAX_HISTORY_RECORDS: usize = 20_000;
const MAX_SESSION_RECORDS: usize = 2_000;
const MAX_FALLBACK_FILES: usize = 5_000;
const MAX_JSONL_LINE_BYTES: usize = 2 * 1024 * 1024;
const MAX_TRANSCRIPT_SCAN_BYTES: u64 = 32 * 1024 * 1024;
const MAX_SEARCH_SCAN_BYTES: u64 = 2 * 1024 * 1024;
const MAX_SEARCH_SESSIONS: usize = 200;
const MAX_VISIBLE_TEXT_BYTES: usize = 256 * 1024;
const MAX_PLAN_SESSION_FILES: usize = 500;
const MAX_PLAN_BYTES_PER_SESSION: u64 = 4 * 1024 * 1024;
const MAX_PLAN_TOTAL_SCAN_BYTES: u64 = 128 * 1024 * 1024;
const MAX_PLAN_ITEMS: usize = 100;
const MAX_TODO_ITEMS: usize = 2_000;
const MAX_MEMORY_DOCUMENT_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexPromptEntry {
    pub display: String,
    pub timestamp: String,
    pub timestamp_ms: i64,
    pub project: Option<String>,
    pub project_name: Option<String>,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexPromptHistoryPage {
    pub entries: Vec<CodexPromptEntry>,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub has_more: bool,
    pub source_path: String,
    pub source_type: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexTranscriptSession {
    pub session_id: String,
    pub title: String,
    pub project_path: String,
    pub project_name: String,
    pub created_at: String,
    pub updated_at: String,
    pub file_size_bytes: u64,
    pub archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexTranscriptSessionPage {
    pub sessions: Vec<CodexTranscriptSession>,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub has_more: bool,
    pub source_path: String,
    pub source_type: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexTranscriptMessage {
    pub role: String,
    pub content: String,
    pub timestamp: Option<String>,
    pub ordinal: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexTranscriptPage {
    pub session: CodexTranscriptSession,
    pub messages: Vec<CodexTranscriptMessage>,
    pub offset: usize,
    pub limit: usize,
    pub has_more: bool,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexPlanItem {
    pub content: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexPlanSnapshot {
    pub session_id: String,
    pub title: String,
    pub project_path: String,
    pub updated_at: String,
    pub explanation: Option<String>,
    pub items: Vec<CodexPlanItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexTodoItem {
    pub content: String,
    pub status: String,
    pub session_id: String,
    pub session_title: String,
    pub project_path: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexPlansAndTodos {
    pub plans: Vec<CodexPlanSnapshot>,
    pub todos: Vec<CodexTodoItem>,
    pub source: String,
    pub source_path: String,
    pub source_type: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexMemoryDocument {
    pub id: String,
    pub title: String,
    pub relative_path: String,
    pub size_bytes: u64,
    pub modified_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexMemoryStatus {
    pub available: bool,
    pub scope: String,
    pub source_path: String,
    pub source_type: String,
    pub description: String,
    pub warning: Option<String>,
    pub documents: Vec<CodexMemoryDocument>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexMemoryDocumentContent {
    pub id: String,
    pub title: String,
    pub content: String,
    pub truncated: bool,
}

#[derive(Debug, Clone)]
struct SessionRecord {
    public: CodexTranscriptSession,
    rollout_path: PathBuf,
}

#[derive(Debug, Default, Clone, Copy)]
struct ScanOutcome {
    byte_limit_hit: bool,
    record_limit_hit: bool,
    oversized_line_skipped: bool,
    stopped_early: bool,
}

impl ScanOutcome {
    fn truncated(self) -> bool {
        self.byte_limit_hit || self.record_limit_hit || self.oversized_line_skipped
    }
}

fn bounded_page(limit: Option<usize>, offset: Option<usize>) -> (usize, usize) {
    let limit = limit.unwrap_or(DEFAULT_PAGE_SIZE).clamp(1, MAX_PAGE_SIZE);
    let offset = offset.unwrap_or(0).min(MAX_PAGE_OFFSET);
    (limit, offset)
}

fn canonical_root(root: &Path) -> Option<PathBuf> {
    if !root.is_dir() {
        return None;
    }
    dunce::canonicalize(root).ok()
}

fn safe_existing_file_below(root: &Path, candidate: &Path) -> Option<PathBuf> {
    let canonical_root = canonical_root(root)?;
    let canonical = dunce::canonicalize(candidate).ok()?;
    let metadata = fs::metadata(&canonical).ok()?;
    if !metadata.is_file() || !canonical.starts_with(&canonical_root) {
        return None;
    }
    Some(canonical)
}

fn safe_rollout_path(root: &Path, candidate: &Path) -> Option<PathBuf> {
    if candidate
        .extension()
        .and_then(|extension| extension.to_str())
        != Some("jsonl")
    {
        return None;
    }
    let canonical = safe_existing_file_below(root, candidate)?;
    let allowed = [root.join("sessions"), root.join("archived_sessions")];
    if allowed.iter().any(|directory| {
        canonical_root(directory)
            .map(|allowed_root| canonical.starts_with(allowed_root))
            .unwrap_or(false)
    }) {
        Some(canonical)
    } else {
        None
    }
}

fn is_safe_session_id(session_id: &str) -> bool {
    !session_id.is_empty()
        && session_id.len() <= 128
        && session_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
}

fn path_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.to_string())
}

fn timestamp_to_rfc3339(timestamp: i64) -> String {
    let datetime = if timestamp.abs() >= 10_000_000_000 {
        DateTime::<Utc>::from_timestamp_millis(timestamp)
    } else {
        DateTime::<Utc>::from_timestamp(timestamp, 0)
    };
    datetime.map(|value| value.to_rfc3339()).unwrap_or_default()
}

fn modified_at(path: &Path) -> String {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .map(timestamp_to_rfc3339)
        .unwrap_or_default()
}

fn truncate_utf8(input: &str, max_bytes: usize) -> (String, bool) {
    if input.len() <= max_bytes {
        return (input.to_string(), false);
    }
    let mut end = max_bytes;
    while end > 0 && !input.is_char_boundary(end) {
        end -= 1;
    }
    (input[..end].to_string(), true)
}

fn token_end(input: &str, start: usize) -> usize {
    let mut end = start;
    for (offset, character) in input[start..].char_indices() {
        if character.is_whitespace()
            || matches!(
                character,
                '"' | '\'' | '`' | '<' | '>' | '[' | ']' | '{' | '}' | ',' | ';'
            )
        {
            break;
        }
        end = start + offset + character.len_utf8();
    }
    end
}

fn push_secret_span(spans: &mut Vec<(usize, usize)>, input: &str, start: usize) {
    let end = token_end(input, start);
    if end > start {
        spans.push((start, end));
    }
}

fn is_secret_key_boundary(input: &str, start: usize, end: usize) -> bool {
    let before_is_word = start
        .checked_sub(1)
        .and_then(|index| input.as_bytes().get(index))
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_');
    let after_is_word = input
        .as_bytes()
        .get(end)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_');
    !before_is_word && !after_is_word
}

fn is_contextual_secret_candidate(input: &str, start: usize) -> bool {
    let end = token_end(input, start);
    if end <= start {
        return false;
    }

    let candidate = input[start..end]
        .trim_matches(|character: char| character.is_ascii_punctuation())
        .to_ascii_lowercase();
    if candidate.len() < 4 {
        return false;
    }

    !matches!(
        candidate.as_str(),
        "authentication"
            | "change"
            | "changes"
            | "field"
            | "fields"
            | "form"
            | "input"
            | "login"
            | "manager"
            | "managers"
            | "must"
            | "policy"
            | "policies"
            | "prompt"
            | "prompts"
            | "protection"
            | "required"
            | "requirement"
            | "requirements"
            | "reset"
            | "resets"
            | "rule"
            | "rules"
            | "security"
            | "should"
            | "storage"
    )
}

/// Redact common credential forms without serializing or logging the original.
/// The history UI is still a local prompt browser, so normal user-authored text
/// remains visible.
fn redact_sensitive_text(input: &str) -> String {
    let mut spans = Vec::new();
    let lower = input.to_ascii_lowercase();

    for prefix in [
        "sk-",
        "ghp_",
        "github_pat_",
        "xoxb-",
        "xoxp-",
        "xoxa-",
        "xoxr-",
        "ya29.",
        "aiza",
    ] {
        let haystack = if prefix == "aiza" {
            lower.as_str()
        } else {
            input
        };
        for (start, _) in haystack.match_indices(prefix) {
            push_secret_span(&mut spans, input, start);
        }
    }

    let mut from = 0;
    while let Some(relative) = lower[from..].find("bearer ") {
        let value_start = from + relative + "bearer ".len();
        push_secret_span(&mut spans, input, value_start);
        from = value_start.min(lower.len());
        if from == lower.len() {
            break;
        }
    }

    for (key, allow_conversational_value) in [
        ("api_key", false),
        ("api key", true),
        ("apikey", false),
        ("access_token", false),
        ("access token", true),
        ("refresh_token", false),
        ("refresh token", true),
        ("authorization", false),
        ("password", true),
        ("passcode", true),
        ("client_secret", false),
        ("client secret", true),
    ] {
        let mut cursor = 0;
        while let Some(relative) = lower[cursor..].find(key) {
            let key_start = cursor + relative;
            let key_end = key_start + key.len();
            if !is_secret_key_boundary(&lower, key_start, key_end) {
                cursor = key_end;
                continue;
            }
            let bytes = input.as_bytes();
            let mut separator = key_end;
            while separator < bytes.len() && matches!(bytes[separator], b' ' | b'\t' | b'"' | b'\'')
            {
                separator += 1;
            }

            let has_assignment_separator =
                separator < bytes.len() && matches!(bytes[separator], b'=' | b':');
            let mut value_start = if has_assignment_separator {
                separator + 1
            } else if allow_conversational_value && separator > key_end {
                separator
            } else {
                cursor = key_end;
                continue;
            };
            while value_start < bytes.len()
                && matches!(bytes[value_start], b' ' | b'\t' | b'"' | b'\'')
            {
                value_start += 1;
            }

            if allow_conversational_value
                && !has_assignment_separator
                && lower[value_start..].starts_with("is ")
            {
                value_start += "is ".len();
                while value_start < bytes.len()
                    && matches!(bytes[value_start], b' ' | b'\t' | b'"' | b'\'')
                {
                    value_start += 1;
                }
            }

            if !has_assignment_separator && !is_contextual_secret_candidate(input, value_start) {
                cursor = key_end;
                continue;
            }
            push_secret_span(&mut spans, input, value_start);
            cursor = value_start.max(key_end);
        }
    }

    if spans.is_empty() {
        return input.to_string();
    }
    spans.sort_unstable_by_key(|span| span.0);
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for span in spans {
        if let Some(last) = merged.last_mut() {
            if span.0 <= last.1 {
                last.1 = last.1.max(span.1);
                continue;
            }
        }
        merged.push(span);
    }

    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    for (start, end) in merged {
        if start < cursor || start > input.len() || end > input.len() {
            continue;
        }
        output.push_str(&input[cursor..start]);
        output.push_str("[REDACTED]");
        cursor = end;
    }
    output.push_str(&input[cursor..]);
    output
}

fn bounded_visible_text(input: &str) -> (String, bool) {
    let redacted = redact_sensitive_text(input);
    truncate_utf8(redacted.trim(), MAX_VISIBLE_TEXT_BYTES)
}

fn scan_jsonl_prefix<F>(
    path: &Path,
    max_bytes: u64,
    max_records: usize,
    mut visitor: F,
) -> Result<ScanOutcome, String>
where
    F: FnMut(&Value) -> bool,
{
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut reader = BufReader::new(file.take(max_bytes));
    let mut outcome = ScanOutcome {
        byte_limit_hit: metadata.len() > max_bytes,
        ..ScanOutcome::default()
    };
    let mut records = 0usize;
    loop {
        let mut line = Vec::new();
        let bytes = reader
            .read_until(b'\n', &mut line)
            .map_err(|error| error.to_string())?;
        if bytes == 0 {
            break;
        }
        if line.len() > MAX_JSONL_LINE_BYTES {
            outcome.oversized_line_skipped = true;
            continue;
        }
        let Ok(value) = serde_json::from_slice::<Value>(&line) else {
            continue;
        };
        records += 1;
        if records > max_records {
            outcome.record_limit_hit = true;
            break;
        }
        if !visitor(&value) {
            outcome.stopped_early = true;
            break;
        }
    }
    Ok(outcome)
}

fn scan_jsonl_tail<F>(
    path: &Path,
    max_bytes: u64,
    max_records: usize,
    mut visitor: F,
) -> Result<ScanOutcome, String>
where
    F: FnMut(&Value) -> bool,
{
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let start = metadata.len().saturating_sub(max_bytes);
    file.seek(SeekFrom::Start(start))
        .map_err(|error| error.to_string())?;
    let mut reader = BufReader::new(file.take(max_bytes));
    let mut outcome = ScanOutcome {
        byte_limit_hit: start > 0,
        ..ScanOutcome::default()
    };
    if start > 0 {
        let mut partial = Vec::new();
        reader
            .read_until(b'\n', &mut partial)
            .map_err(|error| error.to_string())?;
    }
    let mut records = 0usize;
    loop {
        let mut line = Vec::new();
        let bytes = reader
            .read_until(b'\n', &mut line)
            .map_err(|error| error.to_string())?;
        if bytes == 0 {
            break;
        }
        if line.len() > MAX_JSONL_LINE_BYTES {
            outcome.oversized_line_skipped = true;
            continue;
        }
        let Ok(value) = serde_json::from_slice::<Value>(&line) else {
            continue;
        };
        records += 1;
        if records > max_records {
            outcome.record_limit_hit = true;
            break;
        }
        if !visitor(&value) {
            outcome.stopped_early = true;
            break;
        }
    }
    Ok(outcome)
}

fn open_state_db(root: &Path) -> Option<Connection> {
    let path = safe_existing_file_below(root, &root.join("state_5.sqlite"))?;
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()
}

fn session_row_to_record(
    root: &Path,
    id: String,
    title: String,
    cwd: String,
    created_at: i64,
    updated_at: i64,
    rollout_path: String,
    archived: i64,
) -> Option<SessionRecord> {
    if !is_safe_session_id(&id) {
        return None;
    }
    let rollout_path = safe_rollout_path(root, Path::new(&rollout_path))?;
    let file_size_bytes = fs::metadata(&rollout_path).ok()?.len();
    let (title, _) = truncate_utf8(&redact_sensitive_text(title.trim()), 512);
    Some(SessionRecord {
        public: CodexTranscriptSession {
            session_id: id.clone(),
            title: if title.is_empty() { id } else { title },
            project_name: path_name(&cwd),
            project_path: cwd,
            created_at: timestamp_to_rfc3339(created_at),
            updated_at: timestamp_to_rfc3339(updated_at),
            file_size_bytes,
            archived: archived != 0,
        },
        rollout_path,
    })
}

fn load_sessions_from_db(root: &Path) -> Option<(Vec<SessionRecord>, bool)> {
    let connection = open_state_db(root)?;
    let mut statement = connection
        .prepare(
            "SELECT id, title, cwd, created_at, updated_at, rollout_path, archived \
             FROM threads WHERE has_user_event = 1 \
             ORDER BY updated_at DESC LIMIT ?1",
        )
        .ok()?;
    let requested = MAX_SESSION_RECORDS + 1;
    let rows = statement
        .query_map([requested as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })
        .ok()?;

    let mut records = Vec::new();
    let mut truncated = false;
    for row in rows.flatten() {
        if records.len() >= MAX_SESSION_RECORDS {
            truncated = true;
            break;
        }
        if let Some(record) =
            session_row_to_record(root, row.0, row.1, row.2, row.3, row.4, row.5, row.6)
        {
            records.push(record);
        }
    }
    Some((records, truncated))
}

fn session_meta_from_rollout(path: &Path) -> Option<(String, String, String)> {
    let mut result = None;
    let _ = scan_jsonl_prefix(path, 1024 * 1024, 32, |value| {
        if value.get("type").and_then(Value::as_str) != Some("session_meta") {
            return true;
        }
        let Some(payload) = value.get("payload") else {
            return true;
        };
        let Some(id) = payload
            .get("id")
            .or_else(|| payload.get("session_id"))
            .and_then(Value::as_str)
        else {
            return true;
        };
        let cwd = payload.get("cwd").and_then(Value::as_str).unwrap_or("");
        let timestamp = payload
            .get("timestamp")
            .and_then(Value::as_str)
            .unwrap_or("");
        result = Some((id.to_string(), cwd.to_string(), timestamp.to_string()));
        false
    });
    result
}

fn load_sessions_from_files(root: &Path) -> (Vec<SessionRecord>, bool) {
    let mut records = Vec::new();
    let mut seen_files = 0usize;
    let mut truncated = false;
    for (directory, archived) in [
        (root.join("sessions"), false),
        (root.join("archived_sessions"), true),
    ] {
        if !directory.is_dir() {
            continue;
        }
        for entry in WalkDir::new(&directory)
            .max_depth(4)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
        {
            if !entry.file_type().is_file()
                || entry.path().extension().and_then(|value| value.to_str()) != Some("jsonl")
            {
                continue;
            }
            seen_files += 1;
            if seen_files > MAX_FALLBACK_FILES {
                truncated = true;
                break;
            }
            let Some(path) = safe_rollout_path(root, entry.path()) else {
                continue;
            };
            let Some((id, cwd, created_at)) = session_meta_from_rollout(&path) else {
                continue;
            };
            if !is_safe_session_id(&id) {
                continue;
            }
            let metadata = match fs::metadata(&path) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let updated_at = modified_at(&path);
            records.push(SessionRecord {
                public: CodexTranscriptSession {
                    session_id: id.clone(),
                    title: id,
                    project_name: path_name(&cwd),
                    project_path: cwd,
                    created_at,
                    updated_at,
                    file_size_bytes: metadata.len(),
                    archived,
                },
                rollout_path: path,
            });
        }
    }
    records.sort_by(|left, right| right.public.updated_at.cmp(&left.public.updated_at));
    if records.len() > MAX_SESSION_RECORDS {
        records.truncate(MAX_SESSION_RECORDS);
        truncated = true;
    }
    (records, truncated)
}

fn load_session_records(root: &Path) -> (Vec<SessionRecord>, bool) {
    load_sessions_from_db(root).unwrap_or_else(|| load_sessions_from_files(root))
}

fn find_session(root: &Path, session_id: &str) -> Option<SessionRecord> {
    if !is_safe_session_id(session_id) {
        return None;
    }
    if let Some(connection) = open_state_db(root) {
        let row = connection.query_row(
            "SELECT id, title, cwd, created_at, updated_at, rollout_path, archived \
             FROM threads WHERE id = ?1 LIMIT 1",
            [session_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            },
        );
        if let Ok(row) = row {
            return session_row_to_record(root, row.0, row.1, row.2, row.3, row.4, row.5, row.6);
        }
    }
    load_sessions_from_files(root)
        .0
        .into_iter()
        .find(|record| record.public.session_id == session_id)
}

fn history_entries_at(root: &Path) -> Result<(Vec<CodexPromptEntry>, bool, PathBuf), String> {
    let source_path = root.join("history.jsonl");
    if !source_path.exists() {
        return Ok((Vec::new(), false, source_path));
    }
    let source_path = safe_existing_file_below(root, &source_path)
        .ok_or_else(|| "Codex history path resolves outside CODEX_HOME".to_string())?;
    let (sessions, sessions_truncated) = load_session_records(root);
    let projects: HashMap<String, String> = sessions
        .into_iter()
        .map(|record| (record.public.session_id, record.public.project_path))
        .collect();
    let mut entries = Vec::new();
    let outcome = scan_jsonl_tail(
        &source_path,
        MAX_HISTORY_BYTES,
        MAX_HISTORY_RECORDS,
        |value| {
            let Some(session_id) = value.get("session_id").and_then(Value::as_str) else {
                return true;
            };
            let Some(text) = value.get("text").and_then(Value::as_str) else {
                return true;
            };
            let Some(timestamp) = value.get("ts").and_then(Value::as_i64) else {
                return true;
            };
            if !is_safe_session_id(session_id) || text.trim().is_empty() {
                return true;
            }
            let (display, _) = bounded_visible_text(text);
            let timestamp_ms = if timestamp.abs() >= 10_000_000_000 {
                timestamp
            } else {
                timestamp.saturating_mul(1_000)
            };
            let project = projects.get(session_id).cloned();
            let project_name = project.as_deref().map(path_name);
            entries.push(CodexPromptEntry {
                display,
                timestamp: timestamp_to_rfc3339(timestamp),
                timestamp_ms,
                project,
                project_name,
                session_id: session_id.to_string(),
            });
            true
        },
    )?;
    entries.sort_by(|left, right| right.timestamp_ms.cmp(&left.timestamp_ms));
    Ok((
        entries,
        outcome.truncated() || sessions_truncated,
        source_path,
    ))
}

#[tauri::command]
pub async fn get_codex_prompt_history(
    limit: Option<usize>,
    offset: Option<usize>,
    project_path: Option<String>,
    query: Option<String>,
) -> Result<CodexPromptHistoryPage, String> {
    tokio::task::spawn_blocking(move || {
        get_codex_prompt_history_sync(limit, offset, project_path, query)
    })
    .await
    .map_err(|error| format!("Codex prompt history worker failed: {error}"))?
}

fn get_codex_prompt_history_sync(
    limit: Option<usize>,
    offset: Option<usize>,
    project_path: Option<String>,
    query: Option<String>,
) -> Result<CodexPromptHistoryPage, String> {
    let root = codex_home()?;
    let (limit, offset) = bounded_page(limit, offset);
    let (entries, truncated, source_path) = history_entries_at(&root)?;
    let query = query.unwrap_or_default().trim().to_lowercase();
    let project_path = project_path.filter(|value| !value.trim().is_empty());
    let filtered: Vec<CodexPromptEntry> = entries
        .into_iter()
        .filter(|entry| {
            project_path
                .as_ref()
                .map(|project| entry.project.as_ref() == Some(project))
                .unwrap_or(true)
                && (query.is_empty() || entry.display.to_lowercase().contains(&query))
        })
        .collect();
    let total = filtered.len();
    let page = filtered.into_iter().skip(offset).take(limit).collect();
    Ok(CodexPromptHistoryPage {
        entries: page,
        total,
        offset,
        limit,
        has_more: offset.saturating_add(limit) < total,
        source_path: source_path.to_string_lossy().to_string(),
        source_type: "file".to_string(),
        truncated,
    })
}

fn visible_message(value: &Value, ordinal: usize) -> Option<(CodexTranscriptMessage, bool)> {
    if value.get("type").and_then(Value::as_str) != Some("response_item") {
        return None;
    }
    let payload = value.get("payload")?;
    if payload.get("type").and_then(Value::as_str) != Some("message") {
        return None;
    }
    let role = payload.get("role").and_then(Value::as_str)?;
    if role != "user" && role != "assistant" {
        return None;
    }
    let allowed_content_type = if role == "user" {
        "input_text"
    } else {
        "output_text"
    };
    let content = payload
        .get("content")
        .and_then(Value::as_array)?
        .iter()
        .filter(|part| part.get("type").and_then(Value::as_str) == Some(allowed_content_type))
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    if content.trim().is_empty() {
        return None;
    }
    let (content, text_truncated) = bounded_visible_text(&content);
    let timestamp = value
        .get("timestamp")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    Some((
        CodexTranscriptMessage {
            role: role.to_string(),
            content,
            timestamp,
            ordinal,
        },
        text_truncated,
    ))
}

fn rollout_has_user_query(path: &Path, query: &str) -> (bool, bool) {
    let mut ordinal = 0usize;
    let mut found = false;
    let outcome = scan_jsonl_prefix(path, MAX_SEARCH_SCAN_BYTES, 20_000, |value| {
        let Some((message, _)) = visible_message(value, ordinal) else {
            return true;
        };
        ordinal += 1;
        if message.role == "user" && message.content.to_lowercase().contains(query) {
            found = true;
            return false;
        }
        true
    })
    .unwrap_or_default();
    (found, outcome.truncated())
}

#[tauri::command]
pub async fn list_codex_transcript_sessions(
    limit: Option<usize>,
    offset: Option<usize>,
    project_path: Option<String>,
    query: Option<String>,
) -> Result<CodexTranscriptSessionPage, String> {
    tokio::task::spawn_blocking(move || {
        list_codex_transcript_sessions_sync(limit, offset, project_path, query)
    })
    .await
    .map_err(|error| format!("Codex transcript list worker failed: {error}"))?
}

fn list_codex_transcript_sessions_sync(
    limit: Option<usize>,
    offset: Option<usize>,
    project_path: Option<String>,
    query: Option<String>,
) -> Result<CodexTranscriptSessionPage, String> {
    let root = codex_home()?;
    let (limit, offset) = bounded_page(limit, offset);
    let (records, mut truncated) = load_session_records(&root);
    let project_path = project_path.filter(|value| !value.trim().is_empty());
    let query = query.unwrap_or_default().trim().to_lowercase();
    let mut filtered = Vec::new();
    let mut searched = 0usize;
    for record in records {
        if project_path
            .as_ref()
            .map(|project| &record.public.project_path != project)
            .unwrap_or(false)
        {
            continue;
        }
        let matches = if query.is_empty() {
            true
        } else if record.public.title.to_lowercase().contains(&query)
            || record.public.project_path.to_lowercase().contains(&query)
        {
            true
        } else if searched < MAX_SEARCH_SESSIONS {
            searched += 1;
            let (matches, search_truncated) = rollout_has_user_query(&record.rollout_path, &query);
            truncated |= search_truncated;
            matches
        } else {
            truncated = true;
            false
        };
        if matches {
            filtered.push(record.public);
        }
    }
    let total = filtered.len();
    let sessions = filtered.into_iter().skip(offset).take(limit).collect();
    Ok(CodexTranscriptSessionPage {
        sessions,
        total,
        offset,
        limit,
        has_more: offset.saturating_add(limit) < total,
        source_path: root.to_string_lossy().to_string(),
        source_type: "directory".to_string(),
        truncated,
    })
}

fn transcript_page_at(
    record: SessionRecord,
    limit: usize,
    offset: usize,
) -> Result<CodexTranscriptPage, String> {
    let mut visible_ordinal = 0usize;
    let mut messages = Vec::new();
    let mut text_truncated = false;
    let target = offset.saturating_add(limit).saturating_add(1);
    let outcome = scan_jsonl_prefix(
        &record.rollout_path,
        MAX_TRANSCRIPT_SCAN_BYTES,
        100_000,
        |value| {
            let Some((message, was_truncated)) = visible_message(value, visible_ordinal) else {
                return true;
            };
            visible_ordinal += 1;
            text_truncated |= was_truncated;
            if visible_ordinal > offset {
                messages.push(message);
            }
            visible_ordinal < target
        },
    )?;
    let has_extra = messages.len() > limit;
    if has_extra {
        messages.truncate(limit);
    }
    let scan_truncated = outcome.truncated();
    Ok(CodexTranscriptPage {
        session: record.public,
        messages,
        offset,
        limit,
        // `hasMore` means another bounded page can be read. A safety scan
        // limit is reported separately through `truncated`; treating it as
        // another page would make the UI offer an endless Load More action.
        has_more: has_extra,
        truncated: scan_truncated || text_truncated,
    })
}

#[tauri::command]
pub async fn read_codex_transcript(
    session_id: String,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<CodexTranscriptPage, String> {
    tokio::task::spawn_blocking(move || read_codex_transcript_sync(session_id, limit, offset))
        .await
        .map_err(|error| format!("Codex transcript worker failed: {error}"))?
}

fn read_codex_transcript_sync(
    session_id: String,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<CodexTranscriptPage, String> {
    let root = codex_home()?;
    let (limit, offset) = bounded_page(limit, offset);
    let record = find_session(&root, &session_id)
        .ok_or_else(|| "Codex transcript session was not found".to_string())?;
    transcript_page_at(record, limit, offset)
}

fn parse_plan_update(value: &Value) -> Option<(Option<String>, Vec<CodexPlanItem>, String)> {
    if value.get("type").and_then(Value::as_str) != Some("response_item") {
        return None;
    }
    let payload = value.get("payload")?;
    if payload.get("type").and_then(Value::as_str) != Some("function_call")
        || payload.get("name").and_then(Value::as_str) != Some("update_plan")
    {
        return None;
    }
    let arguments = payload.get("arguments").or_else(|| payload.get("input"))?;
    let arguments: Value = match arguments {
        Value::String(serialized) => serde_json::from_str(serialized).ok()?,
        Value::Object(_) => arguments.clone(),
        _ => return None,
    };
    let explanation = arguments
        .get("explanation")
        .and_then(Value::as_str)
        .map(bounded_visible_text)
        .map(|value| value.0)
        .filter(|value| !value.is_empty());
    let items = arguments
        .get("plan")
        .and_then(Value::as_array)?
        .iter()
        .take(MAX_PLAN_ITEMS)
        .filter_map(|item| {
            let content = item.get("step").and_then(Value::as_str)?.trim();
            let status = item.get("status").and_then(Value::as_str)?.trim();
            if content.is_empty() || status.is_empty() {
                return None;
            }
            Some(CodexPlanItem {
                content: bounded_visible_text(content).0,
                status: status.to_string(),
            })
        })
        .collect::<Vec<_>>();
    let timestamp = value
        .get("timestamp")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    Some((explanation, items, timestamp))
}

fn latest_plan_from_rollout(
    record: &SessionRecord,
) -> Result<(Option<CodexPlanSnapshot>, bool), String> {
    let mut latest = None;
    let outcome = scan_jsonl_tail(
        &record.rollout_path,
        MAX_PLAN_BYTES_PER_SESSION,
        50_000,
        |value| {
            if let Some((explanation, items, timestamp)) = parse_plan_update(value) {
                latest = Some(CodexPlanSnapshot {
                    session_id: record.public.session_id.clone(),
                    title: record.public.title.clone(),
                    project_path: record.public.project_path.clone(),
                    updated_at: if timestamp.is_empty() {
                        record.public.updated_at.clone()
                    } else {
                        timestamp
                    },
                    explanation,
                    items,
                });
            }
            true
        },
    )?;
    Ok((
        latest.filter(|plan| !plan.items.is_empty()),
        outcome.truncated(),
    ))
}

fn plans_and_todos_at(root: &Path, limit: usize, project_path: Option<&str>) -> CodexPlansAndTodos {
    let (records, mut truncated) = load_session_records(root);
    let mut plans = Vec::new();
    let mut scanned_bytes = 0u64;
    let mut scanned_sessions = 0usize;
    for record in records {
        if project_path
            .map(|project| record.public.project_path != project)
            .unwrap_or(false)
        {
            continue;
        }
        if scanned_sessions >= MAX_PLAN_SESSION_FILES || plans.len() >= limit {
            truncated = true;
            break;
        }
        let bytes = record
            .public
            .file_size_bytes
            .min(MAX_PLAN_BYTES_PER_SESSION);
        if scanned_bytes.saturating_add(bytes) > MAX_PLAN_TOTAL_SCAN_BYTES {
            truncated = true;
            break;
        }
        scanned_sessions += 1;
        scanned_bytes = scanned_bytes.saturating_add(bytes);
        match latest_plan_from_rollout(&record) {
            Ok((Some(plan), was_truncated)) => {
                truncated |= was_truncated;
                plans.push(plan);
            }
            Ok((None, was_truncated)) => truncated |= was_truncated,
            Err(_) => continue,
        }
    }
    plans.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));

    let mut todos = Vec::new();
    for plan in &plans {
        for item in &plan.items {
            if todos.len() >= MAX_TODO_ITEMS {
                truncated = true;
                break;
            }
            todos.push(CodexTodoItem {
                content: item.content.clone(),
                status: item.status.clone(),
                session_id: plan.session_id.clone(),
                session_title: plan.title.clone(),
                project_path: plan.project_path.clone(),
                updated_at: plan.updated_at.clone(),
            });
        }
    }

    CodexPlansAndTodos {
        plans,
        todos,
        source: "Explicit update_plan calls in Codex rollouts".to_string(),
        source_path: root.to_string_lossy().to_string(),
        source_type: "directory".to_string(),
        truncated,
    }
}

#[tauri::command]
pub async fn get_codex_plans_and_todos(
    limit: Option<usize>,
    project_path: Option<String>,
) -> Result<CodexPlansAndTodos, String> {
    tokio::task::spawn_blocking(move || get_codex_plans_and_todos_sync(limit, project_path))
        .await
        .map_err(|error| format!("Codex plans worker failed: {error}"))?
}

fn get_codex_plans_and_todos_sync(
    limit: Option<usize>,
    project_path: Option<String>,
) -> Result<CodexPlansAndTodos, String> {
    let root = codex_home()?;
    let limit = limit.unwrap_or(100).clamp(1, MAX_PAGE_SIZE);
    Ok(plans_and_todos_at(
        &root,
        limit,
        project_path
            .as_deref()
            .filter(|value| !value.trim().is_empty()),
    ))
}

fn memory_document_spec(document_id: &str) -> Option<(&'static str, &'static str)> {
    match document_id {
        "index" => Some(("MEMORY.md", "Memory Index")),
        "summary" => Some(("memory_summary.md", "Memory Summary")),
        _ => None,
    }
}

fn memory_directory(root: &Path) -> Option<PathBuf> {
    let directory = root.join("memories");
    canonical_root(&directory)
}

fn memory_document_path(root: &Path, document_id: &str) -> Option<PathBuf> {
    let (filename, _) = memory_document_spec(document_id)?;
    let directory = memory_directory(root)?;
    safe_existing_file_below(&directory, &directory.join(filename))
}

fn memory_status_at(root: &Path, project_path: Option<&str>) -> CodexMemoryStatus {
    let source = root.join("memories");
    let mut documents = Vec::new();
    for id in ["index", "summary"] {
        let Some((filename, title)) = memory_document_spec(id) else {
            continue;
        };
        let Some(path) = memory_document_path(root, id) else {
            continue;
        };
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        documents.push(CodexMemoryDocument {
            id: id.to_string(),
            title: title.to_string(),
            relative_path: filename.to_string(),
            size_bytes: metadata.len(),
            modified_at: modified_at(&path),
        });
    }
    CodexMemoryStatus {
        available: !documents.is_empty(),
        scope: "global".to_string(),
        source_path: source.to_string_lossy().to_string(),
        source_type: "directory".to_string(),
        description: "Read-only generated Codex memory. Raw memory inputs and rollout summaries are not exposed."
            .to_string(),
        warning: project_path.map(|_| {
            "Codex generated memory is global; this local format does not provide a project-scoped memory view."
                .to_string()
        }),
        documents,
    }
}

#[tauri::command]
pub async fn get_codex_memory_status(
    project_path: Option<String>,
) -> Result<CodexMemoryStatus, String> {
    tokio::task::spawn_blocking(move || get_codex_memory_status_sync(project_path))
        .await
        .map_err(|error| format!("Codex memory worker failed: {error}"))?
}

fn get_codex_memory_status_sync(project_path: Option<String>) -> Result<CodexMemoryStatus, String> {
    let root = codex_home()?;
    Ok(memory_status_at(&root, project_path.as_deref()))
}

fn read_memory_document_at(
    root: &Path,
    document_id: &str,
) -> Result<CodexMemoryDocumentContent, String> {
    let (_, title) = memory_document_spec(document_id)
        .ok_or_else(|| "Unknown Codex memory document".to_string())?;
    let path = memory_document_path(root, document_id)
        .ok_or_else(|| "Codex memory document was not found".to_string())?;
    let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
    let file = File::open(&path).map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    file.take(MAX_MEMORY_DOCUMENT_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    let content = String::from_utf8(bytes)
        .map_err(|_| "Codex memory document is not valid UTF-8".to_string())?;
    Ok(CodexMemoryDocumentContent {
        id: document_id.to_string(),
        title: title.to_string(),
        content: redact_sensitive_text(&content),
        truncated: metadata.len() > MAX_MEMORY_DOCUMENT_BYTES,
    })
}

#[tauri::command]
pub async fn read_codex_memory_document(
    document_id: String,
) -> Result<CodexMemoryDocumentContent, String> {
    tokio::task::spawn_blocking(move || read_codex_memory_document_sync(document_id))
        .await
        .map_err(|error| format!("Codex memory document worker failed: {error}"))?
}

fn read_codex_memory_document_sync(
    document_id: String,
) -> Result<CodexMemoryDocumentContent, String> {
    let root = codex_home()?;
    read_memory_document_at(&root, &document_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_rollout(root: &Path, id: &str, lines: &[Value]) -> PathBuf {
        let directory = root.join("sessions/2026/09/03");
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join(format!("rollout-{id}.jsonl"));
        let content = lines
            .iter()
            .map(|value| serde_json::to_string(value).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&path, format!("{content}\n")).unwrap();
        path
    }

    fn session(root: &Path, id: &str, path: PathBuf) -> SessionRecord {
        SessionRecord {
            public: CodexTranscriptSession {
                session_id: id.to_string(),
                title: "Fixture".to_string(),
                project_path: root.to_string_lossy().to_string(),
                project_name: "fixture".to_string(),
                created_at: "2026-09-03T00:00:00Z".to_string(),
                updated_at: "2026-09-03T00:01:00Z".to_string(),
                file_size_bytes: fs::metadata(&path).unwrap().len(),
                archived: false,
            },
            rollout_path: path,
        }
    }

    #[test]
    fn serialized_payloads_use_camel_case() {
        let page = CodexPromptHistoryPage {
            entries: Vec::new(),
            total: 0,
            offset: 0,
            limit: 100,
            has_more: false,
            source_path: "/tmp/history.jsonl".to_string(),
            source_type: "file".to_string(),
            truncated: false,
        };
        let value = serde_json::to_value(page).unwrap();
        assert!(value.get("hasMore").is_some());
        assert!(value.get("sourcePath").is_some());
        assert!(value.get("sourceType").is_some());
        assert!(value.get("has_more").is_none());
    }

    #[test]
    fn secret_redaction_covers_tokens_bearer_and_assignments() {
        let text = "sk-live-secret Bearer abc.def.ghi access_token=token-value";
        let redacted = redact_sensitive_text(text);
        assert!(!redacted.contains("sk-live-secret"));
        assert!(!redacted.contains("abc.def.ghi"));
        assert!(!redacted.contains("token-value"));
        assert_eq!(redacted.matches("[REDACTED]").count(), 3);
    }

    #[test]
    fn secret_redaction_covers_conversational_credentials() {
        let text = "password Example7x9 passcode is Test1234 api key Demo5678";
        let redacted = redact_sensitive_text(text);
        assert!(!redacted.contains("Example7x9"));
        assert!(!redacted.contains("Test1234"));
        assert!(!redacted.contains("Demo5678"));
        assert_eq!(redacted.matches("[REDACTED]").count(), 3);
    }

    #[test]
    fn secret_redaction_keeps_common_password_topics_readable() {
        let text = "password manager, password reset, and password policy";
        assert_eq!(redact_sensitive_text(text), text);
    }

    #[test]
    fn transcript_only_returns_visible_user_and_assistant_text() {
        let temp = tempfile::tempdir().unwrap();
        let id = "01a00000-0000-7000-8000-000000000001";
        let path = write_rollout(
            temp.path(),
            id,
            &[
                serde_json::json!({"type":"response_item","payload":{"type":"reasoning","summary":[]},"timestamp":"2026-09-03T00:00:00Z"}),
                serde_json::json!({"type":"response_item","payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":"hidden developer context"}]}}),
                serde_json::json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"hello sk-secret-value"},{"type":"input_image","image_url":"private"}]},"timestamp":"2026-09-03T00:00:01Z"}),
                serde_json::json!({"type":"response_item","payload":{"type":"function_call_output","output":"tool secret"}}),
                serde_json::json!({"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"safe answer"}]},"timestamp":"2026-09-03T00:00:02Z"}),
            ],
        );
        let page = transcript_page_at(session(temp.path(), id, path), 20, 0).unwrap();
        assert_eq!(page.messages.len(), 2);
        assert_eq!(page.messages[0].role, "user");
        assert!(page.messages[0].content.contains("[REDACTED]"));
        assert!(!page.messages[0].content.contains("private"));
        assert_eq!(page.messages[1].content, "safe answer");
        let serialized = serde_json::to_string(&page).unwrap();
        assert!(!serialized.contains("hidden developer context"));
        assert!(!serialized.contains("tool secret"));
    }

    #[test]
    fn corrupt_rollout_lines_are_skipped() {
        let temp = tempfile::tempdir().unwrap();
        let id = "01a00000-0000-7000-8000-000000000002";
        let directory = temp.path().join("sessions/2026/09/03");
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("rollout-corrupt.jsonl");
        fs::write(
            &path,
            "not-json\n{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"kept\"}]}}\n",
        )
        .unwrap();
        let page = transcript_page_at(session(temp.path(), id, path), 20, 0).unwrap();
        assert_eq!(page.messages.len(), 1);
        assert_eq!(page.messages[0].content, "kept");
    }

    #[test]
    fn plans_come_only_from_explicit_update_plan_calls() {
        let temp = tempfile::tempdir().unwrap();
        let id = "01a00000-0000-7000-8000-000000000003";
        let path = write_rollout(
            temp.path(),
            id,
            &[
                serde_json::json!({"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"I may make a plan"}]}}),
                serde_json::json!({"type":"response_item","payload":{"type":"function_call","name":"other_tool","arguments":"{\"plan\":[{\"step\":\"wrong\",\"status\":\"pending\"}]}"}}),
                serde_json::json!({"type":"response_item","payload":{"type":"function_call","name":"update_plan","arguments":"{\"explanation\":\"first\",\"plan\":[{\"step\":\"old\",\"status\":\"pending\"}]}"},"timestamp":"2026-09-03T00:00:01Z"}),
                serde_json::json!({"type":"response_item","payload":{"type":"function_call","name":"update_plan","arguments":"{\"explanation\":\"latest\",\"plan\":[{\"step\":\"ship\",\"status\":\"completed\"}]}"},"timestamp":"2026-09-03T00:00:02Z"}),
            ],
        );
        let record = session(temp.path(), id, path);
        let (plan, _) = latest_plan_from_rollout(&record).unwrap();
        let plan = plan.unwrap();
        assert_eq!(plan.explanation.as_deref(), Some("latest"));
        assert_eq!(
            plan.items,
            vec![CodexPlanItem {
                content: "ship".to_string(),
                status: "completed".to_string(),
            }]
        );
    }

    #[test]
    fn memory_status_allowlists_generated_index_and_summary() {
        let temp = tempfile::tempdir().unwrap();
        let memories = temp.path().join("memories");
        fs::create_dir_all(memories.join("rollout_summaries")).unwrap();
        fs::write(memories.join("MEMORY.md"), "index").unwrap();
        fs::write(memories.join("memory_summary.md"), "summary").unwrap();
        fs::write(memories.join("raw_memories.md"), "raw secret").unwrap();
        fs::write(memories.join("rollout_summaries/session.md"), "tool output").unwrap();
        let status = memory_status_at(temp.path(), Some("/project"));
        assert!(status.available);
        assert_eq!(status.source_type, "directory");
        assert_eq!(status.documents.len(), 2);
        assert!(status.documents.iter().all(|document| {
            document.relative_path == "MEMORY.md" || document.relative_path == "memory_summary.md"
        }));
        assert!(status.warning.is_some());
        assert!(read_memory_document_at(temp.path(), "raw_memories").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn memory_symlink_escape_is_rejected() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let memories = temp.path().join("memories");
        fs::create_dir_all(&memories).unwrap();
        fs::write(outside.path().join("outside.md"), "outside").unwrap();
        symlink(
            outside.path().join("outside.md"),
            memories.join("MEMORY.md"),
        )
        .unwrap();
        let status = memory_status_at(temp.path(), None);
        assert!(!status.available);
        assert!(read_memory_document_at(temp.path(), "index").is_err());
    }
}
