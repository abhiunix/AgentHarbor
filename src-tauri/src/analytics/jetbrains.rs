//! JetBrains AI analytics provider.
//! Auth: local file scan (no API calls)
//! Scans AIAssistantQuotaManager2.xml for quota info

use crate::analytics::types::*;
use chrono::Utc;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Find all JetBrains IDE config directories.
fn find_jetbrains_dirs() -> Vec<PathBuf> {
    let mut dirs_to_check = Vec::new();

    // macOS: ~/Library/Application Support/JetBrains/*/
    if let Some(home) = dirs::home_dir() {
        let mac_path = home.join("Library/Application Support/JetBrains");
        if mac_path.is_dir() {
            if let Ok(entries) = fs::read_dir(&mac_path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        dirs_to_check.push(path);
                    }
                }
            }
        }

        // Linux: ~/.config/JetBrains/*/
        let linux_path = home.join(".config/JetBrains");
        if linux_path.is_dir() {
            if let Ok(entries) = fs::read_dir(&linux_path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        dirs_to_check.push(path);
                    }
                }
            }
        }
    }

    dirs_to_check
}

/// Find the most recent AIAssistantQuotaManager2.xml file.
fn find_quota_xml() -> Option<(PathBuf, String)> {
    let jb_dirs = find_jetbrains_dirs();

    let mut newest: Option<(PathBuf, std::time::SystemTime, String)> = None;

    for dir in &jb_dirs {
        let xml_path = dir.join("options/AIAssistantQuotaManager2.xml");
        if xml_path.exists() {
            let modified = fs::metadata(&xml_path)
                .ok()
                .and_then(|m| m.modified().ok())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

            let ide_name = dir.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Unknown")
                .to_string();

            if newest.as_ref().map_or(true, |(_, t, _)| modified > *t) {
                newest = Some((xml_path, modified, ide_name));
            }
        }
    }

    newest.map(|(path, _, ide)| (path, ide))
}

/// Extract the value of a named option from JetBrains XML.
/// Looks for: <option name="KEY" value="VALUE" />
fn extract_option_value(xml: &str, name: &str) -> Option<String> {
    let search = format!("name=\"{}\"", name);
    let idx = xml.find(&search)?;
    let after = &xml[idx..];

    // Find value="..."
    let val_start = after.find("value=\"")?;
    let val_content = &after[val_start + 7..];
    let val_end = val_content.find('"')?;
    let raw = &val_content[..val_end];

    // Decode HTML entities
    Some(decode_html_entities(raw))
}

/// Decode common HTML entities.
fn decode_html_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

/// Parse JSON from a decoded quota string to extract credit info.
/// Expected format: {"used": N, "max": N, "available": N, ...}
fn parse_quota_json(json_str: &str) -> Option<(f64, f64, f64)> {
    let val: serde_json::Value = serde_json::from_str(json_str).ok()?;
    let used = val.get("used").and_then(|v| v.as_f64())?;
    let max = val.get("max").and_then(|v| v.as_f64())?;
    let available = val.get("available").and_then(|v| v.as_f64()).unwrap_or(max - used);
    Some((used, max, available))
}

/// Parse refill info JSON.
/// Expected: {"nextRefillDate": "...", "amount": N, ...}
fn parse_refill_json(json_str: &str) -> Option<(String, f64)> {
    let val: serde_json::Value = serde_json::from_str(json_str).ok()?;
    let date = val.get("nextRefillDate")
        .or_else(|| val.get("next_refill_date"))
        .or_else(|| val.get("date"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())?;
    let amount = val.get("amount")
        .or_else(|| val.get("refillAmount"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    Some((date, amount))
}

// ── Public API ──────────────────────────────────────────────────────────────

pub fn fetch_jetbrains_analytics() -> ProviderAnalytics {
    let now = Utc::now().to_rfc3339();

    let (xml_path, ide_name) = match find_quota_xml() {
        Some(found) => found,
        None => {
            return ProviderAnalytics {
                provider_id: "jetbrains".into(),
                provider_name: "JetBrains AI".into(),
                status: ProviderStatus {
                    provider_id: "jetbrains".into(),
                    provider_name: "JetBrains AI".into(),
                    connected: false,
                    connection_method: "none".into(),
                    account_email: None, plan_name: None, org_name: None,
                    error: Some("No JetBrains AIAssistantQuotaManager2.xml found".into()),
                },
                rate_limits: vec![], credit_usage: None, token_counts: None,
                limit_state: None,
                extra: HashMap::new(), fetched_at: now,
            };
        }
    };

    let xml_content = match fs::read_to_string(&xml_path) {
        Ok(c) => c,
        Err(e) => {
            return ProviderAnalytics {
                provider_id: "jetbrains".into(),
                provider_name: "JetBrains AI".into(),
                status: ProviderStatus {
                    provider_id: "jetbrains".into(),
                    provider_name: "JetBrains AI".into(),
                    connected: false,
                    connection_method: "none".into(),
                    account_email: None, plan_name: None, org_name: None,
                    error: Some(format!("Failed to read XML: {}", e)),
                },
                rate_limits: vec![], credit_usage: None, token_counts: None,
                limit_state: None,
                extra: HashMap::new(), fetched_at: now,
            };
        }
    };

    let mut rate_limits = Vec::new();
    let mut credit_usage = None;
    let mut extra = HashMap::new();

    extra.insert("detected_ide".into(), serde_json::Value::String(ide_name.clone()));
    extra.insert("xml_path".into(), serde_json::Value::String(xml_path.to_string_lossy().to_string()));

    // Parse quotaInfo
    if let Some(quota_str) = extract_option_value(&xml_content, "quotaInfo") {
        extra.insert("quota_info_raw".into(), serde_json::Value::String(quota_str.clone()));

        if let Some((used, max, available)) = parse_quota_json(&quota_str) {
            let used_pct = if max > 0.0 { (used / max * 100.0).min(100.0) } else { 0.0 };

            rate_limits.push(RateLimitWindow {
                provider_id: "jetbrains".into(),
                label: "AI Credits".into(),
                used_percent: used_pct,
                remaining_percent: (100.0 - used_pct).max(0.0),
                resets_at: None, // populated from nextRefill if available
                resets_in_seconds: None,
                window_seconds: None,
            });

            credit_usage = Some(CreditUsage {
                provider_id: "jetbrains".into(),
                used,
                limit: Some(max),
                remaining: available,
                currency: "credits".into(),
                billing_cycle_end: None,
                plan_name: None,
            });
        }
    }

    // Parse nextRefill
    if let Some(refill_str) = extract_option_value(&xml_content, "nextRefill") {
        extra.insert("next_refill_raw".into(), serde_json::Value::String(refill_str.clone()));

        if let Some((date, amount)) = parse_refill_json(&refill_str) {
            extra.insert("next_refill_date".into(), serde_json::Value::String(date.clone()));
            extra.insert("refill_amount".into(), serde_json::Value::from(amount));

            // Update resets_at on rate limits
            for rl in &mut rate_limits {
                rl.resets_at = Some(date.clone());
            }
            // Update billing cycle end on credit usage
            if let Some(ref mut cu) = credit_usage {
                cu.billing_cycle_end = Some(date);
            }
        }
    }

    ProviderAnalytics {
        provider_id: "jetbrains".into(),
        provider_name: "JetBrains AI".into(),
        status: ProviderStatus {
            provider_id: "jetbrains".into(),
            provider_name: "JetBrains AI".into(),
            connected: true,
            connection_method: "local-file".into(),
            account_email: None,
            plan_name: None,
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
    match find_quota_xml() {
        Some((_, ide_name)) => {
            let mut status = ProviderStatus {
                provider_id: "jetbrains".into(),
                provider_name: "JetBrains AI".into(),
                connected: true,
                connection_method: "local-file".into(),
                account_email: None, plan_name: None, org_name: None, error: None,
            };
            // Store IDE name in org_name for display
            status.org_name = Some(ide_name);
            status
        }
        None => ProviderStatus {
            provider_id: "jetbrains".into(),
            provider_name: "JetBrains AI".into(),
            connected: false, connection_method: "none".into(),
            account_email: None, plan_name: None, org_name: None,
            error: Some("No JetBrains AI quota file found".into()),
        },
    }
}
