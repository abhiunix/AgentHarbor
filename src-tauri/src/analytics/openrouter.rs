//! OpenRouter analytics provider.
//! Auth: API key from keychain
//! API: openrouter.ai/api/v1/credits + /api/v1/key

use crate::analytics::http;
use crate::analytics::token_store;
use crate::analytics::types::*;
use chrono::Utc;
use serde::Deserialize;
use std::collections::HashMap;

// ── API response types ──────────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
struct CreditsData {
    total_credits: Option<f64>,
    total_usage: Option<f64>,
}

#[derive(Deserialize, Debug)]
struct CreditsResponse {
    data: Option<CreditsData>,
}

#[derive(Deserialize, Debug)]
struct RateLimit {
    requests: Option<i64>,
    interval: Option<String>,
}

#[derive(Deserialize, Debug)]
struct KeyData {
    label: Option<String>,
    limit: Option<f64>,
    usage: Option<f64>,
    is_free_tier: Option<bool>,
    rate_limit: Option<RateLimit>,
}

#[derive(Deserialize, Debug)]
struct KeyResponse {
    data: Option<KeyData>,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn resolve_token() -> Result<(String, String), String> {
    if let Ok(Some(key)) = token_store::get_provider_token("openrouter", "api-key") {
        return Ok((key, "api-key".into()));
    }
    Err("No OpenRouter API key configured".into())
}

// ── Public API ──────────────────────────────────────────────────────────────

pub fn fetch_openrouter_analytics() -> ProviderAnalytics {
    let now = Utc::now().to_rfc3339();

    let (api_key, method) = match resolve_token() {
        Ok(t) => t,
        Err(e) => {
            return ProviderAnalytics {
                provider_id: "openrouter".into(),
                provider_name: "OpenRouter".into(),
                status: ProviderStatus {
                    provider_id: "openrouter".into(),
                    provider_name: "OpenRouter".into(),
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

    // Fetch credits
    let credits: Option<CreditsResponse> = http::authed_get(
        "https://openrouter.ai/api/v1/credits",
        &api_key,
        None,
    ).ok();

    // Fetch key info
    let key_info: Option<KeyResponse> = http::authed_get(
        "https://openrouter.ai/api/v1/key",
        &api_key,
        None,
    ).ok();

    // Build credit usage from credits endpoint
    let credit_usage = credits.as_ref().and_then(|c| {
        let data = c.data.as_ref()?;
        let total = data.total_credits.unwrap_or(0.0);
        let used = data.total_usage.unwrap_or(0.0);
        Some(CreditUsage {
            provider_id: "openrouter".into(),
            used,
            limit: Some(total),
            remaining: (total - used).max(0.0),
            currency: "USD".into(),
            billing_cycle_end: None,
            plan_name: None,
        })
    });

    let mut extra = HashMap::new();
    let mut rate_limits = Vec::new();

    if let Some(ref ki) = key_info {
        if let Some(ref data) = ki.data {
            if let Some(ref label) = data.label {
                extra.insert("key_label".into(), serde_json::Value::String(label.clone()));
            }
            if let Some(free) = data.is_free_tier {
                extra.insert("is_free_tier".into(), serde_json::Value::Bool(free));
            }
            if let Some(limit) = data.limit {
                extra.insert("key_limit".into(), serde_json::Value::from(limit));
            }
            if let Some(usage) = data.usage {
                extra.insert("key_usage".into(), serde_json::Value::from(usage));
            }

            // Per-key rate limits
            if let Some(ref rl) = data.rate_limit {
                if let Some(requests) = rl.requests {
                    extra.insert("rate_limit_requests".into(), serde_json::Value::from(requests));
                }
                if let Some(ref interval) = rl.interval {
                    extra.insert("rate_limit_interval".into(), serde_json::Value::String(interval.clone()));
                }
            }

            // If per-key limit and usage are available, create a rate limit window
            if let (Some(limit), Some(usage)) = (data.limit, data.usage) {
                if limit > 0.0 {
                    let used_pct = (usage / limit * 100.0).min(100.0);
                    rate_limits.push(RateLimitWindow {
                        provider_id: "openrouter".into(),
                        label: "Key Budget".into(),
                        used_percent: used_pct,
                        remaining_percent: (100.0 - used_pct).max(0.0),
                        resets_at: None,
                        resets_in_seconds: None,
                        window_seconds: None,
                    });
                }
            }
        }
    }

    let plan_name = key_info.as_ref()
        .and_then(|ki| ki.data.as_ref())
        .and_then(|d| {
            if d.is_free_tier == Some(true) {
                Some("Free".into())
            } else {
                Some("Paid".into())
            }
        });

    ProviderAnalytics {
        provider_id: "openrouter".into(),
        provider_name: "OpenRouter".into(),
        status: ProviderStatus {
            provider_id: "openrouter".into(),
            provider_name: "OpenRouter".into(),
            connected: true,
            connection_method: method,
            account_email: None,
            plan_name,
            org_name: None,
            error: None,
        },
        rate_limits,
        credit_usage,
        token_counts: None,
        limit_state: None,
        extra,
        fetched_at: now,
    }
}

pub fn check_connection() -> ProviderStatus {
    match resolve_token() {
        Ok((_, method)) => ProviderStatus {
            provider_id: "openrouter".into(),
            provider_name: "OpenRouter".into(),
            connected: true, connection_method: method,
            account_email: None, plan_name: None, org_name: None, error: None,
        },
        Err(e) => ProviderStatus {
            provider_id: "openrouter".into(),
            provider_name: "OpenRouter".into(),
            connected: false, connection_method: "none".into(),
            account_email: None, plan_name: None, org_name: None, error: Some(e),
        },
    }
}
