//! Kiro CLI analytics provider.
//! Auth: uses `kiro-cli` binary on PATH
//! Runs `kiro-cli chat --no-interactive /usage` and parses output

use crate::analytics::types::*;
use chrono::Utc;
use std::collections::HashMap;
use std::process::Command;
use std::time::Duration;

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Check if kiro-cli is available on PATH.
fn find_kiro_cli() -> Result<std::path::PathBuf, String> {
    which::which("kiro-cli").map_err(|_| "kiro-cli not found on PATH".into())
}

/// Strip ANSI escape codes from a string.
fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            // Skip until we find a letter (end of ANSI sequence)
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            result.push(ch);
        }
    }
    result
}

/// Extract a number after a label like "Credits Used: 42" or "42/100"
fn extract_number_after(text: &str, label: &str) -> Option<f64> {
    let idx = text.find(label)?;
    let after = &text[idx + label.len()..];
    let trimmed = after.trim_start_matches(|c: char| c == ':' || c == ' ');
    let mut num_str = String::new();
    for ch in trimmed.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            num_str.push(ch);
        } else if !num_str.is_empty() {
            break;
        }
    }
    num_str.parse::<f64>().ok()
}

/// Extract a value for "Key: Value" pattern (returns the rest of the line)
fn extract_value_after(text: &str, label: &str) -> Option<String> {
    let idx = text.find(label)?;
    let after = &text[idx + label.len()..];
    let trimmed = after.trim_start_matches(|c: char| c == ':' || c == ' ');
    let line = trimmed.lines().next()?;
    let val = line.trim();
    if val.is_empty() { None } else { Some(val.to_string()) }
}

/// Parse usage fraction like "42/100" or "42 / 100"
fn extract_fraction(text: &str, label: &str) -> Option<(f64, f64)> {
    let idx = text.find(label)?;
    let after = &text[idx + label.len()..];
    let trimmed = after.trim_start_matches(|c: char| c == ':' || c == ' ');

    // Find first number
    let mut first = String::new();
    let mut chars = trimmed.chars().peekable();
    while let Some(&ch) = chars.peek() {
        if ch.is_ascii_digit() || ch == '.' {
            first.push(ch);
            chars.next();
        } else if !first.is_empty() {
            break;
        } else {
            chars.next();
        }
    }

    // Skip the separator (typically '/')
    while let Some(&ch) = chars.peek() {
        if ch == '/' || ch == ' ' {
            chars.next();
        } else {
            break;
        }
    }

    // Find second number
    let mut second = String::new();
    for ch in chars {
        if ch.is_ascii_digit() || ch == '.' {
            second.push(ch);
        } else if !second.is_empty() {
            break;
        }
    }

    let a = first.parse::<f64>().ok()?;
    let b = second.parse::<f64>().ok()?;
    Some((a, b))
}

/// Extract percentage like "42%" or "42.5%"
fn extract_percentage(text: &str, label: &str) -> Option<f64> {
    let idx = text.find(label)?;
    let after = &text[idx + label.len()..];
    let trimmed = after.trim_start_matches(|c: char| c == ':' || c == ' ');
    let mut num_str = String::new();
    for ch in trimmed.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            num_str.push(ch);
        } else if ch == '%' {
            break;
        } else if !num_str.is_empty() {
            break;
        }
    }
    num_str.parse::<f64>().ok()
}

// ── Public API ──────────────────────────────────────────────────────────────

pub fn fetch_kiro_analytics() -> ProviderAnalytics {
    let now = Utc::now().to_rfc3339();

    let kiro_path = match find_kiro_cli() {
        Ok(p) => p,
        Err(e) => {
            return ProviderAnalytics {
                provider_id: "kiro".into(),
                provider_name: "Kiro".into(),
                status: ProviderStatus {
                    provider_id: "kiro".into(),
                    provider_name: "Kiro".into(),
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

    // Run kiro-cli with a 20 second timeout
    let output = Command::new(&kiro_path)
        .args(["chat", "--no-interactive", "/usage"])
        .env("NO_COLOR", "1")
        .output();

    let output = match output {
        Ok(o) => o,
        Err(e) => {
            return ProviderAnalytics {
                provider_id: "kiro".into(),
                provider_name: "Kiro".into(),
                status: ProviderStatus {
                    provider_id: "kiro".into(),
                    provider_name: "Kiro".into(),
                    connected: true,
                    connection_method: "cli".into(),
                    account_email: None, plan_name: None, org_name: None,
                    error: Some(format!("Failed to run kiro-cli: {}", e)),
                },
                rate_limits: vec![], credit_usage: None, token_counts: None,
                limit_state: None,
                extra: HashMap::new(), fetched_at: now,
            };
        }
    };

    let raw_output = String::from_utf8_lossy(&output.stdout).to_string();
    let text = strip_ansi(&raw_output);

    // Parse the output
    let plan = extract_value_after(&text, "Plan");
    let mut extra = HashMap::new();

    if let Some(ref p) = plan {
        extra.insert("plan".into(), serde_json::Value::String(p.clone()));
    }

    // Try to parse credits used/total
    let mut rate_limits = Vec::new();
    let mut credit_usage = None;

    if let Some((used, total)) = extract_fraction(&text, "Credits") {
        let used_pct = if total > 0.0 { (used / total * 100.0).min(100.0) } else { 0.0 };
        rate_limits.push(RateLimitWindow {
            provider_id: "kiro".into(),
            label: "Credits".into(),
            used_percent: used_pct,
            remaining_percent: (100.0 - used_pct).max(0.0),
            resets_at: extract_value_after(&text, "Reset"),
            resets_in_seconds: None,
            window_seconds: None,
        });
        credit_usage = Some(CreditUsage {
            provider_id: "kiro".into(),
            used,
            limit: Some(total),
            remaining: (total - used).max(0.0),
            currency: "credits".into(),
            billing_cycle_end: extract_value_after(&text, "Reset"),
            plan_name: plan.clone(),
        });
    } else if let Some(pct) = extract_percentage(&text, "Credits") {
        // Fallback: just percentage
        rate_limits.push(RateLimitWindow {
            provider_id: "kiro".into(),
            label: "Credits".into(),
            used_percent: pct,
            remaining_percent: (100.0 - pct).max(0.0),
            resets_at: extract_value_after(&text, "Reset"),
            resets_in_seconds: None,
            window_seconds: None,
        });
    }

    // Bonus credits
    if let Some(bonus) = extract_number_after(&text, "Bonus") {
        extra.insert("bonus_credits".into(), serde_json::Value::from(bonus));
    }

    // Reset date
    if let Some(reset) = extract_value_after(&text, "Reset") {
        extra.insert("reset_date".into(), serde_json::Value::String(reset));
    }

    // Store raw output for debugging
    extra.insert("raw_output".into(), serde_json::Value::String(text.clone()));

    ProviderAnalytics {
        provider_id: "kiro".into(),
        provider_name: "Kiro".into(),
        status: ProviderStatus {
            provider_id: "kiro".into(),
            provider_name: "Kiro".into(),
            connected: true,
            connection_method: "cli".into(),
            account_email: None,
            plan_name: plan,
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
    match find_kiro_cli() {
        Ok(_) => ProviderStatus {
            provider_id: "kiro".into(),
            provider_name: "Kiro".into(),
            connected: true, connection_method: "cli".into(),
            account_email: None, plan_name: None, org_name: None, error: None,
        },
        Err(e) => ProviderStatus {
            provider_id: "kiro".into(),
            provider_name: "Kiro".into(),
            connected: false, connection_method: "none".into(),
            account_email: None, plan_name: None, org_name: None, error: Some(e),
        },
    }
}
