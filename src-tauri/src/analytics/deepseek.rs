//! DeepSeek analytics provider.
//! Auth: API key bearer only.
//! API: api.deepseek.com/user/balance (the only account endpoint).
//! Balance-only — no usage-history or rate-limit endpoint exists, so there is
//! no LimitState ladder for this provider.
//
// TODO(pricing): needs verified per-model rates before wiring cost engine.

use crate::analytics::http::{self, HttpCallError};
use crate::analytics::token_store;
use crate::analytics::types::*;
use chrono::Utc;
use serde::Deserialize;
use std::collections::HashMap;

const PROVIDER_ID: &str = "deepseek";
const PROVIDER_NAME: &str = "DeepSeek";
const BALANCE_URL: &str = "https://api.deepseek.com/user/balance";

// ── API response types ──────────────────────────────────────────────────────
// Monetary values are DECIMAL STRINGS (e.g. "110.00"), not numbers.

#[derive(Deserialize, Debug)]
struct DeepSeekBalanceInfo {
    currency: Option<String>,
    total_balance: Option<String>,
    granted_balance: Option<String>,
    topped_up_balance: Option<String>,
}

#[derive(Deserialize, Debug)]
struct DeepSeekBalanceResponse {
    is_available: Option<bool>,
    balance_infos: Option<Vec<DeepSeekBalanceInfo>>,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn resolve_token() -> Result<(String, String), String> {
    if let Ok(Some(key)) = token_store::get_provider_token(PROVIDER_ID, "api-key") {
        return Ok((key, "api-key".into()));
    }
    Err("No DeepSeek API key configured".into())
}

/// Parse a DeepSeek decimal-string money value into f64. Non-numeric or missing
/// values fall back to 0.0.
fn parse_money(s: &Option<String>) -> f64 {
    s.as_deref()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .unwrap_or(0.0)
}

fn disconnected(method: &str, error: String, connected: bool) -> ProviderAnalytics {
    ProviderAnalytics {
        provider_id: PROVIDER_ID.into(),
        provider_name: PROVIDER_NAME.into(),
        status: ProviderStatus {
            provider_id: PROVIDER_ID.into(),
            provider_name: PROVIDER_NAME.into(),
            connected,
            connection_method: method.into(),
            account_email: None,
            plan_name: None,
            org_name: None,
            error: Some(error),
        },
        rate_limits: vec![],
        credit_usage: None,
        token_counts: None,
        limit_state: None,
        extra: HashMap::new(),
        fetched_at: Utc::now().to_rfc3339(),
    }
}

// ── Public API ──────────────────────────────────────────────────────────────

pub fn fetch_deepseek_analytics() -> ProviderAnalytics {
    let now = Utc::now().to_rfc3339();

    let (api_key, method) = match resolve_token() {
        Ok(t) => t,
        Err(e) => return disconnected("none", e, false),
    };

    let resp: Result<DeepSeekBalanceResponse, HttpCallError> =
        http::authed_get(BALANCE_URL, &api_key, None);

    let data = match resp {
        Ok(d) => d,
        Err(HttpCallError::Unsuccessful { status: 401, .. }) => {
            // Stored key rejected — surface as needs-token (disconnected).
            return disconnected("none", "DeepSeek API key was rejected (401)".into(), false);
        }
        Err(e) => {
            // Other errors: keep connection but report the failure.
            return disconnected(&method, e.to_string(), true);
        }
    };

    // Account explicitly flagged unavailable (e.g. suspended / no balance).
    if data.is_available == Some(false) {
        return disconnected(&method, "DeepSeek account is not available".into(), false);
    }

    let mut extra: HashMap<String, serde_json::Value> = HashMap::new();
    let mut credit_usage = None;

    if let Some(ref infos) = data.balance_infos {
        for (idx, info) in infos.iter().enumerate() {
            let currency = info.currency.clone().unwrap_or_else(|| "USD".into());
            let total = parse_money(&info.total_balance);
            let granted = parse_money(&info.granted_balance);
            let topped_up = parse_money(&info.topped_up_balance);

            // Primary card = first balance_info; surface its returned currency.
            if idx == 0 {
                credit_usage = Some(CreditUsage {
                    provider_id: PROVIDER_ID.into(),
                    used: total,
                    limit: None,
                    remaining: total,
                    currency: currency.clone(),
                    billing_cycle_end: None,
                    plan_name: Some("Available balance".into()),
                });
            }

            extra.insert(
                format!("balance_{}", currency.to_lowercase()),
                serde_json::json!({
                    "currency": currency,
                    "total_balance": total,
                    "granted_balance": granted,
                    "topped_up_balance": topped_up,
                }),
            );
        }
    }

    ProviderAnalytics {
        provider_id: PROVIDER_ID.into(),
        provider_name: PROVIDER_NAME.into(),
        status: ProviderStatus {
            provider_id: PROVIDER_ID.into(),
            provider_name: PROVIDER_NAME.into(),
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
        limit_state: None,
        extra,
        fetched_at: now,
    }
}

pub fn check_connection() -> ProviderStatus {
    match resolve_token() {
        Ok((_, method)) => ProviderStatus {
            provider_id: PROVIDER_ID.into(),
            provider_name: PROVIDER_NAME.into(),
            connected: true,
            connection_method: method,
            account_email: None,
            plan_name: None,
            org_name: None,
            error: None,
        },
        Err(e) => ProviderStatus {
            provider_id: PROVIDER_ID.into(),
            provider_name: PROVIDER_NAME.into(),
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
mod tests {
    use super::*;

    #[test]
    fn parses_decimal_string_money() {
        assert_eq!(parse_money(&Some("110.00".into())), 110.0);
        assert_eq!(parse_money(&Some("10.00".into())), 10.0);
        assert_eq!(parse_money(&Some("  3.50 ".into())), 3.5);
        assert_eq!(parse_money(&Some("0".into())), 0.0);
        assert_eq!(parse_money(&None), 0.0);
        assert_eq!(parse_money(&Some("not-a-number".into())), 0.0);
    }

    #[test]
    fn deserializes_multi_currency_balance() {
        let raw = r#"{
            "is_available": true,
            "balance_infos": [
                {"currency":"CNY","total_balance":"110.00","granted_balance":"10.00","topped_up_balance":"100.00"},
                {"currency":"USD","total_balance":"15.50","granted_balance":"0.00","topped_up_balance":"15.50"}
            ]
        }"#;
        let resp: DeepSeekBalanceResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(resp.is_available, Some(true));
        let infos = resp.balance_infos.unwrap();
        assert_eq!(infos.len(), 2);

        let cny = &infos[0];
        assert_eq!(cny.currency.as_deref(), Some("CNY"));
        assert_eq!(parse_money(&cny.total_balance), 110.0);
        assert_eq!(parse_money(&cny.granted_balance), 10.0);
        assert_eq!(parse_money(&cny.topped_up_balance), 100.0);

        let usd = &infos[1];
        assert_eq!(usd.currency.as_deref(), Some("USD"));
        assert_eq!(parse_money(&usd.total_balance), 15.5);
    }
}
