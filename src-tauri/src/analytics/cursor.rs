//! Cursor analytics provider.
//! Auth: session token from keychain
//! API: cursor.com/api/usage-summary + /api/auth/me

use crate::analytics::http;
use crate::analytics::token_store;
use crate::analytics::types::*;
use chrono::Utc;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Mutex;

// ── In-memory cache (300s TTL) ──────────────────────────────────────────────

struct CursorCacheEntry {
    data: ProviderAnalytics,
    fetched_at: std::time::Instant,
}

lazy_static::lazy_static! {
    static ref CURSOR_CACHE: Mutex<Option<CursorCacheEntry>> = Mutex::new(None);
}

const CURSOR_CACHE_TTL_SECS: u64 = 60;

// ── API response types ──────────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct UsageSummary {
    plan_used_cents: Option<f64>,
    plan_limit_cents: Option<f64>,
    on_demand_used_cents: Option<f64>,
    on_demand_limit_cents: Option<f64>,
    billing_cycle_start: Option<String>,
    billing_cycle_end: Option<String>,
    membership_type: Option<String>,
    // V2 nested structure (newer API responses)
    individual_usage: Option<IndividualUsage>,
    team_usage: Option<TeamUsage>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct TeamUsage {
    on_demand: Option<UsageOnDemand>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct UsageOnDemand {
    enabled: Option<bool>,
    used: Option<f64>,      // cents
    limit: Option<f64>,     // cents (null if unlimited)
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct IndividualUsage {
    plan: Option<UsagePlan>,
    on_demand: Option<UsageOnDemand>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct UsagePlan {
    used: Option<f64>,       // cents
    limit: Option<f64>,      // cents
    remaining: Option<f64>,  // cents
    breakdown: Option<UsagePlanBreakdown>,
    total_percent_used: Option<f64>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct UsagePlanBreakdown {
    included: Option<f64>,  // cents
    bonus: Option<f64>,     // cents
    total: Option<f64>,     // cents
}

#[derive(Deserialize, Debug)]
struct CursorAuthMe {
    email: Option<String>,
    name: Option<String>,
    #[serde(rename = "teamName")]
    team_name: Option<String>,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Read Cursor's auth token directly from its local SQLite database.
/// This is the same DB that the Cursor IDE writes to — no keychain access needed.
fn read_cursor_local_token() -> Result<(String, String, Option<String>), String> {
    let db_path = cursor_state_db_path()?;
    if !db_path.exists() {
        return Err("Cursor is not installed or has not been logged into".into());
    }

    let conn = rusqlite::Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("Cannot open Cursor DB: {}", e))?;

    let token: String = conn
        .query_row(
            "SELECT value FROM ItemTable WHERE key = 'cursorAuth/accessToken'",
            [],
            |row| row.get(0),
        )
        .map_err(|_| "No Cursor auth token found in state.vscdb".to_string())?;

    // Also try to read the user email for building the cookie
    let email: Option<String> = conn
        .query_row(
            "SELECT value FROM ItemTable WHERE key = 'cursorAuth/cachedEmail'",
            [],
            |row| row.get(0),
        )
        .ok();

    if token.is_empty() {
        return Err("Cursor auth token is empty".into());
    }

    Ok((token, "auto-detect".into(), email))
}

/// Get the path to Cursor's state database, cross-platform.
pub fn cursor_state_db_path() -> Result<std::path::PathBuf, String> {
    let home = dirs::home_dir().ok_or("Cannot find home directory")?;

    #[cfg(target_os = "macos")]
    let path = home.join("Library/Application Support/Cursor/User/globalStorage/state.vscdb");

    #[cfg(target_os = "linux")]
    let path = home.join(".config/Cursor/User/globalStorage/state.vscdb");

    #[cfg(target_os = "windows")]
    let path = {
        let appdata = std::env::var("APPDATA").unwrap_or_default();
        std::path::PathBuf::from(appdata).join("Cursor/User/globalStorage/state.vscdb")
    };

    Ok(path)
}

/// Resolve the Cursor session token using the fallback chain:
/// 1. User-provided token from keychain
/// 2. Auto-detect from Cursor's local SQLite DB
pub fn resolve_token() -> Result<(String, String), String> {
    // 1. User-provided token
    if let Ok(Some(token)) = token_store::get_provider_token("cursor", "session-token") {
        return Ok((token, "token-manual".into()));
    }

    // 2. Auto-detect from Cursor's SQLite DB
    match read_cursor_local_token() {
        Ok((token, method, _email)) => Ok((token, method)),
        Err(e) => Err(e),
    }
}

/// Resolve token AND get the user ID / email for building cookie headers.
pub fn resolve_token_with_context() -> Result<(String, String, Option<String>, Option<i64>), String> {
    // 1. User-provided token
    if let Ok(Some(token)) = token_store::get_provider_token("cursor", "session-token") {
        return Ok((token, "token-manual".into(), None, None));
    }

    // 2. Auto-detect from Cursor's SQLite DB
    let (token, method, email) = read_cursor_local_token()?;

    // Try to read team_id from the DB as well
    let team_id: Option<i64> = cursor_state_db_path().ok().and_then(|db_path| {
        let conn = rusqlite::Connection::open_with_flags(
            &db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ).ok()?;
        // team_id is stored as part of the cookie string or stripe info
        None // Will be fetched from /api/auth/stripe instead
    });

    Ok((token, method, email, team_id))
}

/// Extract the WorkOS user ID from a JWT's `sub` claim.
/// The sub field is like "auth0|user_01JYE3E41SBRXTZVHJ0DY00QRW" — we need "user_01JYE3E41SBRXTZVHJ0DY00QRW".
fn extract_user_id_from_jwt(jwt: &str) -> Option<String> {
    let parts: Vec<&str> = jwt.split('.').collect();
    if parts.len() < 2 {
        return None;
    }
    // Base64url decode the payload
    let payload = parts[1];
    let padded = match payload.len() % 4 {
        2 => format!("{}==", payload),
        3 => format!("{}=", payload),
        _ => payload.to_string(),
    };
    let b64 = padded.replace('-', "+").replace('_', "/");

    // Simple base64 decode
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bytes = Vec::new();
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for &byte in b64.as_bytes() {
        if byte == b'=' { break; }
        let val = match TABLE.iter().position(|&b| b == byte) {
            Some(v) => v as u32,
            None => continue,
        };
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            bytes.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }

    let json_str = String::from_utf8(bytes).ok()?;
    let val: serde_json::Value = serde_json::from_str(&json_str).ok()?;
    let sub = val.get("sub")?.as_str()?;

    // Strip "auth0|" prefix if present
    if let Some(user_id) = sub.strip_prefix("auth0|") {
        Some(user_id.to_string())
    } else {
        Some(sub.to_string())
    }
}

/// Build the cookie header for Cursor API calls.
/// The cookie format must be: WorkosCursorSessionToken=userId%3A%3AJWT
pub fn cookie_header(token: &str) -> String {
    // If token already contains :: or %3A%3A, it's the full cookie value
    if token.contains("::") || token.contains("%3A%3A") {
        return format!("WorkosCursorSessionToken={}", token);
    }

    // Token is a bare JWT — extract user ID from it and build the full cookie
    if let Some(user_id) = extract_user_id_from_jwt(token) {
        format!("WorkosCursorSessionToken={}%3A%3A{}", user_id, token)
    } else {
        format!("WorkosCursorSessionToken={}", token)
    }
}

// ── Local AI tracking stats ──────────────────────────────────────────────────

/// Enrich extra with AI code tracking stats for today and this week from Cursor's local SQLite.
fn enrich_with_local_stats(extra: &mut HashMap<String, serde_json::Value>) {
    let db_path = dirs::home_dir().unwrap_or_default()
        .join(".cursor").join("ai-tracking").join("ai-code-tracking.db");
    if !db_path.exists() { return; }

    let conn = match rusqlite::Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(_) => return,
    };

    let now_ts = chrono::Utc::now().timestamp();
    let parse_fallback_ms = (now_ts - 86400 * 7) * 1000;

    let local_now = chrono::Local::now();
    use chrono::{Datelike, NaiveTime, TimeZone};
    let today_midnight = local_now.date_naive().and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap());
    let cutoff_today_ms = chrono::Local.from_local_datetime(&today_midnight).single()
        .map(|dt| dt.timestamp() * 1000).unwrap_or(parse_fallback_ms);

    let weekday = local_now.weekday().num_days_from_monday();
    let monday = (local_now.date_naive() - chrono::Duration::days(weekday as i64))
        .and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap());
    let cutoff_week_ms = chrono::Local.from_local_datetime(&monday).single()
        .map(|dt| dt.timestamp() * 1000).unwrap_or(parse_fallback_ms);

    fn query_window(conn: &rusqlite::Connection, cutoff_ms: i64, prefix: &str, extra: &mut HashMap<String, serde_json::Value>) {
        let result: Result<(i64, i64, i64), _> = conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(composerLinesAdded), 0), COALESCE(SUM(humanLinesAdded), 0) FROM scored_commits WHERE scoredAt > ?1",
            [cutoff_ms],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        );
        if let Ok((commits, ai_lines, human_lines)) = result {
            extra.insert(format!("{}_commits", prefix), serde_json::json!(commits));
            extra.insert(format!("{}_ai_lines", prefix), serde_json::json!(ai_lines));
            extra.insert(format!("{}_human_lines", prefix), serde_json::json!(human_lines));
        }
    }

    query_window(&conn, cutoff_today_ms, "start_today", extra);
    query_window(&conn, cutoff_week_ms, "this_week", extra);

    // All-time totals for AI code attribution
    let all_time: Result<(i64, i64, i64), _> = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(composerLinesAdded), 0), COALESCE(SUM(humanLinesAdded), 0) FROM scored_commits",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    );
    if let Ok((commits, ai_lines, human_lines)) = all_time {
        extra.insert("ai_total_commits".into(), serde_json::json!(commits));
        extra.insert("ai_total_ai_lines".into(), serde_json::json!(ai_lines));
        extra.insert("ai_total_human_lines".into(), serde_json::json!(human_lines));
    }
}

// ── Public API ──────────────────────────────────────────────────────────────

pub fn fetch_cursor_analytics() -> ProviderAnalytics {
    // Return cached data if still fresh
    if let Ok(guard) = CURSOR_CACHE.lock() {
        if let Some(ref entry) = *guard {
            if entry.fetched_at.elapsed().as_secs() < CURSOR_CACHE_TTL_SECS {
                return entry.data.clone();
            }
        }
    }

    let result = fetch_cursor_analytics_uncached();

    if result.status.connected {
        if let Ok(mut guard) = CURSOR_CACHE.lock() {
            *guard = Some(CursorCacheEntry {
                data: result.clone(),
                fetched_at: std::time::Instant::now(),
            });
        }
    }

    result
}

fn fetch_cursor_analytics_uncached() -> ProviderAnalytics {
    let now = Utc::now().to_rfc3339();

    let (token, method) = match resolve_token() {
        Ok(t) => t,
        Err(e) => {
            return ProviderAnalytics {
                provider_id: "cursor".into(),
                provider_name: "Cursor".into(),
                status: ProviderStatus {
                    provider_id: "cursor".into(),
                    provider_name: "Cursor".into(),
                    connected: false,
                    connection_method: "none".into(),
                    account_email: None, plan_name: None, org_name: None,
                    error: Some(e),
                },
                rate_limits: vec![], credit_usage: None, token_counts: None,
                limit_state: None,
                extra: HashMap::new(), fetched_at: now,
            };
        }
    };

    let cookie = cookie_header(&token);

    // Fetch usage summary
    let usage: Option<UsageSummary> = http::cookie_get(
        "https://cursor.com/api/usage-summary",
        &cookie,
        None,
    ).ok();

    // Fetch auth/me
    let auth_me: Option<CursorAuthMe> = http::cookie_get(
        "https://cursor.com/api/auth/me",
        &cookie,
        None,
    ).ok();

    // Build credit usage — prefer v2 nested structure, fall back to flat fields
    let credit_usage = usage.as_ref().map(|u| {
        let v2 = u.individual_usage.as_ref().and_then(|iu| iu.plan.as_ref());
        let (used_usd, limit_usd) = if let Some(plan) = v2 {
            (plan.used.unwrap_or(0.0) / 100.0, plan.limit.map(|c| c / 100.0))
        } else {
            (u.plan_used_cents.unwrap_or(0.0) / 100.0, u.plan_limit_cents.map(|c| c / 100.0))
        };
        let remaining = limit_usd.map(|l| (l - used_usd).max(0.0)).unwrap_or(0.0);
        CreditUsage {
            provider_id: "cursor".into(),
            used: used_usd,
            limit: limit_usd,
            remaining,
            currency: "USD".into(),
            billing_cycle_end: u.billing_cycle_end.clone(),
            plan_name: u.membership_type.clone(),
        }
    });

    let mut extra = HashMap::new();
    if let Some(ref u) = usage {
        // Prefer nested individualUsage.onDemand (V2) over top-level onDemandUsedCents (legacy).
        let individual_od = u.individual_usage.as_ref().and_then(|iu| iu.on_demand.as_ref());
        let od_used_cents = individual_od.and_then(|od| od.used).or(u.on_demand_used_cents);
        let od_limit_cents = individual_od.and_then(|od| od.limit).or(u.on_demand_limit_cents);
        if let Some(on_demand) = od_used_cents {
            extra.insert("on_demand_used_usd".into(), serde_json::Value::from(on_demand / 100.0));
        }
        if let Some(on_demand_limit) = od_limit_cents {
            extra.insert("on_demand_limit_usd".into(), serde_json::Value::from(on_demand_limit / 100.0));
        }
        if let Some(enabled) = individual_od.and_then(|od| od.enabled) {
            extra.insert("on_demand_enabled".into(), serde_json::json!(enabled));
        }
        if let Some(ref start) = u.billing_cycle_start {
            extra.insert("billing_cycle_start".into(), serde_json::Value::String(start.clone()));
        }
        // V2 plan breakdown details for the tray card
        if let Some(plan) = u.individual_usage.as_ref().and_then(|iu| iu.plan.as_ref()) {
            if let Some(ref bd) = plan.breakdown {
                if let Some(inc) = bd.included { extra.insert("plan_included_usd".into(), serde_json::json!(inc / 100.0)); }
                if let Some(bon) = bd.bonus { extra.insert("plan_bonus_usd".into(), serde_json::json!(bon / 100.0)); }
                if let Some(tot) = bd.total { extra.insert("plan_total_budget_usd".into(), serde_json::json!(tot / 100.0)); }
            }
            if let Some(pct) = plan.total_percent_used { extra.insert("plan_total_percent_used".into(), serde_json::json!(pct)); }
        }
        // Team on-demand
        if let Some(ref tu) = u.team_usage {
            if let Some(ref od) = tu.on_demand {
                extra.insert("team_od_enabled".into(), serde_json::json!(od.enabled.unwrap_or(false)));
                if let Some(used) = od.used { extra.insert("team_od_used_usd".into(), serde_json::json!(used / 100.0)); }
                if let Some(lim) = od.limit { extra.insert("team_od_limit_usd".into(), serde_json::json!(lim / 100.0)); }
            }
        }
    }

    // Fetch hard limit (needs team_id from /api/auth/stripe)
    #[derive(Deserialize, Debug)]
    #[serde(rename_all = "camelCase")]
    struct StripeInfo { team_id: Option<i64>, }
    #[derive(Deserialize, Debug)]
    #[serde(rename_all = "camelCase")]
    struct HardLimitResp { hard_limit: Option<f64>, is_dynamic_team_limit: Option<bool>, }

    let stripe_info: Option<StripeInfo> = http::cookie_get("https://cursor.com/api/auth/stripe", &cookie, None).ok();
    if let Some(tid) = stripe_info.as_ref().and_then(|s| s.team_id) {
        let post_headers = http::headers(&[("Origin", "https://cursor.com")]);
        if let Ok(hl) = http::cookie_post::<HardLimitResp>(
            "https://cursor.com/api/dashboard/get-hard-limit",
            &cookie,
            &serde_json::json!({"teamId": tid}),
            Some(post_headers),
        ) {
            // get-hard-limit returns dollars, unlike team_usage which is cents
            if let Some(limit) = hl.hard_limit {
                extra.insert("team_hard_limit_usd".into(), serde_json::json!(limit));
            }
            if let Some(dyn_flag) = hl.is_dynamic_team_limit {
                extra.insert("team_hard_limit_dynamic".into(), serde_json::json!(dyn_flag));
            }
        }
    }

    let email = auth_me.as_ref().and_then(|a| a.email.clone());
    let org_name = auth_me.as_ref().and_then(|a| a.team_name.clone());
    let plan_name = usage.as_ref().and_then(|u| u.membership_type.clone());

    // Enrich with local AI tracking stats (today + week)
    enrich_with_local_stats(&mut extra);

    ProviderAnalytics {
        provider_id: "cursor".into(),
        provider_name: "Cursor".into(),
        status: ProviderStatus {
            provider_id: "cursor".into(),
            provider_name: "Cursor".into(),
            connected: true,
            connection_method: method,
            account_email: email,
            plan_name,
            org_name,
            error: None,
        },
        rate_limits: vec![],
        credit_usage,
        token_counts: None,
        limit_state: None,
        extra,
        fetched_at: now,
    }
}

pub fn check_connection() -> ProviderStatus {
    match resolve_token() {
        Ok((_, method)) => ProviderStatus {
            provider_id: "cursor".into(),
            provider_name: "Cursor".into(),
            connected: true, connection_method: method,
            account_email: None, plan_name: None, org_name: None, error: None,
        },
        Err(e) => ProviderStatus {
            provider_id: "cursor".into(),
            provider_name: "Cursor".into(),
            connected: false, connection_method: "none".into(),
            account_email: None, plan_name: None, org_name: None, error: Some(e),
        },
    }
}
