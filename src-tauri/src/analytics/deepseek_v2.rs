//! DeepSeek Harness (`dsh`) Analytics V2 — local session analytics (Phase 1).
//! Reads dsh's local JSON caches under `~/.dsh` (or `$DSH_HOME`):
//!   storages/workspace.json         → workspaces, their sessionIds
//!   storages/session_projcache.json → per-session derived stats (JSON cache,
//!                                     no zstd decompression needed here)
//!   settings.yaml                   → agent-default-model (provider/model/reasoningEffort)
//! Session transcripts (`sessions/<slug>/session-<uuid>/session.jsonl.zstd`)
//! are NOT read here — that needs the zstd crate and is a future agent's job.
//! Modeled on `kimi_v2.rs`.

use crate::commands::session_stats::DailyActivity;
use chrono::{DateTime, Datelike, Local, TimeZone, Timelike};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

// ── V2 Types ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekWorkspaceStat {
    pub path: String,
    pub title: String,
    pub sessions: u64,
    pub turns: u64,
    pub steps: u64,
    pub tokens: u64,
    pub last_activity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekSessionStat {
    pub session_id: String,
    pub title: String,
    pub workspace_path: String,
    pub workspace_name: String,
    pub created_at: String,
    pub turns: u64,
    pub steps: u64,
    pub tokens: u64,
}

/// DeepSeek platform balance (`GET api.deepseek.com/user/balance`), folded in
/// from the existing `deepseek.rs` provider — auth mode here is always
/// "api" (a DeepSeek platform API key), there is no OAuth subscription.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeepSeekBalance {
    pub available: f64,
    pub currency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekV2Overview {
    pub connected: bool,
    pub connection_method: String,
    pub default_model: Option<String>,
    pub default_model_reasoning_effort: Option<String>,

    // Totals
    pub total_sessions: u64,
    pub total_turns: u64,
    pub total_steps: u64,
    pub total_tokens: u64,
    pub total_llm_ms: u64,
    pub total_tool_ms: u64,
    pub active_now: u64,

    // Activity — derived from session `createdAt` timestamps (dsh has no
    // per-turn timestamps in its local JSON caches).
    pub hour_counts: Vec<u64>,
    pub daily_activity: Vec<DailyActivity>,
    pub first_session_date: Option<String>,
    pub active_days: u32,
    pub total_days: u32,
    pub longest_streak: u32,
    pub current_streak: u32,
    pub most_active_weekday: Option<String>,
    pub peak_hour: Option<u32>,

    // Breakdowns
    pub workspaces: Vec<DeepSeekWorkspaceStat>,
    pub recent_sessions: Vec<DeepSeekSessionStat>,

    // ── DeepSeek platform balance (auth mode = API key) ──
    /// Always "api" for DeepSeek — kept for shape-parity with other v2
    /// overviews that distinguish subscription vs API-key auth.
    #[serde(default = "default_auth_mode")]
    pub auth_mode: String,
    #[serde(default)]
    pub balance: Option<DeepSeekBalance>,
    #[serde(default)]
    pub balance_connected: bool,
}

fn default_auth_mode() -> String {
    "api".to_string()
}

// ── Cache ───────────────────────────────────────────────────────────────────

struct CacheEntry<T> {
    data: T,
    fetched_at: Instant,
}

impl<T> CacheEntry<T> {
    fn new(data: T) -> Self {
        Self { data, fetched_at: Instant::now() }
    }
    fn is_valid(&self, ttl_seconds: u64) -> bool {
        self.fetched_at.elapsed() < Duration::from_secs(ttl_seconds)
    }
}

struct DeepSeekV2Cache {
    overview: HashMap<String, CacheEntry<DeepSeekV2Overview>>,
    ttl_seconds: u64,
}

lazy_static::lazy_static! {
    static ref CACHE: Mutex<DeepSeekV2Cache> = Mutex::new(DeepSeekV2Cache {
        overview: HashMap::new(),
        ttl_seconds: 300,
    });
}

/// Sessions with activity within this many minutes count as "active now".
const ACTIVE_NOW_MINUTES: i64 = 15;

// ── Paths ────────────────────────────────────────────────────────────────────

pub(crate) fn dsh_root() -> Option<std::path::PathBuf> {
    if let Ok(p) = std::env::var("DSH_HOME") {
        if !p.trim().is_empty() {
            return Some(std::path::PathBuf::from(p));
        }
    }
    dirs::home_dir().map(|h| h.join(".dsh"))
}

fn session_cache_path(root: &std::path::Path) -> std::path::PathBuf {
    root.join("storages").join("session_projcache.json")
}

// ── storages/workspace.json ──────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
struct WorkspaceFile {
    tables: WorkspaceTables,
}

#[derive(Debug, Clone, Deserialize)]
struct WorkspaceTables {
    #[serde(default)]
    workspaces: HashMap<String, WorkspaceEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct WorkspaceEntry {
    path: String,
    title: String,
    #[serde(default, rename = "sessionIds")]
    session_ids: Vec<String>,
}

fn parse_workspace_file(content: &str) -> HashMap<String, WorkspaceEntry> {
    serde_json::from_str::<WorkspaceFile>(content)
        .map(|f| f.tables.workspaces)
        .unwrap_or_default()
}

fn read_workspaces(root: &std::path::Path) -> HashMap<String, WorkspaceEntry> {
    std::fs::read_to_string(root.join("storages").join("workspace.json"))
        .map(|text| parse_workspace_file(&text))
        .unwrap_or_default()
}

/// session id → (workspace path, workspace title).
fn build_session_workspace_map(workspaces: &HashMap<String, WorkspaceEntry>) -> HashMap<String, (String, String)> {
    let mut map = HashMap::new();
    for w in workspaces.values() {
        for sid in &w.session_ids {
            map.insert(sid.clone(), (w.path.clone(), w.title.clone()));
        }
    }
    map
}

// ── storages/session_projcache.json ──────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
struct SessionCacheFile {
    tables: SessionCacheTables,
}

#[derive(Debug, Clone, Deserialize)]
struct SessionCacheTables {
    #[serde(default)]
    sessions: HashMap<String, SessionEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct SessionEntry {
    #[serde(default)]
    identity: SessionIdentity,
    #[serde(default)]
    rows: SessionRows,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct SessionIdentity {
    #[serde(default, rename = "createdAt")]
    created_at: i64,
    #[serde(default)]
    cwd: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct SessionRows {
    #[serde(default, rename = "sessionStats")]
    session_stats: Option<ValWrap<SessionStatsVal>>,
    #[serde(default)]
    title: Option<ValWrap<String>>,
    #[serde(default, rename = "tokenUsage")]
    token_usage: Option<ValWrap<TokenUsageVal>>,
    #[serde(default, rename = "sessionListMetadata")]
    session_list_metadata: Option<ValWrap<SessionListMetadataVal>>,
    #[serde(default)]
    permissions: Option<ValWrap<PermissionsVal>>,
}

#[derive(Debug, Clone, Deserialize)]
struct ValWrap<T> {
    val: T,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct SessionStatsVal {
    #[serde(default)]
    turns: u64,
    #[serde(default)]
    steps: u64,
    #[serde(default, rename = "llmMs")]
    llm_ms: u64,
    #[serde(default, rename = "toolMs")]
    tool_ms: u64,
    #[serde(default, rename = "decodeTokens")]
    decode_tokens: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct TokenUsageVal {
    #[serde(default)]
    totals: Option<TokenTotals>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct TokenTotals {
    #[serde(default, rename = "uncachedInputTokens")]
    uncached_input_tokens: u64,
    #[serde(default, rename = "outputTokens")]
    output_tokens: u64,
    #[serde(default, rename = "cacheReadTokens")]
    cache_read_tokens: u64,
    #[serde(default, rename = "cacheWriteTokens")]
    cache_write_tokens: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct SessionListMetadataVal {
    #[serde(default, rename = "lastPromptAt")]
    last_prompt_at: Option<i64>,
}

/// `rows.permissions.val` — the session's permission preset / sandbox mode /
/// approval policy, as also emitted by the `permission/preset`, `sandbox/mode`,
/// and `approval/policy` session-log events at session start.
#[derive(Debug, Clone, Default, Deserialize)]
struct PermissionsVal {
    #[serde(default)]
    preset: Option<String>,
    #[serde(default)]
    sandbox: Option<String>,
    #[serde(default)]
    approval: Option<String>,
}

fn parse_session_cache_file(content: &str) -> HashMap<String, SessionEntry> {
    serde_json::from_str::<SessionCacheFile>(content)
        .map(|f| f.tables.sessions)
        .unwrap_or_default()
}

fn read_session_cache(root: &std::path::Path) -> HashMap<String, SessionEntry> {
    std::fs::read_to_string(session_cache_path(root))
        .map(|text| parse_session_cache_file(&text))
        .unwrap_or_default()
}

// ── Derived per-session view ──────────────────────────────────────────────────

struct SessionDerived {
    session_id: String,
    workspace_path: String,
    workspace_name: String,
    title: String,
    created_at_ms: i64,
    last_activity_ms: i64,
    turns: u64,
    steps: u64,
    tokens: u64,
    llm_ms: u64,
    tool_ms: u64,
}

/// Prefer the richer `tokenUsage.totals` sum when present; fall back to
/// `sessionStats.decodeTokens`.
fn session_token_count(rows: &SessionRows) -> u64 {
    let from_usage = rows
        .token_usage
        .as_ref()
        .and_then(|w| w.val.totals.as_ref())
        .map(|t| t.uncached_input_tokens + t.output_tokens + t.cache_read_tokens + t.cache_write_tokens)
        .filter(|&t| t > 0);
    from_usage.unwrap_or_else(|| rows.session_stats.as_ref().map(|w| w.val.decode_tokens).unwrap_or(0))
}

fn derive_sessions(
    sessions: &HashMap<String, SessionEntry>,
    ws_map: &HashMap<String, (String, String)>,
) -> Vec<SessionDerived> {
    sessions
        .iter()
        .map(|(sid, entry)| {
            let stats = entry.rows.session_stats.as_ref().map(|w| &w.val);
            let turns = stats.map(|s| s.turns).unwrap_or(0);
            let steps = stats.map(|s| s.steps).unwrap_or(0);
            let llm_ms = stats.map(|s| s.llm_ms).unwrap_or(0);
            let tool_ms = stats.map(|s| s.tool_ms).unwrap_or(0);
            let tokens = session_token_count(&entry.rows);

            let title = entry
                .rows
                .title
                .as_ref()
                .map(|w| w.val.clone())
                .unwrap_or_else(|| "Untitled session".to_string());

            let (workspace_path, workspace_name) = ws_map.get(sid).cloned().unwrap_or_else(|| {
                let cwd = entry.identity.cwd.clone().unwrap_or_else(|| "unknown".to_string());
                let name = std::path::Path::new(&cwd)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| cwd.clone());
                (cwd, name)
            });

            let last_activity_ms = entry
                .rows
                .session_list_metadata
                .as_ref()
                .and_then(|w| w.val.last_prompt_at)
                .unwrap_or(entry.identity.created_at);

            SessionDerived {
                session_id: sid.clone(),
                workspace_path,
                workspace_name,
                title,
                created_at_ms: entry.identity.created_at,
                last_activity_ms,
                turns,
                steps,
                tokens,
                llm_ms,
                tool_ms,
            }
        })
        .collect()
}

// ── Shared session-log helpers (deepseek_prompts / deepseek_transcripts) ────
// Session transcripts live under sessions/<workspace-slug>/session-<uuid>/
// session.jsonl.zstd — zstd-compressed JSONL, decoded here once and reused by
// both the Prompt History and Transcripts features.

/// One discovered session log on disk.
pub(crate) struct DshSessionFile {
    pub session_id: String,
    pub log_path: std::path::PathBuf,
}

/// Walk `sessions/<workspace-slug>/session-<uuid>/session.jsonl.zstd`.
pub(crate) fn discover_dsh_sessions(root: &std::path::Path) -> Vec<DshSessionFile> {
    let mut out = Vec::new();
    let sessions_root = root.join("sessions");
    let Ok(workspace_dirs) = std::fs::read_dir(&sessions_root) else { return out };
    for ws_entry in workspace_dirs.flatten() {
        if !ws_entry.path().is_dir() {
            continue;
        }
        let Ok(session_dirs) = std::fs::read_dir(ws_entry.path()) else { continue };
        for sess_entry in session_dirs.flatten() {
            let sess_path = sess_entry.path();
            if !sess_path.is_dir() {
                continue;
            }
            let log_path = sess_path.join("session.jsonl.zstd");
            if log_path.is_file() {
                out.push(DshSessionFile {
                    session_id: sess_entry.file_name().to_string_lossy().to_string(),
                    log_path,
                });
            }
        }
    }
    out
}

/// Decode a `session.jsonl.zstd` file into its JSON event lines. Returns
/// `None` if the file can't be read or decompressed (a corrupt session is
/// skipped by the caller, never panics); malformed individual lines are
/// dropped rather than failing the whole session.
pub(crate) fn decode_session_events(path: &std::path::Path) -> Option<Vec<serde_json::Value>> {
    let bytes = std::fs::read(path).ok()?;
    let decoded = zstd::decode_all(&bytes[..]).ok()?;
    let text = String::from_utf8(decoded).ok()?;
    Some(
        text.lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty() { None } else { serde_json::from_str(line).ok() }
            })
            .collect(),
    )
}

/// Session metadata resolved from `session_projcache.json` / `workspace.json`
/// (see `load_session_metadata`).
pub(crate) struct DshSessionMeta {
    pub workspace_path: String,
    pub workspace_name: String,
    pub title: Option<String>,
}

/// `<workspace path file name>`, or the path itself if it has none.
pub(crate) fn workspace_name_from_path(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

/// session id → resolved workspace path/title, built from
/// `session_projcache.json` (`identity.cwd`) and `workspace.json`.
pub(crate) fn load_session_metadata(root: &std::path::Path) -> HashMap<String, DshSessionMeta> {
    let workspaces = read_workspaces(root);
    let ws_map = build_session_workspace_map(&workspaces);
    let session_cache = read_session_cache(root);

    session_cache
        .into_iter()
        .map(|(sid, entry)| {
            let title = entry.rows.title.as_ref().map(|w| w.val.clone());
            let (workspace_path, workspace_name) = ws_map.get(&sid).cloned().unwrap_or_else(|| {
                let cwd = entry.identity.cwd.clone().unwrap_or_else(|| "unknown".to_string());
                let name = workspace_name_from_path(&cwd);
                (cwd, name)
            });
            (sid, DshSessionMeta { workspace_path, workspace_name, title })
        })
        .collect()
}

/// One session's permission/sandbox/approval snapshot, sourced from
/// `session_projcache.json`'s `rows.permissions.val` (used by the
/// "Permissions & Control" page's read-only per-session table).
pub(crate) struct DshSessionPermissions {
    pub preset: Option<String>,
    pub sandbox: Option<String>,
    pub approval: Option<String>,
}

/// session id → its recorded permission/sandbox/approval snapshot, for
/// sessions that have one. Pure parse, no filesystem access — see
/// `read_session_permissions` for the disk-backed wrapper.
pub(crate) fn parse_session_permissions(content: &str) -> HashMap<String, DshSessionPermissions> {
    parse_session_cache_file(content)
        .into_iter()
        .filter_map(|(sid, entry)| {
            let val = entry.rows.permissions?.val;
            Some((sid, DshSessionPermissions { preset: val.preset, sandbox: val.sandbox, approval: val.approval }))
        })
        .collect()
}

/// session id → its recorded permission/sandbox/approval snapshot, read from
/// `session_projcache.json` under `root`.
pub(crate) fn read_session_permissions(root: &std::path::Path) -> HashMap<String, DshSessionPermissions> {
    std::fs::read_to_string(session_cache_path(root))
        .map(|text| parse_session_permissions(&text))
        .unwrap_or_default()
}

/// Resolve a session's workspace path/name: prefer the projcache/workspace
/// metadata, falling back to the first `session` event's `cwd` for sessions
/// dsh hasn't indexed yet.
pub(crate) fn resolve_session_workspace(
    session_id: &str,
    events: &[serde_json::Value],
    metadata: &HashMap<String, DshSessionMeta>,
) -> (Option<String>, Option<String>) {
    if let Some(m) = metadata.get(session_id) {
        return (Some(m.workspace_path.clone()), Some(m.workspace_name.clone()));
    }
    let cwd = events.iter().find_map(|e| {
        if e.get("type").and_then(|t| t.as_str()) == Some("session") {
            e.get("data").and_then(|d| d.get("cwd")).and_then(|c| c.as_str()).map(String::from)
        } else {
            None
        }
    });
    let name = cwd.as_deref().map(workspace_name_from_path);
    (cwd, name)
}

// ── settings.yaml (agent-default-model) ──────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
struct SettingsYaml {
    #[serde(rename = "agent-default-model", default)]
    agent_default_model: Option<AgentDefaultModel>,
}

#[derive(Debug, Default, Deserialize)]
struct AgentDefaultModel {
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default, rename = "reasoningEffort")]
    reasoning_effort: Option<String>,
}

fn parse_default_model_yaml(content: &str) -> (Option<String>, Option<String>) {
    let Ok(doc) = serde_yaml::from_str::<SettingsYaml>(content) else {
        return (None, None);
    };
    let Some(m) = doc.agent_default_model else {
        return (None, None);
    };
    let default_model = match (m.provider, m.model) {
        (Some(p), Some(mo)) => Some(format!("{}/{}", p, mo)),
        (Some(p), None) => Some(p),
        (None, Some(mo)) => Some(mo),
        (None, None) => None,
    };
    (default_model, m.reasoning_effort)
}

fn read_default_model(root: &std::path::Path) -> (Option<String>, Option<String>) {
    std::fs::read_to_string(root.join("settings.yaml"))
        .map(|text| parse_default_model_yaml(&text))
        .unwrap_or((None, None))
}

// ── Activity derivation (mirrors kimi_v2's streak/peak-hour math) ────────────

fn ms_to_local(ms: i64) -> Option<DateTime<Local>> {
    Local.timestamp_millis_opt(ms).single()
}

fn time_range_cutoff(range: &str) -> Option<DateTime<Local>> {
    let hours = match range {
        "all" => return None,
        "5h" => 5,
        "today" | "1d" => 24,
        "week" | "7d" => 24 * 7,
        "month" | "30d" => 24 * 30,
        "90d" => 24 * 90,
        other => {
            if let Some(days) = other.strip_suffix('d').and_then(|n| n.parse::<i64>().ok()) {
                24 * days
            } else {
                return None;
            }
        }
    };
    Some(Local::now() - chrono::Duration::hours(hours))
}

// ── Build overview ────────────────────────────────────────────────────────────

fn build_overview_from_parts(
    workspaces: HashMap<String, WorkspaceEntry>,
    session_cache: HashMap<String, SessionEntry>,
    default_model: Option<String>,
    default_model_reasoning_effort: Option<String>,
    time_range: &str,
) -> DeepSeekV2Overview {
    let ws_map = build_session_workspace_map(&workspaces);
    let all_sessions = derive_sessions(&session_cache, &ws_map);

    let cutoff = time_range_cutoff(time_range);
    let now = Local::now();

    let sessions: Vec<&SessionDerived> = all_sessions
        .iter()
        .filter(|s| match cutoff {
            None => true,
            Some(c) => ms_to_local(s.last_activity_ms).map(|dt| dt >= c).unwrap_or(false),
        })
        .collect();

    let mut total_turns = 0u64;
    let mut total_steps = 0u64;
    let mut total_tokens = 0u64;
    let mut total_llm_ms = 0u64;
    let mut total_tool_ms = 0u64;
    let mut active_now = 0u64;

    // workspace path → (title, sessions, turns, steps, tokens, last_activity_ms)
    type WsAccum = (String, u64, u64, u64, u64, Option<i64>);
    let mut ws_stats: HashMap<String, WsAccum> = HashMap::new();
    // (created_at_ms, weight) — one entry per session, weighted by its turn
    // count since dsh's local caches have no per-turn timestamps.
    let mut activity_events: Vec<(i64, u64)> = Vec::new();

    for s in &sessions {
        total_turns += s.turns;
        total_steps += s.steps;
        total_tokens += s.tokens;
        total_llm_ms += s.llm_ms;
        total_tool_ms += s.tool_ms;

        if let Some(last) = ms_to_local(s.last_activity_ms) {
            if (now - last).num_minutes() <= ACTIVE_NOW_MINUTES {
                active_now += 1;
            }
        }

        let entry = ws_stats
            .entry(s.workspace_path.clone())
            .or_insert_with(|| (s.workspace_name.clone(), 0, 0, 0, 0, None));
        entry.1 += 1;
        entry.2 += s.turns;
        entry.3 += s.steps;
        entry.4 += s.tokens;
        entry.5 = Some(entry.5.map_or(s.last_activity_ms, |cur: i64| cur.max(s.last_activity_ms)));

        activity_events.push((s.created_at_ms, s.turns.max(1)));
    }

    // Hour-of-day distribution + peak hour.
    let mut hour_counts = vec![0u64; 24];
    for &(ts_ms, weight) in &activity_events {
        if let Some(local) = ms_to_local(ts_ms) {
            hour_counts[local.hour() as usize] += weight;
        }
    }
    let max_val = hour_counts.iter().copied().max().unwrap_or(0);
    let peak_hour = if max_val == 0 {
        None
    } else {
        hour_counts.iter().position(|&v| v == max_val).map(|i| i as u32)
    };

    // Daily activity + first-session date.
    let mut daily: std::collections::BTreeMap<String, u64> = Default::default();
    let mut first_ts: Option<DateTime<Local>> = None;
    for &(ts_ms, weight) in &activity_events {
        if let Some(local) = ms_to_local(ts_ms) {
            *daily.entry(local.format("%Y-%m-%d").to_string()).or_insert(0) += weight;
            if first_ts.is_none_or(|f| local < f) {
                first_ts = Some(local);
            }
        }
    }
    let daily_activity: Vec<DailyActivity> = daily
        .iter()
        .map(|(date, count)| DailyActivity {
            date: date.clone(),
            message_count: *count,
            session_count: 0,
            tool_call_count: 0,
        })
        .collect();

    // Streaks / active-days / weekday — same math as kimi_v2/claude_v2.
    let mut active_dates: Vec<chrono::NaiveDate> = daily
        .keys()
        .filter_map(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        .collect();
    active_dates.sort();
    active_dates.dedup();

    let active_days = active_dates.len() as u32;
    let total_days: u32 = {
        let today = Local::now().date_naive();
        active_dates
            .first()
            .map(|first| (today - *first).num_days().max(1) as u32)
            .unwrap_or(0)
    };

    let longest_streak: u32 = if active_dates.is_empty() {
        0
    } else {
        let mut max_streak = 1u32;
        let mut cur = 1u32;
        for w in active_dates.windows(2) {
            if w[1] - w[0] == chrono::Duration::days(1) {
                cur += 1;
                if cur > max_streak {
                    max_streak = cur;
                }
            } else {
                cur = 1;
            }
        }
        max_streak
    };

    let current_streak: u32 = if active_dates.is_empty() {
        0
    } else {
        let today = Local::now().date_naive();
        let last = *active_dates.last().unwrap();
        if (today - last).num_days() > 1 {
            0
        } else {
            let mut streak = 1u32;
            for w in active_dates.windows(2).rev() {
                if w[1] - w[0] == chrono::Duration::days(1) {
                    streak += 1;
                } else {
                    break;
                }
            }
            streak
        }
    };

    let most_active_weekday: Option<String> = {
        let mut weekday_counts: [u64; 7] = [0; 7];
        for (date, count) in &daily {
            if let Ok(d) = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d") {
                weekday_counts[d.weekday().num_days_from_monday() as usize] += *count;
            }
        }
        let max_val = weekday_counts.iter().copied().max().unwrap_or(0);
        if max_val == 0 {
            None
        } else {
            let idx = weekday_counts.iter().position(|&v| v == max_val).unwrap();
            Some(
                ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"][idx]
                    .to_string(),
            )
        }
    };

    let mut workspaces_out: Vec<DeepSeekWorkspaceStat> = ws_stats
        .into_iter()
        .map(|(path, (title, sess, turns, steps, tokens, last))| DeepSeekWorkspaceStat {
            path,
            title,
            sessions: sess,
            turns,
            steps,
            tokens,
            last_activity: last.and_then(ms_to_local).map(|dt| dt.to_rfc3339()),
        })
        .collect();
    workspaces_out.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));

    let mut recent_sessions: Vec<DeepSeekSessionStat> = sessions
        .iter()
        .map(|s| DeepSeekSessionStat {
            session_id: s.session_id.clone(),
            title: s.title.clone(),
            workspace_path: s.workspace_path.clone(),
            workspace_name: s.workspace_name.clone(),
            created_at: ms_to_local(s.created_at_ms).map(|dt| dt.to_rfc3339()).unwrap_or_default(),
            turns: s.turns,
            steps: s.steps,
            tokens: s.tokens,
        })
        .collect();
    recent_sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    DeepSeekV2Overview {
        connected: !session_cache.is_empty() || !workspaces.is_empty(),
        connection_method: "local-file".into(),
        default_model,
        default_model_reasoning_effort,
        total_sessions: sessions.len() as u64,
        total_turns,
        total_steps,
        total_tokens,
        total_llm_ms,
        total_tool_ms,
        active_now,
        hour_counts,
        daily_activity,
        first_session_date: first_ts.map(|f| f.format("%Y-%m-%d").to_string()),
        active_days,
        total_days,
        longest_streak,
        current_streak,
        most_active_weekday,
        peak_hour,
        workspaces: workspaces_out,
        recent_sessions,
        auth_mode: default_auth_mode(),
        balance: None,
        balance_connected: false,
    }
}

fn build_overview(time_range: &str) -> DeepSeekV2Overview {
    let Some(root) = dsh_root() else {
        return build_overview_from_parts(HashMap::new(), HashMap::new(), None, None, time_range);
    };
    let workspaces = read_workspaces(&root);
    let session_cache = read_session_cache(&root);
    let (default_model, reasoning_effort) = read_default_model(&root);
    let connected = session_cache_path(&root).is_file();
    let mut overview =
        build_overview_from_parts(workspaces, session_cache, default_model, reasoning_effort, time_range);
    overview.connected = connected;
    overview
}

// ── DeepSeek balance (folds in the existing `deepseek.rs` provider) ──────────
// Kept off the local-file cache so a missing/rejected API key never blanks
// local session analytics.

fn deepseek_credential_fingerprint() -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    match crate::analytics::token_store::get_provider_token("deepseek", "api-key") {
        Ok(Some(key)) => {
            let mut h = DefaultHasher::new();
            key.hash(&mut h);
            h.finish()
        }
        _ => 0,
    }
}

#[derive(Clone, Default)]
struct BalanceBundle {
    balance: Option<DeepSeekBalance>,
    connected: bool,
}

fn fetch_deepseek_balance() -> BalanceBundle {
    let analytics = crate::analytics::deepseek::fetch_deepseek_analytics();
    BalanceBundle {
        connected: analytics.status.connected,
        balance: analytics
            .credit_usage
            .map(|c| DeepSeekBalance { available: c.remaining, currency: c.currency }),
    }
}

struct BalanceCache {
    cred_fp: u64,
    fetched_at: Instant,
    data: BalanceBundle,
}

lazy_static::lazy_static! {
    static ref BALANCE_CACHE: Mutex<Option<BalanceCache>> = Mutex::new(None);
}

const BALANCE_TTL_SECONDS: u64 = 60;

fn fetch_deepseek_balance_cached(force_refresh: bool) -> BalanceBundle {
    let cred_fp = deepseek_credential_fingerprint();
    if !force_refresh {
        if let Ok(guard) = BALANCE_CACHE.lock() {
            if let Some(c) = guard.as_ref() {
                if c.cred_fp == cred_fp && c.fetched_at.elapsed() < Duration::from_secs(BALANCE_TTL_SECONDS) {
                    return c.data.clone();
                }
            }
        }
    }
    let data = fetch_deepseek_balance();
    if let Ok(mut guard) = BALANCE_CACHE.lock() {
        *guard = Some(BalanceCache { cred_fp, fetched_at: Instant::now(), data: data.clone() });
    }
    data
}

// ── Tauri commands ────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_deepseek_v2_overview(
    time_range: String,
    force_refresh: bool,
) -> Result<DeepSeekV2Overview, String> {
    let mut overview = {
        let cached = if force_refresh {
            None
        } else {
            CACHE.lock().ok().and_then(|cache| {
                cache
                    .overview
                    .get(&time_range)
                    .filter(|e| e.is_valid(cache.ttl_seconds))
                    .map(|e| e.data.clone())
            })
        };
        match cached {
            Some(o) => o,
            None => {
                let range = time_range.clone();
                let built = tokio::task::spawn_blocking(move || build_overview(&range))
                    .await
                    .map_err(|e| format!("Task error: {}", e))?;
                if let Ok(mut cache) = CACHE.lock() {
                    cache.overview.insert(time_range.clone(), CacheEntry::new(built.clone()));
                }
                built
            }
        }
    };

    let balance = tokio::task::spawn_blocking(move || fetch_deepseek_balance_cached(force_refresh))
        .await
        .unwrap_or_default();
    overview.balance = balance.balance;
    overview.balance_connected = balance.connected;

    Ok(overview)
}

#[tauri::command]
pub async fn get_deepseek_v2_connection_status() -> Result<bool, String> {
    Ok(dsh_root().map(|r| session_cache_path(&r).is_file()).unwrap_or(false))
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const WORKSPACE_FIXTURE: &str = r#"{
        "unit": { "name": "workspace", "version": 2 },
        "global": { "initialized": true, "workspaceIds": ["ws-a", "ws-b"], "archivedSessionIds": [] },
        "tables": {
            "workspaces": {
                "ws-a": {
                    "path": "/Users/dev/projects/alpha",
                    "title": "alpha",
                    "sessionIds": ["session-1", "session-2"],
                    "createdAt": "2026-08-20T10:00:00.000Z",
                    "updatedAt": "2026-08-21T10:00:00.000Z"
                },
                "ws-b": {
                    "path": "/Users/dev/projects/beta",
                    "title": "beta",
                    "sessionIds": ["session-3"],
                    "createdAt": "2026-08-22T10:00:00.000Z",
                    "updatedAt": "2026-08-22T10:00:00.000Z"
                }
            }
        }
    }"#;

    const SESSION_CACHE_FIXTURE: &str = r#"{
        "unit": { "name": "session_projcache", "version": 3 },
        "global": null,
        "tables": {
            "sessions": {
                "session-1": {
                    "identity": { "createdAt": 1787661503700, "cwd": "/Users/dev/projects/alpha" },
                    "rows": {
                        "sessionStats": { "ver": 1, "seq": 1, "val": {
                            "turns": 2, "steps": 6, "llmMs": 42735, "toolMs": 690,
                            "decodeTokens": 2702, "lastTurn": 2
                        }},
                        "title": { "ver": 1, "seq": 1, "val": "First session" },
                        "tokenUsage": { "ver": 1, "seq": 1, "val": {
                            "totals": {
                                "uncachedInputTokens": 30913, "outputTokens": 2702,
                                "cacheReadTokens": 61312, "cacheWriteTokens": 0
                            }
                        }},
                        "sessionListMetadata": { "ver": 1, "seq": 1, "val": {
                            "blank": false, "lastPromptAt": 1787661634424
                        }}
                    }
                },
                "session-2": {
                    "identity": { "createdAt": 1787575103700, "cwd": "/Users/dev/projects/alpha" },
                    "rows": {
                        "sessionStats": { "ver": 1, "seq": 1, "val": {
                            "turns": 1, "steps": 2, "llmMs": 1000, "toolMs": 100,
                            "decodeTokens": 500, "lastTurn": 1
                        }},
                        "title": { "ver": 1, "seq": 1, "val": "Second session" }
                    }
                },
                "session-3": {
                    "identity": { "createdAt": 1787488703700, "cwd": "/Users/dev/projects/beta" },
                    "rows": {
                        "sessionStats": { "ver": 1, "seq": 1, "val": {
                            "turns": 3, "steps": 9, "llmMs": 5000, "toolMs": 200,
                            "decodeTokens": 900, "lastTurn": 3
                        }},
                        "title": { "ver": 1, "seq": 1, "val": "Beta session" }
                    }
                }
            }
        }
    }"#;

    const SETTINGS_YAML_FIXTURE: &str = "ui-onboarding:\n  welcomeNoticeVersion: 2026-08-13.1\nagent-default-model:\n  provider: deepseek-official\n  model: deepseek-v4-pro\n  reasoningEffort: max\n";

    #[test]
    fn parses_workspace_file_into_map() {
        let workspaces = parse_workspace_file(WORKSPACE_FIXTURE);
        assert_eq!(workspaces.len(), 2);
        let alpha = workspaces.get("ws-a").expect("ws-a present");
        assert_eq!(alpha.path, "/Users/dev/projects/alpha");
        assert_eq!(alpha.title, "alpha");
        assert_eq!(alpha.session_ids, ["session-1", "session-2"]);
    }

    #[test]
    fn parses_session_cache_and_computes_token_totals() {
        let sessions = parse_session_cache_file(SESSION_CACHE_FIXTURE);
        assert_eq!(sessions.len(), 3);

        // session-1 has richer tokenUsage.totals — prefer that sum over decodeTokens.
        let s1 = sessions.get("session-1").expect("session-1 present");
        assert_eq!(session_token_count(&s1.rows), 30913 + 2702 + 61312);

        // session-2 has no tokenUsage — falls back to sessionStats.decodeTokens.
        let s2 = sessions.get("session-2").expect("session-2 present");
        assert_eq!(session_token_count(&s2.rows), 500);
    }

    #[test]
    fn parses_default_model_from_settings_yaml() {
        let (model, effort) = parse_default_model_yaml(SETTINGS_YAML_FIXTURE);
        assert_eq!(model.as_deref(), Some("deepseek-official/deepseek-v4-pro"));
        assert_eq!(effort.as_deref(), Some("max"));
    }

    #[test]
    fn build_overview_computes_totals_and_workspace_rollup() {
        let workspaces = parse_workspace_file(WORKSPACE_FIXTURE);
        let sessions = parse_session_cache_file(SESSION_CACHE_FIXTURE);
        let overview = build_overview_from_parts(
            workspaces,
            sessions,
            Some("deepseek-official/deepseek-v4-pro".into()),
            Some("max".into()),
            "all",
        );

        assert_eq!(overview.total_sessions, 3);
        assert_eq!(overview.total_turns, 2 + 1 + 3);
        assert_eq!(overview.total_steps, 6 + 2 + 9);
        assert_eq!(overview.total_tokens, (30913 + 2702 + 61312) + 500 + 900);
        assert_eq!(overview.total_llm_ms, 42735 + 1000 + 5000);
        assert_eq!(overview.default_model.as_deref(), Some("deepseek-official/deepseek-v4-pro"));

        assert_eq!(overview.workspaces.len(), 2);
        let alpha = overview.workspaces.iter().find(|w| w.path == "/Users/dev/projects/alpha").unwrap();
        assert_eq!(alpha.sessions, 2);
        assert_eq!(alpha.turns, 3);
        let beta = overview.workspaces.iter().find(|w| w.path == "/Users/dev/projects/beta").unwrap();
        assert_eq!(beta.sessions, 1);
        assert_eq!(beta.turns, 3);
    }

    #[test]
    fn recent_sessions_are_ordered_newest_first() {
        let workspaces = parse_workspace_file(WORKSPACE_FIXTURE);
        let sessions = parse_session_cache_file(SESSION_CACHE_FIXTURE);
        let overview = build_overview_from_parts(workspaces, sessions, None, None, "all");

        assert_eq!(overview.recent_sessions.len(), 3);
        // session-1 (createdAt 1787661503700) is the newest, session-3 the oldest.
        assert_eq!(overview.recent_sessions[0].session_id, "session-1");
        assert_eq!(overview.recent_sessions.last().unwrap().session_id, "session-3");
        // Sorted strictly descending.
        for w in overview.recent_sessions.windows(2) {
            assert!(w[0].created_at >= w[1].created_at);
        }
    }

    #[test]
    fn unknown_session_falls_back_to_cwd_as_workspace() {
        let sessions: HashMap<String, SessionEntry> = parse_session_cache_file(SESSION_CACHE_FIXTURE);
        let overview = build_overview_from_parts(HashMap::new(), sessions, None, None, "all");
        // No workspace.json entries — every session falls back to its `identity.cwd`.
        let paths: Vec<&str> = overview.workspaces.iter().map(|w| w.path.as_str()).collect();
        assert!(paths.contains(&"/Users/dev/projects/alpha"));
        assert!(paths.contains(&"/Users/dev/projects/beta"));
    }

    const SESSION_PERMISSIONS_FIXTURE: &str = r#"{
        "tables": {
            "sessions": {
                "session-1": {
                    "identity": { "createdAt": 1787661503700 },
                    "rows": {
                        "permissions": { "ver": 1, "seq": 1, "val": {
                            "preset": "workspace-write",
                            "sandbox": "workspace-write",
                            "approval": "ask"
                        }}
                    }
                },
                "session-2": {
                    "identity": { "createdAt": 1787661503700 },
                    "rows": {}
                }
            }
        }
    }"#;

    #[test]
    fn parses_session_permissions_from_projcache() {
        let permissions = parse_session_permissions(SESSION_PERMISSIONS_FIXTURE);
        assert_eq!(permissions.len(), 1, "only sessions with a recorded permissions row are included");

        let p = permissions.get("session-1").expect("session-1 present");
        assert_eq!(p.preset.as_deref(), Some("workspace-write"));
        assert_eq!(p.sandbox.as_deref(), Some("workspace-write"));
        assert_eq!(p.approval.as_deref(), Some("ask"));

        assert!(!permissions.contains_key("session-2"), "session-2 has no permissions row");
    }

    #[test]
    fn empty_input_yields_disconnected_empty_overview() {
        let overview = build_overview_from_parts(HashMap::new(), HashMap::new(), None, None, "all");
        assert!(!overview.connected);
        assert_eq!(overview.total_sessions, 0);
        assert!(overview.workspaces.is_empty());
        assert!(overview.recent_sessions.is_empty());
        assert_eq!(overview.auth_mode, "api");
    }
}
