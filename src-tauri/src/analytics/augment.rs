//! Augment analytics provider.
//! Auth: session token from keychain (cookie auth)
//! API: app.augmentcode.com/api/credits + /api/subscription

use crate::analytics::http;
use crate::analytics::token_store;
use crate::analytics::types::*;
use chrono::Utc;
use serde::Deserialize;
use std::collections::HashMap;

// ── API response types ──────────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct AugmentCreditsResponse {
    usage_units_remaining: Option<f64>,
    usage_units_consumed_this_billing_cycle: Option<f64>,
    usage_units_total: Option<f64>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct AugmentSubscriptionResponse {
    plan_name: Option<String>,
    billing_period_end: Option<String>,
    billing_period_start: Option<String>,
    status: Option<String>,
    email: Option<String>,
    team_name: Option<String>,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn resolve_token() -> Result<(String, String), String> {
    if let Ok(Some(token)) = token_store::get_provider_token("augment", "session-token") {
        return Ok((token, "cookie".into()));
    }
    Err("No Augment session token configured".into())
}

fn cookie_header(token: &str) -> String {
    format!("session={}", token)
}

// ── Public API ──────────────────────────────────────────────────────────────

pub fn fetch_augment_analytics() -> ProviderAnalytics {
    let now = Utc::now().to_rfc3339();

    let (token, method) = match resolve_token() {
        Ok(t) => t,
        Err(e) => {
            return ProviderAnalytics {
                provider_id: "augment".into(),
                provider_name: "Augment".into(),
                status: ProviderStatus {
                    provider_id: "augment".into(),
                    provider_name: "Augment".into(),
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

    let cookie = cookie_header(&token);

    // Fetch credits
    let credits: Option<AugmentCreditsResponse> = http::cookie_get(
        "https://app.augmentcode.com/api/credits",
        &cookie,
        None,
    ).ok();

    // Fetch subscription
    let subscription: Option<AugmentSubscriptionResponse> = http::cookie_get(
        "https://app.augmentcode.com/api/subscription",
        &cookie,
        None,
    ).ok();

    let remaining = credits.as_ref().and_then(|c| c.usage_units_remaining).unwrap_or(0.0);
    let consumed = credits.as_ref().and_then(|c| c.usage_units_consumed_this_billing_cycle).unwrap_or(0.0);
    let total = credits.as_ref().and_then(|c| c.usage_units_total).unwrap_or(consumed + remaining);

    let plan_name = subscription.as_ref().and_then(|s| s.plan_name.clone());
    let billing_end = subscription.as_ref().and_then(|s| s.billing_period_end.clone());

    let credit_usage = Some(CreditUsage {
        provider_id: "augment".into(),
        used: consumed,
        limit: Some(total),
        remaining,
        currency: "credits".into(),
        billing_cycle_end: billing_end.clone(),
        plan_name: plan_name.clone(),
    });

    let mut extra = HashMap::new();
    if let Some(ref sub) = subscription {
        if let Some(ref start) = sub.billing_period_start {
            extra.insert("billing_period_start".into(), serde_json::Value::String(start.clone()));
        }
        if let Some(ref status) = sub.status {
            extra.insert("subscription_status".into(), serde_json::Value::String(status.clone()));
        }
    }

    let email = subscription.as_ref().and_then(|s| s.email.clone());
    let org_name = subscription.as_ref().and_then(|s| s.team_name.clone());

    // Build rate limit if we have total
    let mut rate_limits = Vec::new();
    if total > 0.0 {
        let used_pct = (consumed / total * 100.0).min(100.0);
        rate_limits.push(RateLimitWindow {
            provider_id: "augment".into(),
            label: "Usage Credits".into(),
            used_percent: used_pct,
            remaining_percent: (100.0 - used_pct).max(0.0),
            resets_at: billing_end,
            resets_in_seconds: None,
            window_seconds: None,
        });
    }

    ProviderAnalytics {
        provider_id: "augment".into(),
        provider_name: "Augment".into(),
        status: ProviderStatus {
            provider_id: "augment".into(),
            provider_name: "Augment".into(),
            connected: true,
            connection_method: method,
            account_email: email,
            plan_name,
            org_name,
            error: None,
        },
        rate_limits,
        credit_usage,
        token_counts: None,
        extra,
        fetched_at: now,
    }
}

pub fn check_connection() -> ProviderStatus {
    match resolve_token() {
        Ok((_, method)) => ProviderStatus {
            provider_id: "augment".into(),
            provider_name: "Augment".into(),
            connected: true, connection_method: method,
            account_email: None, plan_name: None, org_name: None, error: None,
        },
        Err(e) => ProviderStatus {
            provider_id: "augment".into(),
            provider_name: "Augment".into(),
            connected: false, connection_method: "none".into(),
            account_email: None, plan_name: None, org_name: None, error: Some(e),
        },
    }
}
