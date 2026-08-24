//! Tauri commands for the unified analytics system.

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex as StdMutex;
use std::thread;
use std::time::Duration;
use tauri::{image::Image, AppHandle, Emitter, Wry};
use tauri_plugin_notification::NotificationExt;

use crate::analytics::types::*;
use crate::analytics::{
    claude, claude_desktop, codex, gemini, cursor, copilot,
    openrouter, kimi, deepseek, moonshot, zai, augment, amp, droid, kiro, jetbrains, vertex_ai,
    token_store,
};
use crate::commands::config::load_settings;
use crate::utils::paths::app_data_dir;

// ── Tray background refresh infrastructure ──────────────────────────────────

lazy_static::lazy_static! {
    /// Pre-computed TraySummary, updated by the background thread.
    /// get_tray_summary() reads from here — never fetches.
    static ref TRAY_SUMMARY_CACHE: StdMutex<Option<TraySummary>> = StdMutex::new(None);

    /// AppHandle for emitting events from the background thread.
    static ref TRAY_APP_HANDLE: StdMutex<Option<AppHandle<Wry>>> = StdMutex::new(None);

    /// Whether the background refresh loop is running.
    static ref TRAY_REFRESH_ACTIVE: StdMutex<bool> = StdMutex::new(false);

    /// Last selected tray provider tab from the frontend.
    static ref TRAY_ACTIVE_PROVIDER: StdMutex<Option<String>> = StdMutex::new(None);
}

const TRAY_REFRESH_INTERVAL_SECS: u64 = 60;
const PRIMARY_PROVIDER_IDS: [&str; 4] = ["claude-code", "cursor", "codex", "gemini"];

const CLAUDE_CODE_TRAY_ICON_PNG: &[u8] = include_bytes!("../../icons/providers/claude-code.png");
const CURSOR_TRAY_ICON_PNG: &[u8] = include_bytes!("../../icons/providers/cursor.png");
const CODEX_TRAY_ICON_PNG: &[u8] = include_bytes!("../../icons/providers/codex.png");
const GEMINI_TRAY_ICON_PNG: &[u8] = include_bytes!("../../icons/providers/gemini.png");
const CLAUDE_CODE_TRAY_ICON_ACTIVE_PNG: &[u8] =
    include_bytes!("../../icons/providers/claude-code-active.png");
const CURSOR_TRAY_ICON_ACTIVE_PNG: &[u8] = include_bytes!("../../icons/providers/cursor-active.png");
const CODEX_TRAY_ICON_ACTIVE_PNG: &[u8] = include_bytes!("../../icons/providers/codex-active.png");
const GEMINI_TRAY_ICON_ACTIVE_PNG: &[u8] = include_bytes!("../../icons/providers/gemini-active.png");

// ── Limit-state notification helpers ────────────────────────────────────────

#[derive(Serialize, Deserialize, Default)]
struct LimitNotificationPersist {
    #[serde(default)]
    providers: HashMap<String, serde_json::Value>,
}

fn limit_state_persist_path() -> PathBuf {
    app_data_dir().join("limit-state.json")
}

fn load_limit_notification_persist() -> LimitNotificationPersist {
    let path = limit_state_persist_path();
    if !path.exists() {
        return LimitNotificationPersist::default();
    }
    match fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => LimitNotificationPersist::default(),
    }
}

fn save_limit_notification_persist(p: &LimitNotificationPersist) {
    let path = limit_state_persist_path();
    if let Ok(json) = serde_json::to_string_pretty(p) {
        let _ = crate::utils::paths::atomic_write_str(&path, &json);
    }
}

fn format_limit_countdown(iso: Option<&String>) -> String {
    let Some(s) = iso else { return "soon".to_string() };
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        let dt = dt.with_timezone(&chrono::Utc);
        let diff = (dt - chrono::Utc::now()).num_seconds();
        if diff <= 0 {
            return "now".to_string();
        }
        let h = diff / 3600;
        let m = (diff % 3600) / 60;
        if h > 0 {
            format!("{}h {}m", h, m)
        } else {
            format!("{}m", m)
        }
    } else {
        "soon".to_string()
    }
}

fn format_retry_after(seconds: u64) -> String {
    if seconds <= 60 {
        format!("{}s", seconds.max(1))
    } else if seconds < 3600 {
        format!("{}m", (seconds + 30) / 60)
    } else {
        let h = seconds / 3600;
        let m = (seconds % 3600 + 30) / 60;
        if m > 0 {
            format!("{}h {}m", h, m)
        } else {
            format!("{}h", h)
        }
    }
}

fn describe_api_disabled(reason: &str, org_name: &str) -> (String, String) {
    let norm = reason.trim().to_ascii_lowercase();
    let org_trim = org_name.trim();
    let org = if org_trim.is_empty() { "Your organization" } else { org_trim };
    match norm.as_str() {
        "out_of_credits" => (
            format!("{} has reached its monthly usage limit", org),
            "Top up credits or ask an admin for /extra-usage to keep going.".into(),
        ),
        "trial_expired" => (
            format!("{}'s Claude Code trial has ended", org),
            "Add a payment method to keep using Claude Code.".into(),
        ),
        "payment_failed" | "payment_required" => (
            format!("{} — payment couldn't be processed", org),
            "Update your card to resume API access.".into(),
        ),
        "usage_policy_violation" => (
            format!("{}'s API access is paused for review", org),
            "Anthropic flagged recent usage. Contact support to restore access.".into(),
        ),
        "manual_disable" | "admin_disabled" => (
            format!("{} — API access turned off by an admin", org),
            "Ask an admin in your org to re-enable Claude Code access.".into(),
        ),
        "subscription_canceled" | "subscription_expired" => (
            format!("{}'s Claude subscription is inactive", org),
            "Re-activate billing in the Anthropic console.".into(),
        ),
        _ => {
            let friendly = reason
                .replace('_', " ")
                .split_whitespace()
                .map(|w| {
                    let mut c = w.chars();
                    match c.next() {
                        Some(first) => first.to_uppercase().chain(c).collect::<String>(),
                        None => String::new(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            (
                format!("{} — API access paused", org),
                if friendly.is_empty() {
                    "Open Anthropic billing for details.".into()
                } else {
                    friendly
                },
            )
        }
    }
}

fn limit_notification_for_transition(
    provider_label: &str,
    prev: Option<&LimitState>,
    new: &LimitState,
) -> Option<(String, String)> {
    use LimitState::*;
    match new {
        Reached { resets_at, .. } => {
            let from_ok = matches!(prev, None | Some(Healthy) | Some(Approaching { .. }));
            if !from_ok {
                return None;
            }
            let countdown = format_limit_countdown(resets_at.as_ref());
            Some((
                format!("{} limit reached", provider_label),
                format!("Session or weekly limit reached — resets in {}", countdown),
            ))
        }
        ApiDisabled { reason, org_name, .. } => {
            if matches!(prev, Some(ApiDisabled { .. })) {
                return None;
            }
            let (title, body) = describe_api_disabled(reason, org_name);
            Some((title, body))
        }
        SubscriptionIssue { status, org_name } => {
            if matches!(prev, Some(SubscriptionIssue { .. })) {
                return None;
            }
            let pretty_status = status.replace('_', " ");
            Some((
                "Claude subscription issue".into(),
                format!(
                    "{}'s subscription is {} — update billing in console.",
                    org_name, pretty_status
                ),
            ))
        }
        BillablePaused { until, org_name } => {
            if matches!(prev, Some(BillablePaused { .. })) {
                return None;
            }
            Some((
                "Billing paused".into(),
                format!("{} — billing paused until {}.", org_name, until),
            ))
        }
        RateLimited { retry_after_secs, .. } => {
            if matches!(prev, Some(RateLimited { .. })) {
                return None;
            }
            let when = retry_after_secs
                .map(|s| format!("retry in {}", format_retry_after(s)))
                .unwrap_or_else(|| "Anthropic is throttling requests".into());
            Some((
                format!("{} rate limited", provider_label),
                format!("Slow down — {}.", when),
            ))
        }
        Unauthenticated { .. } => {
            if matches!(prev, Some(Unauthenticated { .. })) {
                return None;
            }
            Some((
                format!("{} needs to reconnect", provider_label),
                "Stored credentials are no longer valid. Sign in again to keep tracking usage."
                    .into(),
            ))
        }
        Approaching { worst_pct, label, resets_at, .. } => match prev {
            Some(Healthy) if *worst_pct >= 80.0 => {
                let countdown = format_limit_countdown(resets_at.as_ref());
                Some((
                    format!("{} usage high", provider_label),
                    format!("{} at {:.0}% — resets in {}", label, worst_pct, countdown),
                ))
            }
            _ => None,
        },
        Healthy => None,
    }
}

fn limit_notification_first_fetch(
    provider_label: &str,
    new: &LimitState,
) -> Option<(String, String)> {
    use LimitState::*;
    match new {
        ApiDisabled { reason, org_name, .. } => {
            let (title, body) = describe_api_disabled(reason, org_name);
            Some((title, body))
        }
        SubscriptionIssue { status, org_name } => {
            let pretty_status = status.replace('_', " ");
            Some((
                "Claude subscription issue".into(),
                format!(
                    "{}'s subscription is {} — update billing in console.",
                    org_name, pretty_status
                ),
            ))
        }
        BillablePaused { until, org_name } => Some((
            "Billing paused".into(),
            format!("{} — billing paused until {}.", org_name, until),
        )),
        RateLimited { retry_after_secs, .. } => {
            let when = retry_after_secs
                .map(|s| format!("retry in {}", format_retry_after(s)))
                .unwrap_or_else(|| "Anthropic is throttling requests".into());
            Some((
                format!("{} rate limited", provider_label),
                format!("Slow down — {}.", when),
            ))
        }
        Unauthenticated { .. } => Some((
            format!("{} needs to reconnect", provider_label),
            "Stored credentials are no longer valid. Sign in again to keep tracking usage.".into(),
        )),
        Reached { resets_at, .. } => {
            let countdown = format_limit_countdown(resets_at.as_ref());
            Some((
                format!("{} limit reached", provider_label),
                format!("Usage limit reached — resets in {}", countdown),
            ))
        }
        Approaching { worst_pct, label, resets_at, .. } if *worst_pct >= 80.0 => {
            let countdown = format_limit_countdown(resets_at.as_ref());
            Some((
                format!("{} usage high", provider_label),
                format!("{} at {:.0}% — resets in {}", label, worst_pct, countdown),
            ))
        }
        _ => None,
    }
}

fn maybe_emit_limit_notifications(app: &AppHandle<Wry>, summary: &TraySummary) {
    let settings = load_settings();
    if !settings.analytics.limit_notifications_enabled {
        return;
    }

    let mut persist = load_limit_notification_persist();
    let snapshot_before = persist.providers.clone();

    for p in &summary.providers {
        if !p.connected {
            persist.providers.remove(&p.provider_id);
            continue;
        }
        let prev_json = persist.providers.get(&p.provider_id);
        let prev_ls: Option<LimitState> = prev_json
            .and_then(|v| serde_json::from_value(v.clone()).ok());

        if let Some(ref nls) = p.limit_state {
            let msg = if prev_json.is_none() {
                limit_notification_first_fetch(&p.provider_name, nls)
            } else {
                limit_notification_for_transition(&p.provider_name, prev_ls.as_ref(), nls)
            };
            if let Some((title, body)) = msg {
                let _ = app.notification().builder().title(title).body(body).show();
            }
            if let Ok(v) = serde_json::to_value(nls) {
                persist.providers.insert(p.provider_id.clone(), v);
            }
        } else {
            persist.providers.remove(&p.provider_id);
        }
    }

    if persist.providers != snapshot_before {
        save_limit_notification_persist(&persist);
    }
}

// ── Provider status ─────────────────────────────────────────────────────────

/// Get connection status for all providers (fast, no API calls for most).
#[tauri::command]
pub fn get_all_provider_status() -> Vec<ProviderStatus> {
    vec![
        claude::check_connection(),
        claude_desktop::check_connection(),
        codex::check_connection(),
        gemini::check_connection(),
        cursor::check_connection(),
        copilot::check_connection(),
        openrouter::check_connection(),
        kimi::check_connection(),
        kimi::check_k2_connection(),
        deepseek::check_connection(),
        moonshot::check_connection(),
        zai::check_connection(),
        augment::check_connection(),
        amp::check_connection(),
        droid::check_connection(),
        kiro::check_connection(),
        jetbrains::check_connection(),
        vertex_ai::check_connection(),
    ]
}

/// Get full analytics for a single provider.
#[tauri::command]
pub fn get_provider_analytics(provider_id: String) -> Result<ProviderAnalytics, String> {
    match provider_id.as_str() {
        "claude-code" => Ok(claude::fetch_claude_analytics()),
        "claude-desktop" => Ok(claude_desktop::fetch_claude_desktop_analytics()),
        "codex" => Ok(codex::fetch_codex_analytics()),
        "gemini" => Ok(gemini::fetch_gemini_analytics()),
        "cursor" => Ok(cursor::fetch_cursor_analytics()),
        "copilot" => Ok(copilot::fetch_copilot_analytics()),
        "openrouter" => Ok(openrouter::fetch_openrouter_analytics()),
        "kimi" => Ok(kimi::fetch_kimi_analytics()),
        "kimi-k2" => Ok(kimi::fetch_kimi_k2_analytics()),
        "deepseek" => Ok(deepseek::fetch_deepseek_analytics()),
        "moonshot" => Ok(moonshot::fetch_moonshot_analytics()),
        "zai" => Ok(zai::fetch_zai_analytics()),
        "augment" => Ok(augment::fetch_augment_analytics()),
        "amp" => Ok(amp::fetch_amp_analytics()),
        "droid" => Ok(droid::fetch_droid_analytics()),
        "kiro" => Ok(kiro::fetch_kiro_analytics()),
        "jetbrains" => Ok(jetbrains::fetch_jetbrains_analytics()),
        "vertex-ai" => Ok(vertex_ai::fetch_vertex_ai_analytics()),
        _ => Err(format!("Unknown provider: {}", provider_id)),
    }
}

/// Fetch analytics for all connected providers.
/// Returns results for every provider (disconnected ones return status only).
#[tauri::command]
pub fn get_all_provider_analytics() -> Vec<ProviderAnalytics> {
    // Check connections first (fast)
    let statuses = get_all_provider_status();

    statuses
        .into_iter()
        .map(|s| {
            if s.connected {
                // Fetch full analytics for connected providers
                get_provider_analytics(s.provider_id.clone()).unwrap_or_else(|e| {
                    ProviderAnalytics {
                        provider_id: s.provider_id.clone(),
                        provider_name: s.provider_name.clone(),
                        status: ProviderStatus {
                            error: Some(e),
                            ..s
                        },
                        rate_limits: vec![],
                        credit_usage: None,
                        token_counts: None,
                        limit_state: None,
                        extra: std::collections::HashMap::new(),
                        fetched_at: chrono::Utc::now().to_rfc3339(),
                    }
                })
            } else {
                // Return status-only for disconnected
                ProviderAnalytics {
                    provider_id: s.provider_id.clone(),
                    provider_name: s.provider_name.clone(),
                    status: s,
                    rate_limits: vec![],
                    credit_usage: None,
                    token_counts: None,
                    limit_state: None,
                    extra: std::collections::HashMap::new(),
                    fetched_at: chrono::Utc::now().to_rfc3339(),
                }
            }
        })
        .collect()
}

// ── Tray popover summary ────────────────────────────────────────────────────

/// Lightweight provider summary for the tray popover.
#[derive(Serialize, Clone, Debug)]
pub struct TrayProviderSummary {
    pub provider_id: String,
    pub provider_name: String,
    pub connected: bool,
    pub connection_method: String,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub rate_limits: Vec<RateLimitWindow>,
    pub credit_usage: Option<CreditUsage>,
    pub error: Option<String>,
    pub extra: std::collections::HashMap<String, serde_json::Value>,
    pub fetched_at: String,
    #[serde(default)]
    pub limit_state: Option<LimitState>,
}

/// Aggregated tray summary across all primary providers.
#[derive(Serialize, Clone, Debug)]
pub struct TraySummary {
    pub providers: Vec<TrayProviderSummary>,
    pub connected_count: u32,
    pub total_count: u32,
    pub worst_rate_limit: Option<RateLimitWindow>,
    pub fetched_at: String,
}

fn is_primary_rate_window(label: &str) -> bool {
    label.contains("5h") || label.contains("Weekly") || label.contains("Session")
}

/// Tray title metric for a provider.
#[derive(Clone, Debug)]
enum DisplayMetric {
    /// Capped usage — rendered as "X%".
    Percent(f64),
    /// Enterprise spend — rendered as "$X" (rounded dollars). Used for both
    /// uncapped Enterprise (no meaningful %) AND capped Enterprise (the user
    /// asked us to surface the dollar figure in the menu bar; the popover
    /// continues to show the percentage in the bar).
    Spend { amount: f64, currency: String },
}

/// Claude Code on an Enterprise plan (capped or uncapped).
fn is_enterprise_provider(provider: &TrayProviderSummary) -> bool {
    if provider.provider_id != "claude-code" {
        return false;
    }
    provider
        .extra
        .get("is_enterprise")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// True for Claude Code accounts on Enterprise plan with no monthly cap set.
/// Such providers have no meaningful percentage to display.
fn is_uncapped_enterprise(provider: &TrayProviderSummary) -> bool {
    if !is_enterprise_provider(provider) {
        return false;
    }
    provider
        .credit_usage
        .as_ref()
        .map(|c| c.limit.is_none())
        .unwrap_or(true)
}

/// Gemini CLI exposes three tier buckets (labels "Pro", "Flash", "Flash Lite").
/// Prefer showing the first tier in priority order that still has quota left; if
/// Pro is exhausted, fall through to Flash, then Flash Lite. If all are
/// exhausted, surface Pro so the title matches the highest-priority tier.
fn pick_gemini_display_rate(provider: &TrayProviderSummary) -> Option<RateLimitWindow> {
    if provider.provider_id != "gemini" {
        return None;
    }
    const ORDER: &[&str] = &["Pro", "Flash", "Flash Lite"];
    let mut by_label: std::collections::HashMap<String, RateLimitWindow> =
        std::collections::HashMap::new();
    for rl in &provider.rate_limits {
        by_label.insert(rl.label.clone(), rl.clone());
    }
    const EPS: f64 = 0.05;
    for tier in ORDER {
        if let Some(rl) = by_label.get(*tier) {
            if rl.remaining_percent > EPS {
                return Some(rl.clone());
            }
        }
    }
    ORDER.iter().find_map(|t| by_label.get(*t).cloned())
}

fn pick_primary_provider_rate(provider: &TrayProviderSummary) -> Option<RateLimitWindow> {
    // Claude-specific: prefer Session (5h) first, fall back to Weekly only when
    // session used% is 0%.  This shows the session rate in the tray unless the
    // user hasn't started a session yet.
    if provider.provider_id == "claude-code" {
        let session = provider
            .rate_limits
            .iter()
            .find(|rl| rl.label.contains("5h") || rl.label.contains("Session"))
            .cloned();
        if let Some(ref s) = session {
            if s.used_percent > 0.0 {
                return session;
            }
        }
        // Session is 0% — fall back to Weekly
        let weekly = provider
            .rate_limits
            .iter()
            .find(|rl| rl.label.contains("Weekly"))
            .cloned();
        if weekly.is_some() {
            return weekly;
        }
        // Neither found, return session anyway (shows 0%)
        if session.is_some() {
            return session;
        }
    }

    // Codex (WHAM): prefer Primary (5h) for menu-bar % so it matches the first
    // bar in the popover — same precedence as Claude session vs weekly. Using
    // "most constrained" across 5h + weekly would pick Weekly (lower remaining).
    if provider.provider_id == "codex" {
        let primary = provider
            .rate_limits
            .iter()
            .find(|rl| rl.label.contains("5h"))
            .cloned();
        if let Some(ref p) = primary {
            if p.used_percent > 0.0 {
                return primary;
            }
        }
        let weekly = provider
            .rate_limits
            .iter()
            .find(|rl| rl.label.contains("Weekly"))
            .cloned();
        if weekly.is_some() {
            return weekly;
        }
        if primary.is_some() {
            return primary;
        }
    }

    // Non-Claude providers: pick the most constrained primary window
    let best_primary = provider
        .rate_limits
        .iter()
        .filter(|rl| is_primary_rate_window(&rl.label))
        .min_by(|a, b| {
            a.remaining_percent
                .partial_cmp(&b.remaining_percent)
                .unwrap_or(Ordering::Equal)
        })
        .cloned();

    if best_primary.is_some() {
        return best_primary;
    }

    provider
        .rate_limits
        .iter()
        .min_by(|a, b| {
            a.remaining_percent
                .partial_cmp(&b.remaining_percent)
                .unwrap_or(Ordering::Equal)
        })
        .cloned()
}

fn numeric_extra_value(
    provider: &TrayProviderSummary,
    key: &str,
) -> Option<f64> {
    provider.extra.get(key).and_then(|value| value.as_f64())
}

fn compute_provider_display_metric(provider: &TrayProviderSummary) -> Option<DisplayMetric> {
    // Claude Enterprise — show $-spend in the menu bar regardless of whether
    // a monthly cap is set. (Capped: low single-digit percentages would just
    // render as "0%". Uncapped: percentage is undefined.) The popover keeps
    // showing the percentage inside the spend bar.
    if is_enterprise_provider(provider) {
        if let Some(credit) = provider.credit_usage.as_ref() {
            return Some(DisplayMetric::Spend {
                amount: credit.used,
                currency: credit.currency.clone(),
            });
        }
        return None;
    }

    // Cursor menu bar: show user's total spend (included + bonus + on-demand),
    // same formula as the tray popover. Must run before pick_primary_provider_rate
    // so dollars win over session/week percentage windows.
    if provider.provider_id == "cursor" {
        let included = numeric_extra_value(provider, "plan_included_usd").unwrap_or(0.0);
        let bonus = numeric_extra_value(provider, "plan_bonus_usd").unwrap_or(0.0);
        let on_demand = numeric_extra_value(provider, "on_demand_used_usd").unwrap_or(0.0);
        let total_spent = included + bonus + on_demand;
        if total_spent > 0.0 {
            return Some(DisplayMetric::Spend {
                amount: total_spent,
                currency: provider
                    .credit_usage
                    .as_ref()
                    .map(|c| c.currency.clone())
                    .unwrap_or_else(|| "USD".into()),
            });
        }
    }

    // Gemini menu bar: follow Pro → Flash → Flash Lite, using the first tier
    // that still has remaining quota (not the globally most-drained bucket).
    if let Some(rate) = pick_gemini_display_rate(provider) {
        return Some(DisplayMetric::Percent(rate.used_percent.clamp(0.0, 100.0)));
    }

    if let Some(rate) = pick_primary_provider_rate(provider) {
        return Some(DisplayMetric::Percent(rate.used_percent.clamp(0.0, 100.0)));
    }

    // Cursor fallback: API's individualUsage.plan.totalPercentUsed (menu bar rounds
    // to a whole number, e.g. 66.3 -> 66%) when spend components are missing/zero.
    if let Some(percent) = numeric_extra_value(provider, "plan_total_percent_used") {
        return Some(DisplayMetric::Percent(percent.clamp(0.0, 100.0)));
    }

    if let Some(credit) = provider.credit_usage.as_ref() {
        if let Some(limit) = credit.limit {
            if limit > 0.0 {
                let percent = (credit.used / limit) * 100.0;
                return Some(DisplayMetric::Percent(percent.clamp(0.0, 100.0)));
            }
        }
    }

    // Fallback for providers that expose team on-demand spend in `extra`.
    let team_used = numeric_extra_value(provider, "team_od_used_usd");
    let team_limit = numeric_extra_value(provider, "team_od_limit_usd");
    if let (Some(used), Some(limit)) = (team_used, team_limit) {
        if limit > 0.0 {
            let percent = (used / limit) * 100.0;
            return Some(DisplayMetric::Percent(percent.clamp(0.0, 100.0)));
        }
    }

    None
}

fn format_spend_for_title(amount: f64, currency: &str) -> String {
    let symbol = if currency.eq_ignore_ascii_case("USD") {
        "$"
    } else {
        // Fall back to currency code prefix for non-USD.
        currency
    };
    // Keep menu-bar title compact: drop cents above $10, single-decimal below.
    if amount >= 10.0 {
        format!("{}{}", symbol, amount.round() as i64)
    } else if amount >= 1.0 {
        format!("{}{:.1}", symbol, amount)
    } else {
        format!("{}{:.2}", symbol, amount)
    }
}

fn pick_display_provider_id(
    summary: &TraySummary,
    active_provider_id: Option<&str>,
) -> Option<String> {
    // The user's selected tab always wins — switching tabs should switch the
    // tray icon immediately, even when another provider is in danger.
    if let Some(provider_id) = active_provider_id {
        if summary
            .providers
            .iter()
            .any(|p| p.provider_id == provider_id && p.connected)
        {
            return Some(provider_id.to_string());
        }
    }

    // No / stale tab selection: prefer a provider in a danger state so the
    // menu bar surfaces the truly-blocked one instead of falling back to %.
    for id in PRIMARY_PROVIDER_IDS {
        if let Some(p) = summary.providers.iter().find(|p| p.provider_id == *id) {
            if p.connected {
                if let Some(ref ls) = p.limit_state {
                    if ls.is_danger() {
                        return Some(p.provider_id.clone());
                    }
                }
            }
        }
    }

    if let Some(worst) = summary.worst_rate_limit.as_ref() {
        return Some(worst.provider_id.clone());
    }

    summary
        .providers
        .iter()
        .find(|p| p.connected)
        .map(|p| p.provider_id.clone())
}

fn icon_bytes_for_provider(provider_id: &str, danger: bool) -> &'static [u8] {
    if danger {
        return match provider_id {
            "claude-code" => CLAUDE_CODE_TRAY_ICON_ACTIVE_PNG,
            "cursor" => CURSOR_TRAY_ICON_ACTIVE_PNG,
            "codex" => CODEX_TRAY_ICON_ACTIVE_PNG,
            "gemini" => GEMINI_TRAY_ICON_ACTIVE_PNG,
            _ => &[],
        };
    }
    match provider_id {
        "claude-code" => CLAUDE_CODE_TRAY_ICON_PNG,
        "cursor" => CURSOR_TRAY_ICON_PNG,
        "codex" => CODEX_TRAY_ICON_PNG,
        "gemini" => GEMINI_TRAY_ICON_PNG,
        _ => &[],
    }
}

fn load_provider_tray_icon(provider_id: &str, danger: bool) -> Option<Image<'static>> {
    let icon_bytes = icon_bytes_for_provider(provider_id, danger);
    if icon_bytes.is_empty() {
        return None;
    }

    if let Ok(img) = Image::from_bytes(icon_bytes) {
        return Some(img.to_owned());
    }

    None
}

fn update_tray_indicator(app: &AppHandle<Wry>, summary: &TraySummary) {
    let active_provider_id = TRAY_ACTIVE_PROVIDER
        .lock()
        .ok()
        .and_then(|guard| guard.clone());
    let display_provider_id =
        pick_display_provider_id(summary, active_provider_id.as_deref());
    let display_provider = display_provider_id.as_ref().and_then(|provider_id| {
        summary
            .providers
            .iter()
            .find(|provider| provider.provider_id == *provider_id)
    });

    // For uncapped Enterprise we never borrow a percentage from a different
    // provider — % is meaningless on this plan. Use the spend metric or
    // nothing at all so the icon stands alone.
    let displayed_is_uncapped_enterprise = display_provider
        .map(is_uncapped_enterprise)
        .unwrap_or(false);
    let display_metric = display_provider
        .and_then(compute_provider_display_metric)
        .or_else(|| {
            if displayed_is_uncapped_enterprise {
                None
            } else {
                summary
                    .worst_rate_limit
                    .as_ref()
                    .map(|rate| DisplayMetric::Percent(rate.used_percent.clamp(0.0, 100.0)))
            }
        });

    let danger_icon = display_provider
        .and_then(|p| p.limit_state.as_ref())
        .map(|ls| ls.is_danger())
        .unwrap_or(false);

    if let Some(tray) = app.tray_by_id("main-tray") {
        let title = match display_metric {
            Some(DisplayMetric::Percent(p)) => {
                let mut s = format!("{}%", p.round() as u32);
                if danger_icon {
                    s.push('!');
                }
                Some(s)
            }
            Some(DisplayMetric::Spend { amount, currency }) => {
                let mut s = format_spend_for_title(amount, &currency);
                if danger_icon {
                    s.push('!');
                }
                Some(s)
            }
            None => None,
        };
        if let Some(ref t) = title {
            let _ = tray.set_title(Some(t.as_str()));
        } else {
            let _ = tray.set_title(Option::<&str>::None);
        }

        // Tooltip: shown on hover on Windows/Linux, harmless on macOS.
        let tooltip = match (display_provider, title.as_deref()) {
            (Some(p), Some(t)) => Some(format!("{} \u{00B7} {}", p.provider_name, t)),
            (Some(p), None)    => Some(p.provider_name.clone()),
            (None, Some(t))    => Some(t.to_string()),
            (None, None)       => None,
        };
        match tooltip {
            Some(ref s) => { let _ = tray.set_tooltip(Some(s.as_str())); }
            None        => { let _ = tray.set_tooltip(Option::<&str>::None); }
        }

        if let Some(ref provider_id) = display_provider_id {
            if let Some(icon) = load_provider_tray_icon(provider_id, danger_icon) {
                let _ = tray.set_icon(Some(icon));
            } else {
                let _ = tray.set_icon(Option::<Image<'_>>::None);
            }
        } else {
            let _ = tray.set_icon(Option::<Image<'_>>::None);
        }
    }
}

/// Build a TraySummary by fetching all 4 providers IN PARALLEL.
/// Each provider runs in its own thread, so total time = max(provider_times).
fn build_tray_summary() -> TraySummary {
    let claude_h = thread::spawn(|| claude::fetch_claude_analytics());
    let cursor_h = thread::spawn(|| cursor::fetch_cursor_analytics());
    let codex_h = thread::spawn(|| codex::fetch_codex_analytics());
    let gemini_h = thread::spawn(|| gemini::fetch_gemini_analytics());

    let disconnected = |id: &str, name: &str| ProviderAnalytics {
        provider_id: id.to_string(),
        provider_name: name.to_string(),
        status: ProviderStatus {
            provider_id: id.to_string(),
            provider_name: name.to_string(),
            connected: false,
            connection_method: "none".into(),
            account_email: None,
            plan_name: None,
            org_name: None,
            error: Some("Provider fetch failed".into()),
        },
        rate_limits: vec![],
        credit_usage: None,
        token_counts: None,
        limit_state: None,
        extra: std::collections::HashMap::new(),
        fetched_at: chrono::Utc::now().to_rfc3339(),
    };

    let analytics_list = vec![
        claude_h.join().unwrap_or_else(|_| disconnected("claude-code", "Claude Code")),
        cursor_h.join().unwrap_or_else(|_| disconnected("cursor", "Cursor")),
        codex_h.join().unwrap_or_else(|_| disconnected("codex", "Codex")),
        gemini_h.join().unwrap_or_else(|_| disconnected("gemini", "Gemini CLI")),
    ];

    let mut providers: Vec<TrayProviderSummary> = analytics_list
        .into_iter()
        .map(|p| TrayProviderSummary {
            provider_id: p.provider_id,
            provider_name: p.provider_name,
            connected: p.status.connected,
            connection_method: p.status.connection_method,
            email: p.status.account_email,
            plan: p.status.plan_name,
            rate_limits: p.rate_limits,
            credit_usage: p.credit_usage,
            error: p.status.error,
            extra: p.extra,
            fetched_at: p.fetched_at,
            limit_state: p.limit_state,
        })
        .collect();

    // Live running-agent count — computed per summary build (cheap PID probes),
    // not taken from the cached analytics snapshot.
    if let Some(p) = providers.iter_mut().find(|p| p.provider_id == "claude-code") {
        let n = crate::commands::claude_history::get_claude_active_sessions()
            .map(|s| s.iter().filter(|x| x.is_running).count())
            .unwrap_or(0);
        p.extra
            .insert("active_agent_count".into(), serde_json::json!(n));
    }

    let connected_count = providers.iter().filter(|p| p.connected).count() as u32;

    // Pick one representative rate per provider (respects Claude session-first
    // logic), then find the worst across all providers. Skip uncapped Enterprise
    // accounts: their "rate limit" is a $-spend ledger with no cap, so
    // remaining_percent is meaningless and would otherwise pin worst_rate_limit
    // to 0% / 100% from a borrowed window.
    let worst_rate_limit = providers
        .iter()
        .filter(|p| !is_uncapped_enterprise(p))
        .filter_map(|p| pick_primary_provider_rate(p))
        .min_by(|a, b| {
            a.remaining_percent
                .partial_cmp(&b.remaining_percent)
                .unwrap_or(Ordering::Equal)
        });

    TraySummary {
        providers,
        connected_count,
        total_count: 4,
        worst_rate_limit,
        fetched_at: chrono::Utc::now().to_rfc3339(),
    }
}

/// Refresh the tray cache and push the update to the frontend.
fn refresh_and_emit() {
    let summary = build_tray_summary();

    if let Ok(mut guard) = TRAY_SUMMARY_CACHE.lock() {
        *guard = Some(summary.clone());
    }

    if let Ok(guard) = TRAY_APP_HANDLE.lock() {
        if let Some(ref app) = *guard {
            maybe_emit_limit_notifications(app, &summary);
            let _ = app.emit("tray-data-updated", &summary);
            update_tray_indicator(app, &summary);
        }
    }
}

/// Start the background refresh loop. Called from `setup_tray()`.
/// Immediately seeds the cache, then refreshes every 120s.
pub fn start_tray_background_refresh(app_handle: AppHandle<Wry>) {
    if let Ok(mut guard) = TRAY_APP_HANDLE.lock() {
        *guard = Some(app_handle);
    }

    {
        let mut active = TRAY_REFRESH_ACTIVE.lock().unwrap();
        if *active {
            return; // already running
        }
        *active = true;
    }

    thread::spawn(move || {
        // Immediate first refresh to seed cache before user clicks tray
        refresh_and_emit();

        loop {
            thread::sleep(Duration::from_secs(TRAY_REFRESH_INTERVAL_SECS));

            if let Ok(active) = TRAY_REFRESH_ACTIVE.lock() {
                if !*active {
                    break;
                }
            }

            refresh_and_emit();
        }
    });
}

/// Get a quick summary of all primary providers for the tray popover.
/// Instant: reads from pre-computed cache (sub-millisecond).
/// Falls back to synchronous fetch only on the very first call.
#[tauri::command]
pub fn get_tray_summary() -> TraySummary {
    // Fast path: read from background-refreshed cache
    if let Ok(guard) = TRAY_SUMMARY_CACHE.lock() {
        if let Some(ref cached) = *guard {
            return cached.clone();
        }
    }

    // Cold start: cache not yet seeded by background thread
    let summary = build_tray_summary();
    if let Ok(mut guard) = TRAY_SUMMARY_CACHE.lock() {
        *guard = Some(summary.clone());
    }
    summary
}

/// Trigger a background refresh of tray data. Fire-and-forget.
#[tauri::command]
pub fn refresh_tray_data() {
    thread::spawn(|| refresh_and_emit());
}

/// Force-refresh a specific provider by clearing its cache and re-fetching.
/// Returns the fresh analytics data.
#[tauri::command]
pub fn force_refresh_provider(provider_id: String) -> Result<ProviderAnalytics, String> {
    match provider_id.as_str() {
        "gemini" => {
            gemini::clear_cache();
            Ok(gemini::fetch_gemini_analytics())
        }
        // Add other providers as needed
        _ => {
            // For providers without explicit cache-clear, just re-fetch
            get_provider_analytics(provider_id)
        }
    }
}

/// Update the tray icon tooltip text (called from frontend).
#[tauri::command]
pub fn update_tray_tooltip(app: tauri::AppHandle, text: String) -> Result<(), String> {
    if let Some(tray) = app.tray_by_id("main-tray") {
        tray.set_tooltip(Some(&text)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Track the tray's active provider tab so Rust can render icon/title for it.
#[tauri::command]
pub fn set_tray_active_provider(app: tauri::AppHandle, provider_id: String) -> Result<(), String> {
    let normalized = provider_id.trim().to_string();

    {
        let mut guard = TRAY_ACTIVE_PROVIDER
            .lock()
            .map_err(|e| format!("Tray active provider lock error: {}", e))?;

        if PRIMARY_PROVIDER_IDS.contains(&normalized.as_str()) {
            *guard = Some(normalized.clone());
        } else {
            *guard = None;
        }
    }

    if let Ok(summary_guard) = TRAY_SUMMARY_CACHE.lock() {
        if let Some(ref summary) = *summary_guard {
            update_tray_indicator(&app, summary);
        }
    }

    Ok(())
}

// ── Provider token management ───────────────────────────────────────────────

/// Save a token for a provider.
#[tauri::command]
pub fn save_provider_token(provider_id: String, key_type: String, value: String) -> Result<(), String> {
    token_store::store_provider_token(&provider_id, &key_type, &value)
}

/// Delete a provider's token.
#[tauri::command]
pub fn delete_provider_token(provider_id: String, key_type: String) -> Result<(), String> {
    token_store::delete_provider_token(&provider_id, &key_type)
}

/// Check if a provider has a stored token.
#[tauri::command]
pub fn has_provider_token(provider_id: String, key_type: String) -> bool {
    token_store::has_provider_token(&provider_id, &key_type)
}

// ── Provider info ───────────────────────────────────────────────────────────

/// Get info about all known providers.
#[tauri::command]
pub fn get_all_provider_info() -> Vec<ProviderInfo> {
    all_provider_info()
}

// ── Copilot device flow ─────────────────────────────────────────────────────

/// Start GitHub device flow for Copilot auth.
#[tauri::command]
pub fn copilot_start_device_flow() -> Result<copilot::DeviceFlowInfo, String> {
    copilot::start_device_flow()
}

/// Poll GitHub device flow for completion.
#[tauri::command]
pub fn copilot_poll_device_flow(device_code: String) -> Result<String, String> {
    copilot::poll_device_flow(device_code)
}
