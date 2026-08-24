//! Moonshot (Kimi Open Platform) analytics provider.
//! API key + GET account balance (balance-only, no rate-limit API).
//! Distinct from the `kimi` provider, which reads the kimi.com consumer
//! subscription; this reads the Moonshot developer platform account.
//
// TODO(pricing): needs verified per-model (kimi-k3 / k2.7-code) rates before
// wiring a cost engine.

use crate::analytics::http::{self, HttpCallError};
use crate::analytics::token_store;
use crate::analytics::types::*;
use chrono::Utc;
use serde::Deserialize;
use std::collections::HashMap;

const PROVIDER_ID: &str = "moonshot";
const PROVIDER_NAME: &str = "Moonshot";
const BALANCE_URL: &str = "https://api.moonshot.ai/v1/users/me/balance";

// Balance numbers are USD floats.
#[derive(Deserialize, Debug)]
struct BalanceData {
    available_balance: Option<f64>,
    voucher_balance: Option<f64>,
    cash_balance: Option<f64>,
}

#[derive(Deserialize, Debug)]
struct BalanceResponse {
    #[allow(dead_code)]
    code: Option<i64>,
    data: Option<BalanceData>,
}

fn resolve_token() -> Result<(String, String), String> {
    if let Ok(Some(key)) = token_store::get_provider_token(PROVIDER_ID, "api-key") {
        return Ok((key, "api-key".into()));
    }
    Err("No Moonshot API key configured".into())
}

fn status(connected: bool, method: &str, error: Option<String>) -> ProviderStatus {
    ProviderStatus {
        provider_id: PROVIDER_ID.into(),
        provider_name: PROVIDER_NAME.into(),
        connected,
        connection_method: method.into(),
        account_email: None,
        plan_name: None,
        org_name: None,
        error,
    }
}

fn analytics(
    status: ProviderStatus,
    credit_usage: Option<CreditUsage>,
    extra: HashMap<String, serde_json::Value>,
) -> ProviderAnalytics {
    ProviderAnalytics {
        provider_id: PROVIDER_ID.into(),
        provider_name: PROVIDER_NAME.into(),
        status,
        rate_limits: vec![],
        credit_usage,
        token_counts: None,
        limit_state: None,
        extra,
        fetched_at: Utc::now().to_rfc3339(),
    }
}

pub fn check_connection() -> ProviderStatus {
    match resolve_token() {
        Ok((_, method)) => status(true, &method, None),
        Err(e) => status(false, "none", Some(e)),
    }
}

pub fn fetch_moonshot_analytics() -> ProviderAnalytics {
    let (api_key, method) = match resolve_token() {
        Ok(t) => t,
        Err(e) => return analytics(status(false, "none", Some(e)), None, HashMap::new()),
    };

    let resp: Result<BalanceResponse, HttpCallError> =
        http::authed_get(BALANCE_URL, &api_key, None);

    let data = match resp {
        Ok(d) => d,
        Err(HttpCallError::Unsuccessful { status: 401, .. }) => {
            return analytics(
                status(false, "none", Some("Moonshot API key was rejected (401)".into())),
                None,
                HashMap::new(),
            );
        }
        Err(e) => {
            return analytics(status(true, &method, Some(e.to_string())), None, HashMap::new());
        }
    };

    let Some(balance) = data.data else {
        return analytics(
            status(true, &method, Some("Moonshot balance response missing data".into())),
            None,
            HashMap::new(),
        );
    };

    let available = balance.available_balance.unwrap_or(0.0);
    let voucher = balance.voucher_balance.unwrap_or(0.0);
    let cash = balance.cash_balance.unwrap_or(0.0);

    let mut extra = HashMap::new();
    extra.insert("voucher_balance".into(), serde_json::json!(voucher));
    extra.insert("cash_balance".into(), serde_json::json!(cash));

    // available_balance <= 0 -> quota exceeded; surface as a low-balance warning
    // (amber status dot) while still showing the balance card.
    let error = if available <= 0.0 {
        extra.insert("quota_exceeded".into(), serde_json::json!(true));
        Some("Available balance depleted — top up to keep using Moonshot".into())
    } else {
        None
    };

    let credit_usage = Some(CreditUsage {
        provider_id: PROVIDER_ID.into(),
        used: available,
        limit: None,
        remaining: available,
        currency: "USD".into(),
        billing_cycle_end: None,
        plan_name: Some("Available balance".into()),
    });

    analytics(status(true, &method, error), credit_usage, extra)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_balance_response_shape() {
        let json = r#"{"code":0,"data":{"available_balance":49.58,"voucher_balance":46.58,"cash_balance":3.00}}"#;
        let resp: BalanceResponse = serde_json::from_str(json).unwrap();
        let d = resp.data.unwrap();
        assert_eq!(d.available_balance, Some(49.58));
        assert_eq!(d.voucher_balance, Some(46.58));
        assert_eq!(d.cash_balance, Some(3.00));
    }

    #[test]
    fn missing_fields_default_to_none() {
        let json = r#"{"data":{"available_balance":10.0}}"#;
        let resp: BalanceResponse = serde_json::from_str(json).unwrap();
        let d = resp.data.unwrap();
        assert_eq!(d.available_balance, Some(10.0));
        assert_eq!(d.voucher_balance, None);
        assert_eq!(d.cash_balance, None);
    }
}
