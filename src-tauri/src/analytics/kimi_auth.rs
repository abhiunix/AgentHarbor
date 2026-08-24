//! Kimi Code OAuth token resolution + auto-refresh (Phase 2).
//!
//! Reads the Kimi CLI's own OAuth credential file and refreshes it in place
//! when it is near expiry, so the usage-limits endpoint can be authorized with
//! the (possibly refreshed) access token. Endpoints/params are taken verbatim
//! from the installed Kimi CLI (`kimi_cli/auth/oauth.py`, `auth/platforms.py`).
//!
//! Concurrency: the CLI holds `~/.kimi/credentials/kimi-code.lock` during its
//! own refresh. We do not take that advisory lock (no lock crate is vendored),
//! but every rewrite goes through `utils::paths::atomic_write_str` (tmp+rename)
//! so a concurrent CLI reader never observes a half-written file or a bad shape.

use crate::analytics::http;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// OAuth client id used by the Kimi CLI (public, from its source).
const CLIENT_ID: &str = "17e5f671-d194-4dfb-9706-5516cb48c098";
const DEFAULT_OAUTH_HOST: &str = "https://auth.kimi.com";
const MAX_REFRESH_ATTEMPTS: u32 = 3;

/// Error prefixes so callers can classify a `String` error without a bespoke
/// enum crossing the Tauri boundary.
pub const ERR_NOT_LOGGED_IN: &str = "not-logged-in:";
pub const ERR_UNAUTHORIZED: &str = "unauthorized:";

/// Shape of `~/.kimi/credentials/kimi-code.json`. Unknown keys are preserved
/// via `extra` so a rewrite never drops fields the CLI may add later.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KimiCredentials {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: f64,
    pub expires_in: f64,
    pub scope: String,
    pub token_type: String,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// 200 response from `POST /api/oauth/token`.
#[derive(Debug, Deserialize)]
struct RefreshResponse {
    access_token: String,
    refresh_token: String,
    expires_in: f64,
    scope: String,
    token_type: String,
}

fn credentials_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".kimi/credentials/kimi-code.json"))
}

/// OAuth host, overridable by env (matches the CLI's override knobs).
fn oauth_host() -> String {
    std::env::var("KIMI_CODE_OAUTH_HOST")
        .or_else(|_| std::env::var("KIMI_OAUTH_HOST"))
        .unwrap_or_else(|_| DEFAULT_OAUTH_HOST.to_string())
}

fn now_unix() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Refresh buffer: `max(60s, expires_in * 0.2)` before `expires_at`.
fn refresh_buffer_secs(expires_in: f64) -> f64 {
    (expires_in * 0.2).max(60.0)
}

/// True when the token is within the refresh buffer of expiry (or already
/// expired). Pure — unit-tested with fixtures.
fn needs_refresh(expires_at: f64, expires_in: f64, now: f64) -> bool {
    now + refresh_buffer_secs(expires_in) >= expires_at
}

fn read_credentials() -> Result<KimiCredentials, String> {
    let path = credentials_path()
        .ok_or_else(|| format!("{ERR_NOT_LOGGED_IN} home directory not found"))?;
    let text = std::fs::read_to_string(&path).map_err(|_| {
        format!(
            "{ERR_NOT_LOGGED_IN} no Kimi credentials at {} — run `kimi login`",
            path.display()
        )
    })?;
    serde_json::from_str::<KimiCredentials>(&text)
        .map_err(|e| format!("Kimi credential file is malformed: {e}"))
}

fn write_credentials(creds: &KimiCredentials) -> Result<(), String> {
    let path = credentials_path().ok_or("home directory not found")?;
    let text = serde_json::to_string_pretty(creds)
        .map_err(|e| format!("Failed to serialize Kimi credentials: {e}"))?;
    crate::utils::paths::atomic_write_str(&path, &text)
}

/// Outcome of one refresh HTTP attempt.
enum RefreshOutcome {
    Ok(RefreshResponse),
    /// 401/403 — the refresh token was rejected; retrying will not help.
    Rejected(String),
    /// Network/5xx — worth retrying.
    Transient(String),
}

fn refresh_once(host: &str, refresh_token: &str) -> RefreshOutcome {
    let url = format!("{}/api/oauth/token", host.trim_end_matches('/'));
    let client = match http::build_client(15) {
        Ok(c) => c,
        Err(e) => return RefreshOutcome::Transient(e.to_string()),
    };
    let params = [
        ("client_id", CLIENT_ID),
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
    ];
    let resp = match client.post(&url).form(&params).send() {
        Ok(r) => r,
        Err(e) => return RefreshOutcome::Transient(format!("refresh request failed: {e}")),
    };
    let status = resp.status();
    if status.is_success() {
        return match resp.json::<RefreshResponse>() {
            Ok(r) => RefreshOutcome::Ok(r),
            Err(e) => RefreshOutcome::Transient(format!("refresh response parse error: {e}")),
        };
    }
    let code = status.as_u16();
    let body = resp.text().unwrap_or_default();
    if code == 401 || code == 403 {
        RefreshOutcome::Rejected(body)
    } else {
        RefreshOutcome::Transient(format!("refresh HTTP {code}: {body}"))
    }
}

/// Refresh the credential file in place and return the new access token.
fn refresh_and_store(mut creds: KimiCredentials) -> Result<String, String> {
    let host = oauth_host();
    let mut last_transient: Option<String> = None;

    for attempt in 0..MAX_REFRESH_ATTEMPTS {
        match refresh_once(&host, &creds.refresh_token) {
            RefreshOutcome::Ok(r) => {
                creds.access_token = r.access_token.clone();
                creds.refresh_token = r.refresh_token;
                creds.expires_in = r.expires_in;
                creds.expires_at = now_unix() + r.expires_in;
                creds.scope = r.scope;
                creds.token_type = r.token_type;
                write_credentials(&creds)?;
                return Ok(r.access_token);
            }
            RefreshOutcome::Rejected(body) => {
                return Err(format!(
                    "{ERR_UNAUTHORIZED} Kimi refresh token rejected — run `kimi login`. {body}"
                ));
            }
            RefreshOutcome::Transient(msg) => {
                last_transient = Some(msg);
                if attempt + 1 < MAX_REFRESH_ATTEMPTS {
                    std::thread::sleep(std::time::Duration::from_millis(300 * (attempt as u64 + 1)));
                }
            }
        }
    }
    Err(format!(
        "Kimi token refresh failed after {MAX_REFRESH_ATTEMPTS} attempts: {}",
        last_transient.unwrap_or_else(|| "unknown error".into())
    ))
}

/// Resolve the Kimi OAuth access token, refreshing in place if near expiry.
/// Returns `(access_token, method)` where method is `"oauth-refreshed"` or
/// `"oauth-cached"`. Errors are prefixed (`not-logged-in:` / `unauthorized:`)
/// so callers can degrade gracefully.
pub fn resolve_kimi_oauth_token() -> Result<(String, String), String> {
    let creds = read_credentials()?;
    if needs_refresh(creds.expires_at, creds.expires_in, now_unix()) {
        let token = refresh_and_store(creds)?;
        Ok((token, "oauth-refreshed".into()))
    } else {
        Ok((creds.access_token, "oauth-cached".into()))
    }
}

/// Stable hash of the current access token, for usage-cache invalidation.
/// Returns 0 when no credentials are present.
pub fn kimi_credential_fingerprint() -> u64 {
    match read_credentials() {
        Ok(c) => {
            let mut h = DefaultHasher::new();
            c.access_token.hash(&mut h);
            h.finish()
        }
        Err(_) => 0,
    }
}

// ── Tests (no live network — fixtures only) ─────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn creds(expires_at: f64, expires_in: f64) -> KimiCredentials {
        KimiCredentials {
            access_token: "a".into(),
            refresh_token: "r".into(),
            expires_at,
            expires_in,
            scope: "kimi-code".into(),
            token_type: "Bearer".into(),
            extra: serde_json::Map::new(),
        }
    }

    #[test]
    fn refresh_buffer_is_at_least_60s() {
        assert_eq!(refresh_buffer_secs(100.0), 60.0); // 20% = 20 < 60
        assert_eq!(refresh_buffer_secs(3600.0), 720.0); // 20% = 720 > 60
    }

    #[test]
    fn needs_refresh_when_expired_or_within_buffer() {
        // expires_in = 3600 → buffer = 720s.
        let now = 1_000_000.0;
        let c = creds(now + 3600.0, 3600.0);
        // Far from expiry: no refresh.
        assert!(!needs_refresh(c.expires_at, c.expires_in, now));
        // Within the 720s buffer: refresh.
        assert!(needs_refresh(now + 700.0, 3600.0, now));
        // Already expired: refresh.
        assert!(needs_refresh(now - 10.0, 3600.0, now));
        // Exactly at the buffer edge: refresh (>=).
        assert!(needs_refresh(now + 720.0, 3600.0, now));
    }

    #[test]
    fn credential_roundtrip_preserves_all_fields_including_unknown() {
        let raw = r#"{
            "access_token": "jwt-access",
            "refresh_token": "jwt-refresh",
            "expires_at": 1787599566.5,
            "expires_in": 3600.0,
            "scope": "kimi-code",
            "token_type": "Bearer",
            "future_field": {"nested": true}
        }"#;
        let parsed: KimiCredentials = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.access_token, "jwt-access");
        assert_eq!(parsed.refresh_token, "jwt-refresh");
        assert_eq!(parsed.expires_at, 1787599566.5);
        assert_eq!(parsed.expires_in, 3600.0);
        assert_eq!(parsed.scope, "kimi-code");
        assert_eq!(parsed.token_type, "Bearer");
        // Unknown field is retained via `extra`.
        assert_eq!(
            parsed.extra.get("future_field"),
            Some(&serde_json::json!({"nested": true}))
        );

        // Round-trip: re-serialize then re-parse — no field is lost.
        let text = serde_json::to_string(&parsed).unwrap();
        let again: KimiCredentials = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed, again);
    }

    #[test]
    fn missing_extra_serializes_without_error() {
        let c = creds(1.0, 2.0);
        let text = serde_json::to_string(&c).unwrap();
        let back: KimiCredentials = serde_json::from_str(&text).unwrap();
        assert_eq!(c, back);
    }
}
