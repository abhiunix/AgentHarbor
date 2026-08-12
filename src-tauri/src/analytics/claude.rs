//! Claude Code analytics provider.
//! Auth: 3-tier — silent auto-detect → user sign-in → keychain import
//! API: api.anthropic.com/api/oauth/usage + /api/oauth/profile + /api/oauth/account

use crate::analytics::claude_account;
use crate::analytics::http;
use crate::analytics::http::HttpCallError;
use crate::analytics::token_store;
use crate::analytics::types::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

// ── In-memory cache (300s TTL, shorter when limits are tight) ───────────────

struct ClaudeCacheEntry {
    data: ProviderAnalytics,
    fetched_at: std::time::Instant,
    /// Fingerprint of the credential the snapshot was fetched with — a
    /// mismatch means the user switched accounts and the entry is dead.
    cred_fp: u64,
}

struct AccountSnapshot {
    raw: serde_json::Value,
    fetched_at: std::time::Instant,
}

lazy_static::lazy_static! {
    static ref CLAUDE_CACHE: Mutex<Option<ClaudeCacheEntry>> = Mutex::new(None);
    /// Single-flight guard: without it, the tray cycle, analytics page, and
    /// popover can all miss the cache in the same instant and fire concurrent
    /// /usage calls — tripping Anthropic's burst limiter on our own traffic.
    static ref CLAUDE_FETCH_FLIGHT: Mutex<()> = Mutex::new(());
    /// Last successful `/api/oauth/account` payload — reused as a fallback
    /// when the endpoint starts 429-ing (Anthropic does this aggressively
    /// once a token is `out_of_credits`). Without it we'd lose the friendlier
    /// "monthly usage limit reached" state and just show "rate limited".
    static ref LAST_ACCOUNT_CACHE: Mutex<Option<AccountSnapshot>> = Mutex::new(None);
}

const CLAUDE_CACHE_TTL_SECS: u64 = 60;
const CLAUDE_CACHE_TTL_SHORT_SECS: u64 = 60; // when approaching / reached limits
/// Never serve Claude API-backed analytics older than this — limits how long
/// we show a "healthy" snapshot after OAuth tokens are rotated or expired.
const CLAUDE_CACHE_MAX_STALE_SECS: u64 = 90;
const ACCOUNT_FALLBACK_TTL_SECS: u64 = 3600; // hold last-good account up to 1h

use std::fs;
use std::path::PathBuf;

// ── Credential types ────────────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ClaudeCredentials {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_at: Option<u64>,
    scopes: Option<Vec<String>>,
    subscription_type: Option<String>,
    rate_limit_tier: Option<String>,
}

// ── API response types ──────────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
struct UsageWindow {
    utilization: Option<f64>,
    resets_at: Option<String>,
}

#[derive(Deserialize, Debug)]
struct ApiLimitScopeName {
    display_name: Option<String>,
}

#[derive(Deserialize, Debug)]
struct ApiLimitScope {
    model: Option<ApiLimitScopeName>,
    surface: Option<ApiLimitScopeName>,
}

#[derive(Deserialize, Debug)]
struct ApiLimitEntry {
    kind: Option<String>,
    percent: Option<f64>,
    resets_at: Option<String>,
    scope: Option<ApiLimitScope>,
}

#[derive(Deserialize, Debug)]
struct ExtraUsage {
    is_enabled: Option<bool>,
    monthly_limit: Option<f64>,
    used_credits: Option<f64>,
    utilization: Option<f64>,
    currency: Option<String>,
    #[serde(default)]
    disabled_reason: Option<String>,
    #[serde(default)]
    credits_ever_enabled: Option<bool>,
}

#[derive(Deserialize, Debug)]
struct ClaudeUsageResponse {
    five_hour: Option<UsageWindow>,
    seven_day: Option<UsageWindow>,
    seven_day_oauth_apps: Option<UsageWindow>,
    seven_day_opus: Option<UsageWindow>,
    seven_day_sonnet: Option<UsageWindow>,
    seven_day_cowork: Option<UsageWindow>,
    #[serde(default)]
    seven_day_omelette: Option<UsageWindow>,
    #[serde(default)]
    tangelo: Option<UsageWindow>,
    #[serde(default)]
    iguana_necktie: Option<UsageWindow>,
    #[serde(default)]
    omelette_promotional: Option<UsageWindow>,
    /// Modern generic shape: per-model windows appear here as `weekly_scoped`
    /// entries while the named `seven_day_*` fields above are null.
    #[serde(default)]
    limits: Option<Vec<ApiLimitEntry>>,
    extra_usage: Option<ExtraUsage>,
}

#[derive(Deserialize, Debug)]
struct ProfileAccount {
    uuid: Option<String>,
    full_name: Option<String>,
    display_name: Option<String>,
    email: Option<String>,
    has_claude_max: Option<bool>,
    has_claude_pro: Option<bool>,
    created_at: Option<String>,
}

#[derive(Deserialize, Debug)]
struct ProfileOrg {
    uuid: Option<String>,
    name: Option<String>,
    organization_type: Option<String>,
    seat_tier: Option<String>,
    billing_type: Option<String>,
    rate_limit_tier: Option<String>,
    has_extra_usage_enabled: Option<bool>,
    subscription_status: Option<String>,
    subscription_created_at: Option<String>,
    #[serde(default)]
    claude_code_trial_ends_at: Option<String>,
    #[serde(default)]
    claude_code_trial_duration_days: Option<i64>,
}

#[derive(Deserialize, Debug)]
struct ProfileApp {
    name: Option<String>,
    slug: Option<String>,
}

#[derive(Deserialize, Debug)]
struct ClaudeProfileResponse {
    account: Option<ProfileAccount>,
    organization: Option<ProfileOrg>,
    application: Option<ProfileApp>,
}

// ── Auth Result Types (for Tauri commands) ──────────────────────────────────

#[derive(Serialize, Clone, Debug)]
pub struct SilentCheckResult {
    pub found: bool,
    pub method: String, // "stored", "credentials-file", "none"
}

#[derive(Serialize, Clone, Debug)]
pub struct KeychainImportResult {
    pub success: bool,
    pub method: String,
    pub error: Option<String>,
}

// ── Helpers: File-based credentials ─────────────────────────────────────────

fn credentials_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".claude")
        .join(".credentials.json")
}

fn read_auto_credentials() -> Result<ClaudeCredentials, String> {
    let path = credentials_path();
    if !path.exists() {
        return Err("Claude credentials file not found".into());
    }
    let content = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read credentials: {}", e))?;
    serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse credentials: {}", e))
}

// ── Helpers: Keychain credentials ───────────────────────────────────────────

/// Read OAuth token from Claude Code's native keychain entry.
/// WARNING: This WILL trigger a macOS password/Touch ID prompt!
/// Returns (access_token, Option<refresh_token>) from the macOS Keychain.
fn read_keychain_credentials() -> Result<(String, Option<String>), String> {
    let usernames = vec![
        whoami::username(),
        "default".to_string(),
    ];

    for username in &usernames {
        if let Ok(entry) = keyring::Entry::new("Claude Code-credentials", username) {
            if let Ok(secret) = entry.get_password() {
                if let Some(tokens) = extract_tokens_from_keychain_json(&secret) {
                    return Ok(tokens);
                }
            }
        }
    }

    Err("No Claude Code credentials found in keychain".into())
}

/// Extract access token + optional refresh token from keychain JSON.
/// Returns (access_token, Option<refresh_token>).
fn extract_tokens_from_keychain_json(secret: &str) -> Option<(String, Option<String>)> {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(secret) {
        // {"claudeAiOauth":{"accessToken":"...","refreshToken":"..."}}
        if let Some(oauth_obj) = json.get("claudeAiOauth") {
            if let Some(token) = oauth_obj.get("accessToken").and_then(|v| v.as_str()) {
                if !token.is_empty() {
                    let refresh = oauth_obj.get("refreshToken")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .map(String::from);
                    return Some((token.to_string(), refresh));
                }
            }
        }
        // {"accessToken":"...","refreshToken":"..."}
        if let Some(token) = json.get("accessToken").and_then(|v| v.as_str()) {
            if !token.is_empty() {
                let refresh = json.get("refreshToken")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(String::from);
                return Some((token.to_string(), refresh));
            }
        }
        // {"access_token":"...","refresh_token":"..."}
        if let Some(token) = json.get("access_token").and_then(|v| v.as_str()) {
            if !token.is_empty() {
                let refresh = json.get("refresh_token")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(String::from);
                return Some((token.to_string(), refresh));
            }
        }
    }
    let trimmed = secret.trim();
    if trimmed.starts_with("sk-ant-") || trimmed.starts_with("ey") {
        return Some((trimmed.to_string(), None));
    }
    None
}

// ── OAuth PKCE Flow ─────────────────────────────────────────────────────────

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use sha2::{Digest, Sha256};

const OAUTH_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const OAUTH_AUTHORIZE_URL: &str = "https://claude.ai/oauth/authorize";
const OAUTH_TOKEN_URL: &str = "https://api.anthropic.com/v1/oauth/token";
const OAUTH_REDIRECT_URI: &str = "https://console.anthropic.com/oauth/code/callback";
const OAUTH_SCOPES: &str = "org:create_api_key user:profile user:inference";

/// In-memory PKCE state for the current OAuth flow
static OAUTH_STATE: std::sync::Mutex<Option<(String, String)>> = std::sync::Mutex::new(None);
// (code_verifier, state_nonce)

fn generate_pkce() -> (String, String) {
    // Generate 32 random bytes for code_verifier
    let mut verifier_bytes = [0u8; 32];
    getrandom::fill(&mut verifier_bytes).expect("Failed to generate random bytes");
    let code_verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);

    // code_challenge = BASE64URL(SHA256(code_verifier))
    let mut hasher = Sha256::new();
    hasher.update(code_verifier.as_bytes());
    let hash = hasher.finalize();
    let code_challenge = URL_SAFE_NO_PAD.encode(hash);

    (code_verifier, code_challenge)
}

fn generate_state() -> String {
    let mut state_bytes = [0u8; 32];
    getrandom::fill(&mut state_bytes).expect("Failed to generate random bytes");
    URL_SAFE_NO_PAD.encode(state_bytes)
}

// ── Token Resolution (silent only — NO keychain) ───────────────────────────

/// Try to refresh an expired access token using the stored refresh token.
/// Returns the new access token on success.
fn try_refresh_access_token() -> Option<String> {
    let refresh_token = token_store::get_provider_token("claude-code", "refresh-token")
        .ok()
        .flatten()
        .filter(|s| !s.is_empty())?;

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;

    let body = serde_json::json!({
        "grant_type": "refresh_token",
        "client_id": OAUTH_CLIENT_ID,
        "refresh_token": refresh_token,
    });

    let resp = client
        .post(OAUTH_TOKEN_URL)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let json: serde_json::Value = resp.json().ok()?;
    let new_token = json.get("access_token")?.as_str()?;
    if new_token.is_empty() {
        return None;
    }

    // Save the refreshed token
    let _ = token_store::store_provider_token("claude-code", "access-token", new_token);
    // Update refresh token if a new one was issued
    if let Some(new_rt) = json.get("refresh_token").and_then(|v| v.as_str()) {
        if !new_rt.is_empty() {
            let _ = token_store::store_provider_token("claude-code", "refresh-token", new_rt);
        }
    }

    Some(new_token.to_string())
}

/// Resolve token WITHOUT touching keychain. Used by analytics fetchers.
/// **Order matters for parity with the Claude Code CLI:** the terminal tool
/// reads `~/.claude/.credentials.json` first. If we preferred AgentHarbor's
/// vault, a still-valid in-app token could make `/oauth/*` succeed while the
/// CLI shows `401 Invalid authentication credentials` for the on-disk token.
/// So: **credentials file → app vault → refresh.**
/// Cheap, side-effect-free fingerprint of whichever credential would be used
/// right now (credentials file first, then the stored token — same order as
/// `resolve_access_token_silent`, minus writes and refresh). Changes on
/// logout/login with a different account, so caches can invalidate
/// immediately instead of serving the previous account's snapshot.
pub fn current_credential_fingerprint() -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    if let Ok(creds) = read_auto_credentials() {
        if let Some(t) = creds.access_token.filter(|t| !t.is_empty()) {
            t.hash(&mut h);
            return h.finish();
        }
    }
    if let Ok(Some(t)) = token_store::get_provider_token("claude-code", "access-token") {
        t.hash(&mut h);
    }
    h.finish()
}

fn resolve_access_token_silent() -> Result<(String, String), String> {
    // 1. ~/.claude/.credentials.json — same file the CLI uses
    if let Ok(creds) = read_auto_credentials() {
        if let Some(token) = creds.access_token {
            if !token.is_empty() {
                let _ = token_store::store_provider_token("claude-code", "access-token", &token);
                if let Some(ref rt) = creds.refresh_token {
                    if !rt.is_empty() {
                        let _ = token_store::store_provider_token("claude-code", "refresh-token", rt);
                    }
                }
                return Ok((token, "credentials-file".into()));
            }
        }
    }

    // 2. AgentHarbor's stored token (in-app sign-in / keychain import) when
    //    there is no usable token in the Claude credentials file.
    if let Ok(Some(token)) = token_store::get_provider_token("claude-code", "access-token") {
        if !token.is_empty() {
            return Ok((token, "stored".into()));
        }
    }

    // 3. Try refreshing with stored refresh token
    if let Some(token) = try_refresh_access_token() {
        return Ok((token, "refreshed".into()));
    }

    Err("No Claude credentials found silently. Use Sign In or Import from Keychain.".into())
}

// ── Tauri Commands: Auth Flow ───────────────────────────────────────────────

/// Tier 0: Silent credential check — no keychain, no prompts, no API calls.
/// Returns whether credentials were found and by which method.
#[tauri::command]
pub fn claude_check_silent_credentials() -> SilentCheckResult {
    match resolve_access_token_silent() {
        Ok((_, method)) => SilentCheckResult { found: true, method },
        Err(_) => SilentCheckResult { found: false, method: "none".into() },
    }
}

/// Option B: Import token from macOS Keychain.
/// WILL trigger system password/Touch ID prompt.
/// Only called after user explicitly consents via UI.
#[tauri::command]
pub fn claude_import_from_keychain() -> KeychainImportResult {
    match read_keychain_credentials() {
        Ok((access_token, refresh_token)) => {
            // Save access token to AgentHarbor's store
            if let Err(e) = token_store::store_provider_token("claude-code", "access-token", &access_token) {
                return KeychainImportResult {
                    success: false,
                    method: "keychain".into(),
                    error: Some(format!("Token found but failed to save: {}", e)),
                };
            }
            // Also save the refresh token if present — needed for profile/account API calls
            if let Some(ref rt) = refresh_token {
                let _ = token_store::store_provider_token("claude-code", "refresh-token", rt);
            }
            KeychainImportResult { success: true, method: "keychain".into(), error: None }
        }
        Err(e) => KeychainImportResult {
            success: false,
            method: "keychain".into(),
            error: Some(e),
        },
    }
}

/// Option A: Store a token provided by the user (after browser sign-in).
#[tauri::command]
pub fn claude_store_manual_token(token: String) -> Result<String, String> {
    if token.trim().is_empty() {
        return Err("Token cannot be empty".into());
    }

    // Quick validation: try fetching profile
    let extra_headers = http::headers(&[("anthropic-beta", "oauth-2025-04-20")]);
    let _profile: serde_json::Value = http::authed_get(
        "https://api.anthropic.com/api/oauth/profile",
        token.trim(),
        Some(extra_headers),
    ).map_err(|e| format!("Token validation failed: {}", e))?;

    // Token is valid — save it
    token_store::store_provider_token("claude-code", "access-token", token.trim())
        .map_err(|e| format!("Failed to save token: {}", e))?;

    Ok("Token saved successfully".into())
}

/// Disconnect: remove stored token.
#[tauri::command]
pub fn claude_disconnect() -> Result<(), String> {
    let _ = token_store::delete_provider_token("claude-code", "access-token");
    Ok(())
}

/// Start OAuth PKCE flow: generate URL for the user to open in browser.
#[tauri::command]
pub fn claude_start_oauth() -> Result<String, String> {
    let (code_verifier, code_challenge) = generate_pkce();
    let state = generate_state();

    // Store PKCE state for the exchange step
    if let Ok(mut guard) = OAUTH_STATE.lock() {
        *guard = Some((code_verifier, state.clone()));
    }

    let url = format!(
        "{}?code=true&client_id={}&response_type=code&redirect_uri={}&scope={}&code_challenge={}&code_challenge_method=S256&state={}",
        OAUTH_AUTHORIZE_URL,
        OAUTH_CLIENT_ID,
        urlencoding::encode(OAUTH_REDIRECT_URI),
        urlencoding::encode(OAUTH_SCOPES),
        code_challenge,
        state,
    );

    Ok(url)
}

/// Complete OAuth flow: exchange the auth code for an access token.
/// The user pastes the full code from the callback page (code#state format or just code).
#[tauri::command]
pub fn claude_exchange_oauth_code(auth_code: String) -> Result<String, String> {
    let auth_code = auth_code.trim().to_string();
    if auth_code.is_empty() {
        return Err("Auth code cannot be empty".into());
    }

    // Retrieve stored PKCE verifier and state
    let (code_verifier, _expected_state) = {
        let guard = OAUTH_STATE.lock().map_err(|e| format!("Lock error: {}", e))?;
        guard.clone().ok_or("No OAuth flow in progress. Click 'Sign In' first.")?
    };

    // The callback page shows "code#state" — split if present
    let code = if auth_code.contains('#') {
        auth_code.split('#').next().unwrap_or(&auth_code).to_string()
    } else {
        auth_code
    };

    // Retrieve stored state for the exchange
    let stored_state = {
        let guard = OAUTH_STATE.lock().map_err(|e| format!("Lock error: {}", e))?;
        guard.as_ref().map(|(_, s)| s.clone()).unwrap_or_default()
    };

    // Exchange code for access token via api.anthropic.com (no Cloudflare)
    // NOTE: anthropic-beta header must NOT be included — it causes "Invalid request format"
    // NOTE: state param IS required — without it the endpoint rejects the request
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let body = serde_json::json!({
        "grant_type": "authorization_code",
        "client_id": OAUTH_CLIENT_ID,
        "code": code,
        "redirect_uri": OAUTH_REDIRECT_URI,
        "code_verifier": code_verifier,
        "state": stored_state,
    });

    let resp = client
        .post(OAUTH_TOKEN_URL)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .map_err(|e| format!("Token exchange request failed: {}", e))?;

    let status = resp.status();
    let body = resp.text().map_err(|e| format!("Failed to read response: {}", e))?;

    if !status.is_success() {
        return Err(format!("Token exchange failed ({}): {}", status, body));
    }

    let json: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| format!("Failed to parse token response: {}", e))?;

    let access_token = json.get("access_token")
        .and_then(|v| v.as_str())
        .ok_or("No access_token in response")?;

    let refresh_token = json.get("refresh_token")
        .and_then(|v| v.as_str());

    // Save tokens to our store
    token_store::store_provider_token("claude-code", "access-token", access_token)
        .map_err(|e| format!("Failed to save access token: {}", e))?;

    if let Some(rt) = refresh_token {
        let _ = token_store::store_provider_token("claude-code", "refresh-token", rt);
    }

    // Clear PKCE state
    if let Ok(mut guard) = OAUTH_STATE.lock() {
        *guard = None;
    }

    Ok("Sign in successful".into())
}

// ── Plan Classification ─────────────────────────────────────────────────────

/// Canonical plan tier derived from `/api/oauth/profile`.
/// `Enterprise` accounts have no 5h/7d windows — usage is reported via
/// `extra_usage` as a $-denominated monthly ledger (cap may be null).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlanTier {
    Free,
    Pro,
    Max,
    Team,
    Enterprise,
    Unknown,
}

impl PlanTier {
    fn label(self) -> Option<&'static str> {
        match self {
            PlanTier::Free => Some("Free"),
            PlanTier::Pro => Some("Pro"),
            PlanTier::Max => Some("Max"),
            PlanTier::Team => Some("Team"),
            PlanTier::Enterprise => Some("Enterprise"),
            PlanTier::Unknown => None,
        }
    }

    fn is_enterprise(self) -> bool {
        matches!(self, PlanTier::Enterprise)
    }
}

/// Classify the plan using `organization_type` as the primary signal,
/// `seat_tier` and `rate_limit_tier` as confirmations/fallbacks.
/// Designed to keep working for legacy (Pro/Max) accounts even after
/// the Enterprise plan rolled out.
fn classify_plan(org: Option<&ProfileOrg>) -> PlanTier {
    let org = match org {
        Some(o) => o,
        None => return PlanTier::Unknown,
    };

    // Primary signal: organization_type
    if let Some(t) = org.organization_type.as_deref() {
        let t = t.to_ascii_lowercase();
        if t.contains("enterprise") {
            return PlanTier::Enterprise;
        }
        if t.contains("team") {
            return PlanTier::Team;
        }
        if t.contains("max") {
            return PlanTier::Max;
        }
        if t.contains("pro") {
            return PlanTier::Pro;
        }
        if t.contains("free") {
            return PlanTier::Free;
        }
    }

    // Confirmation: seat_tier
    if let Some(s) = org.seat_tier.as_deref() {
        let s = s.to_ascii_lowercase();
        if s.contains("enterprise") {
            return PlanTier::Enterprise;
        }
    }

    // Fallback: rate_limit_tier (legacy heuristic)
    if let Some(t) = org.rate_limit_tier.as_deref() {
        let t = t.to_ascii_lowercase();
        if t.contains("max") {
            return PlanTier::Max;
        }
        if t.contains("pro") {
            return PlanTier::Pro;
        }
    }

    PlanTier::Unknown
}

// ── Rate Limit Parsing ──────────────────────────────────────────────────────

fn parse_window(
    window: &Option<UsageWindow>,
    label: &str,
    window_seconds: Option<i64>,
) -> Option<RateLimitWindow> {
    let w = window.as_ref()?;
    let utilization = w.utilization?;
    Some(RateLimitWindow {
        provider_id: "claude-code".into(),
        label: label.into(),
        used_percent: utilization,
        remaining_percent: (100.0 - utilization).max(0.0),
        resets_at: w.resets_at.clone(),
        resets_in_seconds: None,
        window_seconds,
    })
}

/// Build rate-limit windows from a usage response. Prefers the modern
/// `limits[]` array (which carries per-model `weekly_scoped` windows, e.g.
/// "Fable (7d)"); falls back to the legacy named fields when absent.
fn build_rate_limits(u: &ClaudeUsageResponse, show_internal: bool) -> Vec<RateLimitWindow> {
    let mut rate_limits = Vec::new();

    if let Some(ref entries) = u.limits {
        for e in entries {
            let Some(pct) = e.percent else { continue };
            let (label, window_seconds) = match e.kind.as_deref() {
                Some("session") => ("Session (5h)".to_string(), Some(18000)),
                Some("weekly_all") => ("Weekly (All)".to_string(), Some(604800)),
                Some("weekly_scoped") => {
                    let name = e
                        .scope
                        .as_ref()
                        .and_then(|s| s.model.as_ref().or(s.surface.as_ref()))
                        .and_then(|n| n.display_name.clone())
                        .unwrap_or_else(|| "Model".to_string());
                    (format!("{name} (7d)"), Some(604800))
                }
                // Unreleased experiment kinds — ignore rather than chase.
                _ => continue,
            };
            rate_limits.push(RateLimitWindow {
                provider_id: "claude-code".into(),
                label,
                used_percent: pct,
                remaining_percent: (100.0 - pct).max(0.0),
                resets_at: e.resets_at.clone(),
                resets_in_seconds: None,
                window_seconds,
            });
        }
    }

    if rate_limits.is_empty() {
        if let Some(w) = parse_window(&u.five_hour, "Session (5h)", Some(18000)) {
            rate_limits.push(w);
        }
        if let Some(w) = parse_window(&u.seven_day, "Weekly (All)", Some(604800)) {
            rate_limits.push(w);
        }
        if let Some(w) = parse_window(&u.seven_day_opus, "Opus (7d)", Some(604800)) {
            rate_limits.push(w);
        }
        if let Some(w) = parse_window(&u.seven_day_sonnet, "Sonnet (7d)", Some(604800)) {
            rate_limits.push(w);
        }
        if let Some(w) = parse_window(&u.seven_day_oauth_apps, "OAuth Apps (7d)", Some(604800)) {
            rate_limits.push(w);
        }
        if let Some(w) = parse_window(&u.seven_day_cowork, "Cowork (7d)", Some(604800)) {
            rate_limits.push(w);
        }
        if show_internal {
            if let Some(w) = parse_window(&u.seven_day_omelette, "Omelette (7d)", Some(604800)) {
                rate_limits.push(w);
            }
            if let Some(w) = parse_window(&u.tangelo, "Tangelo", Some(604800)) {
                rate_limits.push(w);
            }
            if let Some(w) = parse_window(&u.iguana_necktie, "Iguana Necktie", Some(604800)) {
                rate_limits.push(w);
            }
            if let Some(w) = parse_window(&u.omelette_promotional, "Promotional", Some(18000)) {
                rate_limits.push(w);
            }
        }
    }

    rate_limits
}

// ── Limit state derivation ─────────────────────────────────────────────────

fn label_to_scope(label: &str) -> LimitScope {
    let l = label.to_lowercase();
    if l.contains("session") || l.contains("5h") {
        return LimitScope::Session5h;
    }
    if l.contains("opus") {
        return LimitScope::WeeklyOpus;
    }
    if l.contains("sonnet") {
        return LimitScope::WeeklySonnet;
    }
    if l.contains("oauth apps") {
        return LimitScope::WeeklyOauthApps;
    }
    if l.contains("cowork") {
        return LimitScope::WeeklyCowork;
    }
    if l.contains("weekly") || l.contains("7d") {
        return LimitScope::WeeklyAll;
    }
    LimitScope::Custom(label.to_string())
}

fn iso_in_future(s: &str) -> bool {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc) > Utc::now())
        .unwrap_or(false)
}

fn subscription_is_healthy(status: Option<&str>) -> bool {
    match status.map(|s| s.to_lowercase()).as_deref() {
        None | Some("active") | Some("trialing") => true,
        _ => false,
    }
}

/// Derive limit / billing health for tray + notifications.
fn derive_claude_limit_state(
    usage: &Option<ClaudeUsageResponse>,
    profile: &Option<ClaudeProfileResponse>,
    account: &Option<claude_account::AccountResponse>,
    rate_limits: &[RateLimitWindow],
    oauth_rate_limited: Option<(Option<u64>, String)>,
    oauth_unauthorized: Option<String>,
    plan_tier: PlanTier,
) -> LimitState {
    // If every endpoint that returned an error did so with 401 and we have
    // no usable usage / profile / account data, the stored OAuth token is
    // invalid (the upstream tool likely rotated its credentials). Surface a
    // dedicated "Reconnect" state instead of mislabeling the cascading 429
    // on /usage as plain rate-limiting.
    // `/api/oauth/account` may still deserialize from LAST_ACCOUNT_CACHE after
    // real endpoints return 401 — do not require `account.is_none()` here.
    if let Some(body) = oauth_unauthorized.as_ref() {
        let no_live_usage_or_profile = usage.is_none() && profile.is_none();
        if no_live_usage_or_profile {
            let friendly = serde_json::from_str::<serde_json::Value>(body)
                .ok()
                .and_then(|v| {
                    v.get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|m| m.as_str())
                        .map(String::from)
                })
                .unwrap_or_else(|| {
                    "Stored Claude credentials are no longer valid.".to_string()
                });
            return LimitState::Unauthenticated { message: friendly };
        }
    }

    let org = profile.as_ref().and_then(|p| p.organization.as_ref());
    let org_uuid = org.and_then(|o| o.uuid.as_deref());
    let org_display = org
        .and_then(|o| o.name.clone())
        .unwrap_or_else(|| "Organization".to_string());

    // Anthropic occasionally leaves `api_disabled_reason: "out_of_credits"`
    // set on a sibling/personal membership row even after the active org
    // has been allocated headroom (e.g. an Enterprise admin sets a per-user
    // cap). Cross-check against the live `extra_usage` data — if the user
    // is clearly under their cap, the flag is stale and we should ignore it.
    // Other api_disabled reasons (trial_expired, payment_failed, etc.) are
    // unrelated to credits and remain authoritative.
    let usage_clearly_within_cap = usage
        .as_ref()
        .and_then(|u| u.extra_usage.as_ref())
        .map(|eu| {
            if let Some(limit) = eu.monthly_limit.filter(|&l| l > 0.0) {
                let used = eu.used_credits.unwrap_or(0.0);
                used < limit * 0.99
            } else if let Some(util) = eu.utilization {
                let pct = if util > 1.0 { util } else { util * 100.0 };
                pct < 99.0
            } else {
                false
            }
        })
        .unwrap_or(false);

    if let Some(acc) = account {
        // Try matched org → parent org → any blocked membership (Enterprise
        // tokens often expose api_disabled_reason on the parent / sibling
        // membership row, not the active profile org).
        let matched = claude_account::org_for_profile_uuid(acc, org_uuid);
        let parent = matched.and_then(|m| claude_account::parent_org(acc, m));
        let blocked = claude_account::first_blocked_org(acc);
        let candidates: Vec<&claude_account::AccountOrg> = matched
            .into_iter()
            .chain(parent.into_iter())
            .chain(blocked.into_iter())
            .collect();

        // Use the active profile org name in the user-facing message — even
        // when the block flag lives on a sibling/parent membership row — so
        // it matches what the user sees in the analytics header.
        for cand in &candidates {
            if let Some(ref reason) = cand.api_disabled_reason {
                if reason.is_empty() {
                    continue;
                }
                // "out_of_credits" is only meaningful for Enterprise plans that
                // have a monthly spend cap. Pro/Max/Team users get this flag set
                // on sibling membership rows as a billing artefact, but they have
                // no dollar cap — their limits are time-windowed rate limits, not
                // credits. Suppress it for non-Enterprise plans entirely.
                if reason.eq_ignore_ascii_case("out_of_credits")
                    && (!plan_tier.is_enterprise() || usage_clearly_within_cap)
                {
                    continue;
                }
                return LimitState::ApiDisabled {
                    reason: reason.clone(),
                    until: cand.api_disabled_until.clone(),
                    org_name: org_display.clone(),
                };
            }
        }
        for cand in &candidates {
            if let Some(ref pu) = cand.billable_usage_paused_until {
                if iso_in_future(pu) {
                    return LimitState::BillablePaused {
                        until: pu.clone(),
                        org_name: org_display.clone(),
                    };
                }
            }
        }
    }

    if let Some(o) = org {
        if let Some(ref st) = o.subscription_status {
            // `subscription_status` tracks the web (Stripe) subscription only.
            // Store-billed plans (Google Play / App Store) report "canceled"
            // here even while the plan is active and paid, so the field is
            // not authoritative for them.
            let store_billed = o.billing_type.as_deref().is_some_and(|b| {
                let b = b.to_ascii_lowercase();
                b.contains("google_play") || b.contains("app_store") || b.contains("apple")
            });
            if !store_billed && !subscription_is_healthy(Some(st.as_str())) {
                return LimitState::SubscriptionIssue {
                    status: st.clone(),
                    org_name: o.name.clone().unwrap_or_else(|| org_display.clone()),
                };
            }
        }
    }

    if let Some((retry, msg)) = oauth_rate_limited {
        return LimitState::RateLimited {
            retry_after_secs: retry,
            message: msg,
        };
    }

    for rl in rate_limits {
        if rl.used_percent >= 100.0 - 1e-6 {
            return LimitState::Reached {
                scope: label_to_scope(&rl.label),
                used_pct: rl.used_percent.min(100.0),
                cap: None,
                resets_at: rl.resets_at.clone(),
            };
        }
    }

    if let Some(ref u) = usage {
        if let Some(ref eu) = u.extra_usage {
            if let Some(limit_cents) = eu.monthly_limit {
                let limit_dollars = limit_cents / 100.0;
                let used_dollars = eu.used_credits.unwrap_or(0.0) / 100.0;
                if limit_dollars > 0.0 && used_dollars + 1e-6 >= limit_dollars {
                    return LimitState::Reached {
                        scope: LimitScope::MonthlySpend,
                        used_pct: ((used_dollars / limit_dollars) * 100.0).min(100.0),
                        cap: Some(limit_dollars),
                        resets_at: None,
                    };
                }
            }
            // Enterprise plans report `monthly_limit: null` ("no cap") even
            // when the org enforces an invisible spend ceiling and the API
            // rejects calls. The server still surfaces utilization.
            if let Some(util) = eu.utilization {
                let pct = if util > 1.0 { util } else { util * 100.0 };
                if pct >= 100.0 - 1e-6 {
                    return LimitState::Reached {
                        scope: LimitScope::MonthlySpend,
                        used_pct: pct.min(100.0),
                        cap: None,
                        resets_at: None,
                    };
                }
            }
        }
    }

    let mut worst: Option<&RateLimitWindow> = None;
    for rl in rate_limits {
        if rl.used_percent >= 80.0 && rl.used_percent < 100.0 {
            worst = Some(match worst {
                None => rl,
                Some(w) => {
                    if rl.used_percent > w.used_percent {
                        rl
                    } else {
                        w
                    }
                }
            });
        }
    }
    if let Some(w) = worst {
        return LimitState::Approaching {
            worst_pct: w.used_percent,
            label: w.label.clone(),
            resets_at: w.resets_at.clone(),
            scope: label_to_scope(&w.label),
        };
    }

    LimitState::Healthy
}

// ── Public API (uses silent resolution only) ────────────────────────────────

/// Fetch Claude Code analytics (rate limits + profile).
/// Uses silent token resolution — never triggers keychain prompt.
/// Results are cached for 300s to avoid repeated API calls (60s when limits
/// are tight, so the user sees recovery faster after reset).
pub fn fetch_claude_analytics() -> ProviderAnalytics {
    let cred_fp = current_credential_fingerprint();
    let mut switched_account = false;
    if let Ok(guard) = CLAUDE_CACHE.lock() {
        if let Some(ref entry) = *guard {
            if entry.cred_fp == cred_fp {
                let ttl_secs = if entry
                    .data
                    .limit_state
                    .as_ref()
                    .map(|s| s.prefers_fast_refresh())
                    .unwrap_or(false)
                {
                    CLAUDE_CACHE_TTL_SHORT_SECS
                } else {
                    CLAUDE_CACHE_TTL_SECS
                };
                let effective_ttl = ttl_secs.min(CLAUDE_CACHE_MAX_STALE_SECS);
                if entry.fetched_at.elapsed().as_secs() < effective_ttl {
                    return entry.data.clone();
                }
            } else {
                switched_account = true;
            }
        }
    }
    // The previous account's payload must not bleed into the new account's
    // limit-state derivation.
    if switched_account {
        if let Ok(mut guard) = LAST_ACCOUNT_CACHE.lock() {
            *guard = None;
        }
    }

    // Single-flight: concurrent callers wait here, then re-check the cache —
    // whoever fetched first has usually just filled it.
    let _flight = CLAUDE_FETCH_FLIGHT.lock();
    if let Ok(guard) = CLAUDE_CACHE.lock() {
        if let Some(ref entry) = *guard {
            if entry.cred_fp == cred_fp {
                let ttl_secs = if entry
                    .data
                    .limit_state
                    .as_ref()
                    .map(|s| s.prefers_fast_refresh())
                    .unwrap_or(false)
                {
                    CLAUDE_CACHE_TTL_SHORT_SECS
                } else {
                    CLAUDE_CACHE_TTL_SECS
                };
                if entry.fetched_at.elapsed().as_secs() < ttl_secs.min(CLAUDE_CACHE_MAX_STALE_SECS) {
                    return entry.data.clone();
                }
            }
        }
    }

    let mut result = fetch_claude_analytics_uncached();

    // A rate-limited cycle produces empty windows; caching it would blank the
    // popover bars for a full TTL. Graft the last good windows and keep the
    // previous cache entry so the next cycle retries (same pattern as Codex).
    let rate_limited = matches!(result.limit_state, Some(LimitState::RateLimited { .. }));
    if rate_limited && result.rate_limits.is_empty() {
        if let Ok(guard) = CLAUDE_CACHE.lock() {
            if let Some(ref prev) = *guard {
                if prev.cred_fp == cred_fp && !prev.data.rate_limits.is_empty() {
                    result.rate_limits = prev.data.rate_limits.clone();
                }
            }
        }
        return result;
    }

    // Cache successful results
    if result.status.connected {
        if let Ok(mut guard) = CLAUDE_CACHE.lock() {
            *guard = Some(ClaudeCacheEntry {
                data: result.clone(),
                fetched_at: std::time::Instant::now(),
                cred_fp,
            });
        }
    }

    result
}

fn fetch_claude_analytics_uncached() -> ProviderAnalytics {
    let now = Utc::now().to_rfc3339();

    let (token, method) = match resolve_access_token_silent() {
        Ok(t) => t,
        Err(e) => {
            return ProviderAnalytics {
                provider_id: "claude-code".into(),
                provider_name: "Claude Code".into(),
                status: ProviderStatus {
                    provider_id: "claude-code".into(),
                    provider_name: "Claude Code".into(),
                    connected: false,
                    connection_method: "none".into(),
                    account_email: None,
                    plan_name: None,
                    org_name: None,
                    error: Some(e),
                },
                rate_limits: vec![],
                credit_usage: None,
                token_counts: None,
                limit_state: None,
                extra: HashMap::new(),
                fetched_at: now,
            };
        }
    };

    let extra_headers = http::headers(&[("anthropic-beta", "oauth-2025-04-20")]);

    // Fetch usage + profile with the current token. Capture both 429
    // (rate-limited) and 401 (invalid creds) so derive_claude_limit_state
    // can surface a "Reconnect" banner instead of mislabeling 401-driven
    // 429s on /usage as plain rate limiting.
    let mut active_token = token;
    let mut oauth429: Option<(Option<u64>, String)> = None;
    let mut oauth401: Option<String> = None;

    let mut usage: Option<ClaudeUsageResponse> = match http::authed_get(
        "https://api.anthropic.com/api/oauth/usage",
        &active_token,
        Some(extra_headers.clone()),
    ) {
        Ok(u) => Some(u),
        Err(HttpCallError::RateLimited { retry_after_secs, body, .. }) => {
            oauth429 = Some((retry_after_secs, body));
            None
        }
        Err(HttpCallError::Unsuccessful { status: 401, body, .. }) => {
            oauth401 = Some(body);
            None
        }
        Err(_) => None,
    };

    // A burst 429 with a tiny Retry-After is transient (several local agents
    // share these endpoints) — honor it once instead of surfacing a sticky
    // RateLimited state for a whole refresh cycle.
    if usage.is_none() {
        if let Some((Some(retry), _)) = oauth429 {
            if retry <= 5 {
                std::thread::sleep(std::time::Duration::from_secs(retry.clamp(1, 5)));
                match http::authed_get(
                    "https://api.anthropic.com/api/oauth/usage",
                    &active_token,
                    Some(extra_headers.clone()),
                ) {
                    Ok(u) => {
                        usage = Some(u);
                        oauth429 = None;
                    }
                    Err(HttpCallError::RateLimited { retry_after_secs, body, .. }) => {
                        oauth429 = Some((retry_after_secs, body));
                    }
                    Err(HttpCallError::Unsuccessful { status: 401, body, .. }) => {
                        oauth401 = Some(body);
                    }
                    Err(_) => {}
                }
            }
        }
    }

    let mut profile: Option<ClaudeProfileResponse> = match http::authed_get::<ClaudeProfileResponse>(
        "https://api.anthropic.com/api/oauth/profile",
        &active_token,
        Some(extra_headers.clone()),
    ) {
        Ok(p) => Some(p),
        Err(HttpCallError::Unsuccessful { status: 401, body, .. }) => {
            if oauth401.is_none() {
                oauth401 = Some(body);
            }
            None
        }
        Err(_) => None,
    };

    // If profile or usage is empty (token may be expired), try refreshing and retrying both
    let usage_empty = usage.is_none() || usage.as_ref().and_then(|u| u.five_hour.as_ref()).is_none();
    let profile_empty = profile.is_none() || profile.as_ref().and_then(|p| p.account.as_ref()).is_none();
    if usage_empty || profile_empty {
        if let Some(new_token) = try_refresh_access_token() {
            active_token = new_token;
            let fresh_headers = http::headers(&[("anthropic-beta", "oauth-2025-04-20")]);
            // Clear any previous 429/401 captures — a successful retry on
            // the new token shouldn't flag the user as unauthenticated.
            if usage_empty {
                oauth429 = None;
                oauth401 = None;
                usage = match http::authed_get(
                    "https://api.anthropic.com/api/oauth/usage",
                    &active_token,
                    Some(fresh_headers.clone()),
                ) {
                    Ok(u) => Some(u),
                    Err(HttpCallError::RateLimited { retry_after_secs, body, .. }) => {
                        oauth429 = Some((retry_after_secs, body));
                        None
                    }
                    Err(HttpCallError::Unsuccessful { status: 401, body, .. }) => {
                        oauth401 = Some(body);
                        None
                    }
                    Err(_) => None,
                };
            }
            if profile_empty {
                profile = match http::authed_get::<ClaudeProfileResponse>(
                    "https://api.anthropic.com/api/oauth/profile",
                    &active_token,
                    Some(fresh_headers),
                ) {
                    Ok(p) => Some(p),
                    Err(HttpCallError::Unsuccessful { status: 401, body, .. }) => {
                        if oauth401.is_none() {
                            oauth401 = Some(body);
                        }
                        None
                    }
                    Err(_) => None,
                };
            }
        }
    }

    // Fetch /api/oauth/account; fall back to last successful snapshot when
    // it 429s (Anthropic throttles the account endpoint aggressively once
    // a token is out_of_credits).
    let mut account_raw: Option<serde_json::Value> = match http::authed_get::<serde_json::Value>(
        "https://api.anthropic.com/api/oauth/account",
        &active_token,
        Some(http::headers(&[("anthropic-beta", "oauth-2025-04-20")])),
    ) {
        Ok(v) => Some(v),
        Err(HttpCallError::Unsuccessful { status: 401, body, .. }) => {
            if oauth401.is_none() {
                oauth401 = Some(body);
            }
            None
        }
        Err(_) => None,
    };
    if let Some(ref fresh) = account_raw {
        if let Ok(mut guard) = LAST_ACCOUNT_CACHE.lock() {
            *guard = Some(AccountSnapshot {
                raw: fresh.clone(),
                fetched_at: std::time::Instant::now(),
            });
        }
    } else if oauth401.is_none() {
        // Stale account JSON must not mask HTTP 401 on live OAuth calls.
        if let Ok(guard) = LAST_ACCOUNT_CACHE.lock() {
            if let Some(ref snap) = *guard {
                if snap.fetched_at.elapsed().as_secs() < ACCOUNT_FALLBACK_TTL_SECS {
                    account_raw = Some(snap.raw.clone());
                }
            }
        }
    }
    let account: Option<claude_account::AccountResponse> = account_raw
        .as_ref()
        .and_then(|v| serde_json::from_value(v.clone()).ok());

    let show_internal = crate::commands::config::load_settings()
        .analytics
        .claude_experimental_features;

    // Build rate limits (modern limits[] first, legacy named fields fallback)
    let rate_limits = usage
        .as_ref()
        .map(|u| build_rate_limits(u, show_internal))
        .unwrap_or_default();

    let had_oauth_401 = oauth401.is_some();
    // Classify plan before deriving limit state so we can suppress plan-irrelevant flags.
    let plan_tier = classify_plan(profile.as_ref().and_then(|p| p.organization.as_ref()));
    let limit_state =
        derive_claude_limit_state(&usage, &profile, &account, &rate_limits, oauth429, oauth401, plan_tier);

    if had_oauth_401 {
        if let Ok(mut guard) = CLAUDE_CACHE.lock() {
            *guard = None;
        }
    }

    // plan_tier already classified above (before derive_claude_limit_state).

    // Build credit usage.
    //   * Pro/Max: extra_usage is the on-demand overage meter (cents). Only
    //     surface it when explicitly enabled by the user.
    //   * Enterprise: extra_usage is the *primary* spend ledger. `monthly_limit`
    //     may be null (uncapped) and amounts are already denominated in dollars
    //     (`used_credits` represents whole dollars × 100, same units as Pro/Max).
    //     Surface it even when `is_enabled` isn't reported, so the UI has data.
    let credit_usage = usage.as_ref().and_then(|u| {
        let extra = u.extra_usage.as_ref()?;
        let enabled = extra.is_enabled.unwrap_or(false);
        if !enabled && !plan_tier.is_enterprise() {
            return None;
        }
        let used_dollars = extra.used_credits.unwrap_or(0.0) / 100.0;
        let limit_dollars = extra.monthly_limit.map(|l| l / 100.0);
        let remaining = match limit_dollars {
            Some(limit) => (limit - used_dollars).max(0.0),
            None => 0.0,
        };
        Some(CreditUsage {
            provider_id: "claude-code".into(),
            used: used_dollars,
            limit: limit_dollars,
            remaining,
            currency: extra.currency.clone().unwrap_or_else(|| "USD".into()),
            billing_cycle_end: None,
            plan_name: plan_tier.label().map(String::from),
        })
    });

    // Build extra data from profile
    let mut extra = HashMap::new();
    let (email, plan, org_name) = if let Some(ref p) = profile {
        let email = p.account.as_ref().and_then(|a| a.email.clone());
        let org = p.organization.as_ref();
        let plan = plan_tier.label().map(String::from).or_else(|| {
            org.and_then(|o| o.rate_limit_tier.clone())
                .or_else(|| org.and_then(|o| o.organization_type.clone()))
        });
        let org_name = org.and_then(|o| o.name.clone());
        let sub_status = org.and_then(|o| o.subscription_status.clone());

        if let Some(ref acct) = p.account {
            if let Some(true) = acct.has_claude_max {
                extra.insert("has_claude_max".into(), serde_json::Value::Bool(true));
            }
            if let Some(ref created) = acct.created_at {
                extra.insert("account_created_at".into(), serde_json::Value::String(created.clone()));
            }
        }
        if let Some(ref s) = sub_status {
            extra.insert("subscription_status".into(), serde_json::Value::String(s.clone()));
        }
        if let Some(ref tier) = org.and_then(|o| o.rate_limit_tier.clone()) {
            extra.insert("rate_limit_tier".into(), serde_json::Value::String(tier.clone()));
        }
        if let Some(ref otype) = org.and_then(|o| o.organization_type.clone()) {
            extra.insert("organization_type".into(), serde_json::Value::String(otype.clone()));
        }
        if let Some(ref seat) = org.and_then(|o| o.seat_tier.clone()) {
            extra.insert("seat_tier".into(), serde_json::Value::String(seat.clone()));
        }
        if let Some(ref billing) = org.and_then(|o| o.billing_type.clone()) {
            extra.insert("billing_type".into(), serde_json::Value::String(billing.clone()));
        }
        if let Some(eu_enabled) = org.and_then(|o| o.has_extra_usage_enabled) {
            extra.insert("has_extra_usage_enabled".into(), serde_json::Value::Bool(eu_enabled));
        }

        // Why extra usage is off (e.g. "out_of_credits"), so the UI can explain
        // the DISABLED badge instead of showing a bare dash.
        if let Some(ref u) = usage {
            if let Some(ref eu) = u.extra_usage {
                if let Some(ref reason) = eu.disabled_reason {
                    extra.insert("extra_usage_disabled_reason".into(), serde_json::json!(reason));
                }
                if let Some(ever) = eu.credits_ever_enabled {
                    extra.insert("extra_usage_credits_ever_enabled".into(), serde_json::Value::Bool(ever));
                }
            }
        }

        // Enterprise-specific spend signals (used by tray / analytics page UI).
        if plan_tier.is_enterprise() {
            extra.insert("is_enterprise".into(), serde_json::Value::Bool(true));
            if let Some(ref u) = usage {
                if let Some(ref eu) = u.extra_usage {
                    let used = eu.used_credits.unwrap_or(0.0) / 100.0;
                    extra.insert(
                        "enterprise_used_usd".into(),
                        serde_json::json!(used),
                    );
                    if let Some(limit) = eu.monthly_limit {
                        extra.insert(
                            "enterprise_limit_usd".into(),
                            serde_json::json!(limit / 100.0),
                        );
                    }
                    if let Some(currency) = eu.currency.clone() {
                        extra.insert(
                            "enterprise_currency".into(),
                            serde_json::Value::String(currency),
                        );
                    }
                }
            }
        }

        (email, plan, org_name)
    } else {
        let creds = read_auto_credentials().ok();
        let plan = creds.as_ref().and_then(|c| c.subscription_type.clone());
        (None, plan, None)
    };

    // Stash the raw `/api/oauth/account` payload so the analytics page and
    // debug surfaces can see what the API actually returned.
    if let Some(ref raw) = account_raw {
        extra.insert("account_response".into(), raw.clone());
        if let Some(memberships) = raw.get("memberships") {
            extra.insert("account_memberships".into(), memberships.clone());
        }
    }

    // Enrich with today's local session stats
    enrich_with_today_stats(&mut extra);

    ProviderAnalytics {
        provider_id: "claude-code".into(),
        provider_name: "Claude Code".into(),
        status: ProviderStatus {
            provider_id: "claude-code".into(),
            provider_name: "Claude Code".into(),
            connected: true,
            connection_method: method,
            account_email: email,
            plan_name: plan,
            org_name,
            error: None,
        },
        rate_limits,
        credit_usage,
        token_counts: None,
        limit_state: Some(limit_state),
        extra,
        fetched_at: now,
    }
}

/// Token stats accumulator for a time window with model-aware cost tracking.
#[derive(Default, Clone)]
struct WindowStats {
    sessions: i64,
    messages: i64,
    input_tokens: i64,
    output_tokens: i64,
    cache_read: i64,
    cache_write: i64,
    // Model-aware running cost totals
    total_cost: f64,
    input_cost: f64,
    output_cost: f64,
    cache_read_cost: f64,
    cache_write_cost: f64,
}

impl WindowStats {
    fn total_tokens(&self) -> i64 {
        self.input_tokens + self.output_tokens + self.cache_read + self.cache_write
    }

    /// Add token usage with model-aware cost calculation
    fn add_usage(&mut self, model: Option<&str>, inp: i64, outp: i64, cr: i64, cw: i64, cw_1h: i64) {
        self.input_tokens += inp;
        self.output_tokens += outp;
        self.cache_read += cr;
        self.cache_write += cw;

        let c = crate::analytics::cost_engine::estimate_cost_components(
            model,
            &crate::analytics::cost_engine::TokensForCost {
                input: inp.max(0) as u64,
                output: outp.max(0) as u64,
                cache_read: cr.max(0) as u64,
                cache_write: cw.max(0) as u64,
            },
        );
        let premium = crate::analytics::cost_engine::cache_write_1h_premium(
            model,
            cw_1h.clamp(0, cw.max(0)) as u64,
        );
        self.input_cost += c.input;
        self.output_cost += c.output;
        self.cache_read_cost += c.cache_read;
        self.cache_write_cost += c.cache_write + premium;
        self.total_cost += c.total() + premium;
    }

    fn insert_to_extra(&self, prefix: &str, extra: &mut HashMap<String, serde_json::Value>) {
        if self.sessions == 0 { return; }
        extra.insert(format!("{}_sessions", prefix), serde_json::json!(self.sessions));
        extra.insert(format!("{}_messages", prefix), serde_json::json!(self.messages));
        extra.insert(format!("{}_tokens", prefix), serde_json::json!(self.total_tokens()));
        extra.insert(format!("{}_input_tokens", prefix), serde_json::json!(self.input_tokens));
        extra.insert(format!("{}_output_tokens", prefix), serde_json::json!(self.output_tokens));
        extra.insert(format!("{}_cache_read", prefix), serde_json::json!(self.cache_read));
        extra.insert(format!("{}_cache_write", prefix), serde_json::json!(self.cache_write));
        extra.insert(format!("{}_cost", prefix), serde_json::json!(self.total_cost));
        extra.insert(format!("{}_input_cost", prefix), serde_json::json!(self.input_cost));
        extra.insert(format!("{}_output_cost", prefix), serde_json::json!(self.output_cost));
        extra.insert(format!("{}_cache_read_cost", prefix), serde_json::json!(self.cache_read_cost));
        extra.insert(format!("{}_cache_write_cost", prefix), serde_json::json!(self.cache_write_cost));
    }
}

/// Start of the current **calendar day in IST** (Asia/Kolkata, UTC+5:30, no DST), as UTC.
/// Used for tray "Today" and analytics `today` range so midnight matches Indian Standard Time.
pub fn claude_calendar_day_start_ist_utc() -> chrono::DateTime<chrono::Utc> {
    use chrono::{TimeZone, Utc};
    let ist = chrono::FixedOffset::east_opt(5 * 3600 + 30 * 60).expect("IST offset");
    let now_ist = Utc::now().with_timezone(&ist);
    let naive_midnight = now_ist
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    ist.from_local_datetime(&naive_midnight)
        .single()
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now)
}

/// Start of local calendar week (Monday 00:00) as file `modified` time floor.
/// JSONL files older than this are skipped in tray stats (`enrich_with_today_stats`) and must be
/// skipped when V2 aggregates the **`today`** (IST calendar day) window so totals match the menu bar.
pub fn projects_jsonl_tray_mtime_floor() -> std::time::SystemTime {
    use chrono::{Datelike, NaiveTime, TimeZone};
    let local_now = chrono::Local::now();
    let weekday = local_now.weekday().num_days_from_monday();
    let monday_naive = (local_now.date_naive() - chrono::Duration::days(weekday as i64))
        .and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap());
    chrono::Local
        .from_local_datetime(&monday_naive)
        .single()
        .map(|dt| {
            std::time::UNIX_EPOCH + std::time::Duration::from_secs(dt.timestamp() as u64)
        })
        .unwrap_or_else(|| {
            std::time::SystemTime::now() - std::time::Duration::from_secs(86400)
        })
}

/// Enrich extra with local JSONL stats for **today (IST)**, **this week**, and **all time**.
fn enrich_with_today_stats(extra: &mut HashMap<String, serde_json::Value>) {
    let claude_dir = match dirs::home_dir() {
        Some(h) => h.join(".claude").join("projects"),
        None => return,
    };
    if !claude_dir.exists() { return; }

    use chrono::{Datelike, NaiveTime, TimeZone};
    let local_now = chrono::Local::now();
    let utc_now = chrono::Utc::now();

    fn to_z_string(dt: chrono::DateTime<chrono::Utc>) -> String {
        dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
    }

    let cutoff_today = to_z_string(claude_calendar_day_start_ist_utc());

    let weekday = local_now.weekday().num_days_from_monday();
    let monday_naive = (local_now.date_naive() - chrono::Duration::days(weekday as i64))
        .and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap());
    let cutoff_week = chrono::Local.from_local_datetime(&monday_naive).single()
        .map(|dt| to_z_string(dt.with_timezone(&chrono::Utc)))
        .unwrap_or_else(|| to_z_string(utc_now - chrono::Duration::days(7)));

    let week_sys = projects_jsonl_tray_mtime_floor();

    let mut stats_today = WindowStats::default();
    let mut stats_week = WindowStats::default();
    let mut stats_all = WindowStats::default();

    let mut sessions_today = std::collections::HashSet::new();
    let mut sessions_week = std::collections::HashSet::new();
    let mut sessions_all = std::collections::HashSet::new();

    fn collect_jsonl_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    collect_jsonl_files(&path, out);
                } else if path.extension().map_or(false, |e| e == "jsonl") {
                    out.push(path);
                }
            }
        }
    }

    let mut all_jsonl: Vec<std::path::PathBuf> = Vec::new();
    collect_jsonl_files(&claude_dir, &mut all_jsonl);

    for path in &all_jsonl {
        let modified_after_week = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .map(|t| t >= week_sys)
            .unwrap_or(false);

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let file_id = path.to_string_lossy().to_string();
        // Per-file dedup: skip duplicate streaming chunks by (message.id, requestId)
        let mut seen_keys: std::collections::HashSet<String> = std::collections::HashSet::new();
        for line in content.lines() {
            let ts_ref = {
                let needle1 = "\"timestamp\":\"";
                let needle2 = "\"timestamp\": \"";
                let start_and_len = line.find(needle1).map(|i| (i + needle1.len(), needle1.len()))
                    .or_else(|| line.find(needle2).map(|i| (i + needle2.len(), needle2.len())));
                if let Some((val_start, _)) = start_and_len {
                    let rest = &line[val_start..];
                    rest.find('"').map(|end| &line[val_start..val_start + end])
                } else {
                    None
                }
            };

            let ts_str = match ts_ref {
                Some(s) if !s.is_empty() => s,
                _ => continue,
            };

            let in_week = modified_after_week && ts_str >= cutoff_week.as_str();
            let in_today = modified_after_week && ts_str >= cutoff_today.as_str();

            let is_user = line.contains("\"type\":\"user\"") || line.contains("\"type\": \"user\"");
            let is_assistant = line.contains("\"type\":\"assistant\"") || line.contains("\"type\": \"assistant\"");

            if is_user || is_assistant {
                stats_all.messages += 1; sessions_all.insert(file_id.clone());
                if in_week { stats_week.messages += 1; sessions_week.insert(file_id.clone()); }
                if in_today { stats_today.messages += 1; sessions_today.insert(file_id.clone()); }
            }

            if line.contains("\"usage\"") {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
                    // Deduplicate streaming chunks by (message.id, requestId)
                    let msg = val.get("message");
                    let message_id = msg.and_then(|m| m.get("id")).and_then(|v| v.as_str());
                    let request_id = val.get("requestId").and_then(|v| v.as_str());
                    if let (Some(mid), Some(rid)) = (message_id, request_id) {
                        let key = format!("{}:{}", mid, rid);
                        if seen_keys.contains(&key) {
                            continue;
                        }
                        seen_keys.insert(key);
                    }

                    let usage = msg.and_then(|m| m.get("usage"))
                        .or_else(|| val.get("usage"));
                    let model_str = msg.and_then(|m| m.get("model")).and_then(|v| v.as_str());

                    if let Some(usage) = usage {
                        let inp = usage.get("input_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
                        let outp = usage.get("output_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
                        let cr = usage.get("cache_read_input_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
                        let cw = usage.get("cache_creation_input_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
                        let cw_1h = usage
                            .get("cache_creation")
                            .and_then(|c| c.get("ephemeral_1h_input_tokens"))
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0)
                            .min(cw);

                        stats_all.add_usage(model_str, inp, outp, cr, cw, cw_1h);
                        if in_week { stats_week.add_usage(model_str, inp, outp, cr, cw, cw_1h); }
                        if in_today { stats_today.add_usage(model_str, inp, outp, cr, cw, cw_1h); }
                    }
                }
            }
        }
    }

    stats_today.sessions = sessions_today.len() as i64;
    stats_week.sessions = sessions_week.len() as i64;
    stats_all.sessions = sessions_all.len() as i64;

    stats_today.insert_to_extra("start_today", extra);
    stats_week.insert_to_extra("this_week", extra);
    stats_all.insert_to_extra("all_time", extra);
}

/// Fetch the full account data from /api/oauth/account (silent only).
pub fn fetch_claude_account() -> Result<serde_json::Value, String> {
    let (token, _) = resolve_access_token_silent()?;
    let extra_headers = http::headers(&[("anthropic-beta", "oauth-2025-04-20")]);
    http::authed_get::<serde_json::Value>(
        "https://api.anthropic.com/api/oauth/account",
        &token,
        Some(extra_headers),
    )
    .map_err(String::from)
}

/// Check if credentials exist silently (without API calls or keychain).
pub fn check_connection() -> ProviderStatus {
    match resolve_access_token_silent() {
        Ok((_, method)) => ProviderStatus {
            provider_id: "claude-code".into(),
            provider_name: "Claude Code".into(),
            connected: true,
            connection_method: method,
            account_email: None,
            plan_name: None,
            org_name: None,
            error: None,
        },
        Err(e) => ProviderStatus {
            provider_id: "claude-code".into(),
            provider_name: "Claude Code".into(),
            connected: false,
            connection_method: "none".into(),
            account_email: None,
            plan_name: None,
            org_name: None,
            error: Some(e),
        },
    }
}

#[cfg(test)]
mod derive_limit_state_tests {
    use super::*;
    use crate::analytics::claude_account::AccountResponse;

    fn rl(label: &str, used: f64, rem: f64) -> RateLimitWindow {
        RateLimitWindow {
            provider_id: "claude-code".into(),
            label: label.into(),
            used_percent: used,
            remaining_percent: rem,
            resets_at: None,
            resets_in_seconds: None,
            window_seconds: None,
        }
    }

    #[test]
    fn api_disabled_wins_over_usage() {
        let profile: ClaudeProfileResponse = serde_json::from_value(serde_json::json!({
            "organization": { "uuid": "org-1", "name": "Acme" }
        }))
        .unwrap();
        let account: AccountResponse = serde_json::from_value(serde_json::json!({
            "memberships": [{
                "organization": {
                    "uuid": "org-1",
                    "name": "Acme",
                    "api_disabled_reason": "out_of_credits"
                }
            }]
        }))
        .unwrap();
        let limits = vec![rl("Session (5h)", 50.0, 50.0)];
        // out_of_credits is only surfaced for Enterprise plans
        let s = derive_claude_limit_state(
            &None,
            &Some(profile),
            &Some(account),
            &limits,
            None,
            None,
            PlanTier::Enterprise,
        );
        assert!(matches!(
            s,
            LimitState::ApiDisabled { ref reason, .. } if reason == "out_of_credits"
        ));
    }

    #[test]
    fn oauth_429_maps_to_rate_limited() {
        let profile: ClaudeProfileResponse = serde_json::from_value(serde_json::json!({
            "organization": { "uuid": "org-1", "name": "Acme", "subscription_status": "active" }
        }))
        .unwrap();
        let s = derive_claude_limit_state(
            &None,
            &Some(profile),
            &None,
            &[],
            Some((Some(120), "too many requests".into())),
            None,
            PlanTier::Pro,
        );
        assert!(matches!(
            s,
            LimitState::RateLimited {
                retry_after_secs: Some(120),
                ..
            }
        ));
    }

    #[test]
    fn reached_when_window_at_100() {
        let profile: ClaudeProfileResponse = serde_json::from_value(serde_json::json!({
            "organization": { "uuid": "org-1", "name": "Acme", "subscription_status": "active" }
        }))
        .unwrap();
        let limits = vec![rl("Session (5h)", 100.0, 0.0)];
        let s = derive_claude_limit_state(&None, &Some(profile), &None, &limits, None, None, PlanTier::Pro);
        assert!(matches!(
            s,
            LimitState::Reached { used_pct, .. } if (used_pct - 100.0).abs() < 1e-5
        ));
    }

    #[test]
    fn approaching_over_80() {
        let profile: ClaudeProfileResponse = serde_json::from_value(serde_json::json!({
            "organization": { "uuid": "org-1", "name": "Acme", "subscription_status": "active" }
        }))
        .unwrap();
        let limits = vec![rl("Weekly", 87.0, 13.0)];
        let s = derive_claude_limit_state(&None, &Some(profile), &None, &limits, None, None, PlanTier::Pro);
        assert!(matches!(
            s,
            LimitState::Approaching { worst_pct, .. } if (worst_pct - 87.0).abs() < 1e-5
        ));
    }

    #[test]
    fn subscription_past_due() {
        let profile: ClaudeProfileResponse = serde_json::from_value(serde_json::json!({
            "organization": {
                "uuid": "org-1",
                "name": "Acme",
                "subscription_status": "past_due"
            }
        }))
        .unwrap();
        let s = derive_claude_limit_state(&None, &Some(profile), &None, &[], None, None, PlanTier::Pro);
        assert!(matches!(
            s,
            LimitState::SubscriptionIssue { ref status, .. } if status == "past_due"
        ));
    }

    #[test]
    fn store_billed_canceled_subscription_is_not_an_issue() {
        // Google Play-billed Max plan: Anthropic reports the Stripe-side
        // subscription_status as "canceled" while the plan is active.
        let profile: ClaudeProfileResponse = serde_json::from_value(serde_json::json!({
            "account": { "has_claude_max": true },
            "organization": {
                "uuid": "org-1",
                "name": "abhi's Organization",
                "organization_type": "claude_max",
                "billing_type": "google_play_subscription",
                "rate_limit_tier": "default_claude_max_5x",
                "subscription_status": "canceled"
            }
        }))
        .unwrap();
        let s = derive_claude_limit_state(&None, &Some(profile), &None, &[], None, None, PlanTier::Max);
        assert!(matches!(s, LimitState::Healthy));
    }

    #[test]
    fn stripe_billed_canceled_subscription_still_flags() {
        let profile: ClaudeProfileResponse = serde_json::from_value(serde_json::json!({
            "organization": {
                "uuid": "org-1",
                "name": "Acme",
                "billing_type": "stripe_subscription",
                "subscription_status": "canceled"
            }
        }))
        .unwrap();
        let s = derive_claude_limit_state(&None, &Some(profile), &None, &[], None, None, PlanTier::Pro);
        assert!(matches!(
            s,
            LimitState::SubscriptionIssue { ref status, .. } if status == "canceled"
        ));
    }

    #[test]
    fn modern_limits_array_builds_scoped_windows() {
        // Live response shape (2026-08): legacy seven_day_* null, windows in limits[]
        let usage: ClaudeUsageResponse = serde_json::from_value(serde_json::json!({
            "five_hour": { "utilization": 39.0, "resets_at": "2026-08-02T15:50:00Z" },
            "seven_day": { "utilization": 38.0, "resets_at": "2026-08-06T00:00:00Z" },
            "seven_day_opus": null,
            "seven_day_sonnet": null,
            "limits": [
                { "kind": "session", "group": "session", "percent": 39, "severity": "normal",
                  "resets_at": "2026-08-02T15:50:00Z", "scope": null, "is_active": false },
                { "kind": "weekly_all", "group": "weekly", "percent": 38,
                  "resets_at": "2026-08-06T00:00:00Z", "scope": null },
                { "kind": "weekly_scoped", "group": "weekly", "percent": 57,
                  "resets_at": "2026-08-05T23:59:59Z",
                  "scope": { "model": { "id": null, "display_name": "Fable" }, "surface": null },
                  "is_active": true },
                { "kind": "mystery_experiment", "percent": 12 }
            ]
        }))
        .unwrap();
        let windows = build_rate_limits(&usage, false);
        let labels: Vec<&str> = windows.iter().map(|w| w.label.as_str()).collect();
        assert_eq!(labels, vec!["Session (5h)", "Weekly (All)", "Fable (7d)"]);
        assert!((windows[2].used_percent - 57.0).abs() < 1e-6);
        assert_eq!(windows[2].window_seconds, Some(604800));
    }

    #[test]
    fn legacy_named_fields_build_windows_when_limits_absent() {
        let usage: ClaudeUsageResponse = serde_json::from_value(serde_json::json!({
            "five_hour": { "utilization": 42.0, "resets_at": "2026-08-02T15:50:00Z" },
            "seven_day": { "utilization": 18.0, "resets_at": "2026-08-06T00:00:00Z" },
            "seven_day_opus": { "utilization": 61.0, "resets_at": "2026-08-06T00:00:00Z" }
        }))
        .unwrap();
        let windows = build_rate_limits(&usage, false);
        let labels: Vec<&str> = windows.iter().map(|w| w.label.as_str()).collect();
        assert_eq!(labels, vec!["Session (5h)", "Weekly (All)", "Opus (7d)"]);
    }

    #[test]
    fn unauthenticated_when_all_endpoints_401() {
        let body = r#"{"type":"error","error":{"type":"authentication_error","message":"Invalid authentication credentials"}}"#;
        let s = derive_claude_limit_state(
            &None,
            &None,
            &None,
            &[],
            Some((Some(60), "rate limited".into())),
            Some(body.to_string()),
            PlanTier::Unknown,
        );
        assert!(matches!(
            s,
            LimitState::Unauthenticated { ref message } if message.contains("Invalid authentication credentials")
        ));
    }

    #[test]
    fn unauthenticated_401_still_triggers_with_stale_account_snapshot() {
        let body = r#"{"type":"error","error":{"type":"authentication_error","message":"Invalid authentication credentials"}}"#;
        let account: claude_account::AccountResponse =
            serde_json::from_value(serde_json::json!({ "memberships": [] })).unwrap();
        let s = derive_claude_limit_state(
            &None,
            &None,
            &Some(account),
            &[],
            None,
            Some(body.to_string()),
            PlanTier::Unknown,
        );
        assert!(matches!(s, LimitState::Unauthenticated { .. }));
    }

    #[test]
    fn out_of_credits_suppressed_for_pro_plan() {
        let profile: ClaudeProfileResponse = serde_json::from_value(serde_json::json!({
            "organization": { "uuid": "org-1", "name": "Acme Pro", "subscription_status": "active" }
        }))
        .unwrap();
        let account: claude_account::AccountResponse = serde_json::from_value(serde_json::json!({
            "memberships": [{
                "organization": {
                    "uuid": "org-1",
                    "name": "Acme Pro",
                    "api_disabled_reason": "out_of_credits"
                }
            }]
        }))
        .unwrap();
        let limits = vec![rl("Session (5h)", 50.0, 50.0)];
        // Pro plan — out_of_credits must be silenced
        let s = derive_claude_limit_state(
            &None,
            &Some(profile),
            &Some(account),
            &limits,
            None,
            None,
            PlanTier::Pro,
        );
        // Should fall through to Approaching (50% session usage), not ApiDisabled
        assert!(
            !matches!(s, LimitState::ApiDisabled { .. }),
            "out_of_credits must not show for Pro plans, got: {s:?}"
        );
    }
}
