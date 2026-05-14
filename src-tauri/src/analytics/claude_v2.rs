//! Claude Code Analytics V2 — aggregated overview, timeseries, message log.
//! Combines: OAuth API (rate limits/profile) + local JSONL + stats-cache + metadata.

use crate::analytics::claude;
use crate::analytics::types::LimitState;
use crate::analytics::cost_engine;
use crate::commands::claude_history;
use crate::commands::claude_metadata;
use crate::commands::session_stats;
use crate::commands::usage;
use chrono::Datelike;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

// ── V2 Types ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStat {
    pub model: String,
    pub message_count: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub estimated_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolStat {
    pub tool_name: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeoStat {
    pub region: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectStat {
    pub project_path: String,
    pub project_name: String,
    pub message_count: u64,
    pub total_tokens: u64,
    pub estimated_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub hit_rate_percent: f64,
    pub ephemeral_5m_percent: f64,
    pub ephemeral_1h_percent: f64,
    pub total_cache_read: u64,
    pub total_cache_write: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenTimePoint {
    pub date: String,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub estimated_cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelTimePoint {
    pub date: String,
    pub models: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageLogEntry {
    pub timestamp: String,
    pub model: Option<String>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub tools: Vec<String>,
    pub service_tier: Option<String>,
    pub geo: Option<String>,
    pub speed: Option<String>,
    pub has_thinking: bool,
    pub estimated_cost: f64,
    pub project: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageLogPage {
    pub entries: Vec<MessageLogEntry>,
    pub total_count: u64,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeV2Overview {
    // OAuth data
    pub connected: bool,
    pub connection_method: String,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub org_name: Option<String>,
    pub rate_limits: Vec<crate::analytics::types::RateLimitWindow>,
    pub credit_usage: Option<crate::analytics::types::CreditUsage>,
    pub extra: HashMap<String, serde_json::Value>,

    /// Derived limit / billing health (same as `ProviderAnalytics.limit_state`).
    #[serde(default)]
    pub limit_state: Option<LimitState>,

    // Account & Billing (from /api/oauth/account)
    pub account_info: Option<serde_json::Value>,

    // Session stats
    pub total_sessions: u64,
    pub total_messages: u64,
    pub total_tool_calls: u64,
    pub longest_session_duration: u64,
    pub longest_session_messages: u64,
    pub first_session_date: Option<String>,
    pub hour_counts: Vec<u64>,
    pub daily_activity: Vec<session_stats::DailyActivity>,

    // Token summary
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_read: u64,
    pub total_cache_write: u64,
    pub total_web_searches: u64,
    pub total_web_fetches: u64,
    pub thinking_message_count: u64,
    pub total_message_count: u64,
    pub estimated_total_cost: f64,

    // Breakdowns
    pub model_breakdown: Vec<ModelStat>,
    pub tool_usage: Vec<ToolStat>,
    pub cache_stats: CacheStats,
    pub geo_breakdown: Vec<GeoStat>,
    pub project_breakdown: Vec<ProjectStat>,

    // Stats-cache per-model breakdown (cumulative, all-time)
    pub stats_cache_models: HashMap<String, session_stats::ModelUsageEntry>,

    // Active sessions
    pub active_sessions: Vec<claude_history::ActiveSession>,
    pub active_session_count: u32,

    // App info
    pub num_startups: u64,
    pub install_method: Option<String>,
    pub plugins_count: u32,
    pub custom_commands_count: u32,
    pub todos_summary: claude_metadata::TodosSummary,
    pub plans_count: u32,
    pub hooks_count: u32,
    pub file_history_checkpoints: u32,

    // /stats-derived metrics
    #[serde(default)]
    pub favorite_model: Option<String>,
    #[serde(default)]
    pub active_days: u32,
    #[serde(default)]
    pub total_days: u32,
    #[serde(default)]
    pub longest_streak: u32,
    #[serde(default)]
    pub current_streak: u32,
    #[serde(default)]
    pub most_active_weekday: Option<String>,
    #[serde(default)]
    pub peak_hour: Option<u32>,
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

struct ClaudeV2Cache {
    overview: HashMap<String, CacheEntry<ClaudeV2Overview>>,
    token_ts: HashMap<String, CacheEntry<Vec<TokenTimePoint>>>,
    model_ts: HashMap<String, CacheEntry<Vec<ModelTimePoint>>>,
    ttl_seconds: u64,
}

lazy_static::lazy_static! {
    static ref CACHE: Mutex<ClaudeV2Cache> = Mutex::new(ClaudeV2Cache {
        overview: HashMap::new(),
        token_ts: HashMap::new(),
        model_ts: HashMap::new(),
        ttl_seconds: 300,
    });
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn time_range_cutoff(range: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    use chrono::{Local, Datelike, TimeZone};

    match range {
        // Calendar day: midnight **IST** (Asia/Kolkata) → UTC, same as tray `start_today_*`
        "today" => Some(claude::claude_calendar_day_start_ist_utc()),
        // Start of this week (Monday, local time)
        "week" => {
            let today = Local::now().date_naive();
            let weekday = today.weekday().num_days_from_monday();
            let monday = today - chrono::Duration::days(weekday as i64);
            Local.from_local_datetime(&monday.and_hms_opt(0, 0, 0).unwrap())
                .single()
                .map(|dt| dt.with_timezone(&chrono::Utc))
        }
        // Start of this month (local time)
        "month" => {
            let today = Local::now().date_naive();
            let first = today.with_day(1).unwrap_or(today);
            Local.from_local_datetime(&first.and_hms_opt(0, 0, 0).unwrap())
                .single()
                .map(|dt| dt.with_timezone(&chrono::Utc))
        }
        "all" => None,
        _ => {
            // Fixed durations: "5h", "1d", "7d", "30d", "90d", etc.
            let hours = match range {
                "5h" => 5,
                "1d" => 24,
                "7d" => 24 * 7,
                "30d" => 24 * 30,
                "90d" => 24 * 90,
                other => {
                    if let Some(days) = other.strip_suffix('d').and_then(|n| n.parse::<i64>().ok()) {
                        24 * days
                    } else {
                        24 * 30
                    }
                }
            };
            Some(chrono::Utc::now() - chrono::Duration::hours(hours))
        }
    }
}

fn filter_records_by_time<'a>(records: &'a [usage::ProjectUsageRecord], range: &str) -> Vec<&'a usage::ProjectUsageRecord> {
    let cutoff = time_range_cutoff(range);
    if cutoff.is_none() {
        return records.iter().collect();
    }
    let cutoff = cutoff.unwrap();
    records.iter().filter(|r| {
        if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(&r.timestamp) {
            ts >= cutoff
        } else {
            true // include if unparseable
        }
    }).collect()
}

/// Read positive integer from serde_json (tray extra uses i64 for token/message counts).
fn extra_json_u64(v: Option<&serde_json::Value>) -> Option<u64> {
    v.and_then(|x| x.as_u64().or_else(|| x.as_i64().map(|i| i.max(0) as u64)))
}

/// When JSONL aggregation disagrees with the menu-bar scan, replace headline totals with the same
/// `analytics.extra` keys the tray uses for **today** (`start_today_*`, IST calendar day).
fn try_overlay_tray_totals(
    time_range: &str,
    extra: &HashMap<String, serde_json::Value>,
    total_input: &mut u64,
    total_output: &mut u64,
    total_cache_read: &mut u64,
    total_cache_write: &mut u64,
    total_cost: &mut f64,
) -> Option<u64> {
    let (cost_key, pfx) = match time_range {
        "today" => ("start_today_cost", "start_today"),
        _ => return None,
    };
    let cost = extra.get(cost_key).and_then(|v| v.as_f64())?;
    if cost <= 0.0 {
        return None;
    }
    let inp = extra_json_u64(extra.get(&format!("{}_input_tokens", pfx)))?;
    let out = extra_json_u64(extra.get(&format!("{}_output_tokens", pfx)))?;
    let cr = extra_json_u64(extra.get(&format!("{}_cache_read", pfx)))?;
    let cw = extra_json_u64(extra.get(&format!("{}_cache_write", pfx)))?;
    *total_input = inp;
    *total_output = out;
    *total_cache_read = cr;
    *total_cache_write = cw;
    *total_cost = cost;
    extra_json_u64(extra.get(&format!("{}_messages", pfx)))
}

/// Same JSONL file set as the menu bar scan: skip files not touched since local Monday (tray mtime rule).
fn load_project_usage_for_time_range(time_range: &str) -> Vec<usage::ProjectUsageRecord> {
    let floor = if time_range == "today" {
        Some(claude::projects_jsonl_tray_mtime_floor())
    } else {
        None
    };
    usage::read_project_usage_files_with_mtime_floor(floor).unwrap_or_default()
}

fn short_model(model: &str) -> String {
    model
        .replace("claude-", "")
        .replace("-high-thinking", " HT")
        .replace("-max-thinking-fast", " Max")
        .replace("-max-thinking", " Max")
}

fn date_from_timestamp(ts: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
        dt.format("%Y-%m-%d").to_string()
    } else {
        String::new()
    }
}

// ── Build Overview ──────────────────────────────────────────────────────────

fn build_overview(time_range: &str) -> ClaudeV2Overview {
    // 1. OAuth data
    let analytics = claude::fetch_claude_analytics();
    let connected = analytics.status.connected;
    let connection_method = analytics.status.connection_method.clone();
    let email = analytics.status.account_email.clone();

    // 1b. Account data (the goldmine endpoint)
    let account_info = claude::fetch_claude_account().ok();
    let plan = analytics.status.plan_name.clone();
    let org_name = analytics.status.org_name.clone();

    // 2. Session stats
    let stats = session_stats::get_claude_session_stats().unwrap_or_else(|_| session_stats::SessionStats {
        total_sessions: 0,
        total_messages: 0,
        longest_session: None,
        hour_counts: vec![0; 24],
        daily_activity: vec![],
        model_usage: HashMap::new(),
        first_session_date: None,
        total_cost_usd: 0.0,
    });

    let total_tool_calls: u64 = stats.daily_activity.iter().map(|d| d.tool_call_count).sum();

    // 3. JSONL records
    let all_records = load_project_usage_for_time_range(time_range);
    let records = filter_records_by_time(&all_records, time_range);

    // Aggregate tokens
    let mut total_input = 0u64;
    let mut total_output = 0u64;
    let mut total_cache_read = 0u64;
    let mut total_cache_write = 0u64;
    let mut total_web_searches = 0u64;
    let mut total_web_fetches = 0u64;
    let mut thinking_count = 0u64;
    let mut total_ephemeral_5m = 0u64;
    let mut total_ephemeral_1h = 0u64;
    let mut model_map: HashMap<String, (u64, u64, u64, u64, u64)> = HashMap::new(); // count, in, out, cr, cw
    let mut tool_map: HashMap<String, u64> = HashMap::new();
    let mut geo_map: HashMap<String, u64> = HashMap::new();
    let mut project_map: HashMap<String, (u64, u64, f64)> = HashMap::new(); // count, tokens, cost
    let mut total_cost = 0.0f64;

    for r in &records {
        if let Some(ref u) = r.usage {
            let inp = u.input_tokens.unwrap_or(0);
            let out = u.output_tokens.unwrap_or(0);
            let cr = u.cache_read_input_tokens.unwrap_or(0);
            let cw = u.cache_creation_input_tokens.unwrap_or(0);

            total_input += inp;
            total_output += out;
            total_cache_read += cr;
            total_cache_write += cw;
            total_web_searches += u.web_search_requests.unwrap_or(0);
            total_web_fetches += u.web_fetch_requests.unwrap_or(0);
            total_ephemeral_5m += u.cache_ephemeral_5m_tokens.unwrap_or(0);
            total_ephemeral_1h += u.cache_ephemeral_1h_tokens.unwrap_or(0);

            // Model breakdown
            let model = r.model.as_deref().unwrap_or("unknown");
            let entry = model_map.entry(model.to_string()).or_insert((0, 0, 0, 0, 0));
            entry.0 += 1;
            entry.1 += inp;
            entry.2 += out;
            entry.3 += cr;
            entry.4 += cw;

            // Cost
            let msg_cost = cost_engine::estimate_cost(
                Some(model),
                &cost_engine::TokensForCost { input: inp, output: out, cache_read: cr, cache_write: cw },
            );
            total_cost += msg_cost;

            // Geo — skip empty and "not_available" values
            if let Some(ref geo) = u.inference_geo {
                let g = geo.trim();
                if !g.is_empty() && g != "not_available" && g != "unknown" && g != "null" {
                    *geo_map.entry(g.to_string()).or_insert(0u64) += 1;
                }
            }

            // Project
            if let Some(ref proj) = r.project_path {
                let entry = project_map.entry(proj.to_string()).or_insert((0u64, 0u64, 0.0f64));
                entry.0 += 1;
                entry.1 += inp + out + cr + cw;
                entry.2 += msg_cost;
            }
        }

        if r.has_thinking {
            thinking_count += 1;
        }

        // Tools
        for tool in &r.tools_used {
            *tool_map.entry(tool.to_string()).or_insert(0u64) += 1;
        }
    }

    let tray_overlay_msgs = try_overlay_tray_totals(
        time_range,
        &analytics.extra,
        &mut total_input,
        &mut total_output,
        &mut total_cache_read,
        &mut total_cache_write,
        &mut total_cost,
    );
    let strip_breakdowns = tray_overlay_msgs.is_some();
    if strip_breakdowns {
        thinking_count = 0;
    }

    // Build model breakdown
    let model_breakdown: Vec<ModelStat> = if strip_breakdowns {
        vec![]
    } else {
        let mut m: Vec<ModelStat> = model_map.into_iter().map(|(model, (count, inp, out, cr, cw))| {
            let cost = cost_engine::estimate_cost(
                Some(&model),
                &cost_engine::TokensForCost { input: inp, output: out, cache_read: cr, cache_write: cw },
            );
            ModelStat {
                model: short_model(&model),
                message_count: count,
                input_tokens: inp,
                output_tokens: out,
                cache_read_tokens: cr,
                cache_write_tokens: cw,
                estimated_cost_usd: cost,
            }
        }).collect();
        m.sort_by_key(|stat| std::cmp::Reverse(stat.message_count));
        m
    };

    // Build tool usage
    let tool_usage: Vec<ToolStat> = if strip_breakdowns {
        vec![]
    } else {
        let mut t: Vec<ToolStat> = tool_map.into_iter()
            .map(|(name, count)| ToolStat { tool_name: name, count })
            .collect();
        t.sort_by_key(|stat| std::cmp::Reverse(stat.count));
        t
    };

    // Build geo breakdown
    let geo_breakdown: Vec<GeoStat> = if strip_breakdowns {
        vec![]
    } else {
        let mut g: Vec<GeoStat> = geo_map.into_iter()
            .map(|(region, count)| GeoStat { region, count })
            .collect();
        g.sort_by_key(|stat| std::cmp::Reverse(stat.count));
        g
    };

    // Build project breakdown
    // Project paths are decoded from Claude's encoding where ALL dashes become slashes.
    // So "agentdock-project" becomes "agentdock/project" and file_name() = "project" (wrong).
    // Fix: scan ~/.claude/projects/ dirs, match by decoded path, use raw dir name for display.
    let home_dir = dirs::home_dir().unwrap_or_default();
    let projects_dir = home_dir.join(".claude").join("projects");
    let raw_dir_map: HashMap<String, String> = if let Ok(entries) = std::fs::read_dir(&projects_dir) {
        entries.flatten().filter_map(|e| {
            let raw = e.file_name().to_string_lossy().to_string();
            let decoded = usage::decode_claude_project_path(&raw);
            if decoded.len() > 1 { Some((decoded, raw)) } else { None }
        }).collect()
    } else { HashMap::new() };

    let project_breakdown: Vec<ProjectStat> = if strip_breakdowns {
        vec![]
    } else {
        let mut p: Vec<ProjectStat> = project_map.into_iter()
            .map(|(path, (count, tokens, cost))| {
                // Get display name from raw dir name (preserves original dashes)
                let name = if let Some(raw) = raw_dir_map.get(&path) {
                    // Raw: "-Users-abhijeetsingh-Downloads-projects-agentdock-project"
                    // We want the last meaningful folder name from the raw encoded path.
                    // The raw name has the real path with / replaced by -.
                    // Split at known prefixes to get the project-specific part.
                    let trimmed = raw.trim_start_matches('-');
                    // Find the deepest known parent dir pattern
                    let candidates = ["-Downloads-projects-", "-Downloads-", "-projects-", "-Desktop-", "-Documents-"];
                    let mut project_part = trimmed;
                    for prefix in candidates {
                        if let Some(pos) = trimmed.find(prefix) {
                            project_part = &trimmed[pos + prefix.len()..];
                            break;
                        }
                    }
                    if project_part.is_empty() {
                        // Fallback: take everything after the username
                        let parts: Vec<&str> = trimmed.splitn(4, '-').collect();
                        if parts.len() >= 4 { parts[3..].join("-") } else { trimmed.to_string() }
                    } else {
                        project_part.to_string()
                    }
                } else {
                    // No raw dir found — use last path component
                    std::path::Path::new(&path).file_name()
                        .and_then(|n| n.to_str()).unwrap_or("unknown").to_string()
                };
                ProjectStat { project_path: path, project_name: name, message_count: count, total_tokens: tokens, estimated_cost_usd: cost }
            })
            .collect();
        p.sort_by_key(|stat| std::cmp::Reverse(stat.total_tokens));
        p
    };

    // Cache stats
    let total_cache_all = total_cache_read + total_input;
    let cache_stats = CacheStats {
        hit_rate_percent: if total_cache_all > 0 {
            (total_cache_read as f64 / total_cache_all as f64) * 100.0
        } else { 0.0 },
        ephemeral_5m_percent: if total_cache_write > 0 {
            (total_ephemeral_5m as f64 / total_cache_write as f64) * 100.0
        } else { 0.0 },
        ephemeral_1h_percent: if total_cache_write > 0 {
            (total_ephemeral_1h as f64 / total_cache_write as f64) * 100.0
        } else { 0.0 },
        total_cache_read,
        total_cache_write,
    };

    // Active sessions
    let active_sessions = claude_history::get_claude_active_sessions().unwrap_or_default();
    let active_session_count = active_sessions.iter().filter(|s| s.is_running).count() as u32;

    // App info
    let app_info = claude_metadata::get_claude_app_info().unwrap_or_default();
    let plugins = claude_metadata::get_claude_installed_plugins().unwrap_or_default();
    let commands = claude_metadata::get_claude_custom_commands().unwrap_or_default();
    let todos = claude_metadata::get_claude_todos_summary().unwrap_or_default();
    let plans = claude_metadata::get_claude_plans_summary().unwrap_or_default();
    let hooks = claude_metadata::get_claude_hooks_summary().unwrap_or_default();
    let file_history = claude_metadata::get_claude_file_history_stats().unwrap_or_default();

    // ── /stats-derived metrics ─────────────────────────────────────────────

    // favorite_model: model with highest total (input + output) tokens
    let favorite_model: Option<String> = model_breakdown
        .iter()
        .max_by_key(|m| m.input_tokens + m.output_tokens)
        .map(|m| m.model.clone());

    // active_days: count daily_activity entries where message_count > 0
    let active_days = stats.daily_activity.iter().filter(|d| d.message_count > 0).count() as u32;

    // total_days: days between earliest daily_activity date and today
    let total_days: u32 = {
        let today = chrono::Local::now().date_naive();
        stats.daily_activity.iter()
            .filter(|d| d.message_count > 0)
            .filter_map(|d| chrono::NaiveDate::parse_from_str(&d.date, "%Y-%m-%d").ok())
            .min()
            .map(|first| (today - first).num_days().max(1) as u32)
            .unwrap_or(0)
    };

    // Sort daily_activity dates for streak computation
    let mut active_dates: Vec<chrono::NaiveDate> = stats.daily_activity.iter()
        .filter(|d| d.message_count > 0)
        .filter_map(|d| chrono::NaiveDate::parse_from_str(&d.date, "%Y-%m-%d").ok())
        .collect();
    active_dates.sort();
    active_dates.dedup();

    // longest_streak: max consecutive active days
    let longest_streak: u32 = if active_dates.is_empty() {
        0
    } else {
        let mut max_streak = 1u32;
        let mut cur = 1u32;
        for w in active_dates.windows(2) {
            if w[1] - w[0] == chrono::Duration::days(1) {
                cur += 1;
                if cur > max_streak { max_streak = cur; }
            } else {
                cur = 1;
            }
        }
        max_streak
    };

    // current_streak: consecutive active days ending today or yesterday
    let current_streak: u32 = if active_dates.is_empty() {
        0
    } else {
        let today = chrono::Local::now().date_naive();
        let last = *active_dates.last().unwrap();
        // Only count if the last active day is today or yesterday
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

    // most_active_weekday: weekday with highest total message count
    let most_active_weekday: Option<String> = {
        let mut weekday_counts: [u64; 7] = [0; 7]; // Mon=0 .. Sun=6
        for d in &stats.daily_activity {
            if d.message_count > 0 {
                if let Ok(date) = chrono::NaiveDate::parse_from_str(&d.date, "%Y-%m-%d") {
                    let idx = date.weekday().num_days_from_monday() as usize;
                    weekday_counts[idx] += d.message_count;
                }
            }
        }
        let max_val = weekday_counts.iter().copied().max().unwrap_or(0);
        if max_val == 0 {
            None
        } else {
            let idx = weekday_counts.iter().position(|&v| v == max_val).unwrap();
            Some(["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"][idx].to_string())
        }
    };

    // peak_hour: index (0-23) of the max value in hour_counts
    let peak_hour: Option<u32> = {
        let max_val = stats.hour_counts.iter().copied().max().unwrap_or(0);
        if max_val == 0 {
            None
        } else {
            stats.hour_counts.iter().position(|&v| v == max_val).map(|i| i as u32)
        }
    };

    ClaudeV2Overview {
        connected,
        connection_method,
        email,
        plan,
        org_name,
        rate_limits: analytics.rate_limits,
        credit_usage: analytics.credit_usage,
        extra: analytics.extra,

        limit_state: analytics.limit_state,

        account_info,

        total_sessions: stats.total_sessions,
        total_messages: stats.total_messages,
        total_tool_calls,
        longest_session_duration: stats.longest_session.as_ref().map(|s| s.duration).unwrap_or(0),
        longest_session_messages: stats.longest_session.as_ref().map(|s| s.message_count).unwrap_or(0),
        first_session_date: stats.first_session_date,
        hour_counts: stats.hour_counts,
        daily_activity: stats.daily_activity,

        total_input_tokens: total_input,
        total_output_tokens: total_output,
        total_cache_read,
        total_cache_write,
        total_web_searches,
        total_web_fetches,
        thinking_message_count: thinking_count,
        total_message_count: tray_overlay_msgs.unwrap_or(records.len() as u64),
        estimated_total_cost: total_cost,

        model_breakdown,
        tool_usage,
        cache_stats,
        geo_breakdown,
        project_breakdown,

        stats_cache_models: stats.model_usage.clone(),

        active_sessions,
        active_session_count,

        num_startups: app_info.num_startups,
        install_method: app_info.install_method,
        plugins_count: plugins.len() as u32,
        custom_commands_count: commands.len() as u32,
        todos_summary: todos,
        plans_count: plans.total_plans,
        hooks_count: hooks.total_hook_executions,
        file_history_checkpoints: file_history.total_checkpoints,

        favorite_model,
        active_days,
        total_days,
        longest_streak,
        current_streak,
        most_active_weekday,
        peak_hour,
    }
}

fn build_token_timeseries(time_range: &str, project_filter: Option<&str>) -> Vec<TokenTimePoint> {
    let all_records = load_project_usage_for_time_range(time_range);
    let time_filtered = filter_records_by_time(&all_records, time_range);
    let records: Vec<_> = if let Some(pf) = project_filter {
        time_filtered.into_iter().filter(|r| r.project_path.as_deref().map(|p| p.contains(pf)).unwrap_or(false)).collect()
    } else {
        time_filtered
    };

    let mut daily: HashMap<String, (u64, u64, u64, u64, f64)> = HashMap::new();
    for r in &records {
        if let Some(ref u) = r.usage {
            let date = date_from_timestamp(&r.timestamp);
            if date.is_empty() { continue; }
            let inp = u.input_tokens.unwrap_or(0);
            let out = u.output_tokens.unwrap_or(0);
            let cr = u.cache_read_input_tokens.unwrap_or(0);
            let cw = u.cache_creation_input_tokens.unwrap_or(0);
            let cost = cost_engine::estimate_cost(
                r.model.as_deref(),
                &cost_engine::TokensForCost { input: inp, output: out, cache_read: cr, cache_write: cw },
            );
            let entry = daily.entry(date).or_insert((0, 0, 0, 0, 0.0));
            entry.0 += inp;
            entry.1 += out;
            entry.2 += cr;
            entry.3 += cw;
            entry.4 += cost;
        }
    }

    let mut points: Vec<TokenTimePoint> = daily.into_iter()
        .map(|(date, (inp, out, cr, cw, cost))| TokenTimePoint {
            date,
            input: inp,
            output: out,
            cache_read: cr,
            cache_write: cw,
            estimated_cost: cost,
        })
        .collect();
    points.sort_by(|a, b| a.date.cmp(&b.date));
    points
}

fn build_model_timeseries(time_range: &str) -> Vec<ModelTimePoint> {
    let all_records = load_project_usage_for_time_range(time_range);
    let records = filter_records_by_time(&all_records, time_range);

    let mut daily: HashMap<String, HashMap<String, u64>> = HashMap::new();
    for r in &records {
        if r.usage.is_some() {
            let date = date_from_timestamp(&r.timestamp);
            if date.is_empty() { continue; }
            let model = short_model(r.model.as_deref().unwrap_or("unknown"));
            let day = daily.entry(date).or_default();
            *day.entry(model).or_insert(0) += 1;
        }
    }

    let mut points: Vec<ModelTimePoint> = daily.into_iter()
        .map(|(date, models)| ModelTimePoint { date, models })
        .collect();
    points.sort_by(|a, b| a.date.cmp(&b.date));
    points
}

fn build_message_log(page: u32, page_size: u32, project_filter: Option<&str>) -> MessageLogPage {
    let all_records = usage::read_project_usage_files().unwrap_or_default();
    let all_records: Vec<_> = if let Some(pf) = project_filter {
        all_records.into_iter().filter(|r| r.project_path.as_deref().map(|p| p.contains(pf)).unwrap_or(false)).collect()
    } else {
        all_records
    };

    // Only assistant messages with usage
    let mut entries: Vec<MessageLogEntry> = all_records.iter()
        .filter(|r| r.usage.is_some())
        .map(|r| {
            let u = r.usage.as_ref().unwrap();
            let inp = u.input_tokens.unwrap_or(0);
            let out = u.output_tokens.unwrap_or(0);
            let cr = u.cache_read_input_tokens.unwrap_or(0);
            let cw = u.cache_creation_input_tokens.unwrap_or(0);
            let cost = cost_engine::estimate_cost(
                r.model.as_deref(),
                &cost_engine::TokensForCost { input: inp, output: out, cache_read: cr, cache_write: cw },
            );
            MessageLogEntry {
                timestamp: r.timestamp.clone(),
                model: r.model.clone(),
                input_tokens: inp,
                output_tokens: out,
                cache_read: cr,
                cache_write: cw,
                tools: r.tools_used.clone(),
                service_tier: u.service_tier.clone(),
                geo: u.inference_geo.clone(),
                speed: u.speed.clone(),
                has_thinking: r.has_thinking,
                estimated_cost: cost,
                project: r.project_path.clone(),
            }
        })
        .collect();

    // Sort newest first
    entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    let total_count = entries.len() as u64;
    let start = ((page - 1) * page_size) as usize;
    let paged = if start < entries.len() {
        entries[start..].iter().take(page_size as usize).cloned().collect()
    } else {
        vec![]
    };

    MessageLogPage {
        entries: paged,
        total_count,
        page,
        page_size,
    }
}

// ── Tauri Commands (async — never blocks UI thread) ─────────────────────────

#[tauri::command]
pub async fn get_claude_v2_overview(time_range: String, force_refresh: bool) -> Result<ClaudeV2Overview, String> {
    // Fast path: return cached data without blocking
    if !force_refresh {
        if let Ok(cache) = CACHE.lock() {
            if let Some(entry) = cache.overview.get(&time_range) {
                if entry.is_valid(cache.ttl_seconds) {
                    return Ok(entry.data.clone());
                }
            }
        }
    }

    // Heavy path: run on background thread so UI stays responsive
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
pub async fn get_claude_v2_token_timeseries(
    time_range: String,
    project_filter: Option<String>,
    force_refresh: bool,
) -> Result<Vec<TokenTimePoint>, String> {
    let key = format!("{}:{}", time_range, project_filter.as_deref().unwrap_or("all"));

    if !force_refresh {
        if let Ok(cache) = CACHE.lock() {
            if let Some(entry) = cache.token_ts.get(&key) {
                if entry.is_valid(cache.ttl_seconds) {
                    return Ok(entry.data.clone());
                }
            }
        }
    }

    let range = time_range.clone();
    let filter = project_filter.clone();
    let data = tokio::task::spawn_blocking(move || build_token_timeseries(&range, filter.as_deref()))
        .await
        .map_err(|e| format!("Task error: {}", e))?;

    if let Ok(mut cache) = CACHE.lock() {
        cache.token_ts.insert(key, CacheEntry::new(data.clone()));
    }

    Ok(data)
}

#[tauri::command]
pub async fn get_claude_v2_model_timeseries(
    time_range: String,
    force_refresh: bool,
) -> Result<Vec<ModelTimePoint>, String> {
    if !force_refresh {
        if let Ok(cache) = CACHE.lock() {
            if let Some(entry) = cache.model_ts.get(&time_range) {
                if entry.is_valid(cache.ttl_seconds) {
                    return Ok(entry.data.clone());
                }
            }
        }
    }

    let range = time_range.clone();
    let data = tokio::task::spawn_blocking(move || build_model_timeseries(&range))
        .await
        .map_err(|e| format!("Task error: {}", e))?;

    if let Ok(mut cache) = CACHE.lock() {
        cache.model_ts.insert(time_range, CacheEntry::new(data.clone()));
    }

    Ok(data)
}

#[tauri::command]
pub async fn get_claude_v2_message_log(
    page: u32,
    page_size: u32,
    project_filter: Option<String>,
) -> Result<MessageLogPage, String> {
    let filter = project_filter.clone();
    tokio::task::spawn_blocking(move || build_message_log(page, page_size, filter.as_deref()))
        .await
        .map_err(|e| format!("Task error: {}", e))
}

#[tauri::command]
pub async fn get_claude_v2_prompt_history(
    page: u32,
    page_size: u32,
    search: Option<String>,
) -> Result<serde_json::Value, String> {
    let all = claude_history::get_claude_history(None)?;
    let filtered: Vec<_> = if let Some(ref q) = search {
        let q_lower = q.to_lowercase();
        all.into_iter().filter(|e| e.display.to_lowercase().contains(&q_lower)).collect()
    } else {
        all
    };

    let total = filtered.len() as u64;
    let start = ((page - 1) * page_size) as usize;
    let paged: Vec<_> = if start < filtered.len() {
        filtered[start..].iter().take(page_size as usize).collect()
    } else {
        vec![]
    };

    Ok(serde_json::json!({
        "entries": paged,
        "total_count": total,
        "page": page,
        "page_size": page_size,
    }))
}

#[tauri::command]
pub async fn export_claude_v2_csv(time_range: String, _project_filter: Option<String>) -> Result<String, String> {
    let tr = time_range.clone();
    let all_records = tokio::task::spawn_blocking(move || load_project_usage_for_time_range(&tr))
        .await
        .map_err(|e| format!("Task error: {}", e))?;
    let records = filter_records_by_time(&all_records, &time_range);

    let mut csv = String::from("timestamp,model,input_tokens,output_tokens,cache_read,cache_write,tools,service_tier,geo,speed,thinking,estimated_cost,project\n");

    for r in &records {
        if let Some(ref u) = r.usage {
            let inp = u.input_tokens.unwrap_or(0);
            let out = u.output_tokens.unwrap_or(0);
            let cr = u.cache_read_input_tokens.unwrap_or(0);
            let cw = u.cache_creation_input_tokens.unwrap_or(0);
            let cost = cost_engine::estimate_cost(
                r.model.as_deref(),
                &cost_engine::TokensForCost { input: inp, output: out, cache_read: cr, cache_write: cw },
            );
            csv.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{},{},{:.4},{}\n",
                r.timestamp,
                r.model.as_deref().unwrap_or(""),
                inp, out, cr, cw,
                r.tools_used.join(";"),
                u.service_tier.as_deref().unwrap_or(""),
                u.inference_geo.as_deref().unwrap_or(""),
                u.speed.as_deref().unwrap_or(""),
                r.has_thinking,
                cost,
                r.project_path.as_deref().unwrap_or(""),
            ));
        }
    }

    Ok(csv)
}
