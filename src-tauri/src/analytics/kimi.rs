//! Kimi + Kimi K2 analytics providers.
//! Kimi: auth token + POST billing API
//! Kimi K2: API key + GET credits API

use crate::analytics::http;
use crate::analytics::token_store;
use crate::analytics::types::*;
use chrono::Utc;
use serde::Deserialize;
use std::collections::HashMap;

// ── Kimi API response types ─────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
struct KimiUsageItem {
    scope: Option<String>,
    used: Option<f64>,
    limit: Option<f64>,
    reset_at: Option<String>,
}

#[derive(Deserialize, Debug)]
struct KimiUsageResponse {
    usages: Option<Vec<KimiUsageItem>>,
}

// ── Kimi K2 API response types ──────────────────────────────────────────────

#[derive(Deserialize, Debug)]
struct KimiK2CreditsResponse {
    credits_consumed: Option<f64>,
    credits_remaining: Option<f64>,
    credits_total: Option<f64>,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn resolve_kimi_token() -> Result<(String, String), String> {
    if let Ok(Some(token)) = token_store::get_provider_token("kimi", "auth-token") {
        return Ok((token, "token-manual".into()));
    }
    Err("No Kimi auth token configured".into())
}

fn resolve_k2_token() -> Result<(String, String), String> {
    if let Ok(Some(key)) = token_store::get_provider_token("kimi-k2", "api-key") {
        return Ok((key, "api-key".into()));
    }
    Err("No Kimi K2 API key configured".into())
}

// ── Public API: Kimi ────────────────────────────────────────────────────────

pub fn fetch_kimi_analytics() -> ProviderAnalytics {
    let now = Utc::now().to_rfc3339();

    let (token, method) = match resolve_kimi_token() {
        Ok(t) => t,
        Err(e) => {
            return ProviderAnalytics {
                provider_id: "kimi".into(),
                provider_name: "Kimi".into(),
                status: ProviderStatus {
                    provider_id: "kimi".into(),
                    provider_name: "Kimi".into(),
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

    let body = serde_json::json!({"scope": ["FEATURE_CODING"]});
    let resp: Result<KimiUsageResponse, String> = http::authed_post(
        "https://www.kimi.com/apiv2/kimi.gateway.billing.v1.BillingService/GetUsages",
        &token,
        &body,
        None,
    );

    match resp {
        Ok(data) => {
            let mut rate_limits = Vec::new();
            let mut extra = HashMap::new();
            let mut credit_usage = None;

            if let Some(ref usages) = data.usages {
                for item in usages {
                    let scope = item.scope.as_deref().unwrap_or("unknown");
                    let used = item.used.unwrap_or(0.0);
                    let limit = item.limit.unwrap_or(0.0);

                    if limit > 0.0 {
                        let used_pct = (used / limit * 100.0).min(100.0);
                        rate_limits.push(RateLimitWindow {
                            provider_id: "kimi".into(),
                            label: scope.to_string(),
                            used_percent: used_pct,
                            remaining_percent: (100.0 - used_pct).max(0.0),
                            resets_at: item.reset_at.clone(),
                            resets_in_seconds: None,
                            window_seconds: None,
                        });
                    }

                    credit_usage = Some(CreditUsage {
                        provider_id: "kimi".into(),
                        used,
                        limit: Some(limit),
                        remaining: (limit - used).max(0.0),
                        currency: "credits".into(),
                        billing_cycle_end: item.reset_at.clone(),
                        plan_name: None,
                    });

                    extra.insert(
                        format!("scope_{}", scope),
                        serde_json::json!({"used": used, "limit": limit}),
                    );
                }
            }

            ProviderAnalytics {
                provider_id: "kimi".into(),
                provider_name: "Kimi".into(),
                status: ProviderStatus {
                    provider_id: "kimi".into(),
                    provider_name: "Kimi".into(),
                    connected: true,
                    connection_method: method,
                    account_email: None,
                    plan_name: None,
                    org_name: None,
                    error: None,
                },
                rate_limits,
                credit_usage,
                token_counts: None,
                extra,
                fetched_at: now,
            }
        }
        Err(e) => ProviderAnalytics {
            provider_id: "kimi".into(),
            provider_name: "Kimi".into(),
            status: ProviderStatus {
                provider_id: "kimi".into(),
                provider_name: "Kimi".into(),
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

// ── Public API: Kimi K2 ─────────────────────────────────────────────────────

pub fn fetch_kimi_k2_analytics() -> ProviderAnalytics {
    let now = Utc::now().to_rfc3339();

    let (api_key, method) = match resolve_k2_token() {
        Ok(t) => t,
        Err(e) => {
            return ProviderAnalytics {
                provider_id: "kimi-k2".into(),
                provider_name: "Kimi K2".into(),
                status: ProviderStatus {
                    provider_id: "kimi-k2".into(),
                    provider_name: "Kimi K2".into(),
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

    let resp: Result<KimiK2CreditsResponse, String> = http::authed_get(
        "https://kimi-k2.ai/api/user/credits",
        &api_key,
        None,
    );

    match resp {
        Ok(data) => {
            let consumed = data.credits_consumed.unwrap_or(0.0);
            let remaining = data.credits_remaining.unwrap_or(0.0);
            let total = data.credits_total.unwrap_or(consumed + remaining);

            let credit_usage = Some(CreditUsage {
                provider_id: "kimi-k2".into(),
                used: consumed,
                limit: Some(total),
                remaining,
                currency: "credits".into(),
                billing_cycle_end: None,
                plan_name: None,
            });

            ProviderAnalytics {
                provider_id: "kimi-k2".into(),
                provider_name: "Kimi K2".into(),
                status: ProviderStatus {
                    provider_id: "kimi-k2".into(),
                    provider_name: "Kimi K2".into(),
                    connected: true,
                    connection_method: method,
                    account_email: None,
                    plan_name: None,
                    org_name: None,
                    error: None,
                },
                rate_limits: vec![],
                credit_usage,
                token_counts: None,
                extra: HashMap::new(),
                fetched_at: now,
            }
        }
        Err(e) => ProviderAnalytics {
            provider_id: "kimi-k2".into(),
            provider_name: "Kimi K2".into(),
            status: ProviderStatus {
                provider_id: "kimi-k2".into(),
                provider_name: "Kimi K2".into(),
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
    if let Ok((_, method)) = resolve_kimi_token() {
        return ProviderStatus {
            provider_id: "kimi".into(),
            provider_name: "Kimi".into(),
            connected: true, connection_method: method,
            account_email: None, plan_name: None, org_name: None, error: None,
        };
    }
    ProviderStatus {
        provider_id: "kimi".into(),
        provider_name: "Kimi".into(),
        connected: false, connection_method: "none".into(),
        account_email: None, plan_name: None, org_name: None,
        error: Some("No Kimi credentials configured".into()),
    }
}

pub fn check_k2_connection() -> ProviderStatus {
    if let Ok((_, method)) = resolve_k2_token() {
        return ProviderStatus {
            provider_id: "kimi-k2".into(),
            provider_name: "Kimi K2".into(),
            connected: true, connection_method: method,
            account_email: None, plan_name: None, org_name: None, error: None,
        };
    }
    ProviderStatus {
        provider_id: "kimi-k2".into(),
        provider_name: "Kimi K2".into(),
        connected: false, connection_method: "none".into(),
        account_email: None, plan_name: None, org_name: None,
        error: Some("No Kimi K2 API key configured".into()),
    }
}
