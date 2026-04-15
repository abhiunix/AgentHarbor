//! Tauri commands for the unified analytics system.

use serde::Serialize;
use std::sync::Mutex as StdMutex;
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Wry};

use crate::analytics::types::*;
use crate::analytics::{
    claude, claude_desktop, codex, gemini, cursor, copilot,
    openrouter, kimi, zai, augment, amp, droid, kiro, jetbrains, vertex_ai,
    token_store,
};

// ── Tray background refresh infrastructure ──────────────────────────────────

lazy_static::lazy_static! {
    /// Pre-computed TraySummary, updated by the background thread.
    /// get_tray_summary() reads from here — never fetches.
    static ref TRAY_SUMMARY_CACHE: StdMutex<Option<TraySummary>> = StdMutex::new(None);

    /// AppHandle for emitting events from the background thread.
    static ref TRAY_APP_HANDLE: StdMutex<Option<AppHandle<Wry>>> = StdMutex::new(None);

    /// Whether the background refresh loop is running.
    static ref TRAY_REFRESH_ACTIVE: StdMutex<bool> = StdMutex::new(false);

    /// The currently selected provider in the tray popover (for menu bar percentage display).
    static ref TRAY_SELECTED_PROVIDER: StdMutex<String> = StdMutex::new("claude-code".to_string());
}

const TRAY_REFRESH_INTERVAL_SECS: u64 = 120;

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
        extra: std::collections::HashMap::new(),
        fetched_at: chrono::Utc::now().to_rfc3339(),
    };

    let analytics_list = vec![
        claude_h.join().unwrap_or_else(|_| disconnected("claude-code", "Claude Code")),
        cursor_h.join().unwrap_or_else(|_| disconnected("cursor", "Cursor")),
        codex_h.join().unwrap_or_else(|_| disconnected("codex", "Codex")),
        gemini_h.join().unwrap_or_else(|_| disconnected("gemini", "Gemini CLI")),
    ];

    let providers: Vec<TrayProviderSummary> = analytics_list
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
        })
        .collect();

    let connected_count = providers.iter().filter(|p| p.connected).count() as u32;

    let worst_rate_limit = providers
        .iter()
        .flat_map(|p| p.rate_limits.iter())
        .filter(|rl| {
            rl.label.contains("5h")
                || rl.label.contains("Weekly")
                || rl.label.contains("Session")
        })
        .min_by(|a, b| {
            a.remaining_percent
                .partial_cmp(&b.remaining_percent)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .cloned();

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
            let _ = app.emit("tray-data-updated", &summary);

            // Update menu bar tray title with selected provider's worst rate limit %
            update_tray_title(app, &summary);
        }
    }
}

/// Update the tray icon title text (shown next to icon in macOS menu bar).
/// Shows the worst rate limit percentage of the currently selected provider.
fn update_tray_title(app: &AppHandle<Wry>, summary: &TraySummary) {
    let selected = TRAY_SELECTED_PROVIDER.lock()
        .map(|g| g.clone())
        .unwrap_or_else(|_| "claude-code".to_string());

    let provider = summary.providers.iter()
        .find(|p| p.provider_id == selected);

    let title = if let Some(p) = provider {
        if !p.connected {
            None
        } else if p.rate_limits.is_empty() {
            None
        } else {
            // Pick the worst (highest used_percent) primary rate limit
            let worst = p.rate_limits.iter()
                .filter(|rl| {
                    rl.label.contains("Session") || rl.label.contains("5h")
                        || rl.label.contains("Primary") || rl.label.contains("Pro")
                })
                .max_by(|a, b| a.used_percent.partial_cmp(&b.used_percent).unwrap_or(std::cmp::Ordering::Equal));
            // Fallback to any rate limit if no primary found
            let worst = worst.or_else(|| p.rate_limits.iter()
                .max_by(|a, b| a.used_percent.partial_cmp(&b.used_percent).unwrap_or(std::cmp::Ordering::Equal)));
            worst.map(|rl| format!("{:.0}%", rl.used_percent))
        }
    } else {
        None
    };

    if let Some(tray) = app.tray_by_id("main-tray") {
        let _ = tray.set_title(title.as_deref());
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

/// Set the selected provider for tray menu bar percentage display.
/// Called from the frontend when user switches tabs in the tray popover.
#[tauri::command]
pub fn set_tray_display_provider(provider_id: String) {
    if let Ok(mut guard) = TRAY_SELECTED_PROVIDER.lock() {
        *guard = provider_id;
    }
    // Immediately update the tray title with the new provider's data
    if let Ok(cache_guard) = TRAY_SUMMARY_CACHE.lock() {
        if let Some(ref summary) = *cache_guard {
            if let Ok(app_guard) = TRAY_APP_HANDLE.lock() {
                if let Some(ref app) = *app_guard {
                    update_tray_title(app, summary);
                }
            }
        }
    }
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
