//! Gemini CLI analytics provider.
//! Auth: auto-detect ~/.gemini/oauth_creds.json
//! API: cloudcode-pa.googleapis.com

use crate::analytics::http;
use crate::analytics::token_store;
use crate::analytics::types::*;
use chrono::{Datelike, Local, TimeZone, Utc};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

// ── In-memory cache (60s TTL) ────────────────────────────────────────────────

struct GeminiCacheEntry {
    data: ProviderAnalytics,
    fetched_at: Instant,
}

lazy_static::lazy_static! {
    static ref GEMINI_CACHE: Mutex<Option<GeminiCacheEntry>> = Mutex::new(None);
}

const GEMINI_CACHE_TTL_SECS: u64 = 60;

// ── OAuth constants ─────────────────────────────────────────────────────────

const GEMINI_CLIENT_ID: &str = "681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j.apps.googleusercontent.com";
const GEMINI_CLIENT_SECRET: &str = "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";

// ── Credential & response types ─────────────────────────────────────────────

#[derive(Deserialize, Debug, Clone)]
struct GeminiOAuthCreds {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expiry_date: Option<f64>, // ms since epoch (can be float in some versions)
    id_token: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
}

#[derive(Deserialize, Debug)]
struct GoogleTokenRefreshResponse {
    access_token: String,
    expires_in: Option<u64>,
    token_type: Option<String>,
    id_token: Option<String>,
}

#[derive(Deserialize, Debug)]
struct GoogleAccountsFile {
    active: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct QuotaBucket {
    /// 0.0 = fully used, 1.0 = fully remaining
    remaining_fraction: Option<f64>,
    /// Absolute requests remaining. The API returns this as a string; be
    /// lenient and accept a number too.
    remaining_amount: Option<serde_json::Value>,
    reset_time: Option<String>,
    model_id: Option<String>,
    token_type: Option<String>,
}

#[derive(Deserialize, Debug)]
struct QuotaResponse {
    buckets: Option<Vec<QuotaBucket>>,
}

#[derive(Deserialize, Debug)]
struct CodeAssistTier {
    id: Option<String>,
    name: Option<String>,
    description: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct PaidTier {
    id: Option<String>,
    name: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct IneligibleTier {
    reason_code: Option<String>,
    reason_message: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct CodeAssistResponse {
    current_tier: Option<CodeAssistTier>,
    paid_tier: Option<PaidTier>,
    #[serde(default)]
    ineligible_tiers: Vec<IneligibleTier>,
    /// API returns "cloudaicompanionProject" (NOT "cloudaiCompanionProject")
    #[serde(alias = "cloudaicompanionProject")]
    cloudai_companion_project: Option<String>,
    upgrade_subscription_uri: Option<String>,
    #[serde(alias = "gcpManaged")]
    gcp_managed: Option<bool>,
}

/// Resolved plan display + the "individual tier sunset" signal.
#[derive(Debug, Default, Clone, PartialEq)]
struct PlanResolution {
    plan_name: Option<String>,
    sunset: bool,
    sunset_message: Option<String>,
}

/// Resolution order: `paidTier` (id/name) → `currentTier` map → if neither is
/// present, look at `ineligibleTiers` for the `UNSUPPORTED_CLIENT` reason
/// (individual-tier sunset — Google's migrate-to-Antigravity message).
fn resolve_plan(ca: &CodeAssistResponse) -> PlanResolution {
    if let Some(ref paid) = ca.paid_tier {
        let name = paid.name.clone().or_else(|| paid.id.clone());
        if name.is_some() {
            return PlanResolution { plan_name: name, sunset: false, sunset_message: None };
        }
    }

    if let Some(ref tier) = ca.current_tier {
        let name = match tier.id.as_deref() {
            Some("free-tier") => "Free".to_string(),
            Some("standard-tier") => "Paid".to_string(),
            Some("legacy-tier") => "Legacy".to_string(),
            _ => tier.name.clone().unwrap_or_else(|| "Unknown".into()),
        };
        return PlanResolution { plan_name: Some(name), sunset: false, sunset_message: None };
    }

    let sunset_entry = ca
        .ineligible_tiers
        .iter()
        .find(|t| t.reason_code.as_deref() == Some("UNSUPPORTED_CLIENT"));
    if let Some(entry) = sunset_entry {
        return PlanResolution {
            plan_name: Some("Individual tier (sunset)".to_string()),
            sunset: true,
            sunset_message: entry.reason_message.clone(),
        };
    }

    PlanResolution::default()
}

/// Lenient number parse — some API fields are returned as either numbers or
/// numeric strings (e.g. `remainingAmount`).
fn lenient_f64(v: Option<&serde_json::Value>) -> Option<f64> {
    match v {
        Some(serde_json::Value::Number(n)) => n.as_f64(),
        Some(serde_json::Value::String(s)) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

// ── Local session log entry ─────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GeminiLogEntry {
    timestamp: Option<String>,
    session_id: Option<String>,
    message: Option<serde_json::Value>,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn creds_path() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".gemini").join("oauth_creds.json")
}

fn google_accounts_path() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".gemini").join("google_accounts.json")
}

fn gemini_tmp_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".gemini").join("tmp")
}

/// Read the active email from ~/.gemini/google_accounts.json as a fallback.
fn read_fallback_email() -> Option<String> {
    let path = google_accounts_path();
    if !path.exists() {
        return None;
    }
    let content = fs::read_to_string(&path).ok()?;
    let accounts: GoogleAccountsFile = serde_json::from_str(&content).ok()?;
    accounts.active.filter(|e| !e.is_empty())
}

/// Extract the account email from the `id_token` JWT (no signature
/// verification needed — we're only reading claims from our own cached
/// creds), falling back to `google_accounts.json`. Takes the already-parsed
/// creds so callers never re-read `oauth_creds.json` just for the email.
fn extract_email(creds: Option<&GeminiOAuthCreds>) -> Option<String> {
    let jwt_email = creds.and_then(|c| c.id_token.as_ref()).and_then(|jwt| {
        let parts: Vec<&str> = jwt.split('.').collect();
        if parts.len() < 2 {
            return None;
        }
        let payload = parts[1];
        let padded = match payload.len() % 4 {
            2 => format!("{}==", payload),
            3 => format!("{}=", payload),
            _ => payload.to_string(),
        };
        let decoded = padded.replace('-', "+").replace('_', "/");
        let bytes = base64_decode(&decoded)?;
        let val: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
        val.get("email").and_then(|e| e.as_str()).map(|s| s.to_string())
    });
    jwt_email.or_else(read_fallback_email)
}

/// Check whether the access token in oauth_creds.json has expired.
/// Returns true if expired or if expiry_date is missing.
fn is_token_expired(creds: &GeminiOAuthCreds) -> bool {
    match creds.expiry_date {
        Some(expiry_ms) => {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            // Consider expired if less than 60s remaining
            now_ms + 60_000 >= expiry_ms as u64
        }
        None => true, // No expiry info — assume expired
    }
}

/// Refresh the Google OAuth access token using the refresh_token.
/// On success, updates ~/.gemini/oauth_creds.json and returns the new access token.
fn refresh_access_token(creds: &GeminiOAuthCreds) -> Result<String, String> {
    let refresh_token = creds.refresh_token.as_ref()
        .filter(|t| !t.is_empty())
        .ok_or_else(|| "No refresh_token available for Gemini".to_string())?;

    let client = http::build_client(15)?;
    let params = [
        ("grant_type", "refresh_token"),
        ("client_id", GEMINI_CLIENT_ID),
        ("client_secret", GEMINI_CLIENT_SECRET),
        ("refresh_token", refresh_token.as_str()),
    ];

    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&params)
        .send()
        .map_err(|e| format!("Token refresh request failed: {}", e))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(format!("Token refresh HTTP {}: {}", status, body));
    }

    let token_resp: GoogleTokenRefreshResponse = resp
        .json()
        .map_err(|e| format!("Token refresh parse error: {}", e))?;

    // Calculate new expiry (expires_in is seconds from now)
    let new_expiry_ms = token_resp.expires_in.map(|secs| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
            + secs * 1000
    });

    // Save refreshed credentials back to the file
    let path = creds_path();
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(mut raw) = serde_json::from_str::<serde_json::Value>(&content) {
            raw["access_token"] = serde_json::Value::String(token_resp.access_token.clone());
            if let Some(expiry) = new_expiry_ms {
                raw["expiry_date"] = serde_json::Value::Number(serde_json::Number::from(expiry));
            }
            if let Some(ref new_id_token) = token_resp.id_token {
                raw["id_token"] = serde_json::Value::String(new_id_token.clone());
            }
            // Atomic write: write to .tmp then rename
            let tmp_path = path.with_extension("json.tmp");
            if let Ok(serialized) = serde_json::to_string_pretty(&raw) {
                let _ = fs::write(&tmp_path, serialized);
                let _ = fs::rename(&tmp_path, &path);
            }
        }
    }

    Ok(token_resp.access_token)
}

/// Resolves the bearer token, also handing back the parsed creds (when read
/// from `oauth_creds.json`) so callers can pull the email from the same
/// parse instead of re-reading the file.
fn resolve_token() -> Result<(String, String, Option<GeminiOAuthCreds>), String> {
    // 1. User-provided
    if let Ok(Some(token)) = token_store::get_provider_token("gemini", "access-token") {
        return Ok((token, "token-manual".into(), None));
    }

    // 2. Auto-detect
    let path = creds_path();
    if !path.exists() {
        return Err("Gemini credentials not found (~/.gemini/oauth_creds.json)".into());
    }
    let content = fs::read_to_string(&path).map_err(|e| format!("Read error: {}", e))?;
    let mut creds: GeminiOAuthCreds = serde_json::from_str(&content).map_err(|e| format!("Parse error: {}", e))?;

    // 3. Check expiry and refresh if needed
    if is_token_expired(&creds) {
        match refresh_access_token(&creds) {
            Ok(new_token) => {
                creds.access_token = Some(new_token.clone());
                return Ok((new_token, "oauth-auto-refreshed".into(), Some(creds)));
            }
            Err(_e) => {
                // Fall through — try the existing token anyway (it might still work briefly)
            }
        }
    }

    if let Some(ref token) = creds.access_token {
        if !token.is_empty() {
            let tok = token.clone();
            return Ok((tok, "oauth-auto".into(), Some(creds)));
        }
    }
    Err("No Gemini access token found".into())
}

// ── Local session & telemetry data ──────────────────────────────────────────

/// Enrich extra with local session stats from ~/.gemini/tmp/*/logs.json
/// and any telemetry output files found.
fn enrich_local_stats(extra: &mut HashMap<String, serde_json::Value>) {
    // ── Phase 1: Parse logs.json files for session/message counts ────────
    let tmp_dir = gemini_tmp_dir();
    let mut unique_sessions = std::collections::HashSet::new();
    let mut total_messages: u64 = 0;
    let mut project_count: u64 = 0;
    let mut earliest_ts: Option<String> = None;
    let mut latest_ts: Option<String> = None;

    if tmp_dir.exists() {
        if let Ok(entries) = fs::read_dir(&tmp_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                project_count += 1;
                let logs_path = path.join("logs.json");
                if !logs_path.exists() {
                    continue;
                }
                let content = match fs::read_to_string(&logs_path) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let log_entries: Vec<GeminiLogEntry> = match serde_json::from_str(&content) {
                    Ok(entries) => entries,
                    Err(_) => continue,
                };

                for log_entry in &log_entries {
                    total_messages += 1;
                    if let Some(ref sid) = log_entry.session_id {
                        unique_sessions.insert(sid.clone());
                    }
                    if let Some(ref ts) = log_entry.timestamp {
                        if earliest_ts.as_ref().map_or(true, |e| ts < e) {
                            earliest_ts = Some(ts.clone());
                        }
                        if latest_ts.as_ref().map_or(true, |l| ts > l) {
                            latest_ts = Some(ts.clone());
                        }
                    }
                }
            }
        }
    }

    extra.insert("gemini_total_sessions".into(),
        serde_json::Value::Number(serde_json::Number::from(unique_sessions.len() as u64)));
    extra.insert("gemini_total_messages".into(),
        serde_json::Value::Number(serde_json::Number::from(total_messages)));
    extra.insert("gemini_project_count".into(),
        serde_json::Value::Number(serde_json::Number::from(project_count)));
    if let Some(ref ts) = earliest_ts {
        extra.insert("gemini_first_activity".into(), serde_json::Value::String(ts.clone()));
    }
    if let Some(ref ts) = latest_ts {
        extra.insert("gemini_last_activity".into(), serde_json::Value::String(ts.clone()));
    }

    // ── Phase 2: Check for telemetry output files ────────────────────────
    // Users can enable detailed stats by running: gemini --telemetry --telemetry-outfile <path>
    // We check common locations for these files.
    let home = dirs::home_dir().unwrap_or_default();
    let telemetry_paths = [
        home.join(".gemini").join("telemetry.jsonl"),
        home.join(".gemini").join("telemetry.log"),
        home.join(".gemini").join("telemetry.json"),
    ];
    let mut has_telemetry_file = false;
    let mut telemetry_api_requests: u64 = 0;
    let mut telemetry_api_errors: u64 = 0;
    let mut telemetry_total_latency_ms: f64 = 0.0;
    let mut telemetry_input_tokens: u64 = 0;
    let mut telemetry_output_tokens: u64 = 0;
    let mut telemetry_cached_tokens: u64 = 0;
    let mut telemetry_thought_tokens: u64 = 0;
    let mut telemetry_tool_calls: u64 = 0;
    let mut telemetry_tool_success: u64 = 0;
    let mut telemetry_models: std::collections::HashMap<String, u64> = std::collections::HashMap::new();

    for path in &telemetry_paths {
        if !path.exists() {
            continue;
        }
        has_telemetry_file = true;
        if let Ok(content) = fs::read_to_string(path) {
            // Parse JSONL — each line is a JSON object
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || !line.starts_with('{') {
                    continue;
                }
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
                    // Check for OpenTelemetry span/log event names
                    let name = val.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let attrs = val.get("attributes").unwrap_or(&serde_json::Value::Null);

                    if name.contains("api_response") || name.contains("api.request") {
                        telemetry_api_requests += 1;
                        if let Some(lat) = attrs.get("duration_ms").and_then(|v| v.as_f64()) {
                            telemetry_total_latency_ms += lat;
                        }
                        if let Some(m) = attrs.get("model").and_then(|v| v.as_str()) {
                            *telemetry_models.entry(m.to_string()).or_insert(0) += 1;
                        }
                        if let Some(t) = attrs.get("input_token_count").and_then(|v| v.as_u64()) {
                            telemetry_input_tokens += t;
                        }
                        if let Some(t) = attrs.get("output_token_count").and_then(|v| v.as_u64()) {
                            telemetry_output_tokens += t;
                        }
                        if let Some(t) = attrs.get("cached_content_token_count").and_then(|v| v.as_u64()) {
                            telemetry_cached_tokens += t;
                        }
                        if let Some(t) = attrs.get("thoughts_token_count").and_then(|v| v.as_u64()) {
                            telemetry_thought_tokens += t;
                        }
                    }
                    if name.contains("api_error") {
                        telemetry_api_errors += 1;
                    }
                    if name.contains("tool_call") || name.contains("tool.call") {
                        telemetry_tool_calls += 1;
                        if attrs.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
                            telemetry_tool_success += 1;
                        }
                    }
                }
            }
        }
        break; // Only parse the first found file
    }

    extra.insert("gemini_has_telemetry".into(), serde_json::Value::Bool(has_telemetry_file));
    if has_telemetry_file && telemetry_api_requests > 0 {
        extra.insert("gemini_api_requests".into(),
            serde_json::Value::Number(serde_json::Number::from(telemetry_api_requests)));
        extra.insert("gemini_api_errors".into(),
            serde_json::Value::Number(serde_json::Number::from(telemetry_api_errors)));
        let avg_latency = if telemetry_api_requests > 0 {
            telemetry_total_latency_ms / telemetry_api_requests as f64
        } else { 0.0 };
        extra.insert("gemini_avg_latency_ms".into(),
            serde_json::json!(avg_latency));
        extra.insert("gemini_input_tokens".into(),
            serde_json::Value::Number(serde_json::Number::from(telemetry_input_tokens)));
        extra.insert("gemini_output_tokens".into(),
            serde_json::Value::Number(serde_json::Number::from(telemetry_output_tokens)));
        extra.insert("gemini_cached_tokens".into(),
            serde_json::Value::Number(serde_json::Number::from(telemetry_cached_tokens)));
        extra.insert("gemini_thought_tokens".into(),
            serde_json::Value::Number(serde_json::Number::from(telemetry_thought_tokens)));
        extra.insert("gemini_tool_calls".into(),
            serde_json::Value::Number(serde_json::Number::from(telemetry_tool_calls)));
        extra.insert("gemini_tool_success".into(),
            serde_json::Value::Number(serde_json::Number::from(telemetry_tool_success)));
        // Per-model request breakdown
        let model_json: serde_json::Value = telemetry_models.into_iter()
            .map(|(k, v)| (k, serde_json::Value::Number(serde_json::Number::from(v))))
            .collect::<serde_json::Map<String, serde_json::Value>>()
            .into();
        extra.insert("gemini_model_breakdown".into(), model_json);
    }
}

// ── Chats JSONL scanner (auto-recorded sessions, Gemini CLI 0.46.0+) ────────
//
// `~/.gemini/tmp/<projectHash>/chats/session-<ts>-<sid8>.jsonl`, one message
// per line plus a leading session-metadata line. Subagent sessions nest one
// level deeper: `chats/<parentSessionId>/<agentId>.jsonl`. Older CLI versions
// wrote a single-record `.json` file per session instead.

#[derive(Deserialize, Debug, Default, Clone)]
struct GeminiTokensField {
    input: Option<u64>,
    output: Option<u64>,
    cached: Option<u64>,
    thoughts: Option<u64>,
    tool: Option<u64>,
    total: Option<u64>,
}

#[derive(Deserialize, Debug, Default, Clone)]
struct GeminiChatRecord {
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    #[serde(rename = "startTime")]
    start_time: Option<serde_json::Value>,
    #[serde(rename = "lastUpdated")]
    last_updated: Option<serde_json::Value>,
    timestamp: Option<serde_json::Value>,
    #[serde(rename = "type")]
    kind: Option<String>,
    model: Option<String>,
    tokens: Option<GeminiTokensField>,
}

#[derive(Debug, Default, Clone)]
struct GeminiTokenTotals {
    input: u64,
    output: u64,
    cached: u64,
    thoughts: u64,
    tool: u64,
}

#[derive(Debug, Default, Clone)]
struct ChatsScanResult {
    total_sessions: u64,
    total_messages: u64,
    total_tokens: u64,
    token_totals: GeminiTokenTotals,
    models_used: HashMap<String, u64>,
    today_sessions: u64,
    today_tokens: u64,
    today_messages: u64,
    week_sessions: u64,
    week_tokens: u64,
    week_messages: u64,
}

/// "Today" / "this week" (Monday-start) cutoffs, computed once per scan.
struct DateCtx {
    today: chrono::NaiveDate,
    monday: chrono::NaiveDate,
}

impl DateCtx {
    fn now() -> Self {
        let today = Local::now().date_naive();
        let monday = today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64);
        Self { today, monday }
    }
}

/// Bounded scan — skip anything bigger than this so a runaway log can't stall
/// the analytics fetch.
const MAX_CHATS_FILE_BYTES: u64 = 10 * 1024 * 1024;

fn value_to_date(v: &serde_json::Value) -> Option<chrono::NaiveDate> {
    match v {
        serde_json::Value::String(s) => parse_date_str(s),
        serde_json::Value::Number(n) => {
            let ms = n.as_i64()?;
            Local.timestamp_millis_opt(ms).single().map(|dt| dt.date_naive())
        }
        _ => None,
    }
}

fn parse_date_str(s: &str) -> Option<chrono::NaiveDate> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Local).date_naive());
    }
    s.parse::<i64>()
        .ok()
        .and_then(|ms| Local.timestamp_millis_opt(ms).single())
        .map(|dt| dt.date_naive())
}

/// Accumulate one message record's tokens/model into the running totals and
/// today/week buckets.
fn record_message(
    rec: &GeminiChatRecord,
    msg_date: Option<chrono::NaiveDate>,
    ctx: &DateCtx,
    result: &mut ChatsScanResult,
) {
    result.total_messages += 1;
    let mut total_tok = 0u64;
    if let Some(tokens) = &rec.tokens {
        let input = tokens.input.unwrap_or(0);
        let output = tokens.output.unwrap_or(0);
        let cached = tokens.cached.unwrap_or(0);
        let thoughts = tokens.thoughts.unwrap_or(0);
        let tool = tokens.tool.unwrap_or(0);
        total_tok = tokens.total.unwrap_or(input + output + cached + thoughts + tool);
        result.token_totals.input += input;
        result.token_totals.output += output;
        result.token_totals.cached += cached;
        result.token_totals.thoughts += thoughts;
        result.token_totals.tool += tool;
        result.total_tokens += total_tok;
        if let Some(model) = &rec.model {
            *result.models_used.entry(model.clone()).or_insert(0) += 1;
        }
    }
    if let Some(d) = msg_date {
        if d == ctx.today {
            result.today_messages += 1;
            result.today_tokens += total_tok;
        }
        if d >= ctx.monday {
            result.week_messages += 1;
            result.week_tokens += total_tok;
        }
    }
}

/// Parse one `chats/*.jsonl` session file: a leading session-metadata record
/// (`sessionId` + `startTime`/`lastUpdated`, no `type`) followed by one
/// message record per line.
fn parse_session_jsonl(content: &str, ctx: &DateCtx, result: &mut ChatsScanResult) {
    let mut counted_session = false;
    let mut touches_today = false;
    let mut touches_week = false;

    let mark_dates = |dates: &[Option<&serde_json::Value>], touches_today: &mut bool, touches_week: &mut bool| {
        for v in dates.iter().flatten() {
            if let Some(d) = value_to_date(v) {
                if d == ctx.today {
                    *touches_today = true;
                }
                if d >= ctx.monday {
                    *touches_week = true;
                }
            }
        }
    };

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(rec) = serde_json::from_str::<GeminiChatRecord>(line) else { continue };

        if rec.kind.is_none() && rec.session_id.is_some() {
            if !counted_session {
                result.total_sessions += 1;
                counted_session = true;
            }
            mark_dates(&[rec.start_time.as_ref(), rec.last_updated.as_ref()], &mut touches_today, &mut touches_week);
            continue;
        }

        if rec.kind.is_some() {
            let msg_date = rec.timestamp.as_ref().and_then(value_to_date);
            mark_dates(&[rec.timestamp.as_ref()], &mut touches_today, &mut touches_week);
            record_message(&rec, msg_date, ctx, result);
        }
    }

    if !counted_session {
        // No recognizable session-metadata line (unexpected shape) — still
        // count the file itself as one session so it isn't silently dropped.
        result.total_sessions += 1;
    }
    if touches_today {
        result.today_sessions += 1;
    }
    if touches_week {
        result.week_sessions += 1;
    }
}

/// Parse a legacy single-record `.json` session file (older Gemini CLI).
fn parse_legacy_session_json(content: &str, ctx: &DateCtx, result: &mut ChatsScanResult) {
    let Ok(rec) = serde_json::from_str::<GeminiChatRecord>(content) else { return };
    result.total_sessions += 1;

    let mut touches_today = false;
    let mut touches_week = false;
    for v in [rec.start_time.as_ref(), rec.last_updated.as_ref(), rec.timestamp.as_ref()].into_iter().flatten() {
        if let Some(d) = value_to_date(v) {
            if d == ctx.today {
                touches_today = true;
            }
            if d >= ctx.monday {
                touches_week = true;
            }
        }
    }
    if touches_today {
        result.today_sessions += 1;
    }
    if touches_week {
        result.week_sessions += 1;
    }

    if rec.kind.is_some() {
        let msg_date = rec.timestamp.as_ref().and_then(value_to_date);
        record_message(&rec, msg_date, ctx, result);
    }
}

/// Walk one `chats/` directory: session files directly inside, plus one
/// level of subagent subdirectories (`chats/<parentSessionId>/<agentId>.jsonl`).
fn scan_chats_dir(dir: &Path, ctx: &DateCtx, result: &mut ChatsScanResult, depth: u8) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth == 0 {
                scan_chats_dir(&path, ctx, result, depth + 1);
            }
            continue;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else { continue };
        if ext != "jsonl" && ext != "json" {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if meta.len() > MAX_CHATS_FILE_BYTES {
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else { continue };
        if ext == "jsonl" {
            parse_session_jsonl(&content, ctx, result);
        } else {
            parse_legacy_session_json(&content, ctx, result);
        }
    }
}

fn scan_chats(tmp_dir: &Path) -> ChatsScanResult {
    let mut result = ChatsScanResult::default();
    let ctx = DateCtx::now();
    let Ok(project_dirs) = fs::read_dir(tmp_dir) else { return result };
    for project_entry in project_dirs.flatten() {
        let chats_dir = project_entry.path().join("chats");
        if chats_dir.is_dir() {
            scan_chats_dir(&chats_dir, &ctx, &mut result, 0);
        }
    }
    result
}

struct ChatsCacheEntry {
    data: ChatsScanResult,
    fetched_at: Instant,
}

lazy_static::lazy_static! {
    static ref CHATS_CACHE: Mutex<Option<ChatsCacheEntry>> = Mutex::new(None);
}

/// Separate, longer-lived cache for the chats-directory walk (it's a real
/// filesystem scan, unlike the rest of `fetch_gemini_analytics`'s 60s cache).
const CHATS_CACHE_TTL_SECS: u64 = 300;

fn scan_chats_cached() -> ChatsScanResult {
    if let Ok(guard) = CHATS_CACHE.lock() {
        if let Some(entry) = guard.as_ref() {
            if entry.fetched_at.elapsed().as_secs() < CHATS_CACHE_TTL_SECS {
                return entry.data.clone();
            }
        }
    }
    let data = scan_chats(&gemini_tmp_dir());
    if let Ok(mut guard) = CHATS_CACHE.lock() {
        *guard = Some(ChatsCacheEntry { data: data.clone(), fetched_at: Instant::now() });
    }
    data
}

// ── Public API ──────────────────────────────────────────────────────────────

pub fn fetch_gemini_analytics() -> ProviderAnalytics {
    // Return cached data if still fresh
    if let Ok(guard) = GEMINI_CACHE.lock() {
        if let Some(ref entry) = *guard {
            if entry.fetched_at.elapsed().as_secs() < GEMINI_CACHE_TTL_SECS {
                return entry.data.clone();
            }
        }
    }

    let result = fetch_gemini_analytics_uncached();

    // Only cache successful results
    if result.status.connected {
        if let Ok(mut guard) = GEMINI_CACHE.lock() {
            *guard = Some(GeminiCacheEntry {
                data: result.clone(),
                fetched_at: Instant::now(),
            });
        }
    }

    result
}

/// Clear the Gemini analytics cache so next fetch is fresh.
pub fn clear_cache() {
    if let Ok(mut guard) = GEMINI_CACHE.lock() {
        *guard = None;
    }
}

/// Per-tier absolute remaining/limit (parsed from `remainingAmount` +
/// `remainingFraction`), tracked alongside the used-percent aggregation
/// below so both can be derived from the same "most constrained bucket wins"
/// pass over the quota buckets.
struct TierAgg {
    remaining_fraction: f64,
    reset_time: Option<String>,
    remaining_amount: Option<f64>,
    limit: Option<f64>,
}

fn fetch_gemini_analytics_uncached() -> ProviderAnalytics {
    let now = Utc::now().to_rfc3339();

    let (token, method, creds) = match resolve_token() {
        Ok(t) => t,
        Err(e) => {
            return ProviderAnalytics {
                provider_id: "gemini".into(),
                provider_name: "Gemini CLI".into(),
                status: ProviderStatus {
                    provider_id: "gemini".into(),
                    provider_name: "Gemini CLI".into(),
                    connected: false, connection_method: "none".into(),
                    account_email: None, plan_name: None, org_name: None,
                    error: Some(e),
                },
                rate_limits: vec![], credit_usage: None, token_counts: None,
                limit_state: None,
                extra: HashMap::new(), fetched_at: now,
            };
        }
    };

    let mut extra = HashMap::new();

    // Fetch tier info FIRST — we need the project ID for the quota call.
    // Errors are surfaced into `extra` rather than silently swallowed so the
    // page can explain why plan data might be missing.
    let code_assist: Option<CodeAssistResponse> = match http::authed_post::<CodeAssistResponse>(
        "https://cloudcode-pa.googleapis.com/v1internal:loadCodeAssist",
        &token,
        &serde_json::json!({"metadata": {"ideType": "GEMINI_CLI", "pluginType": "GEMINI"}}),
        None,
    ) {
        Ok(v) => Some(v),
        Err(e) => {
            extra.insert("codeassist_error".into(), serde_json::Value::String(e.to_string()));
            None
        }
    };

    // Fetch quota — pass the project ID from loadCodeAssist for accurate per-user quotas
    let project_id = code_assist.as_ref()
        .and_then(|ca| ca.cloudai_companion_project.as_deref())
        .unwrap_or("");
    let quota_body = if project_id.is_empty() {
        serde_json::json!({})
    } else {
        serde_json::json!({"project": project_id})
    };
    let quota: Option<QuotaResponse> = match http::authed_post::<QuotaResponse>(
        "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuota",
        &token,
        &quota_body,
        None,
    ) {
        Ok(q) => Some(q),
        Err(http::HttpCallError::Unsuccessful { status, body, .. }) => {
            // Individual OAuth tiers get a 403 SUBSCRIPTION_REQUIRED once the
            // quota endpoint requires Code Assist Standard/Enterprise.
            if status == 403 && body.contains("SUBSCRIPTION_REQUIRED") {
                extra.insert(
                    "quota_unavailable_reason".into(),
                    serde_json::Value::String("subscription_required".into()),
                );
            }
            None
        }
        Err(_) => None,
    };

    // Group quota buckets by model TIER (Pro / Flash / Flash Lite / Other).
    // Multiple model variants (e.g. gemini-2.5-pro, gemini-3-pro-preview) share the same
    // tier quota. Pick the LOWEST remainingFraction per tier (most constrained).
    // Matches CodexBar's approach of showing bars per tier.
    let mut rate_limits = Vec::new();
    if let Some(ref q) = quota {
        if let Some(ref buckets) = q.buckets {
            let tier_of = |m: &str| -> &'static str {
                if m.contains("flash-lite") || m.contains("flash_lite") { "Flash Lite" }
                else if m.contains("flash") { "Flash" }
                else if m.contains("pro") { "Pro" }
                else { "Other" }
            };

            let mut tier_map: HashMap<&str, TierAgg> = HashMap::new();
            for bucket in buckets {
                let model_id = bucket.model_id.as_deref().unwrap_or("unknown");
                let tier = tier_of(model_id);
                let remaining = bucket.remaining_fraction.unwrap_or(1.0);
                let remaining_amount = lenient_f64(bucket.remaining_amount.as_ref());
                let limit = match (remaining_amount, bucket.remaining_fraction) {
                    (Some(ra), Some(rf)) if rf > 0.0 => Some(ra / rf),
                    _ => None,
                };
                let valid_reset = bucket.reset_time.as_deref()
                    .filter(|t| !t.starts_with("1970"))
                    .map(String::from);

                let entry = tier_map.entry(tier).or_insert(TierAgg {
                    remaining_fraction: 1.0,
                    reset_time: None,
                    remaining_amount: None,
                    limit: None,
                });
                if remaining < entry.remaining_fraction {
                    entry.remaining_fraction = remaining;
                    entry.remaining_amount = remaining_amount;
                    entry.limit = limit;
                }
                if entry.reset_time.is_none() && valid_reset.is_some() {
                    entry.reset_time = valid_reset;
                }
            }

            // Display order: Pro, Flash, Flash Lite, then Other (only if present).
            let tier_order = ["Pro", "Flash", "Flash Lite", "Other"];
            let mut quota_absolute = serde_json::Map::new();
            for tier in &tier_order {
                if let Some(agg) = tier_map.get(tier) {
                    let used = ((1.0 - agg.remaining_fraction) * 100.0).max(0.0);
                    rate_limits.push(RateLimitWindow {
                        provider_id: "gemini".into(),
                        label: tier.to_string(),
                        used_percent: used,
                        remaining_percent: (agg.remaining_fraction * 100.0).min(100.0),
                        resets_at: agg.reset_time.clone(),
                        resets_in_seconds: None,
                        window_seconds: Some(86400),
                    });
                    if agg.remaining_amount.is_some() || agg.limit.is_some() {
                        quota_absolute.insert(
                            tier.to_string(),
                            serde_json::json!({ "remaining": agg.remaining_amount, "limit": agg.limit }),
                        );
                    }
                }
            }
            if !quota_absolute.is_empty() {
                extra.insert("quota_absolute".into(), serde_json::Value::Object(quota_absolute));
            }
        }
    }

    let plan = code_assist.as_ref().map(resolve_plan).unwrap_or_default();
    if let Some(ref ca) = code_assist {
        if let Some(ref proj) = ca.cloudai_companion_project {
            extra.insert("project_id".into(), serde_json::Value::String(proj.clone()));
        }
        if let Some(ref uri) = ca.upgrade_subscription_uri {
            extra.insert("upgrade_uri".into(), serde_json::Value::String(uri.clone()));
        }
    }
    if plan.sunset {
        extra.insert("plan_sunset".into(), serde_json::Value::Bool(true));
        if let Some(ref msg) = plan.sunset_message {
            extra.insert("plan_sunset_message".into(), serde_json::Value::String(msg.clone()));
        }
    }

    let email = extract_email(creds.as_ref());

    // Enrich with local session stats (legacy logs.json + opt-in telemetry file)
    enrich_local_stats(&mut extra);

    // Enrich with the auto-recorded chats JSONL sessions (Gemini CLI 0.46.0+)
    let chats = scan_chats_cached();
    extra.insert("gemini_chats_sessions".into(), serde_json::json!(chats.total_sessions));
    extra.insert("start_today_sessions".into(), serde_json::json!(chats.today_sessions));
    extra.insert("start_today_tokens".into(), serde_json::json!(chats.today_tokens));
    extra.insert("start_today_messages".into(), serde_json::json!(chats.today_messages));
    extra.insert("this_week_sessions".into(), serde_json::json!(chats.week_sessions));
    extra.insert("this_week_tokens".into(), serde_json::json!(chats.week_tokens));
    extra.insert("this_week_messages".into(), serde_json::json!(chats.week_messages));
    extra.insert("gemini_token_totals".into(), serde_json::json!({
        "input": chats.token_totals.input,
        "output": chats.token_totals.output,
        "cached": chats.token_totals.cached,
        "thoughts": chats.token_totals.thoughts,
        "tool": chats.token_totals.tool,
    }));
    if !chats.models_used.is_empty() {
        let models_json: serde_json::Value = chats.models_used.iter()
            .map(|(k, v)| (k.clone(), serde_json::json!(v)))
            .collect::<serde_json::Map<String, serde_json::Value>>()
            .into();
        extra.insert("gemini_models_used".into(), models_json);
    }

    ProviderAnalytics {
        provider_id: "gemini".into(),
        provider_name: "Gemini CLI".into(),
        status: ProviderStatus {
            provider_id: "gemini".into(),
            provider_name: "Gemini CLI".into(),
            connected: true,
            connection_method: method,
            account_email: email,
            plan_name: plan.plan_name,
            org_name: None,
            error: None,
        },
        rate_limits,
        credit_usage: None,
        token_counts: None,
        limit_state: None,
        extra,
        fetched_at: now,
    }
}

/// Simple base64 decode (no external crate dependency).
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = Vec::new();
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for &byte in input.as_bytes() {
        if byte == b'=' { break; }
        let val = TABLE.iter().position(|&b| b == byte)? as u32;
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(output)
}

pub fn check_connection() -> ProviderStatus {
    match resolve_token() {
        Ok((_, method, creds)) => ProviderStatus {
            provider_id: "gemini".into(),
            provider_name: "Gemini CLI".into(),
            connected: true, connection_method: method,
            account_email: extract_email(creds.as_ref()), plan_name: None, org_name: None, error: None,
        },
        Err(e) => ProviderStatus {
            provider_id: "gemini".into(),
            provider_name: "Gemini CLI".into(),
            connected: false, connection_method: "none".into(),
            account_email: None, plan_name: None, org_name: None, error: Some(e),
        },
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Plan resolution ──

    #[test]
    fn resolve_plan_prefers_paid_tier() {
        let ca = CodeAssistResponse {
            current_tier: Some(CodeAssistTier { id: Some("free-tier".into()), name: None, description: None }),
            paid_tier: Some(PaidTier { id: Some("standard".into()), name: Some("Standard".into()) }),
            ineligible_tiers: vec![],
            cloudai_companion_project: None,
            upgrade_subscription_uri: None,
            gcp_managed: None,
        };
        let plan = resolve_plan(&ca);
        assert_eq!(plan.plan_name.as_deref(), Some("Standard"));
        assert!(!plan.sunset);
    }

    #[test]
    fn resolve_plan_falls_back_to_current_tier_map() {
        let ca = CodeAssistResponse {
            current_tier: Some(CodeAssistTier { id: Some("standard-tier".into()), name: None, description: None }),
            paid_tier: None,
            ineligible_tiers: vec![],
            cloudai_companion_project: None,
            upgrade_subscription_uri: None,
            gcp_managed: None,
        };
        let plan = resolve_plan(&ca);
        assert_eq!(plan.plan_name.as_deref(), Some("Paid"));
        assert!(!plan.sunset);
    }

    #[test]
    fn resolve_plan_detects_sunset_from_ineligible_tiers() {
        let ca = CodeAssistResponse {
            current_tier: None,
            paid_tier: None,
            ineligible_tiers: vec![IneligibleTier {
                reason_code: Some("UNSUPPORTED_CLIENT".into()),
                reason_message: Some("Migrate to Antigravity".into()),
            }],
            cloudai_companion_project: None,
            upgrade_subscription_uri: None,
            gcp_managed: None,
        };
        let plan = resolve_plan(&ca);
        assert_eq!(plan.plan_name.as_deref(), Some("Individual tier (sunset)"));
        assert!(plan.sunset);
        assert_eq!(plan.sunset_message.as_deref(), Some("Migrate to Antigravity"));
    }

    #[test]
    fn resolve_plan_unknown_when_nothing_present() {
        let ca = CodeAssistResponse {
            current_tier: None,
            paid_tier: None,
            ineligible_tiers: vec![],
            cloudai_companion_project: None,
            upgrade_subscription_uri: None,
            gcp_managed: None,
        };
        let plan = resolve_plan(&ca);
        assert_eq!(plan.plan_name, None);
        assert!(!plan.sunset);
    }

    // ── remainingAmount math ──

    #[test]
    fn lenient_f64_accepts_string_and_number() {
        assert_eq!(lenient_f64(Some(&serde_json::json!("42"))), Some(42.0));
        assert_eq!(lenient_f64(Some(&serde_json::json!(42))), Some(42.0));
        assert_eq!(lenient_f64(Some(&serde_json::json!("not-a-number"))), None);
        assert_eq!(lenient_f64(None), None);
    }

    #[test]
    fn quota_bucket_computes_absolute_limit_from_remaining_and_fraction() {
        // remainingAmount = 150, remainingFraction = 0.75 → limit = 200
        let remaining_amount = lenient_f64(Some(&serde_json::json!("150")));
        let remaining_fraction = Some(0.75_f64);
        let limit = match (remaining_amount, remaining_fraction) {
            (Some(ra), Some(rf)) if rf > 0.0 => Some(ra / rf),
            _ => None,
        };
        assert_eq!(remaining_amount, Some(150.0));
        assert_eq!(limit, Some(200.0));
    }

    // ── Chats JSONL parsing ──

    fn write_file(dir: &Path, rel: &str, content: &str) {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn scan_chats_parses_two_sessions_mixed_messages_and_legacy_json() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        let today = Local::now().date_naive();
        let today_iso = format!("{}T10:00:00Z", today.format("%Y-%m-%d"));

        // Session 1: two messages, one user (no tokens) one gemini (with tokens).
        let session1 = format!(
            r#"{{"sessionId":"s1","projectHash":"abc","startTime":"{ts}","lastUpdated":"{ts}"}}
{{"id":"m1","timestamp":"{ts}","type":"user","content":"hi"}}
{{"id":"m2","timestamp":"{ts}","type":"gemini","content":"hello","model":"gemini-2.5-pro","tokens":{{"input":10,"output":20,"cached":2,"thoughts":3,"tool":1,"total":36}}}}
"#,
            ts = today_iso
        );
        write_file(root, "hash1/chats/session-1-aaaaaaaa.jsonl", &session1);

        // Session 2: subagent file nested one level deep, no explicit total (sum fallback).
        let session2 = format!(
            r#"{{"sessionId":"s2","projectHash":"def","startTime":"{ts}","lastUpdated":"{ts}"}}
{{"id":"m3","timestamp":"{ts}","type":"gemini","model":"gemini-2.5-flash","tokens":{{"input":5,"output":7}}}}
"#,
            ts = today_iso
        );
        write_file(root, "hash1/chats/parentSess/agent1.jsonl", &session2);

        // Legacy single-record .json (older CLI) in the same chats dir.
        let legacy = format!(
            r#"{{"sessionId":"s3","timestamp":"{ts}","type":"gemini","model":"gemini-2.5-pro","tokens":{{"input":1,"output":1,"total":2}}}}"#,
            ts = today_iso
        );
        write_file(root, "hash2/chats/legacy-session.json", &legacy);

        let result = scan_chats(root);

        assert_eq!(result.total_sessions, 3);
        assert_eq!(result.total_messages, 4); // 2 in session1 + 1 in session2 + 1 legacy record
        assert_eq!(result.token_totals.input, 10 + 5 + 1);
        assert_eq!(result.token_totals.output, 20 + 7 + 1);
        assert_eq!(result.total_tokens, 36 + 12 + 2); // session2 falls back to sum (5+7=12)
        assert_eq!(result.models_used.get("gemini-2.5-pro"), Some(&2));
        assert_eq!(result.models_used.get("gemini-2.5-flash"), Some(&1));

        // Everything happened "today" in this fixture.
        assert_eq!(result.today_sessions, 3);
        assert_eq!(result.week_sessions, 3);
    }

    #[test]
    fn scan_chats_skips_oversized_files() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let huge = "x".repeat((MAX_CHATS_FILE_BYTES + 1) as usize);
        write_file(root, "hash1/chats/session-huge.jsonl", &huge);

        let result = scan_chats(root);
        assert_eq!(result.total_sessions, 0);
    }

    #[test]
    fn scan_chats_ignores_unparseable_lines_and_missing_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_file(root, "hash1/chats/session-x.jsonl", "not json\n{\"broken\n");
        // A project dir with no `chats/` subdir at all must not panic.
        fs::create_dir_all(root.join("hash2")).unwrap();

        let result = scan_chats(root);
        // The file still counts as one session (fallback) even though every
        // line failed to parse.
        assert_eq!(result.total_sessions, 1);
        assert_eq!(result.total_messages, 0);
    }

    #[test]
    fn value_to_date_handles_rfc3339_and_epoch_millis() {
        let iso = serde_json::json!("2026-01-15T08:00:00Z");
        assert_eq!(
            value_to_date(&iso),
            chrono::NaiveDate::from_ymd_opt(2026, 1, 15)
        );

        let now_ms = Utc::now().timestamp_millis();
        let numeric = serde_json::json!(now_ms);
        assert_eq!(value_to_date(&numeric), Some(Local::now().date_naive()));
    }
}
