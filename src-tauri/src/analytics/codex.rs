//! Codex (OpenAI) analytics provider.
//! Auth fallback: user token → auto-detect ~/.codex/auth.json
//! API: chatgpt.com/backend-api/wham/usage
//! Local data: ~/.codex/state_5.sqlite, ~/.codex/models_cache.json, ~/.codex/config.toml

use crate::analytics::http;
use crate::analytics::token_store;
use crate::analytics::types::*;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

// ── In-memory cache (300s TTL) ──────────────────────────────────────────────

struct CacheEntry {
    data: ProviderAnalytics,
    fetched_at: std::time::Instant,
}

lazy_static::lazy_static! {
    static ref CACHE: Mutex<Option<CacheEntry>> = Mutex::new(None);
}

const CACHE_TTL_SECS: u64 = 60;

#[derive(Deserialize, Debug)]
struct CodexAuthTokens {
    access_token: Option<String>,
    refresh_token: Option<String>,
    id_token: Option<String>,
    account_id: Option<String>,
}

#[derive(Deserialize, Debug)]
struct CodexAuth {
    auth_mode: Option<String>,
    #[serde(rename = "OPENAI_API_KEY")]
    openai_api_key: Option<String>,
    tokens: Option<CodexAuthTokens>,
    last_refresh: Option<String>,
}

#[derive(Deserialize, Debug)]
struct WhamRateWindow {
    used_percent: Option<f64>,
    reset_at: Option<i64>,            // unix timestamp
    reset_after_seconds: Option<i64>,
    limit_window_seconds: Option<i64>,
}

#[derive(Deserialize, Debug)]
struct WhamRateLimit {
    allowed: Option<bool>,
    limit_reached: Option<bool>,
    primary_window: Option<WhamRateWindow>,
    secondary_window: Option<WhamRateWindow>,
}

#[derive(Deserialize, Debug)]
struct WhamCredits {
    has_credits: Option<bool>,
    unlimited: Option<bool>,
    balance: Option<f64>,
}

#[derive(Deserialize, Debug)]
struct WhamUsageResponse {
    user_id: Option<String>,
    account_id: Option<String>,
    email: Option<String>,
    plan_type: Option<String>,
    rate_limit: Option<WhamRateLimit>,
    code_review_rate_limit: Option<WhamRateLimit>,
    credits: Option<WhamCredits>,
}

fn auth_path() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".codex").join("auth.json")
}

fn resolve_token() -> Result<(String, String, Option<String>), String> {
    // 1. User-provided
    if let Ok(Some(token)) = token_store::get_provider_token("codex", "access-token") {
        let account_id = token_store::get_provider_token("codex", "account-id").ok().flatten();
        return Ok((token, "token-manual".into(), account_id));
    }

    // 2. Auto-detect
    let path = auth_path();
    if !path.exists() {
        return Err("Codex auth.json not found".into());
    }
    let content = fs::read_to_string(&path).map_err(|e| format!("Read error: {}", e))?;
    let auth: CodexAuth = serde_json::from_str(&content).map_err(|e| format!("Parse error: {}", e))?;

    if let Some(ref tokens) = auth.tokens {
        if let Some(ref at) = tokens.access_token {
            return Ok((at.clone(), "oauth-auto".into(), tokens.account_id.clone()));
        }
    }
    Err("No Codex access token found".into())
}

fn parse_wham_window(w: &Option<WhamRateWindow>, label: &str) -> Option<RateLimitWindow> {
    let win = w.as_ref()?;
    let used = win.used_percent?;
    Some(RateLimitWindow {
        provider_id: "codex".into(),
        label: label.into(),
        used_percent: used,
        remaining_percent: (100.0 - used).max(0.0),
        resets_at: win.reset_at.map(|ts| {
            chrono::DateTime::from_timestamp(ts, 0)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_default()
        }),
        resets_in_seconds: win.reset_after_seconds,
        window_seconds: win.limit_window_seconds,
    })
}

// ── Cost estimation ─────────────────────────────────────────────────────────

/// Estimate USD cost for a Codex session based on model and token count.
/// Uses combined (average of input+output) per-million-token rates.
fn estimate_codex_cost(model: &str, tokens: i64) -> f64 {
    let rate_per_million = match model.to_lowercase().as_str() {
        m if m.contains("gpt-5.4-mini") => 2.5,
        m if m.contains("gpt-5.4") => 10.0,
        m if m.contains("gpt-5.3-codex") => 10.0,
        m if m.contains("gpt-5.2-codex") => 7.5,
        m if m.contains("gpt-5.1-codex-max") => 7.5,
        m if m.contains("gpt-5.1-codex") => 7.5,
        m if m.contains("gpt-5") => 5.0,
        _ => 5.0, // default
    };
    (tokens as f64 / 1_000_000.0) * rate_per_million
}

// ── Local data types ────────────────────────────────────────────────────────

#[derive(Serialize, Clone, Debug)]
struct CodexSession {
    id: String,
    title: Option<String>,
    model: Option<String>,
    tokens_used: i64,
    source: Option<String>,
    cwd: Option<String>,
    git_branch: Option<String>,
    reasoning_effort: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
struct CodexLocalStats {
    total_sessions: i64,
    total_tokens_used: i64,
    sessions: Vec<CodexSession>,
    tokens_by_model: HashMap<String, i64>,
    sessions_by_project: HashMap<String, i64>,
    estimated_total_cost: f64,
    cost_by_model: HashMap<String, f64>,
}

#[derive(Deserialize, Debug)]
struct OpenAIMeOrg {
    id: Option<String>,
    title: Option<String>,
    role: Option<String>,
}

#[derive(Deserialize, Debug)]
struct OpenAIMeResponse {
    name: Option<String>,
    email: Option<String>,
    orgs: Option<Vec<OpenAIMeOrg>>,
}

// ── Local data parsing ──────────────────────────────────────────────────────

fn codex_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".codex")
}

/// Read sessions from ~/.codex/state_5.sqlite
fn fetch_local_stats() -> Result<CodexLocalStats, String> {
    let db_path = codex_dir().join("state_5.sqlite");
    if !db_path.exists() {
        return Err("state_5.sqlite not found".into());
    }

    let conn = rusqlite::Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("Cannot open Codex DB: {}", e))?;

    // Get total counts
    let total_sessions: i64 = conn
        .query_row("SELECT COUNT(*) FROM threads", [], |row| row.get(0))
        .unwrap_or(0);

    let total_tokens_used: i64 = conn
        .query_row("SELECT COALESCE(SUM(tokens_used), 0) FROM threads", [], |row| row.get(0))
        .unwrap_or(0);

    // Get recent sessions (last 50)
    let mut stmt = conn
        .prepare(
            "SELECT id, title, model, COALESCE(tokens_used, 0), source, cwd, git_branch, \
             reasoning_effort, created_at, updated_at \
             FROM threads ORDER BY updated_at DESC LIMIT 50",
        )
        .map_err(|e| format!("Query prepare error: {}", e))?;

    let sessions: Vec<CodexSession> = stmt
        .query_map([], |row| {
            Ok(CodexSession {
                id: row.get(0)?,
                title: row.get(1)?,
                model: row.get(2)?,
                tokens_used: row.get(3)?,
                source: row.get(4)?,
                cwd: row.get(5)?,
                git_branch: row.get(6)?,
                reasoning_effort: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })
        .map_err(|e| format!("Query error: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    // Compute tokens_by_model
    let mut tokens_by_model: HashMap<String, i64> = HashMap::new();
    let mut model_stmt = conn
        .prepare("SELECT COALESCE(model, 'unknown'), COALESCE(SUM(tokens_used), 0) FROM threads GROUP BY model")
        .map_err(|e| format!("Model query error: {}", e))?;
    let model_rows = model_stmt
        .query_map([], |row| {
            let model: String = row.get(0)?;
            let tokens: i64 = row.get(1)?;
            Ok((model, tokens))
        })
        .map_err(|e| format!("Model query error: {}", e))?;
    for row in model_rows {
        if let Ok((model, tokens)) = row {
            tokens_by_model.insert(model, tokens);
        }
    }

    // Compute sessions_by_project (from cwd, use last path component)
    let mut sessions_by_project: HashMap<String, i64> = HashMap::new();
    let mut proj_stmt = conn
        .prepare("SELECT cwd, COUNT(*) FROM threads WHERE cwd IS NOT NULL GROUP BY cwd")
        .map_err(|e| format!("Project query error: {}", e))?;
    let proj_rows = proj_stmt
        .query_map([], |row| {
            let cwd: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            Ok((cwd, count))
        })
        .map_err(|e| format!("Project query error: {}", e))?;
    for row in proj_rows {
        if let Ok((cwd, count)) = row {
            let project_name = std::path::Path::new(&cwd)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| cwd.clone());
            *sessions_by_project.entry(project_name).or_insert(0) += count;
        }
    }

    // Compute cost estimates per model
    let mut estimated_total_cost = 0.0f64;
    let mut cost_by_model: HashMap<String, f64> = HashMap::new();
    for (model, &tokens) in &tokens_by_model {
        let cost = estimate_codex_cost(model, tokens);
        estimated_total_cost += cost;
        cost_by_model.insert(model.clone(), cost);
    }

    Ok(CodexLocalStats {
        total_sessions,
        total_tokens_used,
        sessions,
        tokens_by_model,
        sessions_by_project,
        estimated_total_cost,
        cost_by_model,
    })
}

/// Read available models from ~/.codex/models_cache.json
fn read_models_cache() -> Vec<String> {
    let path = codex_dir().join("models_cache.json");
    if !path.exists() {
        return vec![];
    }
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    // models_cache.json is an array of objects with "slug" or "id" fields,
    // or just an array of strings. Try both.
    if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&content) {
        arr.iter()
            .filter_map(|v| {
                if let Some(s) = v.as_str() {
                    Some(s.to_string())
                } else {
                    v.get("slug")
                        .or_else(|| v.get("id"))
                        .and_then(|s| s.as_str())
                        .map(|s| s.to_string())
                }
            })
            .collect()
    } else {
        vec![]
    }
}

/// Read config from ~/.codex/config.toml (simple key=value parsing)
fn read_codex_config() -> (Option<String>, Option<String>) {
    let path = codex_dir().join("config.toml");
    if !path.exists() {
        return (None, None);
    }
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return (None, None),
    };
    let mut model = None;
    let mut reasoning_effort = None;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || !line.contains('=') {
            continue;
        }
        let mut parts = line.splitn(2, '=');
        let key = parts.next().unwrap_or("").trim().trim_matches('"');
        let val = parts.next().unwrap_or("").trim().trim_matches('"');
        match key {
            "model" => model = Some(val.to_string()),
            "reasoning_effort" => reasoning_effort = Some(val.to_string()),
            _ => {}
        }
    }
    (model, reasoning_effort)
}

/// Fetch account profile from api.openai.com/v1/me
fn fetch_account_profile(token: &str) -> Option<OpenAIMeResponse> {
    http::authed_get::<OpenAIMeResponse>(
        "https://api.openai.com/v1/me",
        token,
        None,
    )
    .ok()
}

/// Stats for a time window
struct CodexWindowStats {
    sessions: i64,
    tokens: i64,
    cost: f64,
}

/// Get stats for 2 time windows from SQLite: today (since midnight), this week (since Monday)
fn fetch_multi_window_stats() -> Option<(CodexWindowStats, CodexWindowStats)> {
    let db_path = codex_dir().join("state_5.sqlite");
    if !db_path.exists() { return None; }
    let conn = rusqlite::Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ).ok()?;

    let local_now = chrono::Local::now();
    let today_str = local_now.format("%Y-%m-%d").to_string();

    // This week = Monday's date
    use chrono::Datelike;
    let weekday = local_now.weekday().num_days_from_monday();
    let monday = local_now.date_naive() - chrono::Duration::days(weekday as i64);
    let monday_str = monday.format("%Y-%m-%d").to_string();

    fn query_window(conn: &rusqlite::Connection, where_clause: &str, params: &[&str]) -> CodexWindowStats {
        let sql_count = format!(
            "SELECT COUNT(*), COALESCE(SUM(tokens_used), 0) FROM threads WHERE {}",
            where_clause
        );
        let (sessions, tokens): (i64, i64) = conn.query_row(&sql_count, rusqlite::params_from_iter(params.iter()), |row| {
            Ok((row.get(0)?, row.get(1)?))
        }).unwrap_or((0, 0));

        let sql_cost = format!(
            "SELECT COALESCE(model, 'unknown'), COALESCE(tokens_used, 0) FROM threads WHERE {}",
            where_clause
        );
        let mut cost = 0.0f64;
        if let Ok(mut stmt) = conn.prepare(&sql_cost) {
            if let Ok(rows) = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
                let model: String = row.get(0)?;
                let tok: i64 = row.get(1)?;
                Ok((model, tok))
            }) {
                for row in rows.flatten() {
                    cost += estimate_codex_cost(&row.0, row.1);
                }
            }
        }
        CodexWindowStats { sessions, tokens, cost }
    }

    let stats_today = query_window(&conn,
        "date(created_at) = ?1 OR date(updated_at) = ?1", &[&today_str]);
    let stats_week = query_window(&conn,
        "date(created_at) >= ?1 OR date(updated_at) >= ?1", &[&monday_str]);

    Some((stats_today, stats_week))
}

/// Enrich an extra HashMap with all local data
fn enrich_with_local_data(extra: &mut HashMap<String, serde_json::Value>) {
    // SQLite local stats
    if let Ok(stats) = fetch_local_stats() {
        extra.insert("total_sessions".into(), serde_json::json!(stats.total_sessions));
        extra.insert("total_tokens_used".into(), serde_json::json!(stats.total_tokens_used));
        extra.insert("sessions".into(), serde_json::to_value(&stats.sessions).unwrap_or_default());
        extra.insert("tokens_by_model".into(), serde_json::to_value(&stats.tokens_by_model).unwrap_or_default());
        extra.insert("sessions_by_project".into(), serde_json::to_value(&stats.sessions_by_project).unwrap_or_default());
        extra.insert("estimated_total_cost".into(), serde_json::json!(stats.estimated_total_cost));
        extra.insert("cost_by_model".into(), serde_json::to_value(&stats.cost_by_model).unwrap_or_default());
    }

    // Multi-window stats (today, this week)
    if let Some((stats_today, stats_week)) = fetch_multi_window_stats() {
        extra.insert("start_today_sessions".into(), serde_json::json!(stats_today.sessions));
        extra.insert("start_today_tokens".into(), serde_json::json!(stats_today.tokens));
        extra.insert("start_today_cost".into(), serde_json::json!(stats_today.cost));
        extra.insert("this_week_sessions".into(), serde_json::json!(stats_week.sessions));
        extra.insert("this_week_tokens".into(), serde_json::json!(stats_week.tokens));
        extra.insert("this_week_cost".into(), serde_json::json!(stats_week.cost));
    }

    // Models cache
    let models = read_models_cache();
    if !models.is_empty() {
        extra.insert("available_models".into(), serde_json::json!(models));
    }

    // Config
    let (config_model, config_reasoning) = read_codex_config();
    if let Some(m) = config_model {
        extra.insert("config_model".into(), serde_json::Value::String(m));
    }
    if let Some(r) = config_reasoning {
        extra.insert("config_reasoning_effort".into(), serde_json::Value::String(r));
    }
}

/// Enrich extra HashMap with account profile from /v1/me
fn enrich_with_account_profile(extra: &mut HashMap<String, serde_json::Value>, token: &str) {
    if let Some(profile) = fetch_account_profile(token) {
        if let Some(name) = profile.name {
            extra.insert("account_name".into(), serde_json::Value::String(name));
        }
        if let Some(orgs) = profile.orgs {
            let org_values: Vec<serde_json::Value> = orgs
                .iter()
                .map(|o| {
                    serde_json::json!({
                        "id": o.id,
                        "title": o.title,
                        "role": o.role,
                    })
                })
                .collect();
            extra.insert("organizations".into(), serde_json::json!(org_values));
        }
    }
}

pub fn fetch_codex_analytics() -> ProviderAnalytics {
    // Return cached data if still fresh
    if let Ok(guard) = CACHE.lock() {
        if let Some(ref entry) = *guard {
            if entry.fetched_at.elapsed().as_secs() < CACHE_TTL_SECS {
                return entry.data.clone();
            }
        }
    }

    let mut result = fetch_codex_analytics_uncached();

    // A transient WHAM failure still reports connected (local data exists) but
    // with empty rate_limits. Caching that blanks the popover bars for a full
    // TTL while the menu-bar title may show the previous cycle's percentage.
    // Instead, graft the last good windows into the result and keep the old
    // cache entry so the next cycle retries.
    let wham_failed = result.status.error.is_some() && result.rate_limits.is_empty();
    if wham_failed {
        if let Ok(guard) = CACHE.lock() {
            if let Some(ref prev) = *guard {
                if !prev.data.rate_limits.is_empty() {
                    result.rate_limits = prev.data.rate_limits.clone();
                    result.limit_state = prev.data.limit_state.clone();
                }
            }
        }
        return result;
    }

    // Cache successful results (connected = true)
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

fn fetch_codex_analytics_uncached() -> ProviderAnalytics {
    let now = Utc::now().to_rfc3339();

    let (token, method, account_id) = match resolve_token() {
        Ok(t) => t,
        Err(e) => {
            // Even without a token, enrich with local data if available
            let mut extra = HashMap::new();
            enrich_with_local_data(&mut extra);
            let has_local = extra.contains_key("total_sessions");

            return ProviderAnalytics {
                provider_id: "codex".into(),
                provider_name: "Codex (OpenAI)".into(),
                status: ProviderStatus {
                    provider_id: "codex".into(),
                    provider_name: "Codex (OpenAI)".into(),
                    connected: has_local, // connected if we have local data
                    connection_method: if has_local { "local-file".into() } else { "none".into() },
                    account_email: None, plan_name: None, org_name: None,
                    error: if has_local { None } else { Some(e) },
                },
                rate_limits: vec![], credit_usage: None, token_counts: None,
                limit_state: None,
                extra, fetched_at: now,
            };
        }
    };

    let mut headers_vec: Vec<(&str, &str)> = vec![
        ("User-Agent", "CodexBar"),
    ];
    let account_id_owned = account_id.unwrap_or_default();
    if !account_id_owned.is_empty() {
        headers_vec.push(("ChatGPT-Account-Id", &account_id_owned));
    }
    let extra_headers = http::headers(&headers_vec);

    let resp: Result<WhamUsageResponse, String> = http::authed_get(
        "https://chatgpt.com/backend-api/wham/usage",
        &token,
        Some(extra_headers),
    )
    .map_err(String::from);

    match resp {
        Ok(wham) => {
            let codex_limit_state = wham.rate_limit.as_ref().and_then(|rl| {
                if rl.limit_reached == Some(true) || rl.allowed == Some(false) {
                    Some(LimitState::Reached {
                        scope: LimitScope::Custom("wham_primary".into()),
                        used_pct: 100.0,
                        cap: None,
                        resets_at: None,
                    })
                } else {
                    None
                }
            });

            let mut rate_limits = Vec::new();
            if let Some(ref rl) = wham.rate_limit {
                if let Some(w) = parse_wham_window(&rl.primary_window, "Primary (5h)") {
                    rate_limits.push(w);
                }
                if let Some(w) = parse_wham_window(&rl.secondary_window, "Weekly (7d)") {
                    rate_limits.push(w);
                }
            }
            if let Some(ref cr) = wham.code_review_rate_limit {
                if let Some(w) = parse_wham_window(&cr.primary_window, "Code Review (7d)") {
                    rate_limits.push(w);
                }
            }

            let credit_usage = wham.credits.as_ref().and_then(|c| {
                c.balance.map(|b| CreditUsage {
                    provider_id: "codex".into(),
                    used: 0.0,
                    limit: None,
                    remaining: b,
                    currency: "credits".into(),
                    billing_cycle_end: None,
                    plan_name: wham.plan_type.clone(),
                })
            });

            let mut extra = HashMap::new();
            if let Some(ref pt) = wham.plan_type {
                extra.insert("plan_type".into(), serde_json::Value::String(pt.clone()));
            }
            if let Some(ref cr) = wham.credits {
                if let Some(true) = cr.unlimited {
                    extra.insert("unlimited_credits".into(), serde_json::Value::Bool(true));
                }
            }

            // Enrich with local data (SQLite, models_cache, config)
            enrich_with_local_data(&mut extra);

            // Enrich with account profile from /v1/me
            enrich_with_account_profile(&mut extra, &token);

            ProviderAnalytics {
                provider_id: "codex".into(),
                provider_name: "Codex (OpenAI)".into(),
                status: ProviderStatus {
                    provider_id: "codex".into(),
                    provider_name: "Codex (OpenAI)".into(),
                    connected: true,
                    connection_method: method,
                    account_email: wham.email,
                    plan_name: wham.plan_type,
                    org_name: None,
                    error: None,
                },
                rate_limits,
                credit_usage,
                token_counts: None,
                limit_state: codex_limit_state,
                extra,
                fetched_at: now,
            }
        }
        Err(e) => {
            // Even if API fails, enrich with local data
            let mut extra = HashMap::new();
            enrich_with_local_data(&mut extra);

            ProviderAnalytics {
                provider_id: "codex".into(),
                provider_name: "Codex (OpenAI)".into(),
                status: ProviderStatus {
                    provider_id: "codex".into(),
                    provider_name: "Codex (OpenAI)".into(),
                    connected: true,
                    connection_method: method,
                    account_email: None, plan_name: None, org_name: None,
                    error: Some(e),
                },
                rate_limits: vec![], credit_usage: None, token_counts: None,
                limit_state: None,
                extra, fetched_at: now,
            }
        }
    }
}

pub fn check_connection() -> ProviderStatus {
    // 1. Try resolving a token (auth.json or manual)
    if let Ok((_, method, _)) = resolve_token() {
        return ProviderStatus {
            provider_id: "codex".into(),
            provider_name: "Codex (OpenAI)".into(),
            connected: true,
            connection_method: method,
            account_email: None,
            plan_name: None,
            org_name: None,
            error: None,
        };
    }
    // 2. Fallback: check local SQLite database
    let db_path = codex_dir().join("state_5.sqlite");
    if db_path.exists() {
        return ProviderStatus {
            provider_id: "codex".into(),
            provider_name: "Codex (OpenAI)".into(),
            connected: true,
            connection_method: "local-file".into(),
            account_email: None,
            plan_name: None,
            org_name: None,
            error: None,
        };
    }
    // 3. Not connected
    ProviderStatus {
        provider_id: "codex".into(),
        provider_name: "Codex (OpenAI)".into(),
        connected: false,
        connection_method: "none".into(),
        account_email: None,
        plan_name: None,
        org_name: None,
        error: Some("Codex not installed or not signed in".into()),
    }
}
