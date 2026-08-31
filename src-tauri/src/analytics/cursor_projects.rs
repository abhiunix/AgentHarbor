//! Cursor Projects — per-project analytics assembled from Cursor's local
//! `state.vscdb` + `workspaceStorage` + `~/.cursor/projects`, since neither
//! Cursor's chat storage nor its ai-tracking DB carry a direct project/repo
//! column. Modeled on `kimi_v2.rs` (standalone commands, `CacheEntry`) and
//! `claude_v2.rs` (single-flight `CorpusCache`).
//!
//! SQLite access rules (verified against a live 1.5GB `state.vscdb`):
//! - Every connection: `READ_ONLY | NO_MUTEX` + `busy_timeout(2000)`.
//! - `composerHeaders` is dual-read: the `composerHeaders` table (if present,
//!   checked via `sqlite_master`) is merged over the `ItemTable` mirror
//!   (`composer.composerHeaders`), table wins on conflicting ids.
//! - `composerData:<id>` and `bubbleId:<id>:<msgId>` are read by exact PK /
//!   half-open PK range only — never a `LIKE` scan of `cursorDiskKV`.
//! - `agentKv:`, `messageRequestContext:`, `checkpointId:` families are never
//!   touched (784MB combined, irrelevant to analytics).

use crate::analytics::cursor::cursor_state_db_path;
use crate::commands::ai_tracking::ScoredCommit;
use chrono::TimeZone;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ── Public types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CursorProjectTotals {
    pub sessions: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub lines_added: i64,
    pub lines_removed: i64,
    pub files_changed: u64,
    pub commit_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorProjectStat {
    pub path: String,
    pub name: String,
    pub sessions: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub lines_added: i64,
    pub lines_removed: i64,
    pub files_changed: u64,
    pub commit_count: u64,
    pub ai_line_pct: f64,
    pub mcp_count: u64,
    pub plan_count: u64,
    pub last_activity: Option<String>,
    pub repo_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorProjectsOverview {
    pub projects: Vec<CursorProjectStat>,
    pub unattributed_commits: u64,
    pub unresolved_sessions: u64,
    pub commit_resolution_pending: bool,
    pub totals: CursorProjectTotals,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorComposerSummary {
    pub composer_id: String,
    pub name: Option<String>,
    pub model: Option<String>,
    pub context_usage_percent: Option<f64>,
    pub lines_added: i64,
    pub lines_removed: i64,
    pub files_changed: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub is_subagent: bool,
    pub is_archived: bool,
    pub resolution_source: String,
    pub last_updated_at: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorProjectCommit {
    pub commit_hash: String,
    pub branch_name: String,
    pub commit_message: Option<String>,
    pub commit_date: Option<String>,
    pub ai_percentage: f64,
    pub lines_added: Option<i64>,
    pub lines_deleted: Option<i64>,
    pub tab_lines_added: Option<i64>,
    pub tab_lines_deleted: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorGeneration {
    pub unix_ms: i64,
    pub generation_uuid: String,
    pub kind: String,
    pub text_description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorMcpEntry {
    pub server_identifier: String,
    pub server_name: Option<String>,
    pub status_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorPlanSummary {
    pub name: String,
    pub file_path: String,
    pub total_todos: usize,
    pub completed_todos: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorProjectDetail {
    pub path: String,
    pub name: String,
    pub sessions: Vec<CursorComposerSummary>,
    pub commits: Vec<CursorProjectCommit>,
    pub model_mix: HashMap<String, u64>,
    pub generations: Vec<CursorGeneration>,
    pub mcps: Vec<CursorMcpEntry>,
    pub plans: Vec<CursorPlanSummary>,
}

// ── Paths ─────────────────────────────────────────────────────────────────

fn workspace_storage_root() -> Option<PathBuf> {
    let home = dirs::home_dir()?;

    #[cfg(target_os = "macos")]
    let p = home.join("Library/Application Support/Cursor/User/workspaceStorage");

    #[cfg(target_os = "linux")]
    let p = home.join(".config/Cursor/User/workspaceStorage");

    #[cfg(target_os = "windows")]
    let p = {
        let appdata = std::env::var("APPDATA").unwrap_or_default();
        PathBuf::from(appdata).join("Cursor/User/workspaceStorage")
    };

    Some(p)
}

fn cursor_projects_root() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".cursor").join("projects"))
}

fn open_state_db() -> Result<Connection, String> {
    let path = cursor_state_db_path()?;
    let conn = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("Failed to open Cursor state.vscdb: {}", e))?;
    let _ = conn.busy_timeout(Duration::from_millis(2000));
    Ok(conn)
}

fn open_workspace_db(hash: &str) -> Option<Connection> {
    let root = workspace_storage_root()?;
    let path = root.join(hash).join("state.vscdb");
    if !path.exists() {
        return None;
    }
    let conn = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    let _ = conn.busy_timeout(Duration::from_millis(2000));
    Some(conn)
}

fn app_data_commit_cache_path() -> PathBuf {
    crate::utils::paths::app_data_dir().join("cursor-commit-repo-cache.json")
}

// ── Small helpers ────────────────────────────────────────────────────────

/// Minimal `file://` URI → filesystem path (percent-decoded). Cursor's own
/// JSON `fsPath` fields are already plain paths and never go through this.
fn file_uri_to_path(uri: &str) -> Option<String> {
    let stripped = uri.strip_prefix("file://")?;
    Some(percent_decode(stripped))
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// `strip leading '/', replace '/' with '-'` — never parsed backward.
fn path_to_slug(path: &str) -> String {
    path.trim_start_matches('/').replace('/', "-")
}

fn is_indexable_slug(slug: &str) -> bool {
    !slug.is_empty() && slug != "empty-window" && !slug.chars().all(|c| c.is_ascii_digit())
}

/// True when `repo` equals or is an ancestor directory of `project_root`.
fn is_ancestor_or_equal(repo: &str, project_root: &str) -> bool {
    project_root == repo || project_root.starts_with(&format!("{}/", repo))
}

fn unix_ms_to_rfc3339(ms: i64) -> Option<String> {
    chrono::Utc.timestamp_millis_opt(ms).single().map(|dt| dt.to_rfc3339())
}

fn read_item_table_string(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row("SELECT value FROM ItemTable WHERE key = ?1", [key], |row| {
        row.get::<_, String>(0)
    })
    .ok()
}

fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
        [name],
        |_| Ok(()),
    )
    .is_ok()
}

// ── composerHeaders (dual-read) ──────────────────────────────────────────

#[derive(Debug, Clone)]
struct ComposerHeader {
    composer_id: String,
    workspace_id: Option<String>,
    /// `value.workspaceIdentifier.uri.fsPath` — a fallback route when the
    /// workspaceStorage hash → folder mapping has been pruned.
    fs_path_hint: Option<String>,
    created_at: Option<i64>,
    last_updated_at: Option<i64>,
    is_archived: bool,
    is_subagent: bool,
    parent_composer_id: Option<String>,
    name: Option<String>,
    lines_added: i64,
    lines_removed: i64,
    files_changed: u64,
    context_usage_percent: Option<f64>,
}

#[allow(clippy::too_many_arguments)]
fn header_from_value(
    composer_id: String,
    value: &serde_json::Value,
    workspace_id_col: Option<String>,
    created_at_col: Option<i64>,
    last_updated_col: Option<i64>,
    is_archived_col: Option<bool>,
    is_subagent_col: Option<bool>,
) -> ComposerHeader {
    let workspace_identifier = value.get("workspaceIdentifier");
    let workspace_id = workspace_id_col.or_else(|| {
        workspace_identifier.and_then(|w| w.get("id")).and_then(|v| v.as_str()).map(String::from)
    });
    let fs_path_hint = workspace_identifier
        .and_then(|w| w.get("uri"))
        .and_then(|u| u.get("fsPath"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let subagent_info = value.get("subagentInfo");
    let is_subagent = is_subagent_col.unwrap_or_else(|| subagent_info.is_some());
    let parent_composer_id = subagent_info
        .and_then(|s| s.get("parentComposerId"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let name = value
        .get("name")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| value.get("subtitle").and_then(|v| v.as_str()).map(String::from));
    let lines_added = value.get("totalLinesAdded").and_then(|v| v.as_i64()).unwrap_or(0);
    let lines_removed = value.get("totalLinesRemoved").and_then(|v| v.as_i64()).unwrap_or(0);
    let files_changed = value.get("filesChangedCount").and_then(|v| v.as_u64()).unwrap_or(0);
    let context_usage_percent = value.get("contextUsagePercent").and_then(|v| v.as_f64());
    let is_archived = is_archived_col
        .unwrap_or_else(|| value.get("isArchived").and_then(|v| v.as_bool()).unwrap_or(false));
    let created_at = created_at_col.or_else(|| value.get("createdAt").and_then(|v| v.as_i64()));
    let last_updated_at =
        last_updated_col.or_else(|| value.get("lastUpdatedAt").and_then(|v| v.as_i64()));

    ComposerHeader {
        composer_id,
        workspace_id,
        fs_path_hint,
        created_at,
        last_updated_at,
        is_archived,
        is_subagent,
        parent_composer_id,
        name,
        lines_added,
        lines_removed,
        files_changed,
        context_usage_percent,
    }
}

/// Dual-read: the `ItemTable` mirror (`composer.composerHeaders`) is loaded
/// first, then the `composerHeaders` table (existence checked via
/// `sqlite_master`) is merged over it — table wins on shared ids. Handles
/// both a fully-migrated Cursor (table present) and an older one (mirror
/// only).
fn read_composer_headers(conn: &Connection) -> HashMap<String, ComposerHeader> {
    let mut map: HashMap<String, ComposerHeader> = HashMap::new();

    if let Some(raw) = read_item_table_string(conn, "composer.composerHeaders") {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(arr) = parsed.get("allComposers").and_then(|v| v.as_array()) {
                for entry in arr {
                    let Some(id) = entry.get("composerId").and_then(|v| v.as_str()) else { continue };
                    let header = header_from_value(id.to_string(), entry, None, None, None, None, None);
                    map.insert(header.composer_id.clone(), header);
                }
            }
        }
    }

    if table_exists(conn, "composerHeaders") {
        if let Ok(mut stmt) = conn.prepare(
            "SELECT composerId, workspaceId, createdAt, lastUpdatedAt, isArchived, isSubagent, value
             FROM composerHeaders",
        ) {
            if let Ok(rows) = stmt.query_map([], |row| {
                let composer_id: String = row.get(0)?;
                let workspace_id: Option<String> = row.get(1)?;
                let created_at: Option<i64> = row.get(2)?;
                let last_updated_at: Option<i64> = row.get(3)?;
                let is_archived: i64 = row.get(4).unwrap_or(0);
                let is_subagent: i64 = row.get(5).unwrap_or(0);
                let value_str: Option<String> = row.get(6)?;
                Ok((
                    composer_id,
                    workspace_id,
                    created_at,
                    last_updated_at,
                    is_archived != 0,
                    is_subagent != 0,
                    value_str,
                ))
            }) {
                for row in rows.flatten() {
                    let (composer_id, workspace_id, created_at, last_updated_at, is_archived, is_subagent, value_str) =
                        row;
                    let value = value_str
                        .as_deref()
                        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                        .unwrap_or(serde_json::Value::Null);
                    let header = header_from_value(
                        composer_id.clone(),
                        &value,
                        workspace_id,
                        created_at,
                        last_updated_at,
                        Some(is_archived),
                        Some(is_subagent),
                    );
                    map.insert(composer_id, header);
                }
            }
        }
    }

    map
}

/// Exact-PK fetch of `composerData:<id>` — never a `LIKE` scan.
/// Key-only PK range scan `[composerData: , composerData;)` — enumerates every
/// composer id ever stored (322 keys live), cheaply. This is the universe
/// extension for legacy chats that predate the `composerHeaders` era: without
/// it only the ~44 recent header rows are visible and the older majority of
/// sessions (and most token history) never resolves.
fn list_composer_data_ids(conn: &Connection) -> Vec<String> {
    let mut ids = Vec::new();
    let Ok(mut stmt) = conn
        .prepare("SELECT key FROM cursorDiskKV WHERE key >= 'composerData:' AND key < 'composerData;'")
    else {
        return ids;
    };
    if let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
        for key in rows.flatten() {
            if let Some(id) = key.strip_prefix("composerData:") {
                if !id.is_empty() {
                    ids.push(id.to_string());
                }
            }
        }
    }
    ids
}

/// Synthesize a `ComposerHeader` for a legacy composer that has no
/// `composerHeaders` row, from its `composerData:` JSON.
fn header_from_composer_data(composer_id: &str, data: &serde_json::Value) -> ComposerHeader {
    ComposerHeader {
        composer_id: composer_id.to_string(),
        workspace_id: None,
        fs_path_hint: None,
        created_at: data.get("createdAt").and_then(|v| v.as_i64()),
        last_updated_at: data.get("lastUpdatedAt").and_then(|v| v.as_i64()),
        is_archived: false,
        is_subagent: false,
        parent_composer_id: None,
        name: data.get("name").and_then(|v| v.as_str()).map(String::from),
        lines_added: data.get("totalLinesAdded").and_then(|v| v.as_i64()).unwrap_or(0),
        lines_removed: data.get("totalLinesRemoved").and_then(|v| v.as_i64()).unwrap_or(0),
        files_changed: data.get("filesChangedCount").and_then(|v| v.as_u64()).unwrap_or(0),
        context_usage_percent: data.get("contextUsagePercent").and_then(|v| v.as_f64()),
    }
}

fn read_composer_data(conn: &Connection, composer_id: &str) -> Option<serde_json::Value> {
    let key = format!("composerData:{}", composer_id);
    let raw: String = conn
        .query_row("SELECT value FROM cursorDiskKV WHERE key = ?1", [&key], |row| row.get(0))
        .ok()?;
    serde_json::from_str(&raw).ok()
}

/// Half-open PK range `[bubbleId:<id>: , bubbleId:<id>;)` so a composerId
/// that's a text-prefix of another id can never cross-match (`:` < `;` in
/// ASCII bounds the scan to exactly this composer's bubbles).
fn bubble_token_sum(conn: &Connection, composer_id: &str) -> (u64, u64) {
    let lower = format!("bubbleId:{}:", composer_id);
    let upper = format!("bubbleId:{};", composer_id);
    let Ok(mut stmt) = conn.prepare(
        "SELECT COALESCE(SUM(json_extract(value,'$.tokenCount.inputTokens')),0),
                COALESCE(SUM(json_extract(value,'$.tokenCount.outputTokens')),0)
         FROM cursorDiskKV WHERE key >= ?1 AND key < ?2",
    ) else {
        return (0, 0);
    };
    stmt.query_row([&lower, &upper], |row| {
        let input: f64 = row.get(0).unwrap_or(0.0);
        let output: f64 = row.get(1).unwrap_or(0.0);
        Ok((input as u64, output as u64))
    })
    .unwrap_or((0, 0))
}

// Per-composer token memo, keyed by (composerId, lastUpdatedAt) so a warm
// refresh only rescans composers whose bubbles actually changed.
lazy_static::lazy_static! {
    static ref TOKEN_MEMO: Mutex<HashMap<(String, i64), (u64, u64)>> = Mutex::new(HashMap::new());
}

fn bubble_tokens_memoized(conn: &Connection, composer_id: &str, last_updated_at: i64) -> (u64, u64) {
    let key = (composer_id.to_string(), last_updated_at);
    if let Ok(memo) = TOKEN_MEMO.lock() {
        if let Some(v) = memo.get(&key) {
            return *v;
        }
    }
    let computed = bubble_token_sum(conn, composer_id);
    if let Ok(mut memo) = TOKEN_MEMO.lock() {
        memo.insert(key, computed);
    }
    computed
}

// ── workspaceStorage map (hash → project path) ───────────────────────────

fn build_workspace_map() -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Some(root) = workspace_storage_root() else { return map };
    let Ok(entries) = std::fs::read_dir(&root) else { return map };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let hash = entry.file_name().to_string_lossy().to_string();
        let Ok(text) = std::fs::read_to_string(path.join("workspace.json")) else { continue };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
        if let Some(folder) = json.get("folder").and_then(|f| f.as_str()) {
            if let Some(p) = file_uri_to_path(folder) {
                map.insert(hash, p);
            }
        }
    }
    map
}

// ── glass.* (Cursor's own project registry) ──────────────────────────────

/// `projectId -> path`.
fn glass_project_paths(conn: &Connection) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Some(raw) = read_item_table_string(conn, "glass.localAgentProjects.v1") else { return map };
    let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&raw) else { return map };
    for p in arr {
        let Some(id) = p.get("id").and_then(|v| v.as_str()) else { continue };
        let Some(fs_path) = p
            .get("workspace")
            .and_then(|w| w.get("uri"))
            .and_then(|u| u.get("fsPath"))
            .and_then(|v| v.as_str())
        else {
            continue;
        };
        map.insert(id.to_string(), fs_path.to_string());
    }
    map
}

/// `composerId -> projectId`.
fn glass_membership(conn: &Connection) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Some(raw) = read_item_table_string(conn, "glass.localAgentProjectMembership.v1") else {
        return map;
    };
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) else { return map };
    if let Some(obj) = parsed.as_object() {
        for (composer_id, project_id) in obj {
            if let Some(pid) = project_id.as_str() {
                map.insert(composer_id.clone(), pid.to_string());
            }
        }
    }
    map
}

/// `repositoryTracker.paths` candidate repo roots (best-effort; a workspace
/// may also carry a `cachedSelectedRemote.url` but no machine we've seen
/// populates that key, so it isn't scanned for specifically).
fn repository_tracker_paths(conn: &Connection) -> Vec<String> {
    let Some(raw) = read_item_table_string(conn, "repositoryTracker.paths") else { return vec![] };
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) else { return vec![] };
    let Some(obj) = parsed.as_object() else { return vec![] };
    obj.values()
        .filter_map(|v| v.get("localPath").and_then(|p| p.as_str()))
        .filter_map(file_uri_to_path)
        .collect()
}

/// `composer.planRegistry` → `(planId, fsPath)`.
fn plan_registry_entries(conn: &Connection) -> Vec<(String, String)> {
    let Some(raw) = read_item_table_string(conn, "composer.planRegistry") else { return vec![] };
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) else { return vec![] };
    let Some(obj) = parsed.as_object() else { return vec![] };
    obj.iter()
        .filter_map(|(id, v)| {
            let fs_path = v.get("uri").and_then(|u| u.get("fsPath")).and_then(|p| p.as_str())?;
            Some((id.clone(), fs_path.to_string()))
        })
        .collect()
}

// ── ~/.cursor/projects/<slug> forward-match ──────────────────────────────

/// For every known project root, computes its slug and — if a
/// `~/.cursor/projects/<slug>/agent-transcripts/` directory exists — resolves
/// every composerId found there (each subdirectory is named by composerId)
/// to that root. Never parses a slug back into a path.
fn slug_forward_match_under(
    projects_root: &Path,
    headers: &HashMap<String, ComposerHeader>,
    resolved: &mut HashMap<String, (String, String)>,
    known_roots: &HashSet<String>,
) {
    for root in known_roots {
        let slug = path_to_slug(root);
        if !is_indexable_slug(&slug) {
            continue;
        }
        let dir = projects_root.join(&slug).join("agent-transcripts");
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let composer_id = entry.file_name().to_string_lossy().to_string();
            if headers.contains_key(&composer_id) && !resolved.contains_key(&composer_id) {
                resolved.insert(composer_id, (root.clone(), "cursor-projects-slug".to_string()));
            }
        }
    }
}

fn slug_forward_match(
    headers: &HashMap<String, ComposerHeader>,
    resolved: &mut HashMap<String, (String, String)>,
    known_roots: &HashSet<String>,
) {
    let Some(root) = cursor_projects_root() else { return };
    slug_forward_match_under(&root, headers, resolved, known_roots);
}

// ── originalFileStates longest-prefix fallback ───────────────────────────

fn longest_prefix_root<'a>(file_path: &str, known_roots: &'a HashSet<String>) -> Option<&'a str> {
    known_roots
        .iter()
        .filter(|root| file_path == root.as_str() || file_path.starts_with(&format!("{}/", root)))
        .max_by_key(|root| root.len())
        .map(|s| s.as_str())
}

fn resolve_via_original_file_states(
    conn: &Connection,
    composer_id: &str,
    known_roots: &HashSet<String>,
) -> Option<String> {
    let data = read_composer_data(conn, composer_id)?;
    let files = data.get("originalFileStates")?.as_object()?;
    let mut best: Option<(usize, String)> = None;
    for key in files.keys() {
        let Some(path) = file_uri_to_path(key) else { continue };
        if let Some(root) = longest_prefix_root(&path, known_roots) {
            let len = root.len();
            if best.as_ref().map(|(l, _)| len > *l).unwrap_or(true) {
                best = Some((len, root.to_string()));
            }
        }
    }
    best.map(|(_, r)| r)
}

// ── Corpus (single-flight, TTL-cached) ───────────────────────────────────

struct ProjectsCorpus {
    headers: HashMap<String, ComposerHeader>,
    /// composerId -> (project path, resolution source).
    resolved: HashMap<String, (String, String)>,
    known_roots: HashSet<String>,
    /// workspaceStorage hash -> project path (kept for the detail view's
    /// per-workspace `aiService.generations` lookup).
    workspace_map: HashMap<String, String>,
    plan_registry: Vec<(String, String)>,
}

fn build_corpus() -> ProjectsCorpus {
    let conn = open_state_db().ok();
    let mut headers = conn.as_ref().map(read_composer_headers).unwrap_or_default();
    // Extend the universe with legacy composerData-only chats (pre-headers
    // era). Their headers are synthesized from the composerData JSON so the
    // later resolution routes (glass membership, slug match, file states)
    // get a chance at them.
    if let Some(conn) = conn.as_ref() {
        for id in list_composer_data_ids(conn) {
            if !headers.contains_key(&id) {
                if let Some(data) = read_composer_data(conn, &id) {
                    headers.insert(id.clone(), header_from_composer_data(&id, &data));
                }
            }
        }
    }
    let workspace_map = build_workspace_map();

    let glass_membership_map = conn.as_ref().map(glass_membership).unwrap_or_default();
    let glass_project_paths_map = conn.as_ref().map(glass_project_paths).unwrap_or_default();
    let repo_tracker_paths = conn.as_ref().map(repository_tracker_paths).unwrap_or_default();
    let plan_registry = conn.as_ref().map(plan_registry_entries).unwrap_or_default();

    let mut known_roots: HashSet<String> = HashSet::new();
    known_roots.extend(workspace_map.values().cloned());
    known_roots.extend(glass_project_paths_map.values().cloned());
    known_roots.extend(repo_tracker_paths.iter().cloned());
    for h in headers.values() {
        if let Some(p) = &h.fs_path_hint {
            known_roots.insert(p.clone());
        }
    }

    let mut resolved: HashMap<String, (String, String)> = HashMap::new();
    for (id, h) in &headers {
        if let Some(wid) = &h.workspace_id {
            if let Some(p) = workspace_map.get(wid) {
                resolved.insert(id.clone(), (p.clone(), "workspace-storage".to_string()));
                continue;
            }
        }
        if let Some(p) = &h.fs_path_hint {
            resolved.insert(id.clone(), (p.clone(), "workspace-identifier-fspath".to_string()));
            continue;
        }
        if let Some(pid) = glass_membership_map.get(id) {
            if let Some(p) = glass_project_paths_map.get(pid) {
                resolved.insert(id.clone(), (p.clone(), "glass-membership".to_string()));
                continue;
            }
        }
    }

    slug_forward_match(&headers, &mut resolved, &known_roots);

    if let Some(conn) = conn.as_ref() {
        let still_unresolved: Vec<String> =
            headers.keys().filter(|id| !resolved.contains_key(*id)).cloned().collect();
        for id in still_unresolved {
            if let Some(path) = resolve_via_original_file_states(conn, &id, &known_roots) {
                resolved.insert(id, (path, "original-file-states".to_string()));
            }
        }
    }

    // Subagents inherit their parent's resolution when they don't resolve
    // directly (their own workspaceId/fsPath usually matches anyway, but
    // this covers headers where it doesn't).
    let subagent_fallbacks: Vec<(String, String)> = headers
        .values()
        .filter(|h| h.is_subagent && !resolved.contains_key(&h.composer_id))
        .filter_map(|h| h.parent_composer_id.clone().map(|p| (h.composer_id.clone(), p)))
        .collect();
    for (id, parent) in subagent_fallbacks {
        if let Some((path, _)) = resolved.get(&parent).cloned() {
            resolved.insert(id, (path, "subagent-of-parent".to_string()));
        }
    }

    ProjectsCorpus { headers, resolved, known_roots, workspace_map, plan_registry }
}

const CORPUS_TTL_SECS: u64 = 300;
const CORPUS_FLIGHT_COLLAPSE_SECS: u64 = 3;

struct CorpusCacheEntry {
    corpus: Arc<ProjectsCorpus>,
    fetched_at: Instant,
}

struct ProjectsCorpusCache {
    state: Mutex<Option<CorpusCacheEntry>>,
    flight: Mutex<()>,
}

impl ProjectsCorpusCache {
    fn new() -> Self {
        Self { state: Mutex::new(None), flight: Mutex::new(()) }
    }

    fn hit(&self, ttl_secs: u64) -> Option<Arc<ProjectsCorpus>> {
        let guard = self.state.lock().ok()?;
        let entry = guard.as_ref()?;
        if entry.fetched_at.elapsed() < Duration::from_secs(ttl_secs) {
            Some(entry.corpus.clone())
        } else {
            None
        }
    }

    fn get(&self, force_refresh: bool, loader: impl FnOnce() -> ProjectsCorpus) -> Arc<ProjectsCorpus> {
        if !force_refresh {
            if let Some(hit) = self.hit(CORPUS_TTL_SECS) {
                return hit;
            }
        }
        let _flight = self.flight.lock();
        let recheck_ttl = if force_refresh { CORPUS_FLIGHT_COLLAPSE_SECS } else { CORPUS_TTL_SECS };
        if let Some(hit) = self.hit(recheck_ttl) {
            return hit;
        }
        let corpus = Arc::new(loader());
        if let Ok(mut guard) = self.state.lock() {
            *guard = Some(CorpusCacheEntry { corpus: corpus.clone(), fetched_at: Instant::now() });
        }
        corpus
    }
}

lazy_static::lazy_static! {
    static ref PROJECTS_CORPUS_CACHE: ProjectsCorpusCache = ProjectsCorpusCache::new();
}

// ── Commit → repo resolution (git, persistent cache, two-phase) ─────────

pub(crate) trait GitRunner: Send + Sync {
    fn rev_list_all(&self, repo_path: &Path, max_count: usize) -> Result<Vec<String>, String>;
}

struct RealGitRunner;

impl GitRunner for RealGitRunner {
    fn rev_list_all(&self, repo_path: &Path, max_count: usize) -> Result<Vec<String>, String> {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .arg("rev-list")
            .arg("--all")
            .arg(format!("--max-count={}", max_count))
            .output()
            .map_err(|e| format!("git spawn failed: {}", e))?;
        if !output.status.success() {
            return Err(format!("git rev-list failed: {}", String::from_utf8_lossy(&output.stderr)));
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect())
    }
}

const GIT_MAX_COMMITS_PER_REPO: usize = 20_000;
const GIT_MAX_CONCURRENT: usize = 4;

/// Scans `repos` (≤`GIT_MAX_CONCURRENT` concurrent `git` children) for any of
/// `wanted` commit hashes, returning `hash -> repoPath` for whatever was
/// found. Test-injectable via `GitRunner`.
fn scan_repos_for_commits(
    runner: Arc<dyn GitRunner>,
    repos: Vec<PathBuf>,
    wanted: HashSet<String>,
) -> HashMap<String, String> {
    let queue = Arc::new(Mutex::new(repos));
    let wanted = Arc::new(wanted);
    let found: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));
    let n_workers = GIT_MAX_CONCURRENT;

    let mut handles = Vec::new();
    for _ in 0..n_workers {
        let queue = queue.clone();
        let wanted = wanted.clone();
        let found = found.clone();
        let runner = runner.clone();
        handles.push(std::thread::spawn(move || loop {
            let repo = {
                let mut q = queue.lock().unwrap();
                q.pop()
            };
            let Some(repo) = repo else { break };
            if let Ok(hashes) = runner.rev_list_all(&repo, GIT_MAX_COMMITS_PER_REPO) {
                let mut f = found.lock().unwrap();
                for h in hashes {
                    if wanted.contains(&h) {
                        f.entry(h).or_insert_with(|| repo.to_string_lossy().to_string());
                    }
                }
            }
        }));
    }
    for h in handles {
        let _ = h.join();
    }

    Arc::try_unwrap(found).map(|m| m.into_inner().unwrap()).unwrap_or_default()
}

/// Resolves whatever in `wanted` is missing from `cache` by scanning
/// `candidate_repos`, merging fresh results into `cache`. A fully-cached
/// `wanted` set makes zero `GitRunner` calls.
///
/// Anything still unresolved after the scan (no candidate repo's history
/// contains it — commonly a commit whose repo has since moved or was never
/// cloned locally) is negative-cached as `""` so it isn't rescanned on every
/// future call; `attribute_commits` treats an empty repo path the same as no
/// repo at all (unattributed).
fn resolve_commits_with_cache(
    runner: &Arc<dyn GitRunner>,
    candidate_repos: &[PathBuf],
    wanted: &HashSet<String>,
    cache: &mut HashMap<String, String>,
) {
    let missing: HashSet<String> =
        wanted.iter().filter(|h| !cache.contains_key(h.as_str())).cloned().collect();
    if missing.is_empty() {
        return;
    }
    let found = scan_repos_for_commits(runner.clone(), candidate_repos.to_vec(), missing.clone());
    for h in &missing {
        cache.entry(h.clone()).or_default();
    }
    cache.extend(found);
}

fn load_commit_repo_cache() -> HashMap<String, String> {
    std::fs::read_to_string(app_data_commit_cache_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_commit_repo_cache(map: &HashMap<String, String>) {
    if let Ok(json) = serde_json::to_string(map) {
        let _ = crate::utils::paths::atomic_write_str(&app_data_commit_cache_path(), &json);
    }
}

fn git_available() -> bool {
    std::process::Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

lazy_static::lazy_static! {
    static ref COMMIT_REPO_CACHE: Mutex<HashMap<String, String>> = Mutex::new(load_commit_repo_cache());
    static ref COMMIT_SCAN_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
    static ref GIT_AVAILABLE: bool = git_available();
}

/// Kicks off a background commit→repo scan when there's anything missing
/// from the persistent cache. Returns whether a scan is (now, or already)
/// in flight — the overview's `commit_resolution_pending` flag.
fn maybe_start_commit_scan(candidate_roots: &HashSet<String>, wanted: HashSet<String>) -> bool {
    if wanted.is_empty() || !*GIT_AVAILABLE {
        return false;
    }
    {
        let cache = COMMIT_REPO_CACHE.lock().unwrap();
        if wanted.iter().all(|h| cache.contains_key(h)) {
            return false;
        }
    }
    if COMMIT_SCAN_IN_PROGRESS.swap(true, Ordering::SeqCst) {
        return true;
    }
    let candidate_repos: Vec<PathBuf> = candidate_roots.iter().map(PathBuf::from).collect();
    std::thread::spawn(move || {
        let runner: Arc<dyn GitRunner> = Arc::new(RealGitRunner);
        let mut cache = COMMIT_REPO_CACHE.lock().unwrap().clone();
        resolve_commits_with_cache(&runner, &candidate_repos, &wanted, &mut cache);
        if let Ok(mut guard) = COMMIT_REPO_CACHE.lock() {
            *guard = cache.clone();
        }
        save_commit_repo_cache(&cache);
        COMMIT_SCAN_IN_PROGRESS.store(false, Ordering::SeqCst);
    });
    true
}

/// Attributes `commits` to whichever of `project_paths` its resolved repo is
/// an ancestor of. Returns `project path -> (commit_count, ai_pct_sum, repo)`
/// plus the count of commits that resolved to no repo, or to a repo outside
/// every known project.
fn attribute_commits(
    commits: &[ScoredCommit],
    commit_repo_cache: &HashMap<String, String>,
    project_paths: &[String],
) -> (HashMap<String, (u64, f64, String)>, u64) {
    let mut per_project: HashMap<String, (u64, f64, String)> = HashMap::new();
    let mut unattributed = 0u64;
    for c in commits {
        // An empty string is the negative-cache sentinel (scanned, not
        // found in any candidate repo) — treat exactly like "no repo".
        let Some(repo) = commit_repo_cache.get(&c.commit_hash).filter(|r| !r.is_empty()) else {
            unattributed += 1;
            continue;
        };
        match project_paths.iter().find(|p| is_ancestor_or_equal(repo, p)) {
            Some(p) => {
                let entry = per_project.entry(p.clone()).or_insert((0, 0.0, repo.clone()));
                entry.0 += 1;
                entry.1 += c.ai_percentage;
            }
            None => unattributed += 1,
        }
    }
    (per_project, unattributed)
}

// ── Project extras: MCPs / plans ─────────────────────────────────────────

fn count_mcp_dirs(slug: &str) -> u64 {
    let Some(root) = cursor_projects_root() else { return 0 };
    std::fs::read_dir(root.join(slug).join("mcps"))
        .map(|rd| rd.flatten().filter(|e| e.path().is_dir()).count() as u64)
        .unwrap_or(0)
}

fn read_mcp_roster(slug: &str) -> Vec<CursorMcpEntry> {
    let Some(root) = cursor_projects_root() else { return vec![] };
    let mcps_dir = root.join(slug).join("mcps");
    let Ok(entries) = std::fs::read_dir(&mcps_dir) else { return vec![] };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let dir_name = entry.file_name().to_string_lossy().to_string();
        let meta = std::fs::read_to_string(path.join("SERVER_METADATA.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
        let server_identifier = meta
            .as_ref()
            .and_then(|m| m.get("serverIdentifier"))
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| dir_name.clone());
        let server_name =
            meta.as_ref().and_then(|m| m.get("serverName")).and_then(|v| v.as_str()).map(String::from);
        let status_summary = std::fs::read_to_string(path.join("STATUS.md"))
            .ok()
            .and_then(|s| s.lines().next().map(|l| l.trim().to_string()));
        out.push(CursorMcpEntry { server_identifier, server_name, status_summary });
    }
    out.sort_by(|a, b| a.server_identifier.cmp(&b.server_identifier));
    out
}

fn read_plan_summary(fs_path: &str) -> Option<CursorPlanSummary> {
    let content = std::fs::read_to_string(fs_path).ok()?;
    let fm = crate::commands::plans::parse_cursor_frontmatter(&content)?;
    let total_todos = fm.todos.len();
    let completed_todos = fm.todos.iter().filter(|t| t.status == "completed").count();
    let name = if fm.name.is_empty() {
        Path::new(fs_path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
    } else {
        fm.name
    };
    Some(CursorPlanSummary { name, file_path: fs_path.to_string(), total_todos, completed_todos })
}

fn read_generations(hash: &str) -> Vec<CursorGeneration> {
    let Some(conn) = open_workspace_db(hash) else { return vec![] };
    let Some(raw) = read_item_table_string(&conn, "aiService.generations") else { return vec![] };
    let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&raw) else { return vec![] };
    let mut out: Vec<CursorGeneration> = arr
        .iter()
        .filter_map(|g| {
            Some(CursorGeneration {
                unix_ms: g.get("unixMs")?.as_i64()?,
                generation_uuid: g.get("generationUUID")?.as_str()?.to_string(),
                kind: g.get("type").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                text_description: g.get("textDescription").and_then(|v| v.as_str()).map(String::from),
            })
        })
        .collect();
    out.sort_by(|a, b| b.unix_ms.cmp(&a.unix_ms));
    out.truncate(200);
    out
}

// ── Sorting (pure, unit-tested) ──────────────────────────────────────────

fn sort_projects(projects: &mut [CursorProjectStat]) {
    projects.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));
}

fn sort_commits(commits: &mut [CursorProjectCommit]) {
    commits.sort_by(|a, b| b.commit_date.cmp(&a.commit_date));
}

// ── Overview build ────────────────────────────────────────────────────────

#[derive(Default)]
struct ProjectAgg {
    sessions: u64,
    input_tokens: u64,
    output_tokens: u64,
    lines_added: i64,
    lines_removed: i64,
    files_changed: u64,
    last_activity: Option<i64>,
}

fn resolve_target<'a>(
    id: &str,
    h: &ComposerHeader,
    resolved: &'a HashMap<String, (String, String)>,
) -> Option<&'a (String, String)> {
    if h.is_subagent {
        resolved
            .get(id)
            .or_else(|| h.parent_composer_id.as_ref().and_then(|p| resolved.get(p)))
    } else {
        resolved.get(id)
    }
}

fn build_overview(force_refresh: bool) -> CursorProjectsOverview {
    let corpus = PROJECTS_CORPUS_CACHE.get(force_refresh, build_corpus);
    let conn = open_state_db().ok();

    let mut agg: HashMap<String, ProjectAgg> = HashMap::new();
    let mut unresolved_sessions: u64 = 0;

    for (id, h) in &corpus.headers {
        match resolve_target(id, h, &corpus.resolved) {
            Some((path, _src)) => {
                let (in_tok, out_tok) = conn
                    .as_ref()
                    .map(|c| bubble_tokens_memoized(c, id, h.last_updated_at.unwrap_or(0)))
                    .unwrap_or((0, 0));
                let a = agg.entry(path.clone()).or_default();
                a.input_tokens += in_tok;
                a.output_tokens += out_tok;
                a.lines_added += h.lines_added;
                a.lines_removed += h.lines_removed;
                a.files_changed += h.files_changed;
                if !h.is_subagent {
                    a.sessions += 1;
                }
                if let Some(ts) = h.last_updated_at.or(h.created_at) {
                    a.last_activity = Some(a.last_activity.map_or(ts, |cur| cur.max(ts)));
                }
            }
            None => {
                if !h.is_subagent {
                    unresolved_sessions += 1;
                }
            }
        }
    }

    let commits = crate::commands::ai_tracking::get_ai_commit_scores(Some(20_000), Some(0)).unwrap_or_default();
    let wanted_hashes: HashSet<String> = commits.iter().map(|c| c.commit_hash.clone()).collect();
    let pending = maybe_start_commit_scan(&corpus.known_roots, wanted_hashes);
    let commit_repo_cache = COMMIT_REPO_CACHE.lock().map(|g| g.clone()).unwrap_or_default();

    let project_paths: Vec<String> = agg.keys().cloned().collect();
    let (commit_stats, unattributed_commits) = attribute_commits(&commits, &commit_repo_cache, &project_paths);

    let mut projects: Vec<CursorProjectStat> = agg
        .into_iter()
        .map(|(path, a)| {
            let name = Path::new(&path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.clone());
            let slug = path_to_slug(&path);
            let mcp_count = count_mcp_dirs(&slug);
            let plan_count = corpus
                .plan_registry
                .iter()
                .filter(|(_, fs_path)| fs_path.starts_with(&format!("{}/", path)))
                .count() as u64;
            let (commit_count, ai_pct_sum, repo_path) =
                commit_stats.get(&path).cloned().unwrap_or((0, 0.0, String::new()));
            let ai_line_pct = if commit_count > 0 { ai_pct_sum / commit_count as f64 } else { 0.0 };
            CursorProjectStat {
                path: path.clone(),
                name,
                sessions: a.sessions,
                input_tokens: a.input_tokens,
                output_tokens: a.output_tokens,
                lines_added: a.lines_added,
                lines_removed: a.lines_removed,
                files_changed: a.files_changed,
                commit_count,
                ai_line_pct,
                mcp_count,
                plan_count,
                last_activity: a.last_activity.and_then(unix_ms_to_rfc3339),
                repo_path: if repo_path.is_empty() { None } else { Some(repo_path) },
            }
        })
        .collect();
    sort_projects(&mut projects);

    let totals = projects.iter().fold(CursorProjectTotals::default(), |mut t, p| {
        t.sessions += p.sessions;
        t.input_tokens += p.input_tokens;
        t.output_tokens += p.output_tokens;
        t.lines_added += p.lines_added;
        t.lines_removed += p.lines_removed;
        t.files_changed += p.files_changed;
        t.commit_count += p.commit_count;
        t
    });

    CursorProjectsOverview {
        projects,
        unattributed_commits,
        unresolved_sessions,
        commit_resolution_pending: pending,
        totals,
    }
}

// ── Detail build ──────────────────────────────────────────────────────────

fn build_detail(project_path: &str) -> CursorProjectDetail {
    let corpus = PROJECTS_CORPUS_CACHE.get(false, build_corpus);
    let conn = open_state_db().ok();

    let mut sessions: Vec<CursorComposerSummary> = Vec::new();
    let mut model_mix: HashMap<String, u64> = HashMap::new();

    for (id, h) in &corpus.headers {
        let Some((path, source)) = resolve_target(id, h, &corpus.resolved) else { continue };
        if path != project_path {
            continue;
        }

        let (in_tok, out_tok) = conn
            .as_ref()
            .map(|c| bubble_tokens_memoized(c, id, h.last_updated_at.unwrap_or(0)))
            .unwrap_or((0, 0));
        let model = conn.as_ref().and_then(|c| read_composer_data(c, id)).and_then(|d| {
            d.get("modelConfig").and_then(|m| m.get("modelName")).and_then(|v| v.as_str()).map(String::from)
        });
        if let Some(m) = &model {
            *model_mix.entry(m.clone()).or_insert(0) += 1;
        }

        sessions.push(CursorComposerSummary {
            composer_id: id.clone(),
            name: h.name.clone(),
            model,
            context_usage_percent: h.context_usage_percent,
            lines_added: h.lines_added,
            lines_removed: h.lines_removed,
            files_changed: h.files_changed,
            input_tokens: in_tok,
            output_tokens: out_tok,
            is_subagent: h.is_subagent,
            is_archived: h.is_archived,
            resolution_source: source.clone(),
            last_updated_at: h.last_updated_at.and_then(unix_ms_to_rfc3339),
            created_at: h.created_at.and_then(unix_ms_to_rfc3339),
        });
    }
    sessions.sort_by(|a, b| b.last_updated_at.cmp(&a.last_updated_at));

    let commit_repo_cache = COMMIT_REPO_CACHE.lock().map(|g| g.clone()).unwrap_or_default();
    let all_commits = crate::commands::ai_tracking::get_ai_commit_scores(Some(20_000), Some(0)).unwrap_or_default();
    let mut commits: Vec<CursorProjectCommit> = all_commits
        .into_iter()
        .filter(|c| {
            commit_repo_cache
                .get(&c.commit_hash)
                .filter(|r| !r.is_empty())
                .map(|repo| is_ancestor_or_equal(repo, project_path))
                .unwrap_or(false)
        })
        .map(|c| CursorProjectCommit {
            commit_hash: c.commit_hash,
            branch_name: c.branch_name,
            commit_message: c.commit_message,
            commit_date: c.commit_date,
            ai_percentage: c.ai_percentage,
            lines_added: c.lines_added,
            lines_deleted: c.lines_deleted,
            tab_lines_added: c.tab_lines_added,
            tab_lines_deleted: c.tab_lines_deleted,
        })
        .collect();
    sort_commits(&mut commits);

    let generations = corpus
        .workspace_map
        .iter()
        .find(|(_, p)| p.as_str() == project_path)
        .map(|(hash, _)| read_generations(hash))
        .unwrap_or_default();

    let slug = path_to_slug(project_path);
    let mcps = read_mcp_roster(&slug);

    let plans: Vec<CursorPlanSummary> = corpus
        .plan_registry
        .iter()
        .filter(|(_, fs_path)| fs_path.starts_with(&format!("{}/", project_path)))
        .filter_map(|(_, fs_path)| read_plan_summary(fs_path))
        .collect();

    CursorProjectDetail {
        path: project_path.to_string(),
        name: Path::new(project_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| project_path.to_string()),
        sessions,
        commits,
        model_mix,
        generations,
        mcps,
        plans,
    }
}

// ── Tauri commands ────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_cursor_projects_overview(
    force_refresh: Option<bool>,
) -> Result<CursorProjectsOverview, String> {
    let force = force_refresh.unwrap_or(false);
    tokio::task::spawn_blocking(move || build_overview(force))
        .await
        .map_err(|e| format!("Task error: {}", e))
}

#[tauri::command]
pub async fn get_cursor_project_detail(project_path: String) -> Result<CursorProjectDetail, String> {
    tokio::task::spawn_blocking(move || build_detail(&project_path))
        .await
        .map_err(|e| format!("Task error: {}", e))
}

/// Mirrors `start_kimi_in_project` / `open_project_in_cursor`: `open -a Cursor
/// <path>` on macOS, `cursor <path>` (or its `.cmd` shim on Windows) elsewhere.
#[tauri::command]
pub fn start_cursor_in_project(project_path: String) -> Result<(), String> {
    crate::utils::platform::open_in_ide("cursor", &project_path)
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    fn fixture_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB);
             CREATE TABLE cursorDiskKV (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB);
             CREATE TABLE composerHeaders (
                composerId TEXT PRIMARY KEY, workspaceId TEXT, createdAt INTEGER,
                lastUpdatedAt INTEGER, isArchived INTEGER, isSubagent INTEGER,
                recency INTEGER, checkpointAt INTEGER, value TEXT
             );",
        )
        .unwrap();
        conn
    }

    // ── Dual-read + resolution priority ──────────────────────────────────

    #[test]
    fn dual_read_merges_mirror_and_table_with_table_winning() {
        let conn = fixture_conn();

        // ItemTable mirror carries two composers…
        let mirror = serde_json::json!({
            "allComposers": [
                {"composerId": "mirror-only", "workspaceIdentifier": {"id": "ws-a"},
                 "lastUpdatedAt": 100, "totalLinesAdded": 1},
                {"composerId": "both", "workspaceIdentifier": {"id": "ws-stale"},
                 "lastUpdatedAt": 1, "totalLinesAdded": 1},
            ]
        });
        conn.execute(
            "INSERT INTO ItemTable (key, value) VALUES ('composer.composerHeaders', ?1)",
            [mirror.to_string()],
        )
        .unwrap();

        // …but the table has a fresher row for "both" — table must win.
        let table_value = serde_json::json!({"totalLinesAdded": 42}).to_string();
        conn.execute(
            "INSERT INTO composerHeaders (composerId, workspaceId, createdAt, lastUpdatedAt, isArchived, isSubagent, value)
             VALUES ('both', 'ws-fresh', 5, 200, 0, 0, ?1)",
            [table_value],
        )
        .unwrap();

        let headers = read_composer_headers(&conn);
        assert_eq!(headers.len(), 2);
        assert!(headers.contains_key("mirror-only"));
        let both = &headers["both"];
        assert_eq!(both.workspace_id.as_deref(), Some("ws-fresh"), "table row must win over the mirror");
        assert_eq!(both.last_updated_at, Some(200));
        assert_eq!(both.lines_added, 42);
    }

    #[test]
    fn resolution_priority_workspace_storage_before_fspath_hint_before_glass() {
        // A composer with BOTH a workspaceId that maps via workspaceStorage
        // AND a fsPath hint AND a glass membership entry must resolve via
        // workspace-storage (the highest-priority route).
        let mut headers = HashMap::new();
        headers.insert(
            "c1".to_string(),
            ComposerHeader {
                composer_id: "c1".into(),
                workspace_id: Some("hash-1".into()),
                fs_path_hint: Some("/fallback/fspath".into()),
                created_at: None,
                last_updated_at: Some(10),
                is_archived: false,
                is_subagent: false,
                parent_composer_id: None,
                name: None,
                lines_added: 0,
                lines_removed: 0,
                files_changed: 0,
                context_usage_percent: None,
            },
        );
        let mut workspace_map = HashMap::new();
        workspace_map.insert("hash-1".to_string(), "/real/project".to_string());

        let mut resolved: HashMap<String, (String, String)> = HashMap::new();
        for (id, h) in &headers {
            if let Some(wid) = &h.workspace_id {
                if let Some(p) = workspace_map.get(wid) {
                    resolved.insert(id.clone(), (p.clone(), "workspace-storage".to_string()));
                    continue;
                }
            }
            if let Some(p) = &h.fs_path_hint {
                resolved.insert(id.clone(), (p.clone(), "workspace-identifier-fspath".to_string()));
            }
        }

        assert_eq!(resolved["c1"], ("/real/project".to_string(), "workspace-storage".to_string()));
    }

    #[test]
    fn fspath_hint_used_when_workspace_storage_map_is_missing() {
        let h = header_from_value(
            "c2".to_string(),
            &serde_json::json!({
                "workspaceIdentifier": {"id": "hash-missing", "uri": {"fsPath": "/hint/path"}}
            }),
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(h.workspace_id.as_deref(), Some("hash-missing"));
        assert_eq!(h.fs_path_hint.as_deref(), Some("/hint/path"));
        // Simulate: workspace_map has no entry for "hash-missing" → falls to fsPath hint.
        let workspace_map: HashMap<String, String> = HashMap::new();
        let mut resolved: HashMap<String, (String, String)> = HashMap::new();
        if let Some(wid) = &h.workspace_id {
            if let Some(p) = workspace_map.get(wid) {
                resolved.insert(h.composer_id.clone(), (p.clone(), "workspace-storage".to_string()));
            }
        }
        if !resolved.contains_key(&h.composer_id) {
            if let Some(p) = &h.fs_path_hint {
                resolved.insert(h.composer_id.clone(), (p.clone(), "workspace-identifier-fspath".to_string()));
            }
        }
        assert_eq!(
            resolved["c2"],
            ("/hint/path".to_string(), "workspace-identifier-fspath".to_string())
        );
    }

    #[test]
    fn subagent_info_marks_is_subagent_and_parent_from_header_value_alone() {
        let h = header_from_value(
            "sub-1".to_string(),
            &serde_json::json!({
                "subagentInfo": {"parentComposerId": "parent-1", "subagentTypeName": "explore"}
            }),
            None,
            None,
            None,
            None,
            None,
        );
        assert!(h.is_subagent);
        assert_eq!(h.parent_composer_id.as_deref(), Some("parent-1"));
    }

    // ── Slug forward-match ────────────────────────────────────────────────

    #[test]
    fn slug_forward_match_resolves_composer_via_dash_containing_project_path() {
        let tmp = tempfile::tempdir().unwrap();
        let root_path = "/Users/dev/my-cool-project"; // dashes inside the real path
        let slug = path_to_slug(root_path);
        assert_eq!(slug, "Users-dev-my-cool-project");

        let transcripts_dir = tmp.path().join(&slug).join("agent-transcripts").join("composer-xyz");
        std::fs::create_dir_all(&transcripts_dir).unwrap();

        let mut headers = HashMap::new();
        headers.insert(
            "composer-xyz".to_string(),
            header_from_value("composer-xyz".to_string(), &serde_json::Value::Null, None, None, None, None, None),
        );
        let mut known_roots = HashSet::new();
        known_roots.insert(root_path.to_string());
        let mut resolved: HashMap<String, (String, String)> = HashMap::new();

        slug_forward_match_under(tmp.path(), &headers, &mut resolved, &known_roots);

        assert_eq!(
            resolved.get("composer-xyz"),
            Some(&(root_path.to_string(), "cursor-projects-slug".to_string()))
        );
    }

    #[test]
    fn slug_forward_match_skips_empty_window_and_numeric_dirs() {
        assert!(!is_indexable_slug("empty-window"));
        assert!(!is_indexable_slug("1778313793976"));
        assert!(is_indexable_slug("Users-dev-my-cool-project"));
    }

    // ── originalFileStates longest-prefix ─────────────────────────────────

    #[test]
    fn original_file_states_picks_longest_matching_root() {
        let conn = fixture_conn();
        let data = serde_json::json!({
            "originalFileStates": {
                "file:///Users/dev/parent/child/file.py": {"content": ""}
            }
        });
        conn.execute(
            "INSERT INTO cursorDiskKV (key, value) VALUES ('composerData:c3', ?1)",
            [data.to_string()],
        )
        .unwrap();

        let mut known_roots = HashSet::new();
        known_roots.insert("/Users/dev/parent".to_string());
        known_roots.insert("/Users/dev/parent/child".to_string()); // longer — must win

        let resolved = resolve_via_original_file_states(&conn, "c3", &known_roots);
        assert_eq!(resolved.as_deref(), Some("/Users/dev/parent/child"));
    }

    // ── Bubble token range (no cross-match on prefix ids) ─────────────────

    #[test]
    fn bubble_range_does_not_cross_match_when_one_composer_id_prefixes_another() {
        let conn = fixture_conn();
        // "abc" is a text-prefix of "abcdef" — the half-open range bound by
        // ':'/';' must keep their bubbles separate.
        conn.execute(
            "INSERT INTO cursorDiskKV (key, value) VALUES ('bubbleId:abc:msg1', ?1)",
            [serde_json::json!({"tokenCount": {"inputTokens": 10, "outputTokens": 20}}).to_string()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO cursorDiskKV (key, value) VALUES ('bubbleId:abcdef:msg1', ?1)",
            [serde_json::json!({"tokenCount": {"inputTokens": 1000, "outputTokens": 2000}}).to_string()],
        )
        .unwrap();

        let (input, output) = bubble_token_sum(&conn, "abc");
        assert_eq!((input, output), (10, 20), "must not pick up abcdef's much larger tokens");

        let (input2, output2) = bubble_token_sum(&conn, "abcdef");
        assert_eq!((input2, output2), (1000, 2000));
    }

    // ── GitRunner cache-hit behavior ───────────────────────────────────────

    struct FakeGitRunner {
        calls: AtomicUsize,
        hashes_by_repo: HashMap<PathBuf, Vec<String>>,
    }

    impl GitRunner for FakeGitRunner {
        fn rev_list_all(&self, repo_path: &Path, _max_count: usize) -> Result<Vec<String>, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.hashes_by_repo.get(repo_path).cloned().unwrap_or_default())
        }
    }

    #[test]
    fn second_resolve_with_a_warm_cache_makes_zero_git_calls() {
        let mut hashes_by_repo = HashMap::new();
        hashes_by_repo.insert(PathBuf::from("/repo/a"), vec!["abc123".to_string()]);
        let concrete = Arc::new(FakeGitRunner { calls: AtomicUsize::new(0), hashes_by_repo });
        let runner: Arc<dyn GitRunner> = concrete.clone();
        let mut cache: HashMap<String, String> = HashMap::new();
        let wanted: HashSet<String> = ["abc123".to_string()].into_iter().collect();
        let repos = vec![PathBuf::from("/repo/a")];

        // First resolve — cache miss, must call git exactly once.
        resolve_commits_with_cache(&runner, &repos, &wanted, &mut cache);
        assert_eq!(concrete.calls.load(Ordering::SeqCst), 1, "first resolve must call git once");
        assert_eq!(cache.get("abc123"), Some(&"/repo/a".to_string()));

        // Second resolve: everything in `wanted` is already cached — must
        // not spawn any git process at all.
        resolve_commits_with_cache(&runner, &repos, &wanted, &mut cache);
        assert_eq!(concrete.calls.load(Ordering::SeqCst), 1, "cache hit must make zero further git calls");
    }

    #[test]
    fn commit_absent_from_every_candidate_repo_is_negative_cached() {
        // No repo's history contains this hash (e.g. its repo moved or was
        // never cloned locally) — the scan must still mark it as "tried" so
        // a later call doesn't rescan every repo again for it.
        let concrete = Arc::new(FakeGitRunner { calls: AtomicUsize::new(0), hashes_by_repo: HashMap::new() });
        let runner: Arc<dyn GitRunner> = concrete.clone();
        let mut cache: HashMap<String, String> = HashMap::new();
        let wanted: HashSet<String> = ["neverfound".to_string()].into_iter().collect();
        let repos = vec![PathBuf::from("/repo/a")];

        resolve_commits_with_cache(&runner, &repos, &wanted, &mut cache);
        assert_eq!(concrete.calls.load(Ordering::SeqCst), 1);
        assert_eq!(cache.get("neverfound"), Some(&String::new()), "unresolved hash must be negative-cached as \"\"");

        resolve_commits_with_cache(&runner, &repos, &wanted, &mut cache);
        assert_eq!(concrete.calls.load(Ordering::SeqCst), 1, "negative cache must also make the 2nd call a no-op");
    }

    // ── Commit attribution / unattributed sums ────────────────────────────

    fn commit(hash: &str, ai_pct: f64) -> ScoredCommit {
        ScoredCommit {
            commit_hash: hash.to_string(),
            branch_name: "main".to_string(),
            scored_at: 0,
            lines_added: None,
            lines_deleted: None,
            tab_lines_added: None,
            tab_lines_deleted: None,
            composer_lines_added: None,
            composer_lines_deleted: None,
            human_lines_added: None,
            human_lines_deleted: None,
            blank_lines_added: None,
            blank_lines_deleted: None,
            commit_message: None,
            commit_date: Some("2026-01-01T00:00:00Z".to_string()),
            ai_percentage: ai_pct,
        }
    }

    #[test]
    fn attribute_commits_sums_per_project_and_counts_unattributed() {
        let commits = vec![
            commit("h1", 80.0),
            commit("h2", 20.0),
            commit("h3", 50.0), // repo known but not an ancestor of any project
            commit("h4", 10.0), // no repo resolved at all
            commit("h5", 5.0),  // negative-cache sentinel ("" = scanned, not found)
        ];
        let mut cache = HashMap::new();
        cache.insert("h1".to_string(), "/repo/proj-a".to_string());
        cache.insert("h2".to_string(), "/repo/proj-a".to_string());
        cache.insert("h3".to_string(), "/repo/unrelated".to_string());
        cache.insert("h5".to_string(), String::new());
        // h4 intentionally absent from cache.

        let project_paths = vec!["/repo/proj-a".to_string()];
        let (per_project, unattributed) = attribute_commits(&commits, &cache, &project_paths);

        let (count, ai_sum, repo) = &per_project["/repo/proj-a"];
        assert_eq!(*count, 2);
        assert!((ai_sum - 100.0).abs() < 1e-9);
        assert_eq!(repo, "/repo/proj-a");
        assert_eq!(
            unattributed, 3,
            "h3 (unrelated repo) + h4 (no repo) + h5 (negative-cached sentinel) must all count as unattributed"
        );
    }

    // ── Ordering ───────────────────────────────────────────────────────────

    fn project_stat(path: &str, last_activity: Option<&str>) -> CursorProjectStat {
        CursorProjectStat {
            path: path.to_string(),
            name: path.to_string(),
            sessions: 0,
            input_tokens: 0,
            output_tokens: 0,
            lines_added: 0,
            lines_removed: 0,
            files_changed: 0,
            commit_count: 0,
            ai_line_pct: 0.0,
            mcp_count: 0,
            plan_count: 0,
            last_activity: last_activity.map(String::from),
            repo_path: None,
        }
    }

    #[test]
    fn projects_sort_by_last_activity_descending() {
        let mut projects = vec![
            project_stat("/a", Some("2026-01-01T00:00:00Z")),
            project_stat("/b", Some("2026-06-01T00:00:00Z")),
            project_stat("/c", None),
        ];
        sort_projects(&mut projects);
        assert_eq!(projects[0].path, "/b");
        assert_eq!(projects[1].path, "/a");
        assert_eq!(projects[2].path, "/c");
    }

    #[test]
    fn commits_sort_by_commit_date_descending() {
        let mut commits = vec![
            CursorProjectCommit {
                commit_hash: "old".into(), branch_name: "main".into(), commit_message: None,
                commit_date: Some("2026-01-01T00:00:00Z".into()), ai_percentage: 0.0,
                lines_added: None, lines_deleted: None, tab_lines_added: None, tab_lines_deleted: None,
            },
            CursorProjectCommit {
                commit_hash: "new".into(), branch_name: "main".into(), commit_message: None,
                commit_date: Some("2026-06-01T00:00:00Z".into()), ai_percentage: 0.0,
                lines_added: None, lines_deleted: None, tab_lines_added: None, tab_lines_deleted: None,
            },
        ];
        sort_commits(&mut commits);
        assert_eq!(commits[0].commit_hash, "new");
        assert_eq!(commits[1].commit_hash, "old");
    }

    // ── Plan todo counting ────────────────────────────────────────────────

    #[test]
    fn plan_summary_counts_completed_todos_from_frontmatter() {
        let tmp = tempfile::tempdir().unwrap();
        let plan_path = tmp.path().join("my_plan.plan.md");
        std::fs::write(
            &plan_path,
            r#"---
name: My Plan
overview: Does a thing
todos:
  - content: step one
    status: completed
  - content: step two
    status: pending
  - content: step three
    status: completed
---

# Body
"#,
        )
        .unwrap();

        let summary = read_plan_summary(plan_path.to_str().unwrap()).expect("plan parses");
        assert_eq!(summary.name, "My Plan");
        assert_eq!(summary.total_todos, 3);
        assert_eq!(summary.completed_todos, 2);
    }

    // ── is_ancestor_or_equal ───────────────────────────────────────────────

    #[test]
    fn ancestor_check_matches_equal_and_proper_ancestor_only() {
        assert!(is_ancestor_or_equal("/repo/a", "/repo/a"));
        assert!(is_ancestor_or_equal("/repo/a", "/repo/a/sub"));
        assert!(!is_ancestor_or_equal("/repo/a", "/repo/ab")); // no boundary — must not match
        assert!(!is_ancestor_or_equal("/repo/a", "/repo/b"));
    }

    // ── Live smoke check ───────────────────────────────────────────────────
    // Exercises the full resolution pipeline against the developer's REAL
    // ~/Library/Application Support/Cursor data (read-only). Not run in CI —
    // `cargo test -- --ignored cursor_projects_live` to run it manually and
    // print the real counts.
    #[test]
    #[ignore]
    fn cursor_projects_live() {
        let corpus = build_corpus();
        let total_composers = corpus.headers.len();
        let resolved_composers = corpus.resolved.len();
        let unresolved_composers = total_composers - resolved_composers;

        let mut by_source: HashMap<String, u64> = HashMap::new();
        for (_, source) in corpus.resolved.values() {
            *by_source.entry(source.clone()).or_insert(0) += 1;
        }

        let mut overview = build_overview(false);

        // Give the background git scan (kicked off by the call above) a
        // bounded window to finish, then rebuild once more so the report
        // reflects real commit attribution rather than just "pending".
        if overview.commit_resolution_pending {
            for _ in 0..60 {
                if !COMMIT_SCAN_IN_PROGRESS.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(500));
            }
            overview = build_overview(false);
        }

        eprintln!("=== cursor_projects_live smoke check ===");
        eprintln!("composerHeaders total:      {}", total_composers);
        eprintln!("resolved:                    {}", resolved_composers);
        eprintln!("unresolved:                  {}", unresolved_composers);
        eprintln!("resolution sources:          {:?}", by_source);
        eprintln!("known project roots:         {}", corpus.known_roots.len());
        eprintln!("projects found:              {}", overview.projects.len());
        eprintln!("total sessions:              {}", overview.totals.sessions);
        eprintln!(
            "total tokens (in/out):      {}/{}",
            overview.totals.input_tokens, overview.totals.output_tokens
        );
        eprintln!("total lines (+/-):           {}/{}", overview.totals.lines_added, overview.totals.lines_removed);
        eprintln!("total commits attributed:    {}", overview.totals.commit_count);
        eprintln!("unattributed commits:        {}", overview.unattributed_commits);
        eprintln!("unresolved sessions:         {}", overview.unresolved_sessions);
        eprintln!("commit_resolution_pending:   {}", overview.commit_resolution_pending);
        for p in overview.projects.iter().take(10) {
            eprintln!(
                "  - {} | sessions={} tokens={}/{} lines=+{}/-{} commits={} ai%={:.1}",
                p.name, p.sessions, p.input_tokens, p.output_tokens, p.lines_added, p.lines_removed,
                p.commit_count, p.ai_line_pct
            );
        }
    }
}
