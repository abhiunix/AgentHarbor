//! OpenCode analytics provider.
//! Local data only — no cloud usage/limits API exists for OpenCode.
//! - `~/.local/share/opencode/opencode.db` (SQLite, WAL, since v1.2.0): `session` table
//!   carries precomputed per-session aggregates (cost, token splits) — read them, don't
//!   recompute (OpenCode already prices each message against its model catalog).
//! - `~/.local/share/opencode/auth.json`: providerID -> {"type": "api"|"oauth"|"wellknown", ...}.
//!
//! Two schema quirks (verified against a live v1.18.x install — see
//! docs/opencode-adapter-research.md):
//! - `time_created`/`time_updated` are millisecond-epoch INTEGERs (`date(x/1000,'unixepoch',...)`).
//! - `model` is a JSON string (`{"id":"...","providerID":"...","variant":"..."}`), not a plain name.
//!
//! `parent_id` is non-null for subagent sessions — top-level filter: `parent_id IS NULL OR = ''`.

use crate::analytics::types::*;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

// ── In-memory cache (60s TTL) ────────────────────────────────────────────────

struct CacheEntry {
    data: ProviderAnalytics,
    fetched_at: std::time::Instant,
}

lazy_static::lazy_static! {
    static ref CACHE: Mutex<Option<CacheEntry>> = Mutex::new(None);
}

const CACHE_TTL_SECS: u64 = 60;
const ACTIVE_WINDOW_SECS: i64 = 5 * 60;

/// Sessions with `parent_id` NULL or empty are top-level; non-empty means subagent.
const TOP_LEVEL_FILTER: &str = "(parent_id IS NULL OR parent_id = '')";

// ── Paths ─────────────────────────────────────────────────────────────────────

fn opencode_data_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".local").join("share").join("opencode")
}

fn db_path() -> PathBuf {
    opencode_data_dir().join("opencode.db")
}

fn auth_path() -> PathBuf {
    opencode_data_dir().join("auth.json")
}

/// Open `opencode.db` read-only + `query_only`, or `None` if missing/unreadable.
/// The DB may be held open (WAL) by a running `opencode` TUI — read-only open
/// degrades to `None` rather than erroring the whole fetch.
fn open_opencode_db() -> Option<rusqlite::Connection> {
    let path = db_path();
    if !path.exists() {
        return None;
    }
    let conn = rusqlite::Connection::open_with_flags(
        &path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    let _ = conn.execute_batch("PRAGMA query_only = ON;");
    Some(conn)
}

fn ms_to_rfc3339(ms: i64) -> Option<String> {
    if ms <= 0 {
        return None;
    }
    chrono::DateTime::from_timestamp_millis(ms).map(|dt| dt.to_rfc3339())
}

/// Extract the model id from OpenCode's `model` column, which stores a JSON
/// string like `{"id":"nvidia/nemotron-3-ultra-550b-a55b","providerID":"nvidia",
/// "variant":"high"}`. Falls back to the raw string when it isn't JSON or has
/// no `id` field, so unfamiliar/older row shapes still produce a label.
fn extract_model_id(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "unknown".to_string();
    }
    match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(v) => v
            .get("id")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| raw.to_string()),
        Err(_) => raw.to_string(),
    }
}

// ── Windowed stats (today / this week) ───────────────────────────────────────

struct OpencodeWindowStats {
    sessions: i64,
    tokens: i64,
    cost: f64,
}

/// Run a windowed session/token/cost query against `session`, restricted to
/// top-level sessions. Hoisted to module scope (rather than nested) so the
/// ms-epoch date-window SQL is directly unit-testable against a fixture DB.
fn query_window(conn: &rusqlite::Connection, time_clause: &str, params: &[&str]) -> OpencodeWindowStats {
    let sql = format!(
        "SELECT COUNT(*), COALESCE(SUM(tokens_input + tokens_output + tokens_reasoning), 0), \
         COALESCE(SUM(cost), 0) FROM session WHERE {} AND ({})",
        TOP_LEVEL_FILTER, time_clause
    );
    conn.query_row(&sql, rusqlite::params_from_iter(params.iter()), |row| {
        Ok(OpencodeWindowStats {
            sessions: row.get(0)?,
            tokens: row.get(1)?,
            cost: row.get(2)?,
        })
    })
    .unwrap_or(OpencodeWindowStats { sessions: 0, tokens: 0, cost: 0.0 })
}

/// Exact all-time totals (COUNT/SUM), restricted to top-level sessions —
/// precomputed columns, no message parsing needed.
fn aggregate_top_level(conn: &rusqlite::Connection) -> (i64, i64, f64) {
    let sql = format!(
        "SELECT COUNT(*), COALESCE(SUM(tokens_input + tokens_output + tokens_reasoning), 0), \
         COALESCE(SUM(cost), 0) FROM session WHERE {}",
        TOP_LEVEL_FILTER
    );
    conn.query_row(&sql, [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap_or((0, 0, 0.0))
}

fn fetch_window_stats(conn: &rusqlite::Connection) -> (OpencodeWindowStats, OpencodeWindowStats) {
    let local_now = chrono::Local::now();
    let today_str = local_now.format("%Y-%m-%d").to_string();

    use chrono::Datelike;
    let weekday = local_now.weekday().num_days_from_monday();
    let monday = local_now.date_naive() - chrono::Duration::days(weekday as i64);
    let monday_str = monday.format("%Y-%m-%d").to_string();

    let today = query_window(
        conn,
        "date(time_created/1000,'unixepoch','localtime') = ?1 OR date(time_updated/1000,'unixepoch','localtime') = ?1",
        &[&today_str],
    );
    let week = query_window(
        conn,
        "date(time_created/1000,'unixepoch','localtime') >= ?1 OR date(time_updated/1000,'unixepoch','localtime') >= ?1",
        &[&monday_str],
    );
    (today, week)
}

// ── Local stats (sessions, tokens by model, sessions by project) ────────────

#[derive(Serialize, Clone, Debug)]
struct OpencodeSession {
    id: String,
    title: Option<String>,
    model: Option<String>,
    tokens_used: i64,
    cost: f64,
    directory: Option<String>,
    agent: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
}

#[derive(Serialize, Clone, Debug, Default)]
struct OpencodeLocalStats {
    total_sessions: i64,
    total_tokens_used: i64,
    total_cost: f64,
    sessions: Vec<OpencodeSession>,
    tokens_by_model: HashMap<String, i64>,
    cost_by_model: HashMap<String, f64>,
    sessions_by_project: HashMap<String, i64>,
    active_now: bool,
}

type SessionRow = (
    String,
    Option<String>,
    Option<String>,
    i64,
    i64,
    i64,
    f64,
    Option<String>,
    Option<String>,
    i64,
    i64,
);

const RECENT_SESSIONS_LIMIT: usize = 50;

fn fetch_local_stats(conn: &rusqlite::Connection) -> Result<OpencodeLocalStats, String> {
    let (total_sessions, total_tokens_used, total_cost) = aggregate_top_level(conn);

    let mut stmt = conn
        .prepare(&format!(
            "SELECT id, title, model, COALESCE(tokens_input,0), COALESCE(tokens_output,0), \
             COALESCE(tokens_reasoning,0), COALESCE(cost,0), directory, agent, \
             COALESCE(time_created,0), COALESCE(time_updated,0) \
             FROM session WHERE {} ORDER BY time_updated DESC",
            TOP_LEVEL_FILTER
        ))
        .map_err(|e| format!("Query prepare error: {}", e))?;

    let rows = stmt
        .query_map([], |row| -> rusqlite::Result<SessionRow> {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
                row.get(9)?,
                row.get(10)?,
            ))
        })
        .map_err(|e| format!("Query error: {}", e))?;

    let mut sessions = Vec::new();
    let mut tokens_by_model: HashMap<String, i64> = HashMap::new();
    let mut cost_by_model: HashMap<String, f64> = HashMap::new();
    let mut sessions_by_project: HashMap<String, i64> = HashMap::new();
    let now_ms = chrono::Utc::now().timestamp_millis();
    let mut active_now = false;

    for row in rows.flatten() {
        let (id, title, model_raw, tin, tout, treason, cost, directory, agent, created_ms, updated_ms) = row;
        let tokens = tin + tout + treason;

        let model_id = model_raw.as_deref().map(extract_model_id);
        if let Some(ref m) = model_id {
            *tokens_by_model.entry(m.clone()).or_insert(0) += tokens;
            *cost_by_model.entry(m.clone()).or_insert(0.0) += cost;
        }

        if let Some(ref dir) = directory {
            let project_name = std::path::Path::new(dir)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| dir.clone());
            *sessions_by_project.entry(project_name).or_insert(0) += 1;
        }

        if updated_ms > 0 && now_ms.saturating_sub(updated_ms) <= ACTIVE_WINDOW_SECS * 1000 {
            active_now = true;
        }

        if sessions.len() < RECENT_SESSIONS_LIMIT {
            sessions.push(OpencodeSession {
                id,
                title,
                model: model_id,
                tokens_used: tokens,
                cost,
                directory,
                agent,
                created_at: ms_to_rfc3339(created_ms),
                updated_at: ms_to_rfc3339(updated_ms),
            });
        }
    }

    Ok(OpencodeLocalStats {
        total_sessions,
        total_tokens_used,
        total_cost,
        sessions,
        tokens_by_model,
        cost_by_model,
        sessions_by_project,
        active_now,
    })
}

// ── auth.json parsing ─────────────────────────────────────────────────────────

#[derive(Serialize, Clone, Debug)]
struct AuthProviderInfo {
    provider_id: String,
    #[serde(rename = "type")]
    auth_type: String,
}

/// Parse `auth.json`'s providerID -> {"type": ..., ...} map. Pure so it's
/// directly unit-testable; unrecognized/malformed entries are skipped rather
/// than failing the whole parse.
fn parse_auth_json(content: &str) -> Vec<AuthProviderInfo> {
    let parsed: serde_json::Value = match serde_json::from_str(content) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let Some(obj) = parsed.as_object() else {
        return vec![];
    };
    obj.iter()
        .map(|(provider_id, v)| AuthProviderInfo {
            provider_id: provider_id.clone(),
            auth_type: v.get("type").and_then(|t| t.as_str()).unwrap_or("unknown").to_string(),
        })
        .collect()
}

fn read_auth_providers() -> Option<Vec<AuthProviderInfo>> {
    let content = fs::read_to_string(auth_path()).ok()?;
    let providers = parse_auth_json(&content);
    if providers.is_empty() {
        None
    } else {
        Some(providers)
    }
}

// ── Enrichment ────────────────────────────────────────────────────────────────

fn enrich_with_local_data(extra: &mut HashMap<String, serde_json::Value>) {
    if let Some(conn) = open_opencode_db() {
        if let Ok(stats) = fetch_local_stats(&conn) {
            extra.insert("total_sessions".into(), serde_json::json!(stats.total_sessions));
            extra.insert("total_tokens_used".into(), serde_json::json!(stats.total_tokens_used));
            // Named to match codex's convention for tray reuse — the number is
            // real (read from OpenCode's own cost engine), not estimated.
            extra.insert("estimated_total_cost".into(), serde_json::json!(stats.total_cost));
            extra.insert("sessions".into(), serde_json::to_value(&stats.sessions).unwrap_or_default());
            extra.insert("tokens_by_model".into(), serde_json::to_value(&stats.tokens_by_model).unwrap_or_default());
            extra.insert("cost_by_model".into(), serde_json::to_value(&stats.cost_by_model).unwrap_or_default());
            extra.insert(
                "sessions_by_project".into(),
                serde_json::to_value(&stats.sessions_by_project).unwrap_or_default(),
            );
            extra.insert("active_now".into(), serde_json::json!(stats.active_now));
        }

        let (today, week) = fetch_window_stats(&conn);
        extra.insert("start_today_sessions".into(), serde_json::json!(today.sessions));
        extra.insert("start_today_tokens".into(), serde_json::json!(today.tokens));
        extra.insert("start_today_cost".into(), serde_json::json!(today.cost));
        extra.insert("this_week_sessions".into(), serde_json::json!(week.sessions));
        extra.insert("this_week_tokens".into(), serde_json::json!(week.tokens));
        extra.insert("this_week_cost".into(), serde_json::json!(week.cost));
    }

    if let Some(providers) = read_auth_providers() {
        extra.insert("auth_providers".into(), serde_json::to_value(&providers).unwrap_or_default());
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

pub fn check_connection() -> ProviderStatus {
    let has_data = opencode_data_dir().exists() || db_path().exists();
    if has_data {
        ProviderStatus {
            provider_id: "opencode".into(),
            provider_name: "OpenCode".into(),
            connected: true,
            connection_method: "local-file".into(),
            account_email: None,
            plan_name: None,
            org_name: None,
            error: None,
        }
    } else {
        ProviderStatus {
            provider_id: "opencode".into(),
            provider_name: "OpenCode".into(),
            connected: false,
            connection_method: "none".into(),
            account_email: None,
            plan_name: None,
            org_name: None,
            error: Some("OpenCode data directory not found (~/.local/share/opencode)".into()),
        }
    }
}

fn fetch_opencode_analytics_uncached() -> ProviderAnalytics {
    let now = chrono::Utc::now().to_rfc3339();
    let status = check_connection();

    let mut extra = HashMap::new();
    if status.connected {
        enrich_with_local_data(&mut extra);
    }

    ProviderAnalytics {
        provider_id: "opencode".into(),
        provider_name: "OpenCode".into(),
        status,
        rate_limits: vec![],
        credit_usage: None,
        token_counts: None,
        limit_state: None,
        extra,
        fetched_at: now,
    }
}

pub fn fetch_opencode_analytics() -> ProviderAnalytics {
    if let Ok(guard) = CACHE.lock() {
        if let Some(ref entry) = *guard {
            if entry.fetched_at.elapsed().as_secs() < CACHE_TTL_SECS {
                return entry.data.clone();
            }
        }
    }

    let result = fetch_opencode_analytics_uncached();

    if result.status.connected {
        if let Ok(mut guard) = CACHE.lock() {
            *guard = Some(CacheEntry {
                data: result.clone(),
                fetched_at: std::time::Instant::now(),
            });
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── model JSON extraction ────────────────────────────────────────────────

    #[test]
    fn extracts_model_id_from_json_string() {
        let raw = r#"{"id":"nvidia/nemotron-3-ultra-550b-a55b","providerID":"nvidia","variant":"high"}"#;
        assert_eq!(extract_model_id(raw), "nvidia/nemotron-3-ultra-550b-a55b");
    }

    #[test]
    fn falls_back_to_raw_string_when_not_json() {
        assert_eq!(extract_model_id("claude-sonnet-4-5"), "claude-sonnet-4-5");
    }

    #[test]
    fn falls_back_to_raw_string_when_json_missing_id() {
        let raw = r#"{"providerID":"nvidia"}"#;
        assert_eq!(extract_model_id(raw), raw);
    }

    #[test]
    fn empty_model_is_unknown() {
        assert_eq!(extract_model_id(""), "unknown");
        assert_eq!(extract_model_id("   "), "unknown");
    }

    // ── auth.json parsing ─────────────────────────────────────────────────────

    #[test]
    fn parses_auth_json_provider_map() {
        let content = r#"{
            "anthropic": {"type":"oauth","refresh":"r","access":"a","expires":123},
            "nvidia": {"type":"api","key":"sk-abc"}
        }"#;
        let mut providers = parse_auth_json(content);
        providers.sort_by(|a, b| a.provider_id.cmp(&b.provider_id));
        assert_eq!(providers.len(), 2);
        assert_eq!(providers[0].provider_id, "anthropic");
        assert_eq!(providers[0].auth_type, "oauth");
        assert_eq!(providers[1].provider_id, "nvidia");
        assert_eq!(providers[1].auth_type, "api");
    }

    #[test]
    fn malformed_auth_json_yields_empty_not_panic() {
        assert!(parse_auth_json("not json").is_empty());
        assert!(parse_auth_json("[]").is_empty());
        assert!(parse_auth_json("").is_empty());
    }

    // ── SQLite fixture (exact DDL from Phase 8.1 research) ───────────────────

    fn make_fixture_session_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE project (
                id TEXT PRIMARY KEY,
                worktree TEXT,
                vcs TEXT,
                name TEXT
            );
            CREATE TABLE session (
                id TEXT PRIMARY KEY,
                project_id TEXT,
                parent_id TEXT,
                slug TEXT,
                directory TEXT,
                title TEXT,
                version TEXT,
                share_url TEXT,
                summary_additions INTEGER,
                summary_deletions INTEGER,
                summary_files INTEGER,
                summary_diffs INTEGER,
                revert TEXT,
                permission TEXT,
                time_created INTEGER,
                time_updated INTEGER,
                time_compacting INTEGER,
                time_archived INTEGER,
                workspace_id TEXT,
                path TEXT,
                agent TEXT,
                model TEXT,
                cost REAL DEFAULT 0,
                tokens_input INTEGER DEFAULT 0,
                tokens_output INTEGER DEFAULT 0,
                tokens_reasoning INTEGER DEFAULT 0,
                tokens_cache_read INTEGER DEFAULT 0,
                tokens_cache_write INTEGER DEFAULT 0,
                metadata TEXT
            );",
        )
        .unwrap();
        conn
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_session(
        conn: &rusqlite::Connection,
        id: &str,
        parent_id: Option<&str>,
        directory: &str,
        model_json: &str,
        cost: f64,
        tokens_input: i64,
        tokens_output: i64,
        tokens_reasoning: i64,
        time_created: i64,
        time_updated: i64,
    ) {
        conn.execute(
            "INSERT INTO session (id, parent_id, directory, model, cost, tokens_input, tokens_output, \
             tokens_reasoning, time_created, time_updated) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            rusqlite::params![
                id,
                parent_id,
                directory,
                model_json,
                cost,
                tokens_input,
                tokens_output,
                tokens_reasoning,
                time_created,
                time_updated,
            ],
        )
        .unwrap();
    }

    #[test]
    fn parent_filter_excludes_subagents_includes_empty_string_parent() {
        let conn = make_fixture_session_db();
        insert_session(&conn, "top1", None, "/repo/a", r#"{"id":"anthropic/claude"}"#, 1.0, 100, 50, 0, 1, 1);
        insert_session(&conn, "top2", Some(""), "/repo/a", r#"{"id":"anthropic/claude"}"#, 2.0, 100, 50, 0, 1, 1);
        insert_session(&conn, "sub1", Some("top1"), "/repo/a", r#"{"id":"anthropic/claude"}"#, 5.0, 999, 999, 0, 1, 1);

        let (sessions, tokens, cost) = aggregate_top_level(&conn);
        assert_eq!(sessions, 2);
        assert_eq!(tokens, 300); // (100+50) * 2 top-level rows
        assert!((cost - 3.0).abs() < 1e-9);
    }

    #[test]
    fn model_extraction_via_fetch_local_stats_groups_by_id() {
        let conn = make_fixture_session_db();
        insert_session(
            &conn,
            "s1",
            None,
            "/repo/proj-a",
            r#"{"id":"nvidia/nemotron-3-ultra-550b-a55b","providerID":"nvidia"}"#,
            19.84,
            7_500_000,
            100_000,
            4_000_000,
            1_788_033_168_797,
            1_788_033_168_797,
        );
        insert_session(&conn, "s2", None, "/repo/proj-b", "plain-model-name", 0.5, 10, 5, 0, 1_788_000_000_000, 1_788_000_000_000);

        let stats = fetch_local_stats(&conn).unwrap();
        assert_eq!(stats.total_sessions, 2);
        assert_eq!(stats.sessions.len(), 2);
        assert!(stats.tokens_by_model.contains_key("nvidia/nemotron-3-ultra-550b-a55b"));
        assert!(stats.tokens_by_model.contains_key("plain-model-name"));
        assert!((stats.cost_by_model["nvidia/nemotron-3-ultra-550b-a55b"] - 19.84).abs() < 1e-9);
        assert_eq!(stats.sessions_by_project.get("proj-a"), Some(&1));
        assert_eq!(stats.sessions_by_project.get("proj-b"), Some(&1));
    }

    #[test]
    fn empty_db_yields_zeroed_stats_without_panicking() {
        let conn = make_fixture_session_db();
        let stats = fetch_local_stats(&conn).unwrap();
        assert_eq!(stats.total_sessions, 0);
        assert_eq!(stats.total_tokens_used, 0);
        assert!(stats.sessions.is_empty());
        assert!(!stats.active_now);

        let (sessions, tokens, cost) = aggregate_top_level(&conn);
        assert_eq!(sessions, 0);
        assert_eq!(tokens, 0);
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn window_query_matches_ms_epoch_today_row_only() {
        let conn = make_fixture_session_db();
        let now_ms = chrono::Local::now().timestamp_millis();
        let ten_days_ago_ms = now_ms - 10 * 24 * 60 * 60 * 1000;

        insert_session(&conn, "today", None, "/repo/a", r#"{"id":"m"}"#, 1.0, 1000, 0, 0, now_ms, now_ms);
        insert_session(&conn, "old", None, "/repo/a", r#"{"id":"m"}"#, 2.0, 2000, 0, 0, ten_days_ago_ms, ten_days_ago_ms);

        let today_str = chrono::Local::now().format("%Y-%m-%d").to_string();
        let stats = query_window(
            &conn,
            "date(time_created/1000,'unixepoch','localtime') = ?1 OR date(time_updated/1000,'unixepoch','localtime') = ?1",
            &[&today_str],
        );
        assert_eq!(stats.sessions, 1);
        assert_eq!(stats.tokens, 1000);
        assert!((stats.cost - 1.0).abs() < 1e-9);
    }

    #[test]
    fn buggy_query_without_unixepoch_modifier_on_ms_epoch_finds_nothing() {
        // Regression guard: date() on a raw millisecond epoch integer without
        // both the /1000 division and the 'unixepoch' modifier never matches.
        let conn = make_fixture_session_db();
        let now_ms = chrono::Local::now().timestamp_millis();
        insert_session(&conn, "today", None, "/repo/a", r#"{"id":"m"}"#, 1.0, 1000, 0, 0, now_ms, now_ms);

        let today_str = chrono::Local::now().format("%Y-%m-%d").to_string();
        let stats = query_window(&conn, "date(time_created) = ?1 OR date(time_updated) = ?1", &[&today_str]);
        assert_eq!(stats.sessions, 0);
    }

    #[test]
    fn this_week_window_includes_today_and_excludes_far_past() {
        let conn = make_fixture_session_db();
        let now_ms = chrono::Local::now().timestamp_millis();
        let far_past_ms = now_ms - 30 * 24 * 60 * 60 * 1000;
        insert_session(&conn, "recent", None, "/repo/a", r#"{"id":"m"}"#, 1.0, 100, 0, 0, now_ms, now_ms);
        insert_session(&conn, "old", None, "/repo/a", r#"{"id":"m"}"#, 2.0, 200, 0, 0, far_past_ms, far_past_ms);

        let (_today, week) = fetch_window_stats(&conn);
        assert!(week.sessions >= 1);
        assert!(week.tokens >= 100);
    }
}
