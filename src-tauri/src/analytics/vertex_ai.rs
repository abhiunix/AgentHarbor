//! Vertex AI analytics provider.
//! Auth: auto-detect gcloud application default credentials
//! For v1: connection status + project info only (Cloud Monitoring API is complex)

use crate::analytics::http;
use crate::analytics::types::*;
use chrono::Utc;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

// ── Credential types ────────────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
struct GCloudADC {
    client_id: Option<String>,
    client_secret: Option<String>,
    refresh_token: Option<String>,
    #[serde(rename = "type")]
    cred_type: Option<String>,
    quota_project_id: Option<String>,
}

#[derive(Deserialize, Debug)]
struct TokenRefreshResponse {
    access_token: Option<String>,
    expires_in: Option<i64>,
    token_type: Option<String>,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn adc_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".config/gcloud/application_default_credentials.json")
}

fn config_default_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".config/gcloud/configurations/config_default")
}

/// Read the default project from gcloud config.
fn read_project() -> Option<String> {
    let path = config_default_path();
    let content = fs::read_to_string(&path).ok()?;
    // Parse INI-style: find "project = VALUE" under [core]
    let mut in_core = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_core = trimmed == "[core]";
            continue;
        }
        if in_core && trimmed.starts_with("project") {
            if let Some(val) = trimmed.split('=').nth(1) {
                return Some(val.trim().to_string());
            }
        }
    }
    None
}

fn read_adc() -> Result<GCloudADC, String> {
    let path = adc_path();
    if !path.exists() {
        return Err("Application default credentials not found".into());
    }
    let content = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read ADC: {}", e))?;
    serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse ADC: {}", e))
}

/// Refresh the access token using ADC refresh_token.
fn refresh_access_token(adc: &GCloudADC) -> Result<String, String> {
    let refresh_token = adc.refresh_token.as_ref()
        .ok_or("No refresh_token in ADC")?;
    let client_id = adc.client_id.as_ref()
        .ok_or("No client_id in ADC")?;
    let client_secret = adc.client_secret.as_ref()
        .ok_or("No client_secret in ADC")?;

    let body = serde_json::json!({
        "client_id": client_id,
        "client_secret": client_secret,
        "refresh_token": refresh_token,
        "grant_type": "refresh_token",
    });

    let resp: TokenRefreshResponse = http::post_json(
        "https://oauth2.googleapis.com/token",
        &body,
        None,
    )?;

    resp.access_token.ok_or("No access_token in refresh response".into())
}

// ── Public API ──────────────────────────────────────────────────────────────

pub fn fetch_vertex_ai_analytics() -> ProviderAnalytics {
    let now = Utc::now().to_rfc3339();

    let adc = match read_adc() {
        Ok(a) => a,
        Err(e) => {
            return ProviderAnalytics {
                provider_id: "vertex-ai".into(),
                provider_name: "Vertex AI".into(),
                status: ProviderStatus {
                    provider_id: "vertex-ai".into(),
                    provider_name: "Vertex AI".into(),
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

    let project = read_project();

    // Try to refresh token to verify credentials are valid
    let token_valid = refresh_access_token(&adc).is_ok();

    let mut extra = HashMap::new();
    if let Some(ref proj) = project {
        extra.insert("project_id".into(), serde_json::Value::String(proj.clone()));
    }
    if let Some(ref quota_proj) = adc.quota_project_id {
        extra.insert("quota_project_id".into(), serde_json::Value::String(quota_proj.clone()));
    }
    if let Some(ref cred_type) = adc.cred_type {
        extra.insert("credential_type".into(), serde_json::Value::String(cred_type.clone()));
    }
    extra.insert("token_refresh_valid".into(), serde_json::Value::Bool(token_valid));
    extra.insert("adc_path".into(), serde_json::Value::String(adc_path().to_string_lossy().to_string()));

    // v1: No Cloud Monitoring API calls — just connection status + project info
    let connection_method = if token_valid { "oauth-auto" } else { "local-file" };

    ProviderAnalytics {
        provider_id: "vertex-ai".into(),
        provider_name: "Vertex AI".into(),
        status: ProviderStatus {
            provider_id: "vertex-ai".into(),
            provider_name: "Vertex AI".into(),
            connected: true,
            connection_method: connection_method.into(),
            account_email: None,
            plan_name: None,
            org_name: project,
            error: if !token_valid {
                Some("Token refresh failed — credentials may be stale".into())
            } else {
                None
            },
        },
        rate_limits: vec![],
        credit_usage: None,
        token_counts: None,
        limit_state: None,
        extra,
        fetched_at: now,
    }
}

pub fn check_connection() -> ProviderStatus {
    match read_adc() {
        Ok(adc) => {
            let project = read_project();
            let token_valid = refresh_access_token(&adc).is_ok();
            ProviderStatus {
                provider_id: "vertex-ai".into(),
                provider_name: "Vertex AI".into(),
                connected: true,
                connection_method: if token_valid { "oauth-auto".into() } else { "local-file".into() },
                account_email: None,
                plan_name: None,
                org_name: project,
                error: None,
            }
        }
        Err(e) => ProviderStatus {
            provider_id: "vertex-ai".into(),
            provider_name: "Vertex AI".into(),
            connected: false, connection_method: "none".into(),
            account_email: None, plan_name: None, org_name: None, error: Some(e),
        },
    }
}
