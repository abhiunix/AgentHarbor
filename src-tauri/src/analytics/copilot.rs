//! GitHub Copilot analytics provider.
//! Auth: OAuth token from keychain or device flow
//! API: api.github.com/copilot_internal/user

use crate::analytics::http;
use crate::analytics::token_store;
use crate::analytics::types::*;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── API response types ──────────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
struct QuotaSnapshot {
    entitlement: Option<String>,
    remaining: Option<i64>,
    percent_remaining: Option<f64>,
    reset_date: Option<String>,
    overage_allowed: Option<bool>,
}

#[derive(Deserialize, Debug)]
struct QuotaSnapshots {
    premium_interactions: Option<QuotaSnapshot>,
    chat: Option<QuotaSnapshot>,
}

#[derive(Deserialize, Debug)]
struct CopilotUserResponse {
    #[serde(rename = "copilotPlan")]
    copilot_plan: Option<String>,
    #[serde(rename = "quotaResetDate")]
    quota_reset_date: Option<String>,
    quota_snapshots: Option<QuotaSnapshots>,
}

// ── Device Flow types ───────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DeviceFlowInfo {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: i64,
    pub interval: i64,
}

#[derive(Deserialize, Debug)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: i64,
    interval: i64,
}

#[derive(Deserialize, Debug)]
struct DeviceTokenResponse {
    access_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

// ── Constants ───────────────────────────────────────────────────────────────

const GITHUB_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98"; // VS Code Copilot client ID

// ── Helpers ─────────────────────────────────────────────────────────────────

fn resolve_token() -> Result<(String, String), String> {
    if let Ok(Some(token)) = token_store::get_provider_token("copilot", "oauth-token") {
        return Ok((token, "device-flow".into()));
    }
    Err("No GitHub Copilot OAuth token configured".into())
}

fn copilot_headers() -> reqwest::header::HeaderMap {
    http::headers(&[
        ("Editor-Version", "vscode/1.96.2"),
        ("Editor-Plugin-Version", "copilot-chat/0.26.7"),
        ("User-Agent", "GitHubCopilotChat/0.26.7"),
        ("X-Github-Api-Version", "2025-04-01"),
    ])
}

// ── Public API ──────────────────────────────────────────────────────────────

pub fn fetch_copilot_analytics() -> ProviderAnalytics {
    let now = Utc::now().to_rfc3339();

    let (token, method) = match resolve_token() {
        Ok(t) => t,
        Err(e) => {
            return ProviderAnalytics {
                provider_id: "copilot".into(),
                provider_name: "GitHub Copilot".into(),
                status: ProviderStatus {
                    provider_id: "copilot".into(),
                    provider_name: "GitHub Copilot".into(),
                    connected: false,
                    connection_method: "none".into(),
                    account_email: None, plan_name: None, org_name: None,
                    error: Some(e),
                },
                rate_limits: vec![], credit_usage: None, token_counts: None,
                extra: HashMap::new(), fetched_at: now,
            };
        }
    };

    let resp: Result<CopilotUserResponse, String> = http::authed_get(
        "https://api.github.com/copilot_internal/user",
        &token,
        Some(copilot_headers()),
    );

    match resp {
        Ok(data) => {
            let mut rate_limits = Vec::new();
            let mut extra = HashMap::new();

            if let Some(ref snapshots) = data.quota_snapshots {
                if let Some(ref premium) = snapshots.premium_interactions {
                    let pct_remaining = premium.percent_remaining.unwrap_or(100.0);
                    let used = 100.0 - pct_remaining;
                    rate_limits.push(RateLimitWindow {
                        provider_id: "copilot".into(),
                        label: "Premium Interactions".into(),
                        used_percent: used,
                        remaining_percent: pct_remaining,
                        resets_at: premium.reset_date.clone().or_else(|| data.quota_reset_date.clone()),
                        resets_in_seconds: None,
                        window_seconds: None,
                    });
                    if let Some(remaining) = premium.remaining {
                        extra.insert("premium_remaining".into(), serde_json::Value::from(remaining));
                    }
                    if let Some(ref ent) = premium.entitlement {
                        extra.insert("premium_entitlement".into(), serde_json::Value::String(ent.clone()));
                    }
                }
                if let Some(ref chat) = snapshots.chat {
                    let pct_remaining = chat.percent_remaining.unwrap_or(100.0);
                    let used = 100.0 - pct_remaining;
                    rate_limits.push(RateLimitWindow {
                        provider_id: "copilot".into(),
                        label: "Chat".into(),
                        used_percent: used,
                        remaining_percent: pct_remaining,
                        resets_at: chat.reset_date.clone().or_else(|| data.quota_reset_date.clone()),
                        resets_in_seconds: None,
                        window_seconds: None,
                    });
                    if let Some(remaining) = chat.remaining {
                        extra.insert("chat_remaining".into(), serde_json::Value::from(remaining));
                    }
                }
            }

            if let Some(ref reset) = data.quota_reset_date {
                extra.insert("quota_reset_date".into(), serde_json::Value::String(reset.clone()));
            }

            ProviderAnalytics {
                provider_id: "copilot".into(),
                provider_name: "GitHub Copilot".into(),
                status: ProviderStatus {
                    provider_id: "copilot".into(),
                    provider_name: "GitHub Copilot".into(),
                    connected: true,
                    connection_method: method,
                    account_email: None,
                    plan_name: data.copilot_plan.clone(),
                    org_name: None,
                    error: None,
                },
                rate_limits,
                credit_usage: None,
                token_counts: None,
                extra,
                fetched_at: now,
            }
        }
        Err(e) => ProviderAnalytics {
            provider_id: "copilot".into(),
            provider_name: "GitHub Copilot".into(),
            status: ProviderStatus {
                provider_id: "copilot".into(),
                provider_name: "GitHub Copilot".into(),
                connected: true,
                connection_method: method,
                account_email: None, plan_name: None, org_name: None,
                error: Some(e),
            },
            rate_limits: vec![], credit_usage: None, token_counts: None,
            extra: HashMap::new(), fetched_at: now,
        },
    }
}

pub fn check_connection() -> ProviderStatus {
    match resolve_token() {
        Ok((_, method)) => ProviderStatus {
            provider_id: "copilot".into(),
            provider_name: "GitHub Copilot".into(),
            connected: true, connection_method: method,
            account_email: None, plan_name: None, org_name: None, error: None,
        },
        Err(e) => ProviderStatus {
            provider_id: "copilot".into(),
            provider_name: "GitHub Copilot".into(),
            connected: false, connection_method: "none".into(),
            account_email: None, plan_name: None, org_name: None, error: Some(e),
        },
    }
}

/// Start the GitHub device flow for Copilot authentication.
pub fn start_device_flow() -> Result<DeviceFlowInfo, String> {
    let body = serde_json::json!({
        "client_id": GITHUB_CLIENT_ID,
        "scope": "copilot"
    });

    let resp: DeviceCodeResponse = http::post_json(
        "https://github.com/login/device/code",
        &body,
        Some(http::headers(&[("Accept", "application/json")])),
    )?;

    Ok(DeviceFlowInfo {
        device_code: resp.device_code,
        user_code: resp.user_code,
        verification_uri: resp.verification_uri,
        expires_in: resp.expires_in,
        interval: resp.interval,
    })
}

/// Poll the GitHub device flow for a token.
/// Returns the access token on success, or an error describing the state.
pub fn poll_device_flow(device_code: String) -> Result<String, String> {
    let body = serde_json::json!({
        "client_id": GITHUB_CLIENT_ID,
        "device_code": device_code,
        "grant_type": "urn:ietf:params:oauth:grant-type:device_code"
    });

    let resp: DeviceTokenResponse = http::post_json(
        "https://github.com/login/oauth/access_token",
        &body,
        Some(http::headers(&[("Accept", "application/json")])),
    )?;

    if let Some(token) = resp.access_token {
        // Store the token automatically
        let _ = token_store::store_provider_token("copilot", "oauth-token", &token);
        return Ok(token);
    }

    if let Some(error) = resp.error {
        let desc = resp.error_description.unwrap_or_default();
        return Err(format!("{}: {}", error, desc));
    }

    Err("Unknown device flow response".into())
}
