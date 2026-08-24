//! Kimi Code Analytics V2 — local session analytics (Phase 1, no auth/API).
//! Reads Kimi Code CLI's local files under `~/.kimi`:
//!   sessions/<md5(project path)>/<sessionId>/{context.jsonl,wire.jsonl,state.json}
//!   user-history/<md5(project path)>.jsonl
//!   kimi.json (work_dirs → md5→path map)  ·  config.toml (model catalog)
//! Modeled on `claude_v2.rs`.

use crate::commands::session_stats::DailyActivity;
use chrono::{Datelike, Local, TimeZone, Timelike};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

// ── V2 Types ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KimiModelInfo {
    pub id: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub max_context_size: Option<u64>,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KimiProjectStat {
    pub project_path: String,
    pub project_name: String,
    pub sessions: u64,
    pub messages: u64,
    pub context_tokens_peak: u64,
    pub last_activity: Option<String>,
    /// Most recent session id (from kimi.json work_dirs) — used for `kimi --resume`.
    pub last_session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KimiPromptEntry {
    pub content: String,
    pub project_path: String,
    pub project_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KimiPromptHistoryPage {
    pub entries: Vec<KimiPromptEntry>,
    pub total_count: u64,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KimiV2Overview {
    pub connected: bool,
    pub connection_method: String,
    pub default_model: Option<String>,

    // Totals
    pub total_sessions: u64,
    pub total_messages: u64,
    pub user_messages: u64,
    pub assistant_messages: u64,
    pub total_turns: u64,
    pub context_tokens_peak: u64,
    pub active_now: u64,

    // Activity
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
    pub projects: Vec<KimiProjectStat>,
    pub models: Vec<KimiModelInfo>,
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

struct KimiV2Cache {
    overview: HashMap<String, CacheEntry<KimiV2Overview>>,
    ttl_seconds: u64,
}

lazy_static::lazy_static! {
    static ref CACHE: Mutex<KimiV2Cache> = Mutex::new(KimiV2Cache {
        overview: HashMap::new(),
        ttl_seconds: 300,
    });
}

/// Sessions modified within this many minutes count as "active now".
const ACTIVE_NOW_MINUTES: i64 = 15;

// ── Paths ────────────────────────────────────────────────────────────────────

fn kimi_root() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|h| h.join(".kimi"))
}

/// The session directory name for a project path is the md5 hex of the path.
fn md5_dir(path: &str) -> String {
    format!("{:x}", md5::compute(path.as_bytes()))
}

// ── kimi.json (work_dirs) ────────────────────────────────────────────────────

struct WorkDir {
    path: String,
    last_session_id: Option<String>,
}

fn read_work_dirs() -> Vec<WorkDir> {
    let Some(root) = kimi_root() else { return vec![] };
    let Ok(text) = std::fs::read_to_string(root.join("kimi.json")) else { return vec![] };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else { return vec![] };
    json.get("work_dirs")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|w| {
                    let path = w.get("path").and_then(|p| p.as_str())?.to_string();
                    let last_session_id = w
                        .get("last_session_id")
                        .and_then(|s| s.as_str())
                        .map(String::from);
                    Some(WorkDir { path, last_session_id })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// md5(dir) → (project path, last_session_id).
fn build_dir_map() -> HashMap<String, (String, Option<String>)> {
    read_work_dirs()
        .into_iter()
        .map(|w| (md5_dir(&w.path), (w.path, w.last_session_id)))
        .collect()
}

// ── context.jsonl ────────────────────────────────────────────────────────────

#[derive(Default, Debug, PartialEq)]
struct ContextCounts {
    user: u64,
    assistant: u64,
    /// Peak cumulative context size (max `_usage.token_count`).
    peak_context: u64,
}

fn parse_context(content: &str) -> ContextCounts {
    let mut c = ContextCounts::default();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(obj) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        match obj.get("role").and_then(|r| r.as_str()) {
            Some("user") => c.user += 1,
            Some("assistant") => c.assistant += 1,
            Some("_usage") => {
                if let Some(tc) = obj.get("token_count").and_then(|t| t.as_u64()) {
                    c.peak_context = c.peak_context.max(tc);
                }
            }
            _ => {}
        }
    }
    c
}

// ── wire.jsonl ───────────────────────────────────────────────────────────────

#[derive(Default, Debug)]
struct WireData {
    turn_count: u64,
    /// Unix timestamps of `TurnBegin` messages — the activity signal.
    turn_timestamps: Vec<f64>,
    /// Latest timestamp across all wire messages (for "active now").
    last_timestamp: Option<f64>,
}

fn parse_wire(content: &str) -> WireData {
    let mut w = WireData::default();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(obj) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        let Some(ts) = obj.get("timestamp").and_then(|t| t.as_f64()) else { continue };
        w.last_timestamp = Some(w.last_timestamp.map_or(ts, |cur: f64| cur.max(ts)));
        if obj.get("message").and_then(|m| m.get("type")).and_then(|t| t.as_str()) == Some("TurnBegin") {
            w.turn_count += 1;
            w.turn_timestamps.push(ts);
        }
    }
    w
}

// ── config.toml (model catalog) ──────────────────────────────────────────────
// Minimal hand-parse: we only need `default_model`, and the `[models."<id>"]`
// tables' `provider` / `model` / `max_context_size` / `capabilities`. We never
// touch the `[providers.*]` tables — they hold the API key.

fn toml_string_value(line: &str) -> Option<String> {
    let (_, rhs) = line.split_once('=')?;
    let rhs = rhs.trim();
    Some(rhs.trim_matches('"').to_string())
}

fn toml_string_array(line: &str) -> Vec<String> {
    let Some((_, rhs)) = line.split_once('=') else { return vec![] };
    let rhs = rhs.trim().trim_start_matches('[').trim_end_matches(']');
    rhs.split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn parse_config(content: &str) -> (Option<String>, Vec<KimiModelInfo>) {
    let mut default_model: Option<String> = None;
    let mut models: Vec<KimiModelInfo> = vec![];
    let mut cur: Option<KimiModelInfo> = None;

    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('[') {
            // New table header — flush the current model.
            if let Some(m) = cur.take() {
                models.push(m);
            }
            let header = rest.trim_end_matches(']');
            if let Some(id) = header.strip_prefix("models.") {
                cur = Some(KimiModelInfo {
                    id: id.trim().trim_matches('"').to_string(),
                    provider: None,
                    model: None,
                    max_context_size: None,
                    capabilities: vec![],
                });
            }
            continue;
        }
        if cur.is_none() {
            if default_model.is_none() && line.starts_with("default_model") {
                default_model = toml_string_value(line);
            }
            continue;
        }
        let m = cur.as_mut().unwrap();
        if line.starts_with("provider") {
            m.provider = toml_string_value(line);
        } else if line.starts_with("max_context_size") {
            m.max_context_size = line.split_once('=').and_then(|(_, v)| v.trim().parse::<u64>().ok());
        } else if line.starts_with("model") {
            m.model = toml_string_value(line);
        } else if line.starts_with("capabilities") {
            m.capabilities = toml_string_array(line);
        }
    }
    if let Some(m) = cur.take() {
        models.push(m);
    }
    (default_model, models)
}

fn read_model_catalog() -> (Option<String>, Vec<KimiModelInfo>) {
    let Some(root) = kimi_root() else { return (None, vec![]) };
    match std::fs::read_to_string(root.join("config.toml")) {
        Ok(text) => parse_config(&text),
        Err(_) => (None, vec![]),
    }
}

// ── Session enumeration ──────────────────────────────────────────────────────

struct SessionData {
    project_path: String,
    user: u64,
    assistant: u64,
    peak_context: u64,
    turn_count: u64,
    turn_timestamps: Vec<f64>,
    last_timestamp: Option<f64>,
}

fn read_sessions() -> Vec<SessionData> {
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
        // Resolve project path from kimi.json; fall back to the md5 name if unknown.
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
            let ctx = std::fs::read_to_string(sess_path.join("context.jsonl"))
                .map(|t| parse_context(&t))
                .unwrap_or_default();
            let wire = std::fs::read_to_string(sess_path.join("wire.jsonl"))
                .map(|t| parse_wire(&t))
                .unwrap_or_default();
            out.push(SessionData {
                project_path: project_path.clone(),
                user: ctx.user,
                assistant: ctx.assistant,
                peak_context: ctx.peak_context,
                turn_count: wire.turn_count,
                turn_timestamps: wire.turn_timestamps,
                last_timestamp: wire.last_timestamp,
            });
        }
    }
    out
}

// ── Activity derivation (mirrors claude_v2 streak/peak-hour math) ─────────────

fn hour_counts_and_peak(turn_unix: &[f64]) -> (Vec<u64>, Option<u32>) {
    let mut hour_counts = vec![0u64; 24];
    for &ts in turn_unix {
        if let Some(local) = unix_to_local(ts) {
            hour_counts[local.hour() as usize] += 1;
        }
    }
    let max_val = hour_counts.iter().copied().max().unwrap_or(0);
    let peak = if max_val == 0 {
        None
    } else {
        hour_counts.iter().position(|&v| v == max_val).map(|i| i as u32)
    };
    (hour_counts, peak)
}

fn unix_to_local(ts: f64) -> Option<chrono::DateTime<Local>> {
    Local.timestamp_opt(ts as i64, 0).single()
}

fn time_range_cutoff(range: &str) -> Option<chrono::DateTime<Local>> {
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

// ── Build Overview ───────────────────────────────────────────────────────────

fn build_overview(time_range: &str) -> KimiV2Overview {
    let (default_model, models) = read_model_catalog();
    let all_sessions = read_sessions();
    let connected = kimi_root().map(|r| r.join("sessions").is_dir()).unwrap_or(false);

    let cutoff = time_range_cutoff(time_range);
    let now = Local::now();

    // A session is "in range" if its latest activity is at/after the cutoff.
    let sessions: Vec<&SessionData> = all_sessions
        .iter()
        .filter(|s| match cutoff {
            None => true,
            Some(c) => s
                .last_timestamp
                .and_then(unix_to_local)
                .map(|dt| dt >= c)
                .unwrap_or(false),
        })
        .collect();

    let mut total_messages = 0u64;
    let mut user_messages = 0u64;
    let mut assistant_messages = 0u64;
    let mut total_turns = 0u64;
    let mut context_tokens_peak = 0u64;
    let mut active_now = 0u64;

    // project path → (sessions, messages, context peak, last activity unix)
    let mut project_map: HashMap<String, (u64, u64, u64, Option<f64>)> = HashMap::new();
    // Turn events (respecting the time-range cutoff) for activity stats.
    let mut turn_events: Vec<f64> = Vec::new();

    for s in &sessions {
        user_messages += s.user;
        assistant_messages += s.assistant;
        total_messages += s.user + s.assistant;
        total_turns += s.turn_count;
        context_tokens_peak = context_tokens_peak.max(s.peak_context);

        if let Some(last) = s.last_timestamp.and_then(unix_to_local) {
            if (now - last).num_minutes() <= ACTIVE_NOW_MINUTES {
                active_now += 1;
            }
        }

        let entry = project_map.entry(s.project_path.clone()).or_insert((0, 0, 0, None));
        entry.0 += 1;
        entry.1 += s.user + s.assistant;
        entry.2 = entry.2.max(s.peak_context);
        if let Some(ts) = s.last_timestamp {
            entry.3 = Some(entry.3.map_or(ts, |cur: f64| cur.max(ts)));
        }

        for &ts in &s.turn_timestamps {
            if cutoff.map(|c| unix_to_local(ts).map(|dt| dt >= c).unwrap_or(false)).unwrap_or(true) {
                turn_events.push(ts);
            }
        }
    }

    // Daily activity + hour counts from turn events (local time).
    let (hour_counts, peak_hour) = hour_counts_and_peak(&turn_events);

    // local date → (turn count, distinct session-ish via day) — Kimi has no
    // per-turn session id in wire, so session_count tracks distinct active days
    // is not meaningful; we approximate session_count as 0 and use message_count.
    let mut daily: std::collections::BTreeMap<String, u64> = Default::default();
    let mut first_ts: Option<chrono::DateTime<Local>> = None;
    for &ts in &turn_events {
        if let Some(local) = unix_to_local(ts) {
            *daily.entry(local.format("%Y-%m-%d").to_string()).or_insert(0) += 1;
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

    // Streaks / active-days / weekday — same math as claude_v2.
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

    // Resolve last_session_id per project from kimi.json.
    let dir_map = build_dir_map();
    let last_session_by_path: HashMap<String, Option<String>> = dir_map.into_values().collect();

    let mut projects: Vec<KimiProjectStat> = project_map
        .into_iter()
        .map(|(path, (sessions, messages, peak, last))| {
            let name = std::path::Path::new(&path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.clone());
            let last_activity = last.and_then(unix_to_local).map(|dt| dt.to_rfc3339());
            KimiProjectStat {
                project_path: path.clone(),
                project_name: name,
                sessions,
                messages,
                context_tokens_peak: peak,
                last_activity,
                last_session_id: last_session_by_path.get(&path).cloned().flatten(),
            }
        })
        .collect();
    projects.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));

    KimiV2Overview {
        connected,
        connection_method: "local-file".into(),
        default_model,
        total_sessions: sessions.len() as u64,
        total_messages,
        user_messages,
        assistant_messages,
        total_turns,
        context_tokens_peak,
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
        projects,
        models,
    }
}

// ── Prompt history (user-history/<md5>.jsonl) ────────────────────────────────

fn read_prompt_history() -> Vec<KimiPromptEntry> {
    let Some(root) = kimi_root() else { return vec![] };
    let dir_map = build_dir_map();
    let hist_dir = root.join("user-history");
    let mut out = Vec::new();

    let Ok(files) = std::fs::read_dir(&hist_dir) else { return out };
    for file in files.flatten() {
        let path = file.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let md5_name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let project_path = dir_map
            .get(&md5_name)
            .map(|(p, _)| p.clone())
            .unwrap_or_else(|| md5_name.clone());
        let project_name = std::path::Path::new(&project_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| project_path.clone());

        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let Ok(obj) = serde_json::from_str::<serde_json::Value>(line) else { continue };
            if let Some(content) = obj.get("content").and_then(|c| c.as_str()) {
                out.push(KimiPromptEntry {
                    content: content.to_string(),
                    project_path: project_path.clone(),
                    project_name: project_name.clone(),
                });
            }
        }
    }
    // Newest first (files are append-order, oldest first).
    out.reverse();
    out
}

// ── Tauri Commands ───────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_kimi_v2_overview(
    time_range: String,
    force_refresh: bool,
) -> Result<KimiV2Overview, String> {
    if !force_refresh {
        if let Ok(cache) = CACHE.lock() {
            if let Some(entry) = cache.overview.get(&time_range) {
                if entry.is_valid(cache.ttl_seconds) {
                    return Ok(entry.data.clone());
                }
            }
        }
    }

    let range = time_range.clone();
    let overview = tokio::task::spawn_blocking(move || build_overview(&range))
        .await
        .map_err(|e| format!("Task error: {}", e))?;

    if let Ok(mut cache) = CACHE.lock() {
        cache.overview.insert(time_range, CacheEntry::new(overview.clone()));
    }
    Ok(overview)
}

#[tauri::command]
pub async fn get_kimi_v2_prompt_history(
    query: Option<String>,
    page: u32,
    page_size: u32,
) -> Result<KimiPromptHistoryPage, String> {
    tokio::task::spawn_blocking(move || {
        let all = read_prompt_history();
        let filtered: Vec<KimiPromptEntry> = if let Some(ref q) = query {
            let q = q.to_lowercase();
            all.into_iter().filter(|e| e.content.to_lowercase().contains(&q)).collect()
        } else {
            all
        };
        let total_count = filtered.len() as u64;
        let start = ((page.saturating_sub(1)) * page_size) as usize;
        let entries: Vec<KimiPromptEntry> = if start < filtered.len() {
            filtered[start..].iter().take(page_size as usize).cloned().collect()
        } else {
            vec![]
        };
        KimiPromptHistoryPage { entries, total_count, page, page_size }
    })
    .await
    .map_err(|e| format!("Task error: {}", e))
}

#[tauri::command]
pub async fn get_kimi_v2_connection_status() -> Result<bool, String> {
    Ok(kimi_root().map(|r| r.join("sessions").is_dir()).unwrap_or(false))
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_dir_maps_project_path_to_session_dir() {
        // Verified on-machine: md5 hex of the project path == its sessions/<dir>.
        assert_eq!(
            md5_dir("/Users/abhijeetsingh/Downloads/openxsecurity/agentharbor-app"),
            "6741b495a1f19cbb4f94a5609e79dd0d"
        );
        assert_eq!(
            md5_dir("/Users/abhijeetsingh/Downloads/kimi-cli"),
            "f54bbe890314169df1ddac6b806df0d6"
        );
    }

    #[test]
    fn parse_context_counts_roles_and_peak() {
        let content = r#"{"role": "_system_prompt", "content": "sys"}
{"role": "_checkpoint", "id": "a"}
{"role": "user", "content": "hi yo"}
{"role": "_usage", "token_count": 12265}
{"role": "assistant", "content": [{"type": "text", "text": "yo"}]}
{"role": "_usage", "token_count": 12356}
{"role": "user", "content": "eit"}
{"role": "assistant", "content": "hey"}
"#;
        let c = parse_context(content);
        assert_eq!(c.user, 2);
        assert_eq!(c.assistant, 2);
        assert_eq!(c.peak_context, 12356);
    }

    #[test]
    fn parse_wire_counts_turns_and_last_timestamp() {
        let content = r#"{"type": "metadata", "protocol_version": "1.10"}
{"timestamp": 100.5, "message": {"type": "TurnBegin", "payload": {}}}
{"timestamp": 101.0, "message": {"type": "StepBegin", "payload": {}}}
{"timestamp": 102.0, "message": {"type": "TurnEnd", "payload": {}}}
{"timestamp": 200.0, "message": {"type": "TurnBegin", "payload": {}}}
"#;
        let w = parse_wire(content);
        assert_eq!(w.turn_count, 2);
        assert_eq!(w.turn_timestamps, vec![100.5, 200.0]);
        assert_eq!(w.last_timestamp, Some(200.0));
    }

    #[test]
    fn wire_timestamps_bucket_into_hours_and_pick_peak() {
        // Three turns share one clock hour, one lands 3h later — that busy hour wins.
        let base = 1_787_599_566.0_f64; // arbitrary unix time
        let hour = 3600.0;
        let turns = vec![base, base + 60.0, base + 120.0, base + 3.0 * hour];
        let (hours, peak) = hour_counts_and_peak(&turns);
        assert_eq!(hours.iter().sum::<u64>(), 4);
        assert_eq!(hours.iter().copied().max().unwrap(), 3);
        let peak = peak.unwrap();
        assert_eq!(hours[peak as usize], 3);
    }

    #[test]
    fn parse_config_reads_default_model_and_catalog() {
        let content = r#"default_model = "moonshot-ai/kimi-k2.7-code"
theme = "dark"

[models."moonshot-ai/kimi-k2.7-code"]
provider = "managed:moonshot-ai"
model = "kimi-k2.7-code"
max_context_size = 262144
capabilities = ["video_in", "image_in", "thinking"]

[models."moonshot-ai/kimi-k3"]
provider = "managed:moonshot-ai"
model = "kimi-k3"
max_context_size = 1048576
capabilities = ["thinking"]

[providers."managed:moonshot-ai"]
type = "kimi"
api_key = "should-not-be-parsed"
"#;
        let (default_model, models) = parse_config(content);
        assert_eq!(default_model.as_deref(), Some("moonshot-ai/kimi-k2.7-code"));
        assert_eq!(models.len(), 2);
        let first = &models[0];
        assert_eq!(first.id, "moonshot-ai/kimi-k2.7-code");
        assert_eq!(first.model.as_deref(), Some("kimi-k2.7-code"));
        assert_eq!(first.max_context_size, Some(262144));
        assert_eq!(first.capabilities, vec!["video_in", "image_in", "thinking"]);
        // The providers table (with the api_key) must never surface as a model.
        assert!(models.iter().all(|m| m.id.starts_with("moonshot-ai/")));
    }
}
