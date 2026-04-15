//! Droid (Factory) analytics provider.
//! Auth: bearer token from keychain
//! API: app.factory.ai/api/app/auth/me + /api/organization/subscription/usage

use crate::analytics::http;
use crate::analytics::token_store;
use crate::analytics::types::*;
use chrono::Utc;
use serde::Deserialize;
use std::collections::HashMap;

// ── API response types ──────────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
struct DroidAuthMe {
    email: Option<String>,
    name: Option<String>,
    #[serde(rename = "organizationName")]
    organization_name: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct TokenUsage {
    token_type: Option<String>,
    used: Option<f64>,
    allowance: Option<f64>,
    used_ratio: Option<f64>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct DroidUsageResponse {
    token_usages: Option<Vec<TokenUsage>>,
    billing_period_start: Option<String>,
    billing_period_end: Option<String>,
    plan_name: Option<String>,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn resolve_token() -> Result<(String, String), String> {
    if let Ok(Some(token)) = token_store::get_provider_token("droid", "bearer-token") {
        return Ok((token, "token-manual".into()));
    }
    Err("No Droid bearer token configured".into())
}

// ── Public API ──────────────────────────────────────────────────────────────

pub fn fetch_droid_analytics() -> ProviderAnalytics {
    let now = Utc::now().to_rfc3339();

    let (token, method) = match resolve_token() {
        Ok(t) => t,
        Err(e) => {
            return ProviderAnalytics {
                provider_id: "droid".into(),
                provider_name: "Droid (Factory)".into(),
                status: ProviderStatus {
                    provider_id: "droid".into(),
                    provider_name: "Droid (Factory)".into(),
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

    // Fetch auth/me
    let auth_me: Option<DroidAuthMe> = http::authed_get(
        "https://app.factory.ai/api/app/auth/me",
        &token,
        None,
    ).ok();

    // Fetch subscription usage
    let body = serde_json::json!({"useCache": true});
    let usage: Result<DroidUsageResponse, String> = http::authed_post(
        "https://app.factory.ai/api/organization/subscription/usage",
        &token,
        &body,
        None,
    );

    match usage {
        Ok(data) => {
            let mut rate_limits = Vec::new();
            let mut extra = HashMap::new();

            if let Some(ref token_usages) = data.token_usages {
                for tu in token_usages {
                    let token_type = tu.token_type.as_deref().unwrap_or("unknown");
                    let label = match token_type {
                        "standard" => "Standard Tokens".to_string(),
                        "premium" => "Premium Tokens".to_string(),
                        other => format!("{} Tokens", other),
                    };

                    let used = tu.used.unwrap_or(0.0);
                    let allowance = tu.allowance.unwrap_or(0.0);
                    let used_ratio = tu.used_ratio.unwrap_or(
                        if allowance > 0.0 { used / allowance } else { 0.0 }
                    );
                    let used_pct = (used_ratio * 100.0).min(100.0);

                    rate_limits.push(RateLimitWindow {
                        provider_id: "droid".into(),
                        label: label.clone(),
                        used_percent: used_pct,
                        remaining_percent: (100.0 - used_pct).max(0.0),
                        resets_at: data.billing_period_end.clone(),
                        resets_in_seconds: None,
                        window_seconds: None,
                    });

                    extra.insert(
                        format!("{}_tokens", token_type),
                        serde_json::json!({
                            "used": used,
                            "allowance": allowance,
                            "used_ratio": used_ratio,
                        }),
                    );
                }
            }

            if let Some(ref start) = data.billing_period_start {
                extra.insert("billing_period_start".into(), serde_json::Value::String(start.clone()));
            }
            if let Some(ref end) = data.billing_period_end {
                extra.insert("billing_period_end".into(), serde_json::Value::String(end.clone()));
            }

            let email = auth_me.as_ref().and_then(|a| a.email.clone());
            let org_name = auth_me.as_ref().and_then(|a| a.organization_name.clone());
            let plan_name = data.plan_name.clone();

            ProviderAnalytics {
                provider_id: "droid".into(),
                provider_name: "Droid (Factory)".into(),
                status: ProviderStatus {
                    provider_id: "droid".into(),
                    provider_name: "Droid (Factory)".into(),
                    connected: true,
                    connection_method: method,
                    account_email: email,
                    plan_name,
                    org_name,
                    error: None,
                },
                rate_limits,
                credit_usage: None,
                token_counts: None,
                extra,
                fetched_at: now,
            }
        }
        Err(e) => {
            let email = auth_me.as_ref().and_then(|a| a.email.clone());
            ProviderAnalytics {
                provider_id: "droid".into(),
                provider_name: "Droid (Factory)".into(),
                status: ProviderStatus {
                    provider_id: "droid".into(),
                    provider_name: "Droid (Factory)".into(),
                    connected: true,
                    connection_method: method,
                    account_email: email, plan_name: None, org_name: None,
                    error: Some(e),
                },
                rate_limits: vec![], credit_usage: None, token_counts: None,
                extra: HashMap::new(), fetched_at: now,
            }
        }
    }
}

pub fn check_connection() -> ProviderStatus {
    match resolve_token() {
        Ok((_, method)) => ProviderStatus {
            provider_id: "droid".into(),
            provider_name: "Droid (Factory)".into(),
            connected: true, connection_method: method,
            account_email: None, plan_name: None, org_name: None, error: None,
        },
        Err(e) => ProviderStatus {
            provider_id: "droid".into(),
            provider_name: "Droid (Factory)".into(),
            connected: false, connection_method: "none".into(),
            account_email: None, plan_name: None, org_name: None, error: Some(e),
        },
    }
}
