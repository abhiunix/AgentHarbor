//! Claude Desktop analytics provider.
//! Thin wrapper over claude.rs — shares the same credentials and API.
//! Only changes provider_id to "claude-desktop".

use crate::analytics::types::*;

// ── Public API ──────────────────────────────────────────────────────────────

/// Fetch Claude Desktop analytics by reusing Claude Code's implementation.
pub fn fetch_claude_desktop_analytics() -> ProviderAnalytics {
    let mut analytics = super::claude::fetch_claude_analytics();

    // Override provider identity
    analytics.provider_id = "claude-desktop".into();
    analytics.provider_name = "Claude Desktop".into();
    analytics.status.provider_id = "claude-desktop".into();
    analytics.status.provider_name = "Claude Desktop".into();

    // Update provider_id on rate limits
    for rl in &mut analytics.rate_limits {
        rl.provider_id = "claude-desktop".into();
    }

    // Update provider_id on credit usage
    if let Some(ref mut cu) = analytics.credit_usage {
        cu.provider_id = "claude-desktop".into();
    }

    analytics
}

pub fn check_connection() -> ProviderStatus {
    let mut status = super::claude::check_connection();
    status.provider_id = "claude-desktop".into();
    status.provider_name = "Claude Desktop".into();
    status
}
