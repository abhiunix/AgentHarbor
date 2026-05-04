//! Amp analytics provider.
//! Auth: session cookie from keychain
//! API: ampcode.com/settings → parse freeTierUsage from HTML/JS

use crate::analytics::http;
use crate::analytics::token_store;
use crate::analytics::types::*;
use chrono::Utc;
use std::collections::HashMap;

// ── Helpers ─────────────────────────────────────────────────────────────────

fn resolve_token() -> Result<(String, String), String> {
    if let Ok(Some(cookie)) = token_store::get_provider_token("amp", "session-cookie") {
        return Ok((cookie, "cookie".into()));
    }
    Err("No Amp session cookie configured".into())
}

/// Parse the freeTierUsage JS object from settings HTML.
/// Looks for a pattern like: freeTierUsage: { quota: 100, used: 42, hourlyReplenishment: 5, windowHours: 24 }
fn parse_free_tier_usage(html: &str) -> Option<(f64, f64, f64, f64)> {
    // Find "freeTierUsage" in the HTML
    let start_marker = "freeTierUsage";
    let start_idx = html.find(start_marker)?;
    let after = &html[start_idx..];

    // Find the opening brace
    let brace_idx = after.find('{')?;
    let after_brace = &after[brace_idx..];

    // Find the closing brace
    let end_idx = after_brace.find('}')?;
    let obj_str = &after_brace[..=end_idx];

    let quota = extract_number(obj_str, "quota")?;
    let used = extract_number(obj_str, "used")?;
    let hourly = extract_number(obj_str, "hourlyReplenishment").unwrap_or(0.0);
    let window = extract_number(obj_str, "windowHours").unwrap_or(24.0);

    Some((quota, used, hourly, window))
}

/// Extract a numeric value for a given key from a JS object string.
/// e.g., from "{ quota: 100, used: 42 }" extract 100 for key "quota"
fn extract_number(obj: &str, key: &str) -> Option<f64> {
    let key_idx = obj.find(key)?;
    let after_key = &obj[key_idx + key.len()..];

    // Skip past the colon and whitespace
    let colon_idx = after_key.find(':')?;
    let after_colon = after_key[colon_idx + 1..].trim_start();

    // Read digits (and optional decimal point)
    let mut num_str = String::new();
    for ch in after_colon.chars() {
        if ch.is_ascii_digit() || ch == '.' || ch == '-' {
            num_str.push(ch);
        } else if !num_str.is_empty() {
            break;
        }
    }

    num_str.parse::<f64>().ok()
}

// ── Public API ──────────────────────────────────────────────────────────────

pub fn fetch_amp_analytics() -> ProviderAnalytics {
    let now = Utc::now().to_rfc3339();

    let (cookie, method) = match resolve_token() {
        Ok(t) => t,
        Err(e) => {
            return ProviderAnalytics {
                provider_id: "amp".into(),
                provider_name: "Amp".into(),
                status: ProviderStatus {
                    provider_id: "amp".into(),
                    provider_name: "Amp".into(),
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

    // Fetch settings page HTML
    let html = match http::cookie_get_text("https://ampcode.com/settings", &cookie) {
        Ok(h) => h,
        Err(e) => {
            return ProviderAnalytics {
                provider_id: "amp".into(),
                provider_name: "Amp".into(),
                status: ProviderStatus {
                    provider_id: "amp".into(),
                    provider_name: "Amp".into(),
                    connected: true,
                    connection_method: method,
                    account_email: None, plan_name: None, org_name: None,
                    error: Some(format!("Failed to fetch settings: {}", e)),
                },
                rate_limits: vec![], credit_usage: None, token_counts: None,
                limit_state: None,
                extra: HashMap::new(), fetched_at: now,
            };
        }
    };

    match parse_free_tier_usage(&html) {
        Some((quota, used, hourly_replenishment, window_hours)) => {
            let used_pct = if quota > 0.0 { (used / quota * 100.0).min(100.0) } else { 0.0 };
            let window_seconds = (window_hours * 3600.0) as i64;

            let rate_limits = vec![RateLimitWindow {
                provider_id: "amp".into(),
                label: "Free Tier Quota".into(),
                used_percent: used_pct,
                remaining_percent: (100.0 - used_pct).max(0.0),
                resets_at: None,
                resets_in_seconds: None,
                window_seconds: Some(window_seconds),
            }];

            let credit_usage = Some(CreditUsage {
                provider_id: "amp".into(),
                used,
                limit: Some(quota),
                remaining: (quota - used).max(0.0),
                currency: "credits".into(),
                billing_cycle_end: None,
                plan_name: Some("Free".into()),
            });

            let mut extra = HashMap::new();
            extra.insert("quota".into(), serde_json::Value::from(quota));
            extra.insert("used".into(), serde_json::Value::from(used));
            extra.insert("hourly_replenishment".into(), serde_json::Value::from(hourly_replenishment));
            extra.insert("window_hours".into(), serde_json::Value::from(window_hours));

            ProviderAnalytics {
                provider_id: "amp".into(),
                provider_name: "Amp".into(),
                status: ProviderStatus {
                    provider_id: "amp".into(),
                    provider_name: "Amp".into(),
                    connected: true,
                    connection_method: method,
                    account_email: None,
                    plan_name: Some("Free".into()),
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
        None => ProviderAnalytics {
            provider_id: "amp".into(),
            provider_name: "Amp".into(),
            status: ProviderStatus {
                provider_id: "amp".into(),
                provider_name: "Amp".into(),
                connected: true,
                connection_method: method,
                account_email: None, plan_name: None, org_name: None,
                error: Some("Could not parse freeTierUsage from settings page".into()),
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
            provider_id: "amp".into(),
            provider_name: "Amp".into(),
            connected: true, connection_method: method,
            account_email: None, plan_name: None, org_name: None, error: None,
        },
        Err(e) => ProviderStatus {
            provider_id: "amp".into(),
            provider_name: "Amp".into(),
            connected: false, connection_method: "none".into(),
            account_email: None, plan_name: None, org_name: None, error: Some(e),
        },
    }
}
