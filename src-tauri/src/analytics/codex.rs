//! Codex (OpenAI) analytics provider.
//! Auth fallback: user token, then auto-detect ~/.codex/auth.json
//! API: documented Codex App Server, with legacy WHAM as a compatibility fallback
//! Local data: ~/.codex/state_5.sqlite, ~/.codex/models_cache.json, ~/.codex/config.toml

use crate::analytics::http;
use crate::analytics::token_store;
use crate::analytics::types::*;
use crate::commands::codex_app_server;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

// ── In-memory cache (300s TTL) ──────────────────────────────────────────────

struct CacheEntry {
    data: ProviderAnalytics,
    fetched_at: std::time::Instant,
}

lazy_static::lazy_static! {
    static ref CACHE: Mutex<Option<CacheEntry>> = Mutex::new(None);
}

const CACHE_TTL_SECS: u64 = 60;

#[derive(Deserialize, Debug)]
struct CodexAuthTokens {
    access_token: Option<String>,
    refresh_token: Option<String>,
    id_token: Option<String>,
    account_id: Option<String>,
}

#[derive(Deserialize, Debug)]
struct CodexAuth {
    auth_mode: Option<String>,
    #[serde(rename = "OPENAI_API_KEY")]
    openai_api_key: Option<String>,
    tokens: Option<CodexAuthTokens>,
    last_refresh: Option<String>,
}

/// Lenient `Option<f64>` deserializer: accepts a JSON number, a numeric
/// string (e.g. `"0"`, `"12.5"`), or null/absent → `None`. OpenAI's WHAM
/// usage API has been observed to send numeric fields as stringified
/// numbers (same class of drift as DeepSeek's decimal strings and Gemini's
/// `remainingAmount`).
fn de_lenient_opt_f64<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    match value {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::Number(n)) => Ok(n.as_f64()),
        Some(serde_json::Value::String(s)) if s.trim().is_empty() => Ok(None),
        Some(serde_json::Value::String(s)) => s.trim().parse::<f64>().map(Some).map_err(|e| {
            serde::de::Error::custom(format!("invalid numeric string {:?}: {}", s, e))
        }),
        Some(other) => Err(serde::de::Error::custom(format!(
            "expected number or numeric string, got {}",
            other
        ))),
    }
}

/// Same as [`de_lenient_opt_f64`] but for `Option<i64>` fields (unix
/// timestamps, window durations in seconds).
fn de_lenient_opt_i64<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    match value {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::Number(n)) => {
            Ok(n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)))
        }
        Some(serde_json::Value::String(s)) if s.trim().is_empty() => Ok(None),
        Some(serde_json::Value::String(s)) => s
            .trim()
            .parse::<i64>()
            .or_else(|_| s.trim().parse::<f64>().map(|f| f as i64))
            .map(Some)
            .map_err(|e| {
                serde::de::Error::custom(format!("invalid integer string {:?}: {}", s, e))
            }),
        Some(other) => Err(serde::de::Error::custom(format!(
            "expected integer or numeric string, got {}",
            other
        ))),
    }
}

#[derive(Deserialize, Debug)]
struct WhamRateWindow {
    #[serde(default, deserialize_with = "de_lenient_opt_f64")]
    used_percent: Option<f64>,
    #[serde(default, deserialize_with = "de_lenient_opt_i64")]
    reset_at: Option<i64>, // unix timestamp
    #[serde(default, deserialize_with = "de_lenient_opt_i64")]
    reset_after_seconds: Option<i64>,
    #[serde(default, deserialize_with = "de_lenient_opt_i64")]
    limit_window_seconds: Option<i64>,
}

#[derive(Deserialize, Debug)]
struct WhamRateLimit {
    allowed: Option<bool>,
    limit_reached: Option<bool>,
    primary_window: Option<WhamRateWindow>,
    secondary_window: Option<WhamRateWindow>,
}

#[derive(Deserialize, Debug)]
struct WhamCredits {
    has_credits: Option<bool>,
    unlimited: Option<bool>,
    #[serde(default, deserialize_with = "de_lenient_opt_f64")]
    balance: Option<f64>,
}

#[derive(Deserialize, Debug)]
struct WhamAdditionalRateLimit {
    limit_name: Option<String>,
    metered_feature: Option<String>,
    rate_limit: Option<WhamRateLimit>,
}

#[derive(Deserialize, Debug)]
struct WhamUsageResponse {
    user_id: Option<String>,
    account_id: Option<String>,
    email: Option<String>,
    plan_type: Option<String>,
    rate_limit: Option<WhamRateLimit>,
    code_review_rate_limit: Option<WhamRateLimit>,
    additional_rate_limits: Option<Vec<WhamAdditionalRateLimit>>,
    credits: Option<WhamCredits>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct AppServerRateWindow {
    used_percent: Option<f64>,
    window_duration_mins: Option<i64>,
    resets_at: Option<i64>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct AppServerCredits {
    has_credits: Option<bool>,
    unlimited: Option<bool>,
    #[serde(default, deserialize_with = "de_lenient_opt_f64")]
    balance: Option<f64>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct AppServerRateLimit {
    limit_id: Option<String>,
    limit_name: Option<String>,
    primary: Option<AppServerRateWindow>,
    secondary: Option<AppServerRateWindow>,
    credits: Option<AppServerCredits>,
    plan_type: Option<String>,
    rate_limit_reached_type: Option<String>,
    spend_control_reached: Option<bool>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct AppServerRateLimitsResponse {
    rate_limits: Option<AppServerRateLimit>,
    rate_limits_by_limit_id: Option<HashMap<String, AppServerRateLimit>>,
}

fn auth_path() -> Result<PathBuf, String> {
    Ok(crate::utils::codex_paths::codex_home()?.join("auth.json"))
}

fn resolve_token() -> Result<(String, String, Option<String>), String> {
    // 1. User-provided
    if let Ok(Some(token)) = token_store::get_provider_token("codex", "access-token") {
        let account_id = token_store::get_provider_token("codex", "account-id")
            .ok()
            .flatten();
        return Ok((token, "token-manual".into(), account_id));
    }

    // 2. Auto-detect
    let path = auth_path()?;
    if !path.exists() {
        return Err("Codex auth.json not found".into());
    }
    let content = fs::read_to_string(&path).map_err(|e| format!("Read error: {}", e))?;
    let auth: CodexAuth =
        serde_json::from_str(&content).map_err(|e| format!("Parse error: {}", e))?;

    if let Some(ref tokens) = auth.tokens {
        if let Some(ref at) = tokens.access_token {
            return Ok((at.clone(), "oauth-auto".into(), tokens.account_id.clone()));
        }
    }
    Err("No Codex access token found".into())
}

fn parse_wham_window(w: &Option<WhamRateWindow>, label: &str) -> Option<RateLimitWindow> {
    let win = w.as_ref()?;
    let used = win.used_percent?;
    Some(RateLimitWindow {
        provider_id: "codex".into(),
        label: label.into(),
        used_percent: used,
        remaining_percent: (100.0 - used).max(0.0),
        resets_at: win.reset_at.map(|ts| {
            chrono::DateTime::from_timestamp(ts, 0)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_default()
        }),
        resets_in_seconds: win.reset_after_seconds,
        window_seconds: win.limit_window_seconds,
    })
}

fn wham_window_label(
    window: &Option<WhamRateWindow>,
    prefix: Option<&str>,
    fallback: &str,
) -> String {
    let period = window
        .as_ref()
        .and_then(|w| w.limit_window_seconds)
        .filter(|seconds| *seconds > 0)
        .map(|seconds| {
            if seconds % 86_400 == 0 {
                format!("{}d", seconds / 86_400)
            } else if seconds % 3_600 == 0 {
                format!("{}h", seconds / 3_600)
            } else if seconds % 60 == 0 {
                format!("{}m", seconds / 60)
            } else {
                format!("{}s", seconds)
            }
        });

    match (prefix, period.as_deref()) {
        (Some(name), Some(period)) => format!("{} ({})", name, period),
        (Some(name), None) => format!("{} ({})", name, fallback),
        (None, Some("7d")) => "Weekly (7d)".into(),
        (None, Some(period)) => format!("{} ({})", fallback, period),
        (None, None) => match fallback {
            "Primary" => "Primary (5h)".into(),
            "Secondary" => "Weekly (7d)".into(),
            _ => fallback.into(),
        },
    }
}

fn append_wham_windows(
    windows: &mut Vec<RateLimitWindow>,
    rate_limit: &WhamRateLimit,
    prefix: Option<&str>,
) {
    let primary_label = wham_window_label(&rate_limit.primary_window, prefix, "Primary");
    if let Some(window) = parse_wham_window(&rate_limit.primary_window, &primary_label) {
        windows.push(window);
    }

    let secondary_label = wham_window_label(&rate_limit.secondary_window, prefix, "Secondary");
    if let Some(window) = parse_wham_window(&rate_limit.secondary_window, &secondary_label) {
        windows.push(window);
    }
}

fn parse_wham_credit_usage(
    credits: Option<&WhamCredits>,
    plan_type: Option<&str>,
) -> Option<CreditUsage> {
    let credits = credits?;
    if credits.has_credits == Some(false) {
        return None;
    }

    let remaining = credits
        .balance
        .or_else(|| (credits.unlimited == Some(true)).then_some(0.0))?;

    Some(CreditUsage {
        provider_id: "codex".into(),
        used: 0.0,
        limit: None,
        remaining,
        currency: "credits".into(),
        billing_cycle_end: None,
        plan_name: plan_type.map(str::to_owned),
    })
}

fn rate_window_period(minutes: Option<i64>) -> Option<String> {
    let minutes = minutes.filter(|value| *value > 0)?;
    if minutes % (24 * 60) == 0 {
        Some(format!("{}d", minutes / (24 * 60)))
    } else if minutes % 60 == 0 {
        Some(format!("{}h", minutes / 60))
    } else {
        Some(format!("{}m", minutes))
    }
}

fn app_server_window_label(
    limit: &AppServerRateLimit,
    window: &AppServerRateWindow,
    secondary: bool,
) -> String {
    let period = rate_window_period(window.window_duration_mins);
    if let Some(name) = limit
        .limit_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        return period
            .map(|period| format!("{name} ({period})"))
            .unwrap_or_else(|| name.to_string());
    }

    match period.as_deref() {
        Some("7d") => "Weekly (7d)".into(),
        Some(period) => format!(
            "{} ({period})",
            if secondary { "Secondary" } else { "Primary" }
        ),
        None if secondary => "Secondary".into(),
        None => "Primary".into(),
    }
}

fn app_server_window(
    limit: &AppServerRateLimit,
    window: &AppServerRateWindow,
    secondary: bool,
) -> Option<RateLimitWindow> {
    let used = window.used_percent?;
    Some(RateLimitWindow {
        provider_id: "codex".into(),
        label: app_server_window_label(limit, window, secondary),
        used_percent: used,
        remaining_percent: (100.0 - used).clamp(0.0, 100.0),
        resets_at: window.resets_at.map(|timestamp| {
            chrono::DateTime::from_timestamp(timestamp, 0)
                .map(|date| date.to_rfc3339())
                .unwrap_or_default()
        }),
        resets_in_seconds: window
            .resets_at
            .map(|timestamp| (timestamp - Utc::now().timestamp()).max(0)),
        window_seconds: window.window_duration_mins.map(|minutes| minutes * 60),
    })
}

fn append_app_server_windows(windows: &mut Vec<RateLimitWindow>, limit: &AppServerRateLimit) {
    if let Some(primary) = limit.primary.as_ref() {
        if let Some(window) = app_server_window(limit, primary, false) {
            windows.push(window);
        }
    }
    if let Some(secondary) = limit.secondary.as_ref() {
        if let Some(window) = app_server_window(limit, secondary, true) {
            windows.push(window);
        }
    }
}

fn ordered_app_server_limits(response: &AppServerRateLimitsResponse) -> Vec<AppServerRateLimit> {
    let mut limits = Vec::new();
    let primary_id = response
        .rate_limits
        .as_ref()
        .and_then(|limit| limit.limit_id.clone());
    if let Some(primary) = response.rate_limits.clone() {
        limits.push(primary);
    }

    let mut additional: Vec<(String, AppServerRateLimit)> = response
        .rate_limits_by_limit_id
        .as_ref()
        .into_iter()
        .flat_map(|limits| limits.iter())
        .filter(|(id, limit)| {
            primary_id.as_deref() != Some(id.as_str())
                && primary_id.as_deref() != limit.limit_id.as_deref()
        })
        .map(|(id, limit)| (id.clone(), limit.clone()))
        .collect();
    additional.sort_by(|(left_id, left), (right_id, right)| {
        left.limit_name
            .as_deref()
            .unwrap_or(left_id)
            .cmp(right.limit_name.as_deref().unwrap_or(right_id))
    });
    limits.extend(additional.into_iter().map(|(_, limit)| limit));
    limits
}

fn app_server_credit_usage(limits: &[AppServerRateLimit]) -> Option<CreditUsage> {
    limits.iter().find_map(|limit| {
        let credits = limit.credits.as_ref()?;
        if credits.has_credits == Some(false) {
            return None;
        }
        Some(CreditUsage {
            provider_id: "codex".into(),
            used: 0.0,
            limit: None,
            remaining: credits
                .balance
                .or_else(|| (credits.unlimited == Some(true)).then_some(0.0))?,
            currency: "credits".into(),
            billing_cycle_end: None,
            plan_name: limit.plan_type.clone(),
        })
    })
}

/// A window is "weekly" when it spans roughly a week. Codex reports several
/// buckets (5h, 7d, per-model); the weekly quota is the one that actually
/// governs how much work is left, so it is what the banner reports.
const WEEKLY_WINDOW_MIN_MINS: i64 = 60 * 24 * 5; // >= ~5 days counts as weekly

fn weekly_window(limit: &AppServerRateLimit) -> Option<&AppServerRateWindow> {
    limit
        .primary
        .iter()
        .chain(limit.secondary.iter())
        .filter(|w| {
            w.window_duration_mins
                .is_some_and(|mins| mins >= WEEKLY_WINDOW_MIN_MINS)
        })
        // If several qualify, the longest is the governing quota.
        .max_by_key(|w| w.window_duration_mins.unwrap_or(0))
}

/// Codex's limit banner is driven by the **weekly** quota.
///
/// Short buckets (the 5h `codex-spark` window, for instance) routinely sit at
/// 100% and refill within hours; surfacing those as "Limit reached" is noise,
/// and because the API returns limits in a `HashMap`, whichever one appeared
/// first was effectively arbitrary. A genuine server-side block still wins,
/// since at that point no work is possible regardless of any percentage.
fn app_server_limit_state(limits: &[AppServerRateLimit]) -> Option<LimitState> {
    // 1. A real block always takes precedence: the server has said no.
    if let Some(blocked) = limits.iter().find(|limit| {
        limit.rate_limit_reached_type.is_some() || limit.spend_control_reached == Some(true)
    }) {
        let used_pct = blocked
            .primary
            .iter()
            .chain(blocked.secondary.iter())
            .filter_map(|w| w.used_percent)
            .fold(100.0_f64, f64::max);
        let resets_at = blocked
            .primary
            .iter()
            .chain(blocked.secondary.iter())
            .filter_map(|w| w.resets_at)
            .min()
            .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0))
            .map(|d| d.to_rfc3339());
        return Some(LimitState::Reached {
            scope: LimitScope::Custom(
                blocked
                    .limit_name
                    .clone()
                    .or_else(|| blocked.limit_id.clone())
                    .unwrap_or_else(|| "codex".into()),
            ),
            used_pct,
            cap: None,
            resets_at,
        });
    }

    // 2. Otherwise report only the weekly quota, and only once it is spent.
    let (limit, window) = limits
        .iter()
        .filter_map(|limit| weekly_window(limit).map(|w| (limit, w)))
        .max_by(|a, b| {
            a.1.used_percent
                .unwrap_or(0.0)
                .partial_cmp(&b.1.used_percent.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        })?;

    if !window.used_percent.is_some_and(|pct| pct >= 100.0) {
        return None;
    }

    Some(LimitState::Reached {
        scope: LimitScope::Custom(
            limit
                .limit_name
                .clone()
                .or_else(|| limit.limit_id.clone())
                .unwrap_or_else(|| "weekly".into()),
        ),
        used_pct: window.used_percent.unwrap_or(100.0),
        cap: None,
        resets_at: window
            .resets_at
            .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0))
            .map(|d| d.to_rfc3339()),
    })
}

// ── Cost estimation ─────────────────────────────────────────────────────────
//
// API-equivalent value only: Codex plans are flat-fee, so these figures show
// what the same usage would cost at pay-per-token API rates.
//
// Rates as of RATES_AS_OF, from developers.openai.com/api/docs/pricing.
// `gpt-daybreak-blue-latest` is a *moving* alias (currently -> gpt-5.6-sol);
// re-check it when updating this table.

/// Basis date for the rate table below. Surfaced to the UI so a stale table is
/// visible rather than silent.
const RATES_AS_OF: &str = "2026-09-08";

/// Input tokens above this threshold in a single request bill at the
/// long-context tier (2x input / 1.5x output).
const LONG_CONTEXT_THRESHOLD: u64 = 272_000;

/// Per-million-token USD rates for one model. `cache_write` is stored
/// explicitly rather than derived: the 1.25x premium is documented for Astra
/// and the gpt-5.6 family, but gpt-5.5 publishes no cache-write rate.
#[derive(Debug, Clone, Copy, PartialEq)]
struct ModelRates {
    input: f64,
    cached_input: f64,
    cache_write: f64,
    output: f64,
}

impl ModelRates {
    const fn new(input: f64, cached_input: f64, cache_write: f64, output: f64) -> Self {
        Self { input, cached_input, cache_write, output }
    }

    /// Long-context tier: input-side rates double, output rises 1.5x.
    fn long_context(self) -> Self {
        Self {
            input: self.input * 2.0,
            cached_input: self.cached_input * 2.0,
            cache_write: self.cache_write * 2.0,
            output: self.output * 1.5,
        }
    }
}

/// Normalize a raw model string to a canonical id: lowercase, strip a
/// provider namespace (`openai/gpt-5.5` -> `gpt-5.5`) and any date suffix.
fn normalize_model_id(model: &str) -> String {
    let lower = model.trim().to_lowercase();
    let base = lower.rsplit('/').next().unwrap_or(&lower);
    base.to_string()
}

/// Exact-match rate lookup. Returns `None` for genuinely unknown models so
/// callers can report them as unpriced instead of inventing a default.
///
/// Exact ids (plus an explicit alias map) rather than substring matching:
/// `gpt-5.5-pro` has different rates and no cached-input discount, so a
/// `contains("gpt-5.5")` test would misprice it.
fn model_rates(model: &str) -> Option<ModelRates> {
    let id = normalize_model_id(model);
    let canonical = match id.as_str() {
        // Moving aliases, resolved explicitly.
        "gpt-daybreak-blue-latest" | "gpt-daybreak-blue" => "gpt-5.6-sol",
        "gpt-5.6" => "gpt-5.6-sol",
        other => other,
    };
    let rates = match canonical {
        "gpt-6-astra" => ModelRates::new(10.0, 1.00, 12.50, 50.0),
        "gpt-5.6-sol" => ModelRates::new(4.0, 0.40, 5.00, 20.0),
        "gpt-5.6-terra" => ModelRates::new(2.0, 0.20, 2.50, 12.0),
        "gpt-5.6-luna" => ModelRates::new(0.20, 0.02, 0.25, 1.20),
        "gpt-5.5" => ModelRates::new(5.0, 0.50, 6.25, 30.0),
        "gpt-5.4-mini" => ModelRates::new(0.75, 0.075, 0.9375, 4.50),
        "gpt-5.4" => ModelRates::new(2.50, 0.25, 3.125, 15.0),
        "gpt-5.3-codex" | "gpt-5.3-codex-spark" => ModelRates::new(1.75, 0.175, 2.1875, 14.0),
        "gpt-5.2" | "gpt-5.2-codex" => ModelRates::new(1.75, 0.175, 2.1875, 14.0),
        "gpt-5.1-codex-max" | "gpt-5.1-codex" | "gpt-5.1" => {
            ModelRates::new(1.25, 0.125, 1.5625, 10.0)
        }
        "gpt-5" => ModelRates::new(1.25, 0.125, 1.5625, 10.0),
        _ => return None,
    };
    Some(rates)
}

/// Cost for one request's usage, priced at its own turn's model.
///
/// `cached_input_tokens` and `cache_write_input_tokens` are *subsets* of
/// `input_tokens` (verified across ~30,600 records), so ordinary input is the
/// remainder. `reasoning_output_tokens` is already inside `output_tokens`
/// (`total == input + output` holds exactly) and must not be added again.
fn estimate_usage_cost(usage: &RolloutTokenUsage, model: &str) -> Option<f64> {
    let mut rates = model_rates(model)?;
    if usage.input_tokens > LONG_CONTEXT_THRESHOLD {
        rates = rates.long_context();
    }
    let ordinary_input = usage
        .input_tokens
        .saturating_sub(usage.cached_input_tokens)
        .saturating_sub(usage.cache_write_input_tokens);
    Some(
        (ordinary_input as f64 / 1_000_000.0) * rates.input
            + (usage.cached_input_tokens as f64 / 1_000_000.0) * rates.cached_input
            + (usage.cache_write_input_tokens as f64 / 1_000_000.0) * rates.cache_write
            + (usage.output_tokens as f64 / 1_000_000.0) * rates.output,
    )
}

// ── Local data types ────────────────────────────────────────────────────────

#[derive(Serialize, Clone, Debug)]
struct CodexSession {
    id: String,
    title: Option<String>,
    model: Option<String>,
    tokens_used: i64,
    source: Option<String>,
    cwd: Option<String>,
    git_branch: Option<String>,
    reasoning_effort: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
    /// True when this session's tokens came from the approximate legacy path.
    #[serde(default)]
    tokens_approximate: bool,
}

#[derive(Serialize, Clone, Debug)]
struct CodexLocalStats {
    total_sessions: i64,
    total_tokens_used: i64,
    sessions: Vec<CodexSession>,
    tokens_by_model: HashMap<String, i64>,
    sessions_by_project: HashMap<String, i64>,
    estimated_total_cost: f64,
    cost_by_model: HashMap<String, f64>,
    /// Per-day usage buckets, each carrying the distinct thread ids active
    /// that day so weekly/monthly views can union rather than sum them.
    usage_daily: Vec<serde_json::Value>,
    /// Models with no known rate; their tokens are excluded from cost.
    unpriced_models: Vec<String>,
    unpriced_tokens: u64,
    /// Sessions valued via the approximate legacy path.
    approximate_sessions: u32,
    /// Sessions where no usage could be recovered at all.
    unaccounted_sessions: u32,
}

#[derive(Deserialize, Debug)]
struct OpenAIMeOrg {
    id: Option<String>,
    title: Option<String>,
    role: Option<String>,
}

#[derive(Deserialize, Debug)]
struct OpenAIMeResponse {
    name: Option<String>,
    email: Option<String>,
    orgs: Option<Vec<OpenAIMeOrg>>,
}

// ── Local data parsing ──────────────────────────────────────────────────────

fn codex_dir() -> Result<PathBuf, String> {
    crate::utils::codex_paths::codex_home()
}

/// Convert a millisecond epoch timestamp (as stored in `threads.created_at_ms`
/// / `updated_at_ms`) to an RFC3339 string for display. Returns `None` for 0/invalid.
fn ms_to_rfc3339(ms: i64) -> Option<String> {
    if ms <= 0 {
        return None;
    }
    chrono::DateTime::from_timestamp_millis(ms).map(|dt| dt.to_rfc3339())
}

/// Open `~/.codex/state_5.sqlite` read-only, or `None` if missing/unreadable.
fn open_codex_db() -> Option<rusqlite::Connection> {
    let db_path = codex_dir().ok()?.join("state_5.sqlite");
    if !db_path.exists() {
        return None;
    }
    rusqlite::Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()
}

// ── Rollout JSONL parsing (~/.codex/sessions/<Y>/<M>/<D>/rollout-*.jsonl) ───

/// Cumulative token usage as reported by a `token_count` event_msg. Numeric
/// fields default to 0 so unfamiliar/older rollout shapes don't fail parsing.
#[derive(Deserialize, Serialize, Debug, Clone, Default)]
struct RolloutTokenUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    cached_input_tokens: u64,
    #[serde(default)]
    cache_write_input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    reasoning_output_tokens: u64,
    #[serde(default)]
    total_tokens: u64,
}

impl RolloutTokenUsage {
    fn add(&mut self, other: &RolloutTokenUsage) {
        self.input_tokens += other.input_tokens;
        self.cached_input_tokens += other.cached_input_tokens;
        self.cache_write_input_tokens += other.cache_write_input_tokens;
        self.output_tokens += other.output_tokens;
        self.reasoning_output_tokens += other.reasoning_output_tokens;
        self.total_tokens += other.total_tokens;
    }
}

#[derive(Deserialize, Debug, Clone, Default)]
struct RolloutRateWindow {
    used_percent: Option<f64>,
    window_minutes: Option<i64>,
    resets_at: Option<i64>,
}

#[derive(Deserialize, Debug, Clone, Default)]
struct RolloutRateLimits {
    primary: Option<RolloutRateWindow>,
    secondary: Option<RolloutRateWindow>,
    plan_type: Option<String>,
}

/// Parse one rollout JSONL line; returns `Some` only for an `event_msg` of
/// subtype `token_count`. Tries the observed `payload.info.total_token_usage`
/// shape first, then a flat `payload.total_token_usage` as a fallback for
/// forward/backward compatibility. Never panics on malformed input.
fn parse_token_count_line(line: &str) -> Option<(RolloutTokenUsage, Option<RolloutRateLimits>)> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    if value.get("type")?.as_str()? != "event_msg" {
        return None;
    }
    let payload = value.get("payload")?;
    if payload.get("type")?.as_str()? != "token_count" {
        return None;
    }
    let usage_value = payload
        .pointer("/info/total_token_usage")
        .or_else(|| payload.get("total_token_usage"))?;
    let usage: RolloutTokenUsage = serde_json::from_value(usage_value.clone()).ok()?;
    let rate_limits = payload
        .get("rate_limits")
        .and_then(|v| serde_json::from_value::<RolloutRateLimits>(v.clone()).ok());
    Some((usage, rate_limits))
}

/// Scan a rollout file for its *last* `token_count` event — the cumulative
/// usage for that session — plus the rate limits attached to that event.
/// Skips oversized/corrupt files rather than failing the whole enrichment.
fn scan_rollout_file(
    path: &std::path::Path,
) -> Option<(RolloutTokenUsage, Option<RolloutRateLimits>)> {
    const MAX_ROLLOUT_BYTES: u64 = 25 * 1024 * 1024;
    if let Ok(meta) = fs::metadata(path) {
        if meta.len() > MAX_ROLLOUT_BYTES {
            return None;
        }
    }
    let content = fs::read_to_string(path).ok()?;
    scan_rollout_content(&content)
}

/// Keep only the *last* `token_count` event in a rollout's JSONL content —
/// pulled out of `scan_rollout_file` so it's testable without touching disk.
fn scan_rollout_content(content: &str) -> Option<(RolloutTokenUsage, Option<RolloutRateLimits>)> {
    let mut last = None;
    for line in content.lines() {
        if let Some(parsed) = parse_token_count_line(line) {
            last = Some(parsed);
        }
    }
    last
}

fn rollout_window_label(minutes: Option<i64>, secondary: bool) -> String {
    let prefix = if secondary { "Secondary" } else { "Primary" };
    match minutes {
        Some(m) if m <= 60 * 6 => format!("{prefix} (5h)"),
        Some(m) if m >= 60 * 24 * 6 => format!("{prefix} (7d)"),
        Some(m) => format!("{prefix} ({m}m)"),
        None => format!("{prefix} (offline)"),
    }
}

fn rollout_rate_window_to_window(
    w: &RolloutRateWindow,
    secondary: bool,
) -> Option<RateLimitWindow> {
    let used = w.used_percent?;
    Some(RateLimitWindow {
        provider_id: "codex".into(),
        label: rollout_window_label(w.window_minutes, secondary),
        used_percent: used,
        remaining_percent: (100.0 - used).max(0.0),
        resets_at: w.resets_at.map(|ts| {
            chrono::DateTime::from_timestamp(ts, 0)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_default()
        }),
        resets_in_seconds: None,
        window_seconds: w.window_minutes.map(|m| m * 60),
    })
}

/// Map an offline (rollout-derived) rate-limit snapshot into the same
/// `RateLimitWindow` shape the WHAM API produces, for use as a fallback.
fn rollout_rate_limits_to_windows(rl: &RolloutRateLimits) -> Vec<RateLimitWindow> {
    let mut out = Vec::new();
    if let Some(ref w) = rl.primary {
        if let Some(win) = rollout_rate_window_to_window(w, false) {
            out.push(win);
        }
    }
    if let Some(ref w) = rl.secondary {
        if let Some(win) = rollout_rate_window_to_window(w, true) {
            out.push(win);
        }
    }
    out
}

// ── Per-request usage accounting ────────────────────────────────────────────
//
// Two rollout formats exist on disk and both must be handled:
//
//  * "modern" files carry one `token_usage_record` per billable request, with
//    a `timestamp`, a `response_id` and a per-request `usage` split. Summing
//    these reconciles exactly (diff = 0) against the session's final
//    cumulative `thread_token_usage`.
//  * "legacy" files carry only `total_token_usage` / `last_token_usage` on
//    `token_count` events. Those counters *reset between turns*, so the last
//    snapshot is not the lifetime total (and `threads.tokens_used`, which
//    inherits that bug, undercounts one verified session ~4.4x).
//
// Legacy reconstruction is inherently approximate: validated against the
// modern files where ground truth exists, the best strategy matched only
// 16/26 within 2%. Legacy rows are therefore flagged `approximate` and the UI
// discloses the count rather than presenting them as exact.

/// One billable request, attributed to the model that produced it.
#[derive(Debug, Clone)]
struct UsageRow {
    /// Thread id (`threads.id`), the identity used for distinct-session counts.
    thread_id: String,
    /// Local-date bucket, `YYYY-MM-DD`, from this request's own timestamp.
    date: String,
    model: String,
    usage: RolloutTokenUsage,
    /// True when derived from the legacy format's resetting counters.
    approximate: bool,
    /// True when the model came from `threads.model` rather than a
    /// `turn_context` in the rollout itself.
    model_fallback: bool,
}

/// Result of scanning one rollout file.
#[derive(Debug, Clone, Default)]
struct RolloutScan {
    rows: Vec<UsageRow>,
    /// No usage could be recovered in either format.
    unaccounted: bool,
}

/// Convert an RFC 3339 rollout timestamp to a local `YYYY-MM-DD` bucket.
/// Rollout timestamps are strings (`2026-09-07T20:08:48.671Z`), not epoch
/// millis, so they must be parsed rather than fed to `timestamp_millis_opt`.
fn rfc3339_to_local_date(ts: &str) -> Option<String> {
    chrono::DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|dt| {
            dt.with_timezone(&chrono::Local)
                .format("%Y-%m-%d")
                .to_string()
        })
}

/// Cheap substring prefilter: only these lines can carry usage or model info,
/// so everything else skips JSON parsing entirely. Rollouts are dominated by
/// response and tool-output lines, which would otherwise cost a full parse.
fn line_is_interesting(line: &str) -> bool {
    line.contains("token_usage_record")
        || line.contains("last_token_usage")
        || line.contains("total_token_usage")
        || line.contains("turn_context")
}

/// Scan one rollout into per-request usage rows.
///
/// `db_model` is the `threads.model` fallback, used only when a record cannot
/// be tied to a `turn_context` (counted via `model_fallback`).
fn scan_rollout_usage(path: &std::path::Path, thread_id: &str, db_model: &str) -> RolloutScan {
    use std::io::BufRead;

    let Ok(file) = fs::File::open(path) else {
        return RolloutScan { rows: Vec::new(), unaccounted: true };
    };
    // Streaming read: rollouts reach multiple GB, so the file is never held in
    // memory. This also replaces the old 25 MB cap, which silently discarded
    // 89% of on-disk bytes.
    let reader = std::io::BufReader::new(file);

    let mut modern: Vec<UsageRow> = Vec::new();
    let mut legacy: std::collections::BTreeMap<String, UsageRow> = std::collections::BTreeMap::new();
    let mut seen_response_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    // Records whose `turn_context` has not been seen yet: a context can appear
    // *after* its usage record (7 verified cases), so running state is unsafe.
    let mut pending: Vec<(usize, String)> = Vec::new();
    let mut turn_models: HashMap<String, String> = HashMap::new();
    let mut current_model: Option<String> = None;

    for line in reader.lines() {
        let Ok(line) = line else { continue };
        if !line_is_interesting(&line) {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            // Partially written or corrupt line: skip it, keep everything else.
            continue;
        };

        // `turn_context` carries the active model for a turn.
        if let Some(payload) = value.get("payload") {
            if payload.get("type").and_then(|v| v.as_str()) == Some("turn_context") {
                if let Some(model) = payload.get("model").and_then(|v| v.as_str()) {
                    current_model = Some(model.to_string());
                    for key in ["turn_id", "root_turn_id"] {
                        if let Some(tid) = payload.get(key).and_then(|v| v.as_str()) {
                            turn_models.insert(tid.to_string(), model.to_string());
                        }
                    }
                }
                continue;
            }
        }

        // Modern: one record per billable request.
        if value.get("type").and_then(|v| v.as_str()) == Some("token_usage_record") {
            let Some(payload) = value.get("payload") else { continue };
            if let Some(rid) = payload.get("response_id").and_then(|v| v.as_str()) {
                if !seen_response_ids.insert(rid.to_string()) {
                    continue; // duplicate request, count once
                }
            }
            let Some(usage_value) = payload.get("usage") else { continue };
            let Ok(usage) = serde_json::from_value::<RolloutTokenUsage>(usage_value.clone()) else {
                continue;
            };
            let date = value
                .get("timestamp")
                .and_then(|v| v.as_str())
                .and_then(rfc3339_to_local_date)
                .unwrap_or_default();
            let turn_id = payload
                .get("turn_id")
                .or_else(|| payload.get("root_turn_id"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let resolved = turn_id.as_ref().and_then(|t| turn_models.get(t).cloned());
            let model = resolved.clone().or_else(|| current_model.clone());
            if model.is_none() {
                if let Some(t) = turn_id.clone() {
                    pending.push((modern.len(), t));
                }
            }
            modern.push(UsageRow {
                thread_id: thread_id.to_string(),
                date,
                model: model.unwrap_or_else(|| db_model.to_string()),
                usage,
                approximate: false,
                model_fallback: resolved.is_none() && current_model.is_none(),
            });
            continue;
        }

        // Legacy: `token_count` events carrying resetting cumulative counters.
        // Keep one entry per turn, keyed by turn id where available.
        if value.get("type").and_then(|v| v.as_str()) == Some("event_msg") {
            let Some(payload) = value.get("payload") else { continue };
            if payload.get("type").and_then(|v| v.as_str()) != Some("token_count") {
                continue;
            }
            let usage_value = payload
                .pointer("/info/last_token_usage")
                .or_else(|| payload.pointer("/info/total_token_usage"))
                .or_else(|| payload.get("last_token_usage"))
                .or_else(|| payload.get("total_token_usage"));
            let Some(usage_value) = usage_value else { continue };
            let Ok(usage) = serde_json::from_value::<RolloutTokenUsage>(usage_value.clone()) else {
                continue;
            };
            // Compaction markers report no real traffic: input and output are
            // zero while `total_tokens` carries only the compacted context.
            if usage.input_tokens == 0 && usage.output_tokens == 0 {
                continue;
            }
            let date = value
                .get("timestamp")
                .and_then(|v| v.as_str())
                .and_then(rfc3339_to_local_date)
                .unwrap_or_default();
            let turn_key = payload
                .get("turn_id")
                .or_else(|| payload.get("root_turn_id"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    // No turn id: fall back to a value fingerprint. Imprecise,
                    // which is part of why legacy rows are flagged approximate.
                    format!(
                        "{}:{}:{}:{}",
                        date, usage.input_tokens, usage.output_tokens, usage.total_tokens
                    )
                });
            // Legacy files are NOT single-model: 10 of 81 carry two models,
            // including a gpt-5.4 + gpt-5.5 pair whose input rates differ 2x.
            let model = current_model.clone();
            legacy.insert(
                turn_key,
                UsageRow {
                    thread_id: thread_id.to_string(),
                    date,
                    model: model.clone().unwrap_or_else(|| db_model.to_string()),
                    usage,
                    approximate: true,
                    model_fallback: model.is_none(),
                },
            );
        }
    }

    // Resolve records whose `turn_context` arrived after them.
    for (idx, turn_id) in pending {
        if let Some(model) = turn_models.get(&turn_id) {
            if let Some(row) = modern.get_mut(idx) {
                row.model = model.clone();
                row.model_fallback = false;
            }
        }
    }

    // Prefer the modern format wherever it exists; it is exact.
    if !modern.is_empty() {
        return RolloutScan { rows: modern, unaccounted: false };
    }
    let rows: Vec<UsageRow> = legacy.into_values().collect();
    let unaccounted = rows.is_empty();
    RolloutScan { rows, unaccounted }
}

/// Cached per-rollout scan, keyed by path and invalidated on `(mtime, size)`.
/// A full cold scan of a large history costs seconds, which is too slow to
/// repeat on every 60s analytics refresh; rollouts are append-only, so an
/// unchanged file can reuse its previous rows verbatim.
struct CachedScan {
    mtime: std::time::SystemTime,
    size: u64,
    rows: Vec<UsageRow>,
    unaccounted: bool,
}

static USAGE_CACHE: Mutex<Option<HashMap<PathBuf, CachedScan>>> = Mutex::new(None);

/// Everything derived from a full pass over the rollout corpus.
#[derive(Default)]
struct UsageCorpus {
    rows: Vec<UsageRow>,
    /// Sessions where no usage could be recovered in either format.
    unaccounted_sessions: u32,
    /// Models seen in usage rows that have no known rate.
    unpriced_models: std::collections::BTreeSet<String>,
    /// Tokens attributed to unpriced models, excluded from cost totals.
    unpriced_tokens: u64,
    /// Sessions whose figures came from the legacy (approximate) path.
    approximate_sessions: u32,
}

impl UsageCorpus {
    /// Total cost across every priced row. Unpriced rows are excluded rather
    /// than guessed, and surface separately via `unpriced_models`.
    fn total_cost(&self) -> f64 {
        self.rows
            .iter()
            .filter_map(|r| estimate_usage_cost(&r.usage, &r.model))
            .sum()
    }

    fn cost_by_model(&self) -> HashMap<String, f64> {
        let mut out: HashMap<String, f64> = HashMap::new();
        for row in &self.rows {
            if let Some(cost) = estimate_usage_cost(&row.usage, &row.model) {
                *out.entry(row.model.clone()).or_insert(0.0) += cost;
            }
        }
        out
    }

    /// Token totals per model, derived from the *same* rows as cost so the two
    /// can never disagree on the page.
    fn tokens_by_model(&self) -> HashMap<String, i64> {
        let mut out: HashMap<String, i64> = HashMap::new();
        for row in &self.rows {
            *out.entry(row.model.clone()).or_insert(0) += row.usage.total_tokens as i64;
        }
        out
    }

    fn total_tokens(&self) -> i64 {
        self.rows.iter().map(|r| r.usage.total_tokens as i64).sum()
    }

    /// Per-thread token totals, replacing the broken `threads.tokens_used`
    /// column for the Recent Sessions list.
    fn tokens_by_thread(&self) -> HashMap<String, (i64, bool)> {
        let mut out: HashMap<String, (i64, bool)> = HashMap::new();
        for row in &self.rows {
            let entry = out.entry(row.thread_id.clone()).or_insert((0, false));
            entry.0 += row.usage.total_tokens as i64;
            entry.1 |= row.approximate;
        }
        out
    }

    /// Daily buckets. Distinct threads are tracked as a set per day so callers
    /// can union them for weekly/monthly views; summing daily counts would
    /// yield session-days, not distinct sessions.
    fn daily(&self) -> Vec<serde_json::Value> {
        use std::collections::BTreeMap;
        let mut by_day: BTreeMap<String, (i64, f64, std::collections::BTreeSet<String>, bool)> =
            BTreeMap::new();
        for row in &self.rows {
            if row.date.is_empty() {
                continue;
            }
            let entry = by_day.entry(row.date.clone()).or_insert((
                0,
                0.0,
                std::collections::BTreeSet::new(),
                false,
            ));
            entry.0 += row.usage.total_tokens as i64;
            if let Some(cost) = estimate_usage_cost(&row.usage, &row.model) {
                entry.1 += cost;
            }
            entry.2.insert(row.thread_id.clone());
            entry.3 |= row.approximate;
        }
        by_day
            .into_iter()
            .map(|(date, (tokens, cost, threads, approximate))| {
                serde_json::json!({
                    "date": date,
                    "tokens": tokens,
                    "cost": cost,
                    "sessions": threads.len(),
                    "session_ids": threads.into_iter().collect::<Vec<_>>(),
                    "approximate": approximate,
                })
            })
            .collect()
    }
}

/// Scan every thread's rollout once and build the corpus all cost, token and
/// time-series figures are derived from.
///
/// Deliberately unbounded: `ROLLOUT_SCAN_LIMIT` must not apply here, or an
/// "all-time" total would silently omit older sessions once the history grows
/// past the limit.
fn build_usage_corpus(conn: &rusqlite::Connection) -> UsageCorpus {
    let mut corpus = UsageCorpus::default();

    let Ok(mut stmt) = conn.prepare(
        "SELECT id, rollout_path, COALESCE(model, 'unknown') FROM threads \
         WHERE rollout_path IS NOT NULL",
    ) else {
        return corpus;
    };
    let Ok(rows) = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    }) else {
        return corpus;
    };

    let mut cache_guard = USAGE_CACHE.lock().ok();
    let mut cache = cache_guard
        .as_mut()
        .map(|slot| slot.get_or_insert_with(HashMap::new));
    let mut live_paths: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

    for (thread_id, rollout_path, db_model) in rows.flatten() {
        let path = PathBuf::from(&rollout_path);
        let Ok(meta) = fs::metadata(&path) else { continue };
        let (mtime, size) = (meta.modified().ok(), meta.len());
        live_paths.insert(path.clone());

        let cached = cache.as_ref().and_then(|c| c.get(&path)).filter(|entry| {
            Some(entry.mtime) == mtime && entry.size == size
        });

        let (rows, unaccounted) = if let Some(entry) = cached {
            (entry.rows.clone(), entry.unaccounted)
        } else {
            let scan = scan_rollout_usage(&path, &thread_id, &db_model);
            (scan.rows, scan.unaccounted)
        };

        if let (Some(cache), Some(mtime)) = (cache.as_mut(), mtime) {
            // Replace (not accumulate) this path's entry.
            cache.insert(
                path.clone(),
                CachedScan { mtime, size, rows: rows.clone(), unaccounted },
            );
        }

        if unaccounted {
            corpus.unaccounted_sessions += 1;
        }
        if rows.iter().any(|r| r.approximate) {
            corpus.approximate_sessions += 1;
        }
        for row in &rows {
            if model_rates(&row.model).is_none() {
                corpus.unpriced_models.insert(row.model.clone());
                corpus.unpriced_tokens += row.usage.total_tokens;
            }
        }
        corpus.rows.extend(rows);
    }

    // Evict entries for rollouts that no longer exist, so the cache cannot
    // grow without bound as sessions are deleted.
    if let Some(cache) = cache {
        cache.retain(|path, _| live_paths.contains(path));
    }

    corpus
}

/// Aggregate token usage summed across the most recently touched sessions.
const ROLLOUT_SCAN_LIMIT: i64 = 150;

#[derive(Serialize, Clone, Debug, Default)]
struct CodexTokenBreakdown {
    input_tokens: u64,
    cached_input_tokens: u64,
    cache_write_input_tokens: u64,
    output_tokens: u64,
    reasoning_output_tokens: u64,
    total_tokens: u64,
    cache_hit_percent: f64,
    sessions_scanned: usize,
}

struct RolloutScanResult {
    token_breakdown: CodexTokenBreakdown,
    /// Rate limits from the newest rollout that reported any (offline fallback).
    offline_rate_limits: Option<RolloutRateLimits>,
}

/// Scan the most recently updated sessions' rollout files: sum token usage
/// across all of them, and capture the newest available rate-limit snapshot
/// for use when the WHAM API is unreachable.
fn scan_recent_rollouts(conn: &rusqlite::Connection) -> RolloutScanResult {
    let empty = RolloutScanResult {
        token_breakdown: CodexTokenBreakdown::default(),
        offline_rate_limits: None,
    };
    let mut stmt = match conn.prepare(
        "SELECT rollout_path FROM threads WHERE rollout_path IS NOT NULL \
         ORDER BY COALESCE(updated_at_ms, updated_at * 1000) DESC LIMIT ?1",
    ) {
        Ok(s) => s,
        Err(_) => return empty,
    };
    let paths: Vec<String> =
        match stmt.query_map([ROLLOUT_SCAN_LIMIT], |row| row.get::<_, String>(0)) {
            Ok(rows) => rows.flatten().collect(),
            Err(_) => return empty,
        };

    let mut total = RolloutTokenUsage::default();
    let mut scanned = 0usize;
    let mut offline_rate_limits = None;
    for p in &paths {
        let path = std::path::Path::new(p);
        if !path.exists() {
            continue;
        }
        if let Some((usage, rate_limits)) = scan_rollout_file(path) {
            total.add(&usage);
            scanned += 1;
            if offline_rate_limits.is_none() && rate_limits.is_some() {
                offline_rate_limits = rate_limits;
            }
        }
    }

    let cache_hit_percent = if total.input_tokens > 0 {
        (total.cached_input_tokens as f64 / total.input_tokens as f64) * 100.0
    } else {
        0.0
    };

    RolloutScanResult {
        token_breakdown: CodexTokenBreakdown {
            input_tokens: total.input_tokens,
            cached_input_tokens: total.cached_input_tokens,
            cache_write_input_tokens: total.cache_write_input_tokens,
            output_tokens: total.output_tokens,
            reasoning_output_tokens: total.reasoning_output_tokens,
            total_tokens: total.total_tokens,
            cache_hit_percent,
            sessions_scanned: scanned,
        },
        offline_rate_limits,
    }
}

// ── Activity stats (daily heatmap, streaks, peak hour) ──────────────────────

#[derive(Debug, Clone, Default)]
struct CodexActivityStats {
    daily_activity: Vec<(String, u32)>,
    hour_counts: [u32; 24],
    active_days: u32,
    longest_streak: u32,
    current_streak: u32,
    peak_hour: Option<u32>,
}

/// Derive daily-activity heatmap, streaks, and peak hour from a set of
/// thread `created_at` timestamps (milliseconds, local time — mirrors the
/// claude_v2 `active_days`/`longest_streak`/`peak_hour` derivation).
fn compute_activity_stats(created_at_ms: &[i64]) -> CodexActivityStats {
    use chrono::{Local, TimeZone, Timelike};

    let mut daily: std::collections::BTreeMap<String, u32> = std::collections::BTreeMap::new();
    let mut hour_counts = [0u32; 24];
    for &ms in created_at_ms {
        if let chrono::LocalResult::Single(dt) = Local.timestamp_millis_opt(ms) {
            *daily.entry(dt.format("%Y-%m-%d").to_string()).or_insert(0) += 1;
            hour_counts[dt.hour() as usize] += 1;
        }
    }

    let active_days = daily.len() as u32;

    let mut dates: Vec<chrono::NaiveDate> = daily
        .keys()
        .filter_map(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        .collect();
    dates.sort();

    let longest_streak: u32 = if dates.is_empty() {
        0
    } else {
        let mut max_streak = 1u32;
        let mut cur = 1u32;
        for w in dates.windows(2) {
            if w[1] - w[0] == chrono::Duration::days(1) {
                cur += 1;
                max_streak = max_streak.max(cur);
            } else {
                cur = 1;
            }
        }
        max_streak
    };

    let current_streak: u32 = if dates.is_empty() {
        0
    } else {
        let today = Local::now().date_naive();
        let last = *dates.last().unwrap();
        if (today - last).num_days() > 1 {
            0
        } else {
            let mut streak = 1u32;
            for w in dates.windows(2).rev() {
                if w[1] - w[0] == chrono::Duration::days(1) {
                    streak += 1;
                } else {
                    break;
                }
            }
            streak
        }
    };

    let peak_hour = {
        let max_val = hour_counts.iter().copied().max().unwrap_or(0);
        if max_val == 0 {
            None
        } else {
            hour_counts
                .iter()
                .position(|&v| v == max_val)
                .map(|i| i as u32)
        }
    };

    CodexActivityStats {
        daily_activity: daily.into_iter().collect(),
        hour_counts,
        active_days,
        longest_streak,
        current_streak,
        peak_hour,
    }
}

fn fetch_activity_timestamps(conn: &rusqlite::Connection) -> Vec<i64> {
    let mut stmt =
        match conn.prepare("SELECT COALESCE(created_at_ms, created_at * 1000) FROM threads") {
            Ok(s) => s,
            Err(_) => return vec![],
        };
    let timestamps: Vec<i64> = match stmt.query_map([], |row| row.get::<_, i64>(0)) {
        Ok(rows) => rows.flatten().collect(),
        Err(_) => vec![],
    };
    timestamps
}

// ── Per-project deep breakdown ───────────────────────────────────────────────

/// One row read back from `threads` for per-project aggregation.
type ThreadProjectRow = (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    i64,
);

#[derive(Serialize, Clone, Debug)]
struct CodexProjectBreakdown {
    cwd: String,
    project_name: String,
    session_count: i64,
    total_tokens: i64,
    git_branch: Option<String>,
    git_sha: Option<String>,
    source: Option<String>,
    sandbox_policy: Option<String>,
    approval_mode: Option<String>,
}

/// Full per-project breakdown (cwd, git branch/sha, source, tokens, session
/// count) — git/source/sandbox fields come from that project's most recently
/// updated thread since threads are read newest-first.
fn fetch_project_breakdown(conn: &rusqlite::Connection) -> Vec<CodexProjectBreakdown> {
    let mut stmt = match conn.prepare(
        "SELECT cwd, git_branch, git_sha, source, sandbox_policy, approval_mode, COALESCE(tokens_used, 0) \
         FROM threads WHERE cwd IS NOT NULL AND cwd != '' \
         ORDER BY COALESCE(updated_at_ms, updated_at * 1000) DESC",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let rows = match stmt.query_map([], |row| -> rusqlite::Result<ThreadProjectRow> {
        Ok((
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
            row.get(6)?,
        ))
    }) {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    let mut agg: HashMap<String, CodexProjectBreakdown> = HashMap::new();
    for row in rows.flatten() {
        let (cwd, git_branch, git_sha, source, sandbox_policy, approval_mode, tokens) = row;
        let entry = agg.entry(cwd.clone()).or_insert_with(|| {
            let project_name = std::path::Path::new(&cwd)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| cwd.clone());
            CodexProjectBreakdown {
                cwd: cwd.clone(),
                project_name,
                session_count: 0,
                total_tokens: 0,
                git_branch,
                git_sha,
                source,
                sandbox_policy,
                approval_mode,
            }
        });
        entry.session_count += 1;
        entry.total_tokens += tokens;
    }

    let mut out: Vec<CodexProjectBreakdown> = agg.into_values().collect();
    out.sort_by(|a, b| b.total_tokens.cmp(&a.total_tokens));
    out
}

// ── Model catalog (~/.codex/models_cache.json) ──────────────────────────────

#[derive(Deserialize, Debug, Clone)]
struct ModelCacheReasoningLevel {
    effort: String,
}

#[derive(Deserialize, Debug, Clone)]
struct ModelCacheEntry {
    slug: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    context_window: Option<i64>,
    #[serde(default)]
    max_context_window: Option<i64>,
    #[serde(default)]
    supported_reasoning_levels: Option<Vec<ModelCacheReasoningLevel>>,
    #[serde(default)]
    default_reasoning_level: Option<String>,
    #[serde(default)]
    visibility: Option<String>,
}

#[derive(Deserialize, Debug, Default)]
struct ModelsCacheFile {
    #[serde(default)]
    models: Vec<ModelCacheEntry>,
}

#[derive(Serialize, Clone, Debug)]
struct ModelCatalogEntry {
    slug: String,
    display_name: String,
    description: Option<String>,
    context_window: Option<i64>,
    max_context_window: Option<i64>,
    reasoning_levels: Vec<String>,
    default_reasoning_level: Option<String>,
    visibility: Option<String>,
}

/// Parse `~/.codex/models_cache.json`'s `.models[]` array (the file is an
/// object with `fetched_at`/`etag`/`client_version`/`models`, not a bare array).
fn parse_models_cache_json(content: &str) -> Vec<ModelCatalogEntry> {
    let parsed: ModelsCacheFile = match serde_json::from_str(content) {
        Ok(p) => p,
        Err(_) => return vec![],
    };
    parsed
        .models
        .into_iter()
        .map(|m| {
            let display_name = m.display_name.unwrap_or_else(|| m.slug.clone());
            let reasoning_levels = m
                .supported_reasoning_levels
                .unwrap_or_default()
                .into_iter()
                .map(|l| l.effort)
                .collect();
            ModelCatalogEntry {
                slug: m.slug,
                display_name,
                description: m.description,
                context_window: m.context_window,
                max_context_window: m.max_context_window,
                reasoning_levels,
                default_reasoning_level: m.default_reasoning_level,
                visibility: m.visibility,
            }
        })
        .collect()
}

fn read_model_catalog() -> Vec<ModelCatalogEntry> {
    let path = match codex_dir() {
        Ok(directory) => directory.join("models_cache.json"),
        Err(_) => return vec![],
    };
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    parse_models_cache_json(&content)
}

/// Read sessions from ~/.codex/state_5.sqlite
fn fetch_local_stats() -> Result<CodexLocalStats, String> {
    let conn = open_codex_db().ok_or("state_5.sqlite not found")?;

    // Get total counts
    let total_sessions: i64 = conn
        .query_row("SELECT COUNT(*) FROM threads", [], |row| row.get(0))
        .unwrap_or(0);

    let total_tokens_used: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(tokens_used), 0) FROM threads",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    // Get recent sessions (last 50). created_at/updated_at are epoch-second
    // INTEGER columns (not strings) — read the millisecond variants (falling
    // back to seconds*1000 for older rows) and convert to RFC3339 for display.
    let mut stmt = conn
        .prepare(
            "SELECT id, title, model, COALESCE(tokens_used, 0), source, cwd, git_branch, \
             reasoning_effort, COALESCE(created_at_ms, created_at * 1000), \
             COALESCE(updated_at_ms, updated_at * 1000) \
             FROM threads ORDER BY updated_at DESC LIMIT 50",
        )
        .map_err(|e| format!("Query prepare error: {}", e))?;

    let sessions: Vec<CodexSession> = stmt
        .query_map([], |row| {
            let created_at_ms: i64 = row.get(8)?;
            let updated_at_ms: i64 = row.get(9)?;
            Ok(CodexSession {
                id: row.get(0)?,
                title: row.get(1)?,
                model: row.get(2)?,
                tokens_used: row.get(3)?,
                source: row.get(4)?,
                cwd: row.get(5)?,
                git_branch: row.get(6)?,
                reasoning_effort: row.get(7)?,
                created_at: ms_to_rfc3339(created_at_ms),
                updated_at: ms_to_rfc3339(updated_at_ms),
                tokens_approximate: false,
            })
        })
        .map_err(|e| format!("Query error: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    // Compute tokens_by_model
    let mut tokens_by_model: HashMap<String, i64> = HashMap::new();
    let mut model_stmt = conn
        .prepare("SELECT COALESCE(model, 'unknown'), COALESCE(SUM(tokens_used), 0) FROM threads GROUP BY model")
        .map_err(|e| format!("Model query error: {}", e))?;
    let model_rows = model_stmt
        .query_map([], |row| {
            let model: String = row.get(0)?;
            let tokens: i64 = row.get(1)?;
            Ok((model, tokens))
        })
        .map_err(|e| format!("Model query error: {}", e))?;
    for row in model_rows {
        if let Ok((model, tokens)) = row {
            tokens_by_model.insert(model, tokens);
        }
    }

    // Compute sessions_by_project (from cwd, use last path component)
    let mut sessions_by_project: HashMap<String, i64> = HashMap::new();
    let mut proj_stmt = conn
        .prepare("SELECT cwd, COUNT(*) FROM threads WHERE cwd IS NOT NULL GROUP BY cwd")
        .map_err(|e| format!("Project query error: {}", e))?;
    let proj_rows = proj_stmt
        .query_map([], |row| {
            let cwd: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            Ok((cwd, count))
        })
        .map_err(|e| format!("Project query error: {}", e))?;
    for row in proj_rows {
        if let Ok((cwd, count)) = row {
            let project_name = std::path::Path::new(&cwd)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| cwd.clone());
            *sessions_by_project.entry(project_name).or_insert(0) += count;
        }
    }

    // Cost, per-model tokens and per-session tokens all come from the same
    // per-request corpus, so the page cannot show tokens and costs that
    // disagree. `threads.tokens_used` is not used for any of them: it counts
    // cached reads at full price and, on legacy rollouts, reflects only the
    // final turn segment.
    let corpus = build_usage_corpus(&conn);
    let estimated_total_cost = corpus.total_cost();
    let cost_by_model = corpus.cost_by_model();
    let corpus_tokens_by_model = corpus.tokens_by_model();
    let tokens_by_model = if corpus_tokens_by_model.is_empty() {
        tokens_by_model
    } else {
        corpus_tokens_by_model
    };
    let corpus_total_tokens = corpus.total_tokens();
    let total_tokens_used = if corpus_total_tokens > 0 {
        corpus_total_tokens
    } else {
        total_tokens_used
    };

    // Replace the Recent Sessions token column too, otherwise the per-session
    // list contradicts the corrected totals on the same page.
    let per_thread = corpus.tokens_by_thread();
    let sessions: Vec<CodexSession> = sessions
        .into_iter()
        .map(|mut session| {
            if let Some((tokens, approximate)) = per_thread.get(&session.id) {
                session.tokens_used = *tokens;
                session.tokens_approximate = *approximate;
            }
            session
        })
        .collect();

    Ok(CodexLocalStats {
        total_sessions,
        total_tokens_used,
        sessions,
        tokens_by_model,
        sessions_by_project,
        estimated_total_cost,
        cost_by_model,
        usage_daily: corpus.daily(),
        unpriced_models: corpus.unpriced_models.iter().cloned().collect(),
        unpriced_tokens: corpus.unpriced_tokens,
        approximate_sessions: corpus.approximate_sessions,
        unaccounted_sessions: corpus.unaccounted_sessions,
    })
}

fn parse_codex_config(content: &str) -> (Option<String>, Option<String>) {
    let document = match content.parse::<toml_edit::DocumentMut>() {
        Ok(document) => document,
        Err(_) => return (None, None),
    };

    let model = document
        .get("model")
        .and_then(toml_edit::Item::as_str)
        .map(str::to_string);
    let reasoning_effort = document
        .get("model_reasoning_effort")
        .and_then(toml_edit::Item::as_str)
        .map(str::to_string);
    (model, reasoning_effort)
}

/// Read model settings from the effective Codex home config.
fn read_codex_config() -> (Option<String>, Option<String>) {
    let path = match codex_dir() {
        Ok(directory) => directory.join("config.toml"),
        Err(_) => return (None, None),
    };
    if !path.exists() {
        return (None, None);
    }
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return (None, None),
    };
    parse_codex_config(&content)
}

/// Fetch account profile from api.openai.com/v1/me
fn fetch_account_profile(token: &str) -> Option<OpenAIMeResponse> {
    http::authed_get::<OpenAIMeResponse>("https://api.openai.com/v1/me", token, None).ok()
}

/// Stats for a time window
struct CodexWindowStats {
    sessions: i64,
    tokens: i64,
    cost: f64,
}

/// Aggregate a time window from per-request usage rows.
///
/// Windows are filtered on each request's **own** local date. The previous
/// SQL version matched whole threads by `created_at`/`updated_at` and then
/// attributed the session's entire lifetime tokens to that window, so a
/// session created on one day and touched days later dumped all of its tokens
/// into both windows.
fn window_from_rows(rows: &[UsageRow], from_date: &str, to_date: Option<&str>) -> CodexWindowStats {
    let mut tokens = 0i64;
    let mut cost = 0.0f64;
    let mut threads: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for row in rows {
        if row.date.is_empty() || row.date.as_str() < from_date {
            continue;
        }
        if let Some(to) = to_date {
            if row.date.as_str() > to {
                continue;
            }
        }
        tokens += row.usage.total_tokens as i64;
        if let Some(c) = estimate_usage_cost(&row.usage, &row.model) {
            cost += c;
        }
        threads.insert(row.thread_id.as_str());
    }
    CodexWindowStats { sessions: threads.len() as i64, tokens, cost }
}

/// Get stats for 2 time windows from SQLite: today (since midnight), this week (since Monday)
fn fetch_multi_window_stats() -> Option<(CodexWindowStats, CodexWindowStats)> {
    let conn = open_codex_db()?;

    let local_now = chrono::Local::now();
    let today_str = local_now.format("%Y-%m-%d").to_string();

    // This week = Monday's date
    use chrono::Datelike;
    let weekday = local_now.weekday().num_days_from_monday();
    let monday = local_now.date_naive() - chrono::Duration::days(weekday as i64);
    let monday_str = monday.format("%Y-%m-%d").to_string();

    let corpus = build_usage_corpus(&conn);
    let stats_today = window_from_rows(&corpus.rows, &today_str, Some(&today_str));
    let stats_week = window_from_rows(&corpus.rows, &monday_str, None);

    Some((stats_today, stats_week))
}

/// Enrich an extra HashMap with all local data. Returns offline (rollout-derived)
/// rate-limit windows so the caller can fall back to them when the WHAM API
/// call fails or returns nothing.
fn enrich_with_local_data(
    extra: &mut HashMap<String, serde_json::Value>,
) -> Option<Vec<RateLimitWindow>> {
    // SQLite local stats
    if let Ok(stats) = fetch_local_stats() {
        extra.insert(
            "total_sessions".into(),
            serde_json::json!(stats.total_sessions),
        );
        extra.insert(
            "total_tokens_used".into(),
            serde_json::json!(stats.total_tokens_used),
        );
        extra.insert(
            "sessions".into(),
            serde_json::to_value(&stats.sessions).unwrap_or_default(),
        );
        extra.insert(
            "tokens_by_model".into(),
            serde_json::to_value(&stats.tokens_by_model).unwrap_or_default(),
        );
        extra.insert(
            "usage_daily".into(),
            serde_json::to_value(&stats.usage_daily).unwrap_or_default(),
        );
        extra.insert(
            "unpriced_models".into(),
            serde_json::to_value(&stats.unpriced_models).unwrap_or_default(),
        );
        extra.insert(
            "unpriced_tokens".into(),
            serde_json::json!(stats.unpriced_tokens),
        );
        extra.insert(
            "approximate_sessions".into(),
            serde_json::json!(stats.approximate_sessions),
        );
        extra.insert(
            "unaccounted_sessions".into(),
            serde_json::json!(stats.unaccounted_sessions),
        );
        extra.insert(
            "sessions_by_project".into(),
            serde_json::to_value(&stats.sessions_by_project).unwrap_or_default(),
        );
        extra.insert(
            "estimated_total_cost".into(),
            serde_json::json!(stats.estimated_total_cost),
        );
        extra.insert(
            "cost_by_model".into(),
            serde_json::to_value(&stats.cost_by_model).unwrap_or_default(),
        );
    }

    // Multi-window stats (today, this week)
    if let Some((stats_today, stats_week)) = fetch_multi_window_stats() {
        extra.insert(
            "start_today_sessions".into(),
            serde_json::json!(stats_today.sessions),
        );
        extra.insert(
            "start_today_tokens".into(),
            serde_json::json!(stats_today.tokens),
        );
        extra.insert(
            "start_today_cost".into(),
            serde_json::json!(stats_today.cost),
        );
        extra.insert(
            "this_week_sessions".into(),
            serde_json::json!(stats_week.sessions),
        );
        extra.insert(
            "this_week_tokens".into(),
            serde_json::json!(stats_week.tokens),
        );
        extra.insert("this_week_cost".into(), serde_json::json!(stats_week.cost));
    }

    // Model catalog (rich cards) + slug list for existing badges
    let model_catalog = read_model_catalog();
    if !model_catalog.is_empty() {
        extra.insert(
            "available_models".into(),
            serde_json::json!(model_catalog
                .iter()
                .map(|m| m.slug.clone())
                .collect::<Vec<_>>()),
        );
        extra.insert(
            "model_catalog".into(),
            serde_json::to_value(&model_catalog).unwrap_or_default(),
        );
    }

    // Config
    let (config_model, config_reasoning) = read_codex_config();
    if let Some(ref m) = config_model {
        extra.insert("config_model".into(), serde_json::Value::String(m.clone()));
    }
    if let Some(r) = config_reasoning {
        extra.insert(
            "config_reasoning_effort".into(),
            serde_json::Value::String(r),
        );
    }

    // Activity (daily heatmap, streaks, peak hour) + per-project breakdown +
    // rollout-derived token totals & offline rate-limit fallback.
    let mut offline_windows = None;
    if let Some(conn) = open_codex_db() {
        let timestamps = fetch_activity_timestamps(&conn);
        if !timestamps.is_empty() {
            let activity = compute_activity_stats(&timestamps);
            extra.insert(
                "daily_activity".into(),
                serde_json::to_value(&activity.daily_activity).unwrap_or_default(),
            );
            extra.insert(
                "hour_counts".into(),
                serde_json::json!(activity.hour_counts.to_vec()),
            );
            extra.insert(
                "active_days".into(),
                serde_json::json!(activity.active_days),
            );
            extra.insert(
                "longest_streak".into(),
                serde_json::json!(activity.longest_streak),
            );
            extra.insert(
                "current_streak".into(),
                serde_json::json!(activity.current_streak),
            );
            if let Some(ph) = activity.peak_hour {
                extra.insert("peak_hour".into(), serde_json::json!(ph));
            }
        }

        let projects = fetch_project_breakdown(&conn);
        if !projects.is_empty() {
            extra.insert(
                "project_breakdown".into(),
                serde_json::to_value(&projects).unwrap_or_default(),
            );
        }

        let scan = scan_recent_rollouts(&conn);
        extra.insert(
            "token_breakdown".into(),
            serde_json::to_value(&scan.token_breakdown).unwrap_or_default(),
        );
        // Cost for the breakdown is taken from the per-request corpus, where
        // every request is priced at its own turn's model. Pricing this
        // aggregate at a single `config_model` was wrong whenever a session
        // used more than one model.
        let breakdown_cost = open_codex_db()
            .map(|conn| build_usage_corpus(&conn).total_cost())
            .unwrap_or(0.0);
        extra.insert(
            "token_breakdown_estimated_cost".into(),
            serde_json::json!(breakdown_cost),
        );
        // Pricing is per turn now, so a single "cost model" label would be
        // false. Expose the rate table's basis date instead.
        extra.insert(
            "rates_as_of".into(),
            serde_json::Value::String(RATES_AS_OF.to_string()),
        );

        if let Some(rl) = scan.offline_rate_limits {
            let windows = rollout_rate_limits_to_windows(&rl);
            if let Some(pt) = rl.plan_type {
                extra.insert("offline_plan_type".into(), serde_json::Value::String(pt));
            }
            if !windows.is_empty() {
                extra.insert(
                    "offline_rate_limits".into(),
                    serde_json::to_value(&windows).unwrap_or_default(),
                );
                offline_windows = Some(windows);
            }
        }
    }

    offline_windows
}

/// Enrich extra HashMap with account profile from /v1/me
fn enrich_with_account_profile(extra: &mut HashMap<String, serde_json::Value>, token: &str) {
    if let Some(profile) = fetch_account_profile(token) {
        if let Some(name) = profile.name {
            extra.insert("account_name".into(), serde_json::Value::String(name));
        }
        if let Some(orgs) = profile.orgs {
            let org_values: Vec<serde_json::Value> = orgs
                .iter()
                .map(|o| {
                    serde_json::json!({
                        "id": o.id,
                        "title": o.title,
                        "role": o.role,
                    })
                })
                .collect();
            extra.insert("organizations".into(), serde_json::json!(org_values));
        }
    }
}

pub fn fetch_codex_analytics() -> ProviderAnalytics {
    // Return cached data if still fresh
    if let Ok(guard) = CACHE.lock() {
        if let Some(ref entry) = *guard {
            if entry.fetched_at.elapsed().as_secs() < CACHE_TTL_SECS {
                return entry.data.clone();
            }
        }
    }

    let mut result = fetch_codex_analytics_uncached();

    // A transient WHAM failure still reports connected (local data exists) but
    // with empty rate_limits. Caching that blanks the popover bars for a full
    // TTL while the menu-bar title may show the previous cycle's percentage.
    // Instead, graft the last good windows into the result and keep the old
    // cache entry so the next cycle retries.
    let wham_failed = result.status.error.is_some() && result.rate_limits.is_empty();
    if wham_failed {
        if let Ok(guard) = CACHE.lock() {
            if let Some(ref prev) = *guard {
                if !prev.data.rate_limits.is_empty() {
                    result.rate_limits = prev.data.rate_limits.clone();
                    result.limit_state = prev.data.limit_state.clone();
                }
            }
        }
        return result;
    }

    // Cache successful results (connected = true)
    if result.status.connected {
        if let Ok(mut guard) = CACHE.lock() {
            *guard = Some(CacheEntry {
                data: result.clone(),
                fetched_at: std::time::Instant::now(),
            });
        }
    }

    result
}

fn fetch_codex_analytics_from_app_server(now: &str) -> Result<ProviderAnalytics, String> {
    let result = codex_app_server::request("account/rateLimits/read", serde_json::json!({}))
        .map_err(|error| error.to_string())?;
    let response: AppServerRateLimitsResponse = serde_json::from_value(result)
        .map_err(|error| format!("Could not parse Codex App Server rate limits: {error}"))?;
    let limits = ordered_app_server_limits(&response);
    let mut rate_limits = Vec::new();
    for limit in &limits {
        append_app_server_windows(&mut rate_limits, limit);
    }

    let account_result =
        codex_app_server::request("account/read", serde_json::json!({ "refreshToken": false }));
    let (account_email, account_plan, account_warning) = match account_result {
        Ok(value) => (
            value
                .pointer("/account/email")
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string),
            value
                .pointer("/account/planType")
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string),
            None,
        ),
        Err(error) => (None, None, Some(error.to_string())),
    };
    let plan_name = account_plan.or_else(|| {
        limits
            .iter()
            .find_map(|limit| limit.plan_type.as_ref().cloned())
    });
    let credit_usage = app_server_credit_usage(&limits);
    let limit_state = app_server_limit_state(&limits);

    let mut extra = HashMap::new();
    extra.insert(
        "rate_limit_source".into(),
        serde_json::Value::String("codex-app-server".into()),
    );
    if let Some(plan) = plan_name.as_ref() {
        extra.insert("plan_type".into(), serde_json::Value::String(plan.clone()));
    }
    if limits
        .iter()
        .any(|limit| limit.credits.as_ref().and_then(|credits| credits.unlimited) == Some(true))
    {
        extra.insert("unlimited_credits".into(), serde_json::Value::Bool(true));
    }
    if let Some(warning) = account_warning {
        extra.insert(
            "account_metadata_warning".into(),
            serde_json::Value::String(warning),
        );
    }

    let offline_rate_limits = enrich_with_local_data(&mut extra).unwrap_or_default();
    if rate_limits.is_empty() {
        rate_limits = offline_rate_limits;
    }

    Ok(ProviderAnalytics {
        provider_id: "codex".into(),
        provider_name: "Codex (OpenAI)".into(),
        status: ProviderStatus {
            provider_id: "codex".into(),
            provider_name: "Codex (OpenAI)".into(),
            connected: true,
            connection_method: "app-server".into(),
            account_email,
            plan_name,
            org_name: None,
            error: None,
        },
        rate_limits,
        credit_usage,
        token_counts: None,
        limit_state,
        extra,
        fetched_at: now.to_string(),
    })
}

fn fetch_codex_analytics_uncached() -> ProviderAnalytics {
    let now = Utc::now().to_rfc3339();
    let app_server_error = match fetch_codex_analytics_from_app_server(&now) {
        Ok(analytics) => return analytics,
        Err(error) => error,
    };

    let (token, method, account_id) = match resolve_token() {
        Ok(t) => t,
        Err(e) => {
            // Even without a token, enrich with local data if available
            let mut extra = HashMap::new();
            let offline_rate_limits = enrich_with_local_data(&mut extra).unwrap_or_default();
            let has_local = extra.contains_key("total_sessions");

            extra.insert(
                "rate_limit_source_warning".into(),
                serde_json::Value::String(app_server_error.clone()),
            );

            return ProviderAnalytics {
                provider_id: "codex".into(),
                provider_name: "Codex (OpenAI)".into(),
                status: ProviderStatus {
                    provider_id: "codex".into(),
                    provider_name: "Codex (OpenAI)".into(),
                    connected: has_local, // connected if we have local data
                    connection_method: if has_local {
                        "local-file".into()
                    } else {
                        "none".into()
                    },
                    account_email: None,
                    plan_name: None,
                    org_name: None,
                    error: if has_local {
                        None
                    } else {
                        Some(format!("{app_server_error}; {e}"))
                    },
                },
                rate_limits: offline_rate_limits,
                credit_usage: None,
                token_counts: None,
                limit_state: None,
                extra,
                fetched_at: now,
            };
        }
    };

    let mut headers_vec: Vec<(&str, &str)> = vec![("User-Agent", "CodexBar")];
    let account_id_owned = account_id.unwrap_or_default();
    if !account_id_owned.is_empty() {
        headers_vec.push(("ChatGPT-Account-Id", &account_id_owned));
    }
    let extra_headers = http::headers(&headers_vec);

    let resp: Result<WhamUsageResponse, String> = http::authed_get(
        "https://chatgpt.com/backend-api/wham/usage",
        &token,
        Some(extra_headers),
    )
    .map_err(String::from);

    match resp {
        Ok(wham) => {
            let codex_limit_state = wham.rate_limit.as_ref().and_then(|rl| {
                if rl.limit_reached == Some(true) || rl.allowed == Some(false) {
                    Some(LimitState::Reached {
                        scope: LimitScope::Custom("wham_primary".into()),
                        used_pct: 100.0,
                        cap: None,
                        resets_at: None,
                    })
                } else {
                    None
                }
            });

            let mut rate_limits = Vec::new();
            if let Some(ref rl) = wham.rate_limit {
                append_wham_windows(&mut rate_limits, rl, None);
            }
            if let Some(ref additional_limits) = wham.additional_rate_limits {
                for additional in additional_limits {
                    if let Some(ref rl) = additional.rate_limit {
                        let fallback_name = additional
                            .metered_feature
                            .as_deref()
                            .unwrap_or("Additional limit");
                        let name = additional.limit_name.as_deref().unwrap_or(fallback_name);
                        append_wham_windows(&mut rate_limits, rl, Some(name));
                    }
                }
            }
            if let Some(ref cr) = wham.code_review_rate_limit {
                if let Some(w) = parse_wham_window(&cr.primary_window, "Code Review (7d)") {
                    rate_limits.push(w);
                }
            }

            let credit_usage =
                parse_wham_credit_usage(wham.credits.as_ref(), wham.plan_type.as_deref());

            let mut extra = HashMap::new();
            extra.insert(
                "rate_limit_source_warning".into(),
                serde_json::Value::String(app_server_error.clone()),
            );
            if let Some(ref pt) = wham.plan_type {
                extra.insert("plan_type".into(), serde_json::Value::String(pt.clone()));
            }
            if let Some(ref cr) = wham.credits {
                if let Some(true) = cr.unlimited {
                    extra.insert("unlimited_credits".into(), serde_json::Value::Bool(true));
                }
            }

            // Enrich with local data (SQLite, models_cache, config)
            let offline_rate_limits = enrich_with_local_data(&mut extra).unwrap_or_default();

            // Enrich with account profile from /v1/me
            enrich_with_account_profile(&mut extra, &token);

            // WHAM returned no usable windows (e.g. empty rate_limit object) —
            // fall back to the rollout-derived offline snapshot.
            let rate_limits = if rate_limits.is_empty() {
                offline_rate_limits
            } else {
                rate_limits
            };

            ProviderAnalytics {
                provider_id: "codex".into(),
                provider_name: "Codex (OpenAI)".into(),
                status: ProviderStatus {
                    provider_id: "codex".into(),
                    provider_name: "Codex (OpenAI)".into(),
                    connected: true,
                    connection_method: method,
                    account_email: wham.email,
                    plan_name: wham.plan_type,
                    org_name: None,
                    error: None,
                },
                rate_limits,
                credit_usage,
                token_counts: None,
                limit_state: codex_limit_state,
                extra,
                fetched_at: now,
            }
        }
        Err(e) => {
            // Even if API fails, enrich with local data — including the
            // rollout-derived rate-limit fallback, since WHAM is unreachable.
            let mut extra = HashMap::new();
            let offline_rate_limits = enrich_with_local_data(&mut extra).unwrap_or_default();

            ProviderAnalytics {
                provider_id: "codex".into(),
                provider_name: "Codex (OpenAI)".into(),
                status: ProviderStatus {
                    provider_id: "codex".into(),
                    provider_name: "Codex (OpenAI)".into(),
                    connected: true,
                    connection_method: method,
                    account_email: None,
                    plan_name: None,
                    org_name: None,
                    error: Some(format!("{app_server_error}; WHAM fallback failed: {e}")),
                },
                rate_limits: offline_rate_limits,
                credit_usage: None,
                token_counts: None,
                limit_state: None,
                extra,
                fetched_at: now,
            }
        }
    }
}

pub fn check_connection() -> ProviderStatus {
    // 1. Prefer the documented App Server account surface. This also works
    // when Codex stores credentials in the operating system keychain.
    if let Ok(result) =
        codex_app_server::request("account/read", serde_json::json!({ "refreshToken": false }))
    {
        if result
            .get("account")
            .is_some_and(|account| !account.is_null())
        {
            return ProviderStatus {
                provider_id: "codex".into(),
                provider_name: "Codex (OpenAI)".into(),
                connected: true,
                connection_method: "app-server".into(),
                account_email: result
                    .pointer("/account/email")
                    .and_then(serde_json::Value::as_str)
                    .map(ToString::to_string),
                plan_name: result
                    .pointer("/account/planType")
                    .and_then(serde_json::Value::as_str)
                    .map(ToString::to_string),
                org_name: None,
                error: None,
            };
        }
    }

    // 2. Try resolving a token (auth.json or manual)
    if let Ok((_, method, _)) = resolve_token() {
        return ProviderStatus {
            provider_id: "codex".into(),
            provider_name: "Codex (OpenAI)".into(),
            connected: true,
            connection_method: method,
            account_email: None,
            plan_name: None,
            org_name: None,
            error: None,
        };
    }
    // 3. Fallback: check local SQLite database
    let db_path = codex_dir().map(|directory| directory.join("state_5.sqlite"));
    if db_path.as_ref().is_ok_and(|path| path.exists()) {
        return ProviderStatus {
            provider_id: "codex".into(),
            provider_name: "Codex (OpenAI)".into(),
            connected: true,
            connection_method: "local-file".into(),
            account_email: None,
            plan_name: None,
            org_name: None,
            error: None,
        };
    }
    // 3. Not connected
    ProviderStatus {
        provider_id: "codex".into(),
        provider_name: "Codex (OpenAI)".into(),
        connected: false,
        connection_method: "none".into(),
        account_email: None,
        plan_name: None,
        org_name: None,
        error: Some("Codex not installed or not signed in".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_documented_model_settings_from_top_level_config() {
        let config = r#"
model = "gpt-5.6-sol"
model_reasoning_effort = "high"

[profiles.fast]
model = "gpt-5.6-luna"
"#;
        let (model, effort) = parse_codex_config(config);
        assert_eq!(model.as_deref(), Some("gpt-5.6-sol"));
        assert_eq!(effort.as_deref(), Some("high"));
    }

    #[test]
    fn invalid_config_does_not_leak_partial_model_settings() {
        let (model, effort) = parse_codex_config("model = [\n");
        assert!(model.is_none());
        assert!(effort.is_none());
    }

    // ── Rollout token_count parsing ─────────────────────────────────────────

    const NESTED_TOKEN_COUNT_LINE: &str = r#"{"timestamp":"2026-08-18T06:28:21.274Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":12515,"cached_input_tokens":4864,"cache_write_input_tokens":0,"output_tokens":161,"reasoning_output_tokens":95,"total_tokens":12676},"last_token_usage":{"input_tokens":100,"cached_input_tokens":0,"cache_write_input_tokens":0,"output_tokens":10,"reasoning_output_tokens":0,"total_tokens":110},"model_context_window":258400},"rate_limits":{"limit_id":"codex","primary":{"used_percent":3.0,"window_minutes":10080,"resets_at":1787421418},"secondary":null,"plan_type":"team"}}}"#;

    #[test]
    fn parses_nested_token_count_line() {
        let (usage, rate_limits) = parse_token_count_line(NESTED_TOKEN_COUNT_LINE).unwrap();
        assert_eq!(usage.input_tokens, 12515);
        assert_eq!(usage.cached_input_tokens, 4864);
        assert_eq!(usage.output_tokens, 161);
        assert_eq!(usage.reasoning_output_tokens, 95);
        assert_eq!(usage.total_tokens, 12676);

        let rl = rate_limits.unwrap();
        assert_eq!(rl.plan_type.as_deref(), Some("team"));
        let primary = rl.primary.unwrap();
        assert_eq!(primary.used_percent, Some(3.0));
        assert_eq!(primary.window_minutes, Some(10080));
        assert_eq!(primary.resets_at, Some(1787421418));
        assert!(rl.secondary.is_none());
    }

    #[test]
    fn parses_flat_token_count_fallback_shape() {
        let line = r#"{"type":"event_msg","payload":{"type":"token_count","total_token_usage":{"input_tokens":50,"cached_input_tokens":10,"cache_write_input_tokens":0,"output_tokens":5,"reasoning_output_tokens":0,"total_tokens":55}}}"#;
        let (usage, rate_limits) = parse_token_count_line(line).unwrap();
        assert_eq!(usage.input_tokens, 50);
        assert_eq!(usage.total_tokens, 55);
        assert!(rate_limits.is_none());
    }

    #[test]
    fn ignores_non_token_count_events() {
        let session_meta = r#"{"type":"session_meta","payload":{"type":"session_meta_data"}}"#;
        assert!(parse_token_count_line(session_meta).is_none());

        let other_event = r#"{"type":"event_msg","payload":{"type":"agent_message"}}"#;
        assert!(parse_token_count_line(other_event).is_none());
    }

    #[test]
    fn skips_corrupt_lines_without_panicking() {
        assert!(parse_token_count_line("not json at all").is_none());
        assert!(parse_token_count_line("").is_none());
        assert!(parse_token_count_line(r#"{"type":"event_msg"}"#).is_none());
    }

    #[test]
    fn scan_rollout_content_keeps_last_token_count_line() {
        let first = r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":0,"cache_write_input_tokens":0,"output_tokens":1,"reasoning_output_tokens":0,"total_tokens":11}}}}"#;
        let second = r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":20,"cache_write_input_tokens":0,"output_tokens":10,"reasoning_output_tokens":5,"total_tokens":110}},"rate_limits":{"primary":{"used_percent":50.0,"window_minutes":10080,"resets_at":1700000000}}}}"#;
        let content = [first, "garbage line", second].join("\n");

        let (usage, rate_limits) = scan_rollout_content(&content).unwrap();
        // Last-wins: cumulative usage should be the second (later) event, not a sum of both lines.
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.total_tokens, 110);
        assert!(rate_limits.is_some());
    }

    #[test]
    fn rollout_token_usage_add_sums_across_sessions() {
        let mut total = RolloutTokenUsage {
            input_tokens: 10,
            cached_input_tokens: 2,
            cache_write_input_tokens: 1,
            output_tokens: 5,
            reasoning_output_tokens: 1,
            total_tokens: 15,
        };
        let other = RolloutTokenUsage {
            input_tokens: 90,
            cached_input_tokens: 8,
            cache_write_input_tokens: 0,
            output_tokens: 15,
            reasoning_output_tokens: 4,
            total_tokens: 105,
        };
        total.add(&other);
        assert_eq!(total.input_tokens, 100);
        assert_eq!(total.cached_input_tokens, 10);
        assert_eq!(total.output_tokens, 20);
        assert_eq!(total.total_tokens, 120);
    }

    // ── models_cache.json `.models[]` parsing ───────────────────────────────

    const MODELS_CACHE_FIXTURE: &str = r#"{
        "fetched_at": "2026-08-26T00:00:00Z",
        "etag": "abc123",
        "client_version": "0.147.0",
        "models": [
            {
                "slug": "gpt-5.6-sol",
                "display_name": "GPT-5.6-Sol",
                "description": "Latest frontier agentic coding model.",
                "context_window": 272000,
                "max_context_window": 872000,
                "default_reasoning_level": "medium",
                "supported_reasoning_levels": [
                    {"effort": "low", "description": "Fast"},
                    {"effort": "medium", "description": "Balanced"},
                    {"effort": "high", "description": "Deep"}
                ],
                "visibility": "list"
            },
            {
                "slug": "gpt-5.4-mini",
                "context_window": 272000,
                "max_context_window": 272000
            }
        ]
    }"#;

    #[test]
    fn parses_models_cache_dot_models_array() {
        let models = parse_models_cache_json(MODELS_CACHE_FIXTURE);
        assert_eq!(models.len(), 2);

        let sol = models.iter().find(|m| m.slug == "gpt-5.6-sol").unwrap();
        assert_eq!(sol.display_name, "GPT-5.6-Sol");
        assert_eq!(sol.context_window, Some(272000));
        assert_eq!(sol.max_context_window, Some(872000));
        assert_eq!(sol.reasoning_levels, ["low", "medium", "high"]);
        assert_eq!(sol.default_reasoning_level.as_deref(), Some("medium"));

        let mini = models.iter().find(|m| m.slug == "gpt-5.4-mini").unwrap();
        // display_name falls back to slug when absent.
        assert_eq!(mini.display_name, "gpt-5.4-mini");
        assert!(mini.reasoning_levels.is_empty());
    }

    #[test]
    fn rejects_a_top_level_array_shape() {
        // The old (buggy) assumption was a bare top-level array — that shape
        // must not be mistaken for the real `{ models: [...] }` object.
        let bare_array = r#"[{"slug":"gpt-5.5"}]"#;
        assert!(parse_models_cache_json(bare_array).is_empty());
    }

    // ── Rate-limit fallback mapping ──────────────────────────────────────────

    #[test]
    fn maps_rollout_rate_limits_to_windows() {
        let rl = RolloutRateLimits {
            primary: Some(RolloutRateWindow {
                used_percent: Some(29.0),
                window_minutes: Some(10080),
                resets_at: Some(1786827418),
            }),
            secondary: Some(RolloutRateWindow {
                used_percent: Some(5.0),
                window_minutes: Some(300),
                resets_at: None,
            }),
            plan_type: Some("team".into()),
        };
        let windows = rollout_rate_limits_to_windows(&rl);
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].label, "Primary (7d)");
        assert_eq!(windows[0].used_percent, 29.0);
        assert_eq!(windows[0].remaining_percent, 71.0);
        assert!(windows[0].resets_at.is_some());

        assert_eq!(windows[1].label, "Secondary (5h)");
        assert_eq!(windows[1].used_percent, 5.0);
        assert!(windows[1].resets_at.is_none());
    }

    #[test]
    fn skips_rate_windows_without_used_percent() {
        let rl = RolloutRateLimits {
            primary: Some(RolloutRateWindow {
                used_percent: None,
                window_minutes: Some(10080),
                resets_at: None,
            }),
            secondary: None,
            plan_type: None,
        };
        assert!(rollout_rate_limits_to_windows(&rl).is_empty());
    }

    // ── SQLite date-window fix (today/this-week) ────────────────────────────
    // ── Cost estimation (per-request, per-model rates) ──────────────────────

    fn usage(input: u64, cached: u64, cache_write: u64, output: u64) -> RolloutTokenUsage {
        RolloutTokenUsage {
            input_tokens: input,
            cached_input_tokens: cached,
            cache_write_input_tokens: cache_write,
            output_tokens: output,
            reasoning_output_tokens: 0,
            total_tokens: input + output,
        }
    }

    #[test]
    fn daybreak_alias_resolves_to_sol_rates() {
        assert_eq!(
            model_rates("gpt-daybreak-blue-latest"),
            model_rates("gpt-5.6-sol")
        );
    }

    #[test]
    fn cached_input_is_ten_percent_of_input() {
        let rates = model_rates("gpt-5.6-sol").unwrap();
        assert!((rates.cached_input - rates.input * 0.10).abs() < 1e-9);
    }

    #[test]
    fn cache_write_is_125_percent_of_input() {
        let rates = model_rates("gpt-5.6-sol").unwrap();
        assert!((rates.cache_write - rates.input * 1.25).abs() < 1e-9);
    }

    #[test]
    fn reasoning_tokens_do_not_change_cost() {
        // reasoning_output_tokens is already inside output_tokens.
        let mut a = usage(1_000, 0, 0, 1_000);
        let base = estimate_usage_cost(&a, "gpt-5.5").unwrap();
        a.reasoning_output_tokens = 900;
        assert!((estimate_usage_cost(&a, "gpt-5.5").unwrap() - base).abs() < 1e-12);
    }

    #[test]
    fn unknown_model_is_unpriced_not_defaulted() {
        assert!(model_rates("totally-unknown-model").is_none());
        assert!(estimate_usage_cost(&usage(1_000_000, 0, 0, 0), "totally-unknown-model").is_none());
    }

    #[test]
    fn exact_ids_do_not_collide_on_substrings() {
        // gpt-5.5-pro has different rates and no cached discount: a
        // contains("gpt-5.5") match would misprice it.
        assert!(model_rates("gpt-5.5-pro").is_none());
        assert_ne!(model_rates("gpt-5.4-mini"), model_rates("gpt-5.4"));
        assert_ne!(model_rates("gpt-5.6-sol"), model_rates("gpt-5.6-terra"));
    }

    #[test]
    fn long_context_requests_use_the_higher_tier() {
        let small = usage(1_000, 0, 0, 1_000);
        let large = usage(LONG_CONTEXT_THRESHOLD + 1, 0, 0, 1_000);
        let rates = model_rates("gpt-5.5").unwrap();

        let small_cost = estimate_usage_cost(&small, "gpt-5.5").unwrap();
        let expected_small =
            (1_000.0 / 1e6) * rates.input + (1_000.0 / 1e6) * rates.output;
        assert!((small_cost - expected_small).abs() < 1e-12);

        let large_cost = estimate_usage_cost(&large, "gpt-5.5").unwrap();
        let expected_large = ((LONG_CONTEXT_THRESHOLD + 1) as f64 / 1e6) * rates.input * 2.0
            + (1_000.0 / 1e6) * rates.output * 1.5;
        assert!((large_cost - expected_large).abs() < 1e-9);
    }

    #[test]
    fn cached_and_cache_write_are_subtracted_from_ordinary_input() {
        let rates = model_rates("gpt-5.5").unwrap();
        // Kept under LONG_CONTEXT_THRESHOLD so this exercises the base tier.
        let u = usage(100_000, 60_000, 30_000, 0);
        let expected = (10_000.0 / 1e6) * rates.input
            + (60_000.0 / 1e6) * rates.cached_input
            + (30_000.0 / 1e6) * rates.cache_write;
        assert!((estimate_usage_cost(&u, "gpt-5.5").unwrap() - expected).abs() < 1e-9);
    }

    #[test]
    fn oversubscribed_cache_counts_do_not_underflow() {
        // Saturating arithmetic: cached + cache_write > input must not panic
        // or wrap around.
        let u = usage(10, 900, 900, 0);
        assert!(estimate_usage_cost(&u, "gpt-5.5").is_some());
    }

    // ── Rollout usage scanning (two formats) ────────────────────────────────

    fn write_rollout(lines: &[String]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollout-test.jsonl");
        fs::write(&path, lines.join("\n")).unwrap();
        (dir, path)
    }

    #[test]
    fn modern_rollout_yields_one_row_per_request() {
        let lines = vec![
            r#"{"type":"event_msg","payload":{"type":"turn_context","turn_id":"t1","model":"gpt-5.5"}}"#.to_string(),
            r#"{"type":"token_usage_record","timestamp":"2026-09-07T20:08:48.671Z","payload":{"response_id":"r1","turn_id":"t1","usage":{"input_tokens":1000,"cached_input_tokens":0,"cache_write_input_tokens":0,"output_tokens":500,"total_tokens":1500}}}"#.to_string(),
        ];
        let (_dir, path) = write_rollout(&lines);
        let scan = scan_rollout_usage(&path, "thread-1", "gpt-5.4");
        assert_eq!(scan.rows.len(), 1);
        assert_eq!(scan.rows[0].model, "gpt-5.5"); // turn model, not db fallback
        assert!(!scan.rows[0].approximate);
        assert_eq!(scan.rows[0].date, "2026-09-08");
    }

    #[test]
    fn duplicate_response_ids_are_counted_once() {
        let record = r#"{"type":"token_usage_record","timestamp":"2026-09-07T20:08:48.671Z","payload":{"response_id":"r1","turn_id":"t1","usage":{"input_tokens":1000,"output_tokens":500,"total_tokens":1500}}}"#;
        let lines = vec![
            r#"{"type":"event_msg","payload":{"type":"turn_context","turn_id":"t1","model":"gpt-5.5"}}"#.to_string(),
            record.to_string(),
            record.to_string(),
        ];
        let (_dir, path) = write_rollout(&lines);
        let scan = scan_rollout_usage(&path, "thread-1", "gpt-5.5");
        assert_eq!(scan.rows.len(), 1);
    }

    #[test]
    fn turn_context_arriving_after_its_record_still_attributes() {
        // Verified to happen in real rollouts: running state alone would
        // mis-attribute the first request of a turn.
        let lines = vec![
            r#"{"type":"token_usage_record","timestamp":"2026-09-07T20:08:48.671Z","payload":{"response_id":"r1","turn_id":"t9","usage":{"input_tokens":1000,"output_tokens":0,"total_tokens":1000}}}"#.to_string(),
            r#"{"type":"event_msg","payload":{"type":"turn_context","turn_id":"t9","model":"gpt-5.6-sol"}}"#.to_string(),
        ];
        let (_dir, path) = write_rollout(&lines);
        let scan = scan_rollout_usage(&path, "thread-1", "gpt-5.4");
        assert_eq!(scan.rows[0].model, "gpt-5.6-sol");
        assert!(!scan.rows[0].model_fallback);
    }

    #[test]
    fn malformed_line_is_skipped_without_losing_others() {
        let lines = vec![
            r#"{"type":"event_msg","payload":{"type":"turn_context","turn_id":"t1","model":"gpt-5.5"}}"#.to_string(),
            r#"{"type":"token_usage_record","timestamp":"2026-09-07T20:08:48.671Z","payload":{"response_id":"r1","usage":{"input_tokens":10,"output_tokens":0,"total_tokens":10}}}"#.to_string(),
            r#"{"type":"token_usage_record","timestamp":"2026-09-"#.to_string(), // truncated
        ];
        let (_dir, path) = write_rollout(&lines);
        let scan = scan_rollout_usage(&path, "thread-1", "gpt-5.5");
        assert_eq!(scan.rows.len(), 1);
    }

    #[test]
    fn legacy_rollout_survives_counter_resets() {
        // Legacy counters reset between turns, so the final snapshot is not
        // the lifetime total. Each turn must be retained separately.
        let lines = vec![
            r#"{"type":"event_msg","timestamp":"2026-09-07T10:00:00.000Z","payload":{"type":"turn_context","turn_id":"t1","model":"gpt-5.5"}}"#.to_string(),
            r#"{"type":"event_msg","timestamp":"2026-09-07T10:00:01.000Z","payload":{"type":"token_count","turn_id":"t1","info":{"last_token_usage":{"input_tokens":5000,"output_tokens":100,"total_tokens":5100}}}}"#.to_string(),
            r#"{"type":"event_msg","timestamp":"2026-09-07T10:05:00.000Z","payload":{"type":"turn_context","turn_id":"t2","model":"gpt-5.5"}}"#.to_string(),
            r#"{"type":"event_msg","timestamp":"2026-09-07T10:05:01.000Z","payload":{"type":"token_count","turn_id":"t2","info":{"last_token_usage":{"input_tokens":200,"output_tokens":50,"total_tokens":250}}}}"#.to_string(),
        ];
        let (_dir, path) = write_rollout(&lines);
        let scan = scan_rollout_usage(&path, "thread-1", "gpt-5.5");
        assert_eq!(scan.rows.len(), 2, "each legacy turn is retained");
        assert!(scan.rows.iter().all(|r| r.approximate));
        let total: u64 = scan.rows.iter().map(|r| r.usage.total_tokens).sum();
        assert_eq!(total, 5350, "not just the final 250-token segment");
    }

    #[test]
    fn legacy_multi_model_attributes_per_turn() {
        // 10 of 81 real legacy files carry two models; a gpt-5.4 + gpt-5.5
        // pair differs 2x on input rate.
        let lines = vec![
            r#"{"type":"event_msg","timestamp":"2026-09-07T10:00:00.000Z","payload":{"type":"turn_context","turn_id":"t1","model":"gpt-5.4"}}"#.to_string(),
            r#"{"type":"event_msg","timestamp":"2026-09-07T10:00:01.000Z","payload":{"type":"token_count","turn_id":"t1","info":{"last_token_usage":{"input_tokens":100,"output_tokens":10,"total_tokens":110}}}}"#.to_string(),
            r#"{"type":"event_msg","timestamp":"2026-09-07T10:05:00.000Z","payload":{"type":"turn_context","turn_id":"t2","model":"gpt-5.5"}}"#.to_string(),
            r#"{"type":"event_msg","timestamp":"2026-09-07T10:05:01.000Z","payload":{"type":"token_count","turn_id":"t2","info":{"last_token_usage":{"input_tokens":100,"output_tokens":10,"total_tokens":110}}}}"#.to_string(),
        ];
        let (_dir, path) = write_rollout(&lines);
        let scan = scan_rollout_usage(&path, "thread-1", "gpt-5.1");
        let models: std::collections::BTreeSet<_> =
            scan.rows.iter().map(|r| r.model.as_str()).collect();
        assert!(models.contains("gpt-5.4") && models.contains("gpt-5.5"));
    }

    #[test]
    fn compaction_markers_are_ignored() {
        let lines = vec![
            r#"{"type":"event_msg","timestamp":"2026-09-07T10:00:00.000Z","payload":{"type":"turn_context","turn_id":"t1","model":"gpt-5.5"}}"#.to_string(),
            r#"{"type":"event_msg","timestamp":"2026-09-07T10:00:01.000Z","payload":{"type":"token_count","turn_id":"t1","info":{"last_token_usage":{"input_tokens":0,"output_tokens":0,"total_tokens":90000}}}}"#.to_string(),
        ];
        let (_dir, path) = write_rollout(&lines);
        let scan = scan_rollout_usage(&path, "thread-1", "gpt-5.5");
        assert!(scan.rows.is_empty());
        assert!(scan.unaccounted);
    }

    #[test]
    fn modern_format_wins_when_both_present() {
        let lines = vec![
            r#"{"type":"event_msg","payload":{"type":"turn_context","turn_id":"t1","model":"gpt-5.5"}}"#.to_string(),
            r#"{"type":"event_msg","timestamp":"2026-09-07T10:00:01.000Z","payload":{"type":"token_count","turn_id":"t1","info":{"last_token_usage":{"input_tokens":9999,"output_tokens":9999,"total_tokens":19998}}}}"#.to_string(),
            r#"{"type":"token_usage_record","timestamp":"2026-09-07T20:08:48.671Z","payload":{"response_id":"r1","turn_id":"t1","usage":{"input_tokens":10,"output_tokens":5,"total_tokens":15}}}"#.to_string(),
        ];
        let (_dir, path) = write_rollout(&lines);
        let scan = scan_rollout_usage(&path, "thread-1", "gpt-5.5");
        assert_eq!(scan.rows.len(), 1);
        assert_eq!(scan.rows[0].usage.total_tokens, 15);
        assert!(!scan.rows[0].approximate);
    }

    #[test]
    fn rfc3339_timestamps_parse_to_local_dates() {
        assert!(rfc3339_to_local_date("2026-09-07T20:08:48.671Z").is_some());
        assert!(rfc3339_to_local_date("2026-09-07T20:08:48+05:30").is_some());
        assert!(rfc3339_to_local_date("not-a-timestamp").is_none());
        // Epoch millis must NOT parse as a date.
        assert!(rfc3339_to_local_date("1788699163000").is_none());
    }

    #[test]
    fn window_rows_filter_by_request_date() {
        let mk = |date: &str, thread: &str, tokens: u64| UsageRow {
            thread_id: thread.into(),
            date: date.into(),
            model: "gpt-5.5".into(),
            usage: usage(tokens, 0, 0, 0),
            approximate: false,
            model_fallback: false,
        };
        // One session active on two days: its tokens must split by day, not
        // land wholesale in both windows.
        let rows = vec![
            mk("2026-09-07", "t1", 1_000),
            mk("2026-09-08", "t1", 2_000),
            mk("2026-09-08", "t2", 3_000),
        ];
        let today = window_from_rows(&rows, "2026-09-08", Some("2026-09-08"));
        assert_eq!(today.tokens, 5_000);
        assert_eq!(today.sessions, 2, "distinct threads, not rows");

        let week = window_from_rows(&rows, "2026-09-07", None);
        assert_eq!(week.tokens, 6_000);
        assert_eq!(week.sessions, 2, "t1 counted once across both days");
    }


    // ── Limit-state prioritisation ──────────────────────────────────────────

    fn window(pct: f64, mins: i64, resets: i64) -> AppServerRateWindow {
        AppServerRateWindow {
            used_percent: Some(pct),
            window_duration_mins: Some(mins),
            resets_at: Some(resets),
        }
    }

    fn limit(
        id: &str,
        primary: Option<AppServerRateWindow>,
        secondary: Option<AppServerRateWindow>,
        reached: Option<&str>,
    ) -> AppServerRateLimit {
        AppServerRateLimit {
            limit_id: Some(id.into()),
            limit_name: Some(id.into()),
            primary,
            secondary,
            credits: None,
            plan_type: None,
            rate_limit_reached_type: reached.map(str::to_string),
            spend_control_reached: None,
        }
    }

    #[test]
    fn spent_short_window_alone_does_not_trigger_the_banner() {
        // The real complaint: a 5h bucket at 100% (refills in hours) while the
        // weekly quota still has headroom must NOT report "Limit reached".
        let spark_5h = limit("spark-5h", Some(window(100.0, 300, 1_000)), None, None);
        let weekly = limit("weekly", Some(window(94.0, 10_080, 5_000)), None, None);
        assert!(
            app_server_limit_state(&[spark_5h, weekly]).is_none(),
            "a spent 5h bucket must not raise the banner while weekly has room"
        );
    }

    #[test]
    fn exhausted_weekly_quota_is_reported() {
        let spark_5h = limit("spark-5h", Some(window(100.0, 300, 1_000)), None, None);
        let weekly = limit("weekly", Some(window(100.0, 10_080, 5_000)), None, None);
        let state = app_server_limit_state(&[spark_5h, weekly]).expect("weekly is spent");
        match state {
            LimitState::Reached { scope, resets_at, .. } => {
                assert!(format!("{scope:?}").contains("weekly"));
                // Reset must be the weekly window's, not the 5h one's.
                let expected = chrono::DateTime::from_timestamp(5_000, 0).unwrap().to_rfc3339();
                assert_eq!(resets_at.expect("reset"), expected);
            }
            other => panic!("expected Reached, got {other:?}"),
        }
    }

    #[test]
    fn hard_block_outranks_any_percentage_window() {
        // A server-side block means no work is possible, so it wins even when
        // the weekly quota still has headroom.
        let weekly = limit("weekly", Some(window(40.0, 10_080, 5_000)), None, None);
        let blocked = limit("blocked", Some(window(100.0, 60, 100)), None, Some("usage"));
        let state = app_server_limit_state(&[weekly, blocked]).expect("reached");
        match state {
            LimitState::Reached { scope, .. } => {
                assert!(format!("{scope:?}").contains("blocked"), "a real block wins");
            }
            other => panic!("expected Reached, got {other:?}"),
        }
    }

    #[test]
    fn no_limit_reached_yields_none() {
        let a = limit("a", Some(window(91.0, 10_080, 5_000)), None, None);
        let b = limit("b", Some(window(45.0, 300, 1_000)), None, None);
        assert!(app_server_limit_state(&[a, b]).is_none());
    }

    #[test]
    fn weekly_window_is_matched_by_duration_not_name() {
        // The 7d per-model bucket also counts as weekly; the busiest weekly
        // window is the one reported.
        let spark_7d = limit("spark-7d", Some(window(100.0, 10_080, 7_000)), None, None);
        let weekly = limit("weekly", Some(window(45.0, 10_080, 5_000)), None, None);
        let state = app_server_limit_state(&[weekly, spark_7d]).expect("one weekly is spent");
        match state {
            LimitState::Reached { scope, used_pct, .. } => {
                assert!(format!("{scope:?}").contains("spark-7d"));
                assert_eq!(used_pct, 100.0);
            }
            other => panic!("expected Reached, got {other:?}"),
        }
    }

    // ── Activity stats (daily heatmap, streaks, peak hour) ──────────────────

    #[test]
    fn computes_active_days_and_current_streak_for_consecutive_days() {
        use chrono::{Local, TimeZone};
        let today = Local::now().date_naive();
        let mk_ms = |days_ago: i64| {
            let date = today - chrono::Duration::days(days_ago);
            Local
                .from_local_datetime(&date.and_hms_opt(10, 0, 0).unwrap())
                .single()
                .unwrap()
                .timestamp_millis()
        };
        let timestamps = [mk_ms(0), mk_ms(1), mk_ms(2), mk_ms(5)];
        let stats = compute_activity_stats(&timestamps);
        assert_eq!(stats.active_days, 4);
        assert_eq!(stats.current_streak, 3); // today, yesterday, day before
        assert_eq!(stats.longest_streak, 3);
    }

    #[test]
    fn empty_timestamps_yield_zeroed_activity() {
        let stats = compute_activity_stats(&[]);
        assert_eq!(stats.active_days, 0);
        assert_eq!(stats.current_streak, 0);
        assert_eq!(stats.longest_streak, 0);
        assert!(stats.peak_hour.is_none());
    }

    // ── Cost estimation ──────────────────────────────────────────────────────


    const APP_SERVER_RATE_LIMITS_WITH_SPARK: &str = r#"{
        "rateLimits": {
            "limitId": "codex",
            "limitName": null,
            "primary": {
                "usedPercent": 22,
                "windowDurationMins": 10080,
                "resetsAt": 1788929319
            },
            "secondary": null,
            "credits": {
                "hasCredits": false,
                "unlimited": false,
                "balance": "0"
            },
            "planType": "pro",
            "rateLimitReachedType": null
        },
        "rateLimitsByLimitId": {
            "codex_bengalfox": {
                "limitId": "codex_bengalfox",
                "limitName": "GPT-5.3-Codex-Spark",
                "primary": {
                    "usedPercent": 0,
                    "windowDurationMins": 300,
                    "resetsAt": 1788406761
                },
                "secondary": {
                    "usedPercent": 0,
                    "windowDurationMins": 10080,
                    "resetsAt": 1788993561
                },
                "credits": null,
                "planType": "pro",
                "rateLimitReachedType": null
            },
            "codex": {
                "limitId": "codex",
                "limitName": null,
                "primary": {
                    "usedPercent": 22,
                    "windowDurationMins": 10080,
                    "resetsAt": 1788929319
                },
                "secondary": null,
                "credits": {
                    "hasCredits": false,
                    "unlimited": false,
                    "balance": "0"
                },
                "planType": "pro",
                "rateLimitReachedType": null
            }
        }
    }"#;

    #[test]
    fn app_server_maps_general_and_spark_rate_limits_without_duplicate_base_bucket() {
        let response: AppServerRateLimitsResponse =
            serde_json::from_str(APP_SERVER_RATE_LIMITS_WITH_SPARK).unwrap();
        let limits = ordered_app_server_limits(&response);
        assert_eq!(limits.len(), 2);

        let mut windows = Vec::new();
        for limit in &limits {
            append_app_server_windows(&mut windows, limit);
        }
        let labels: Vec<&str> = windows.iter().map(|window| window.label.as_str()).collect();
        assert_eq!(
            labels,
            vec![
                "Weekly (7d)",
                "GPT-5.3-Codex-Spark (5h)",
                "GPT-5.3-Codex-Spark (7d)"
            ]
        );
        assert_eq!(windows[0].used_percent, 22.0);
        assert_eq!(windows[1].used_percent, 0.0);
        assert!(app_server_credit_usage(&limits).is_none());
    }

    #[test]
    fn app_server_accepts_numeric_credit_balance_strings() {
        let response: AppServerRateLimitsResponse = serde_json::from_str(
            r#"{
                "rateLimits": {
                    "limitId": "codex",
                    "credits": {
                        "hasCredits": true,
                        "unlimited": false,
                        "balance": "12.5"
                    },
                    "planType": "pro"
                }
            }"#,
        )
        .unwrap();
        let limits = ordered_app_server_limits(&response);
        let credits = app_server_credit_usage(&limits).unwrap();
        assert_eq!(credits.remaining, 12.5);
        assert_eq!(credits.plan_name.as_deref(), Some("pro"));
    }

    #[test]
    fn app_server_hides_credits_when_has_credits_is_false() {
        let response: AppServerRateLimitsResponse = serde_json::from_str(
            r#"{
                "rateLimits": {
                    "limitId": "codex",
                    "credits": {
                        "hasCredits": false,
                        "unlimited": true,
                        "balance": "99.5"
                    },
                    "planType": "pro"
                }
            }"#,
        )
        .unwrap();
        let limits = ordered_app_server_limits(&response);
        assert!(app_server_credit_usage(&limits).is_none());
    }

    // ── WHAM usage API: lenient numeric deserialization ─────────────────────
    // Regression coverage for the "invalid type: string \"0\", expected f64"
    // crash: OpenAI started sending numeric WHAM fields as stringified
    // numbers (e.g. `"used_percent": "0"`) instead of bare JSON numbers.

    const WHAM_RESPONSE_WITH_STRING_NUMBERS: &str = r#"{
        "user_id": "user_abc",
        "account_id": "acct_abc",
        "email": "dev@example.com",
        "plan_type": "team",
        "rate_limit": {
            "allowed": true,
            "limit_reached": false,
            "primary_window": {
                "used_percent": "0",
                "reset_at": "1787421418",
                "reset_after_seconds": "3600",
                "limit_window_seconds": "18000"
            },
            "secondary_window": {
                "used_percent": "12.5",
                "reset_at": 1787421999,
                "reset_after_seconds": null,
                "limit_window_seconds": 604800
            }
        },
        "code_review_rate_limit": null,
        "credits": {
            "has_credits": true,
            "unlimited": false,
            "balance": "0"
        }
    }"#;

    const WHAM_RESPONSE_WITH_NUMBERS: &str = r#"{
        "user_id": "user_abc",
        "account_id": "acct_abc",
        "email": "dev@example.com",
        "plan_type": "team",
        "rate_limit": {
            "allowed": true,
            "limit_reached": false,
            "primary_window": {
                "used_percent": 0,
                "reset_at": 1787421418,
                "reset_after_seconds": 3600,
                "limit_window_seconds": 18000
            },
            "secondary_window": null
        },
        "code_review_rate_limit": null,
        "credits": {
            "has_credits": true,
            "unlimited": false,
            "balance": 12.5
        }
    }"#;

    const WHAM_RESPONSE_WITH_ADDITIONAL_LIMITS: &str = r#"{
        "plan_type": "pro",
        "rate_limit": {
            "allowed": true,
            "limit_reached": false,
            "primary_window": {
                "used_percent": "18",
                "reset_at": "1788990614",
                "reset_after_seconds": "543638",
                "limit_window_seconds": "604800"
            },
            "secondary_window": null
        },
        "additional_rate_limits": [
            {
                "limit_name": "GPT-5.3-Codex-Spark",
                "metered_feature": "codex_bengalfox",
                "rate_limit": {
                    "allowed": true,
                    "limit_reached": false,
                    "primary_window": {
                        "used_percent": "0",
                        "reset_at": "1788403814",
                        "reset_after_seconds": "18000",
                        "limit_window_seconds": "18000"
                    },
                    "secondary_window": {
                        "used_percent": "0",
                        "reset_at": "1788990614",
                        "reset_after_seconds": "604800",
                        "limit_window_seconds": "604800"
                    }
                }
            }
        ],
        "credits": {
            "has_credits": false,
            "unlimited": false,
            "balance": "0"
        }
    }"#;

    #[test]
    fn wham_response_parses_when_numeric_fields_are_strings() {
        let wham: WhamUsageResponse =
            serde_json::from_str(WHAM_RESPONSE_WITH_STRING_NUMBERS).unwrap();
        let rl = wham.rate_limit.unwrap();
        let primary = rl.primary_window.unwrap();
        assert_eq!(primary.used_percent, Some(0.0));
        assert_eq!(primary.reset_at, Some(1787421418));
        assert_eq!(primary.reset_after_seconds, Some(3600));
        assert_eq!(primary.limit_window_seconds, Some(18000));

        let secondary = rl.secondary_window.unwrap();
        assert_eq!(secondary.used_percent, Some(12.5));
        assert_eq!(secondary.reset_at, Some(1787421999));
        assert_eq!(secondary.reset_after_seconds, None);
        assert_eq!(secondary.limit_window_seconds, Some(604800));

        let credits = wham.credits.unwrap();
        assert_eq!(credits.balance, Some(0.0));
    }

    #[test]
    fn wham_response_parses_when_numeric_fields_are_numbers() {
        let wham: WhamUsageResponse = serde_json::from_str(WHAM_RESPONSE_WITH_NUMBERS).unwrap();
        let rl = wham.rate_limit.unwrap();
        let primary = rl.primary_window.unwrap();
        assert_eq!(primary.used_percent, Some(0.0));
        assert_eq!(primary.reset_at, Some(1787421418));
        assert_eq!(primary.reset_after_seconds, Some(3600));
        assert_eq!(primary.limit_window_seconds, Some(18000));
        assert!(rl.secondary_window.is_none());

        let credits = wham.credits.unwrap();
        assert_eq!(credits.balance, Some(12.5));
    }

    #[test]
    fn wham_windows_use_durations_and_include_additional_limits() {
        let wham: WhamUsageResponse =
            serde_json::from_str(WHAM_RESPONSE_WITH_ADDITIONAL_LIMITS).unwrap();
        let mut windows = Vec::new();

        append_wham_windows(&mut windows, wham.rate_limit.as_ref().unwrap(), None);
        for additional in wham.additional_rate_limits.as_ref().unwrap() {
            append_wham_windows(
                &mut windows,
                additional.rate_limit.as_ref().unwrap(),
                additional.limit_name.as_deref(),
            );
        }

        let labels: Vec<&str> = windows.iter().map(|window| window.label.as_str()).collect();
        assert_eq!(
            labels,
            vec![
                "Weekly (7d)",
                "GPT-5.3-Codex-Spark (5h)",
                "GPT-5.3-Codex-Spark (7d)",
            ]
        );
    }

    #[test]
    fn unavailable_codex_credits_are_hidden() {
        let wham: WhamUsageResponse =
            serde_json::from_str(WHAM_RESPONSE_WITH_ADDITIONAL_LIMITS).unwrap();
        assert!(parse_wham_credit_usage(wham.credits.as_ref(), Some("pro")).is_none());
    }

    #[test]
    fn enabled_codex_credits_keep_their_balance() {
        let credits = WhamCredits {
            has_credits: Some(true),
            unlimited: Some(false),
            balance: Some(12.5),
        };
        let usage = parse_wham_credit_usage(Some(&credits), Some("pro")).unwrap();
        assert_eq!(usage.remaining, 12.5);
        assert_eq!(usage.currency, "credits");
        assert_eq!(usage.plan_name.as_deref(), Some("pro"));
    }

    #[test]
    fn de_lenient_opt_f64_accepts_number_string_and_null() {
        #[derive(Deserialize)]
        struct Wrapper {
            #[serde(default, deserialize_with = "de_lenient_opt_f64")]
            v: Option<f64>,
        }
        let from = |s: &str| serde_json::from_str::<Wrapper>(s).unwrap().v;
        assert_eq!(from(r#"{"v":"0"}"#), Some(0.0));
        assert_eq!(from(r#"{"v":"12.5"}"#), Some(12.5));
        assert_eq!(from(r#"{"v":12.5}"#), Some(12.5));
        assert_eq!(from(r#"{"v":null}"#), None);
        assert_eq!(from(r#"{}"#), None);
    }

    #[test]
    fn de_lenient_opt_i64_accepts_number_string_and_null() {
        #[derive(Deserialize)]
        struct Wrapper {
            #[serde(default, deserialize_with = "de_lenient_opt_i64")]
            v: Option<i64>,
        }
        let from = |s: &str| serde_json::from_str::<Wrapper>(s).unwrap().v;
        assert_eq!(from(r#"{"v":"1787421418"}"#), Some(1787421418));
        assert_eq!(from(r#"{"v":1787421418}"#), Some(1787421418));
        assert_eq!(from(r#"{"v":null}"#), None);
        assert_eq!(from(r#"{}"#), None);
    }
}
