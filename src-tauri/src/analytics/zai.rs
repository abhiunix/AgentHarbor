//! z.ai analytics provider.
//! Auth: API key from keychain
//! API: api.z.ai/api/monitor/usage/quota/limit

use crate::analytics::http;
use crate::analytics::token_store;
use crate::analytics::types::*;
use chrono::Utc;
use serde::Deserialize;
use std::collections::HashMap;

// ── API response types ──────────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ZaiLimitEntry {
    limit_type: Option<String>,
    usage: Option<f64>,
    limit: Option<f64>,
    remaining: Option<f64>,
    percentage: Option<f64>,
    next_reset_time: Option<String>,
}

#[derive(Deserialize, Debug)]
struct ZaiQuotaResponse {
    limits: Option<Vec<ZaiLimitEntry>>,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn resolve_token() -> Result<(String, String), String> {
    if let Ok(Some(key)) = token_store::get_provider_token("zai", "api-key") {
        return Ok((key, "api-key".into()));
    }
    Err("No z.ai API key configured".into())
}

// ── Public API ──────────────────────────────────────────────────────────────

pub fn fetch_zai_analytics() -> ProviderAnalytics {
    let now = Utc::now().to_rfc3339();

    let (api_key, method) = match resolve_token() {
        Ok(t) => t,
        Err(e) => {
            return ProviderAnalytics {
                provider_id: "zai".into(),
                provider_name: "z.ai".into(),
                status: ProviderStatus {
                    provider_id: "zai".into(),
                    provider_name: "z.ai".into(),
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

    let resp: Result<ZaiQuotaResponse, String> = http::authed_get(
        "https://api.z.ai/api/monitor/usage/quota/limit",
        &api_key,
        None,
    )
    .map_err(String::from);

    match resp {
        Ok(data) => {
            let mut rate_limits = Vec::new();
            let mut extra = HashMap::new();

            if let Some(ref limits) = data.limits {
                for entry in limits {
                    let limit_type = entry.limit_type.as_deref().unwrap_or("UNKNOWN");

                    let label = match limit_type {
                        "TOKENS_LIMIT" => "Token Quota".to_string(),
                        "TIME_LIMIT" => "Time Limit".to_string(),
                        other => other.to_string(),
                    };

                    let used = entry.usage.unwrap_or(0.0);
                    let limit = entry.limit.unwrap_or(0.0);
                    let remaining = entry.remaining.unwrap_or(0.0);
                    let percentage = entry.percentage.unwrap_or(
                        if limit > 0.0 { used / limit * 100.0 } else { 0.0 }
                    );

                    rate_limits.push(RateLimitWindow {
                        provider_id: "zai".into(),
                        label: label.clone(),
                        used_percent: percentage,
                        remaining_percent: (100.0 - percentage).max(0.0),
                        resets_at: entry.next_reset_time.clone(),
                        resets_in_seconds: None,
                        window_seconds: None,
                    });

                    extra.insert(
                        format!("{}_usage", limit_type.to_lowercase()),
                        serde_json::json!({
                            "used": used,
                            "limit": limit,
                            "remaining": remaining,
                            "percentage": percentage,
                        }),
                    );
                }
            }

            ProviderAnalytics {
                provider_id: "zai".into(),
                provider_name: "z.ai".into(),
                status: ProviderStatus {
                    provider_id: "zai".into(),
                    provider_name: "z.ai".into(),
                    connected: true,
                    connection_method: method,
                    account_email: None,
                    plan_name: None,
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
        Err(e) => ProviderAnalytics {
            provider_id: "zai".into(),
            provider_name: "z.ai".into(),
            status: ProviderStatus {
                provider_id: "zai".into(),
                provider_name: "z.ai".into(),
                connected: true,
                connection_method: method,
                account_email: None, plan_name: None, org_name: None,
                error: Some(e),
            },
            rate_limits: vec![], credit_usage: None, token_counts: None,
            limit_state: None,
            extra: HashMap::new(), fetched_at: now,
        },
    }
}

pub fn check_connection() -> ProviderStatus {
    match resolve_token() {
        Ok((_, method)) => ProviderStatus {
            provider_id: "zai".into(),
            provider_name: "z.ai".into(),
            connected: true, connection_method: method,
            account_email: None, plan_name: None, org_name: None, error: None,
        },
        Err(e) => ProviderStatus {
            provider_id: "zai".into(),
            provider_name: "z.ai".into(),
            connected: false, connection_method: "none".into(),
            account_email: None, plan_name: None, org_name: None, error: Some(e),
        },
    }
}
