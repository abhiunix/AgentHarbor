//! Gemini CLI analytics provider.
//! Auth: auto-detect ~/.gemini/oauth_creds.json
//! API: cloudcode-pa.googleapis.com

use crate::analytics::http;
use crate::analytics::token_store;
use crate::analytics::types::*;
use chrono::Utc;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

// ── In-memory cache (300s TTL) ──────────────────────────────────────────────

struct GeminiCacheEntry {
    data: ProviderAnalytics,
    fetched_at: std::time::Instant,
}

lazy_static::lazy_static! {
    static ref GEMINI_CACHE: Mutex<Option<GeminiCacheEntry>> = Mutex::new(None);
}

const GEMINI_CACHE_TTL_SECS: u64 = 300;

// ── OAuth constants ─────────────────────────────────────────────────────────

const GEMINI_CLIENT_ID: &str = "681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j.apps.googleusercontent.com";
const GEMINI_CLIENT_SECRET: &str = "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";

// ── Credential & response types ─────────────────────────────────────────────

#[derive(Deserialize, Debug)]
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
struct CodeAssistResponse {
    current_tier: Option<CodeAssistTier>,
    /// API returns "cloudaicompanionProject" (NOT "cloudaiCompanionProject")
    #[serde(alias = "cloudaicompanionProject")]
    cloudai_companion_project: Option<String>,
    upgrade_subscription_uri: Option<String>,
    #[serde(alias = "gcpManaged")]
    gcp_managed: Option<bool>,
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

fn resolve_token() -> Result<(String, String), String> {
    // 1. User-provided
    if let Ok(Some(token)) = token_store::get_provider_token("gemini", "access-token") {
        return Ok((token, "token-manual".into()));
    }

    // 2. Auto-detect
    let path = creds_path();
    if !path.exists() {
        return Err("Gemini credentials not found (~/.gemini/oauth_creds.json)".into());
    }
    let content = fs::read_to_string(&path).map_err(|e| format!("Read error: {}", e))?;
    let creds: GeminiOAuthCreds = serde_json::from_str(&content).map_err(|e| format!("Parse error: {}", e))?;

    // 3. Check expiry and refresh if needed
    if is_token_expired(&creds) {
        match refresh_access_token(&creds) {
            Ok(new_token) => {
                return Ok((new_token, "oauth-auto-refreshed".into()));
            }
            Err(_e) => {
                // Fall through — try the existing token anyway (it might still work briefly)
            }
        }
    }

    if let Some(ref token) = creds.access_token {
        if !token.is_empty() {
            return Ok((token.clone(), "oauth-auto".into()));
        }
    }
    Err("No Gemini access token found".into())
}

fn friendly_model_name(model_id: &str) -> String {
    match model_id {
        m if m.contains("3.1-pro") => "Gemini 3.1 Pro".into(),
        m if m.contains("3-pro") => "Gemini 3 Pro".into(),
        m if m.contains("2.5-pro") => "Gemini 2.5 Pro".into(),
        m if m.contains("3.1-flash-lite") => "Gemini 3.1 Flash Lite".into(),
        m if m.contains("2.5-flash-lite") => "Gemini 2.5 Flash Lite".into(),
        m if m.contains("3-flash") => "Gemini 3 Flash".into(),
        m if m.contains("2.5-flash") => "Gemini 2.5 Flash".into(),
        m if m.contains("pro") => format!("Gemini Pro ({})", model_id),
        m if m.contains("flash") => format!("Gemini Flash ({})", model_id),
        _ => model_id.to_string(),
    }
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
                        if earliest_ts.as_ref().is_none_or(|e| ts < e) {
                            earliest_ts = Some(ts.clone());
                        }
                        if latest_ts.as_ref().is_none_or(|l| ts > l) {
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
                fetched_at: std::time::Instant::now(),
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

fn fetch_gemini_analytics_uncached() -> ProviderAnalytics {
    let now = Utc::now().to_rfc3339();

    let (token, method) = match resolve_token() {
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

    // Fetch tier info FIRST — we need the project ID for the quota call
    let code_assist: Option<CodeAssistResponse> = http::authed_post(
        "https://cloudcode-pa.googleapis.com/v1internal:loadCodeAssist",
        &token,
        &serde_json::json!({"metadata": {"ideType": "GEMINI_CLI", "pluginType": "GEMINI"}}),
        None,
    ).ok();

    // Fetch quota — pass the project ID from loadCodeAssist for accurate per-user quotas
    let project_id = code_assist.as_ref()
        .and_then(|ca| ca.cloudai_companion_project.as_deref())
        .unwrap_or("");
    let quota_body = if project_id.is_empty() {
        serde_json::json!({})
    } else {
        serde_json::json!({"project": project_id})
    };
    let quota: Option<QuotaResponse> = http::authed_post(
        "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuota",
        &token,
        &quota_body,
        None,
    ).ok();

    // Group quota buckets by model TIER (Pro / Flash / Flash Lite).
    // Multiple model variants (e.g. gemini-2.5-pro, gemini-3-pro-preview) share the same
    // tier quota. Pick the LOWEST remainingFraction per tier (most constrained).
    // Matches CodexBar's approach of showing 3 bars: Pro, Flash, Flash Lite.
    let mut rate_limits = Vec::new();
    if let Some(ref q) = quota {
        if let Some(ref buckets) = q.buckets {
            // Map model_id to tier category
            let tier_of = |m: &str| -> &'static str {
                if m.contains("flash-lite") || m.contains("flash_lite") { "Flash Lite" }
                else if m.contains("flash") { "Flash" }
                else if m.contains("pro") { "Pro" }
                else { "Other" }
            };

            let mut tier_map: std::collections::HashMap<&str, (f64, Option<String>)> =
                std::collections::HashMap::new();
            for bucket in buckets {
                let model_id = bucket.model_id.as_deref().unwrap_or("unknown");
                let tier = tier_of(model_id);
                let remaining = bucket.remaining_fraction.unwrap_or(1.0);
                let entry = tier_map.entry(tier).or_insert((1.0, None));
                if remaining < entry.0 {
                    entry.0 = remaining;
                    // Use reset_time from the most constrained bucket, skip epoch 0
                    let valid_reset = bucket.reset_time.as_deref()
                        .filter(|t| !t.starts_with("1970"))
                        .map(String::from);
                    if valid_reset.is_some() {
                        entry.1 = valid_reset;
                    }
                }
                if entry.1.is_none() {
                    let valid_reset = bucket.reset_time.as_deref()
                        .filter(|t| !t.starts_with("1970"))
                        .map(String::from);
                    if valid_reset.is_some() {
                        entry.1 = valid_reset;
                    }
                }
            }

            // Display order: Pro, Flash, Flash Lite
            let tier_order = ["Pro", "Flash", "Flash Lite"];
            for tier in &tier_order {
                if let Some((remaining, reset_time)) = tier_map.get(tier) {
                    let used = ((1.0 - remaining) * 100.0).max(0.0);
                    rate_limits.push(RateLimitWindow {
                        provider_id: "gemini".into(),
                        label: tier.to_string(),
                        used_percent: used,
                        remaining_percent: (remaining * 100.0).min(100.0),
                        resets_at: reset_time.clone(),
                        resets_in_seconds: None,
                        window_seconds: Some(86400),
                    });
                }
            }
        }
    }

    let mut extra = HashMap::new();

    let plan_name = code_assist.as_ref().and_then(|ca| {
        ca.current_tier.as_ref().map(|t| {
            match t.id.as_deref() {
                Some("free-tier") => "Free".to_string(),
                Some("standard-tier") => "Paid".to_string(),
                Some("legacy-tier") => "Legacy".to_string(),
                _ => t.name.clone().unwrap_or_else(|| "Unknown".into()),
            }
        })
    });
    if let Some(ref ca) = code_assist {
        if let Some(ref proj) = ca.cloudai_companion_project {
            extra.insert("project_id".into(), serde_json::Value::String(proj.clone()));
        }
        if let Some(ref uri) = ca.upgrade_subscription_uri {
            extra.insert("upgrade_uri".into(), serde_json::Value::String(uri.clone()));
        }
    }

    // Extract email from id_token JWT (if available), with google_accounts.json fallback
    let email = {
        let creds = fs::read_to_string(creds_path()).ok()
            .and_then(|c| serde_json::from_str::<GeminiOAuthCreds>(&c).ok());
        let jwt_email = creds.and_then(|c| c.id_token).and_then(|jwt| {
            // Decode JWT payload (base64url, no verification needed — just reading claims)
            let parts: Vec<&str> = jwt.split('.').collect();
            if parts.len() < 2 { return None; }
            let payload = parts[1];
            // Add padding
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

        // Fallback: read active email from ~/.gemini/google_accounts.json
        jwt_email.or_else(read_fallback_email)
    };

    // Enrich with local session stats
    enrich_local_stats(&mut extra);

    ProviderAnalytics {
        provider_id: "gemini".into(),
        provider_name: "Gemini CLI".into(),
        status: ProviderStatus {
            provider_id: "gemini".into(),
            provider_name: "Gemini CLI".into(),
            connected: true,
            connection_method: method,
            account_email: email,
            plan_name,
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
        Ok((_, method)) => ProviderStatus {
            provider_id: "gemini".into(),
            provider_name: "Gemini CLI".into(),
            connected: true, connection_method: method,
            account_email: None, plan_name: None, org_name: None, error: None,
        },
        Err(e) => ProviderStatus {
            provider_id: "gemini".into(),
            provider_name: "Gemini CLI".into(),
            connected: false, connection_method: "none".into(),
            account_email: None, plan_name: None, org_name: None, error: Some(e),
        },
    }
}
