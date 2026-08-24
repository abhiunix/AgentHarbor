use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Scope of a usage limit window (Claude Code OAuth usage API).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LimitScope {
    Session5h,
    WeeklyAll,
    WeeklyOpus,
    WeeklySonnet,
    WeeklyOauthApps,
    WeeklyCowork,
    SevenDayOmelette,
    Tangelo,
    IguanaNecktie,
    OmelettePromotional,
    MonthlySpend,
    /// Codex WHAM primary/secondary, etc.
    Custom(String),
}

/// Derived limit / billing health for a provider (tray + notifications).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LimitState {
    Healthy,
    Approaching {
        worst_pct: f64,
        label: String,
        resets_at: Option<String>,
        scope: LimitScope,
    },
    Reached {
        scope: LimitScope,
        used_pct: f64,
        cap: Option<f64>,
        resets_at: Option<String>,
    },
    ApiDisabled {
        reason: String,
        until: Option<String>,
        org_name: String,
    },
    SubscriptionIssue {
        status: String,
        org_name: String,
    },
    BillablePaused {
        until: String,
        org_name: String,
    },
    /// HTTP 429 or explicit rate-limit response without utilization.
    RateLimited {
        retry_after_secs: Option<u64>,
        message: String,
    },
    /// Stored OAuth credentials are no longer accepted by the provider
    /// (HTTP 401). User must reconnect — usually because the upstream tool
    /// (e.g. Claude Code) rotated its token without updating ours.
    Unauthenticated {
        message: String,
    },
}

impl LimitState {
    /// Tray / menu bar should emphasize danger styling.
    pub fn is_danger(&self) -> bool {
        matches!(
            self,
            LimitState::Reached { .. }
                | LimitState::ApiDisabled { .. }
                | LimitState::SubscriptionIssue { .. }
                | LimitState::BillablePaused { .. }
                | LimitState::RateLimited { .. }
                | LimitState::Unauthenticated { .. }
        )
    }

    /// Shorter analytics cache TTL when user may be blocked soon or now.
    pub fn prefers_fast_refresh(&self) -> bool {
        match self {
            LimitState::Healthy => false,
            LimitState::Approaching { .. } | LimitState::Reached { .. } => true,
            _ => true,
        }
    }
}

/// A single rate-limit window (session, weekly, per-model, etc.)
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RateLimitWindow {
    /// Provider this belongs to (e.g. "claude-code")
    pub provider_id: String,
    /// Human label (e.g. "Session (5h)", "Weekly", "Sonnet")
    pub label: String,
    /// Percentage used (0.0–100.0)
    pub used_percent: f64,
    /// Percentage remaining (0.0–100.0)
    pub remaining_percent: f64,
    /// ISO8601 reset time (if known)
    pub resets_at: Option<String>,
    /// Seconds until reset (if known)
    pub resets_in_seconds: Option<i64>,
    /// Total window size in seconds (e.g. 18000 for 5h)
    pub window_seconds: Option<i64>,
}

/// Credit or spend information for a provider
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CreditUsage {
    pub provider_id: String,
    /// Amount used
    pub used: f64,
    /// Total limit (None = unlimited)
    pub limit: Option<f64>,
    /// Amount remaining
    pub remaining: f64,
    /// "USD", "credits", etc.
    pub currency: String,
    /// ISO8601 billing cycle end
    pub billing_cycle_end: Option<String>,
    /// Plan name
    pub plan_name: Option<String>,
}

/// Aggregated token counts
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct TokenCounts {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub estimated_cost_usd: Option<f64>,
}

/// Connection status for a provider
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProviderStatus {
    pub provider_id: String,
    pub provider_name: String,
    /// Whether we can fetch data
    pub connected: bool,
    /// How we're connected: "oauth-auto", "token-manual", "cookie", "cli", "local-file", "device-flow", "none"
    pub connection_method: String,
    /// Account email if available
    pub account_email: Option<String>,
    /// Plan/tier name
    pub plan_name: Option<String>,
    /// Organization name
    pub org_name: Option<String>,
    /// Error message if connection failed
    pub error: Option<String>,
}

/// Full analytics for one provider
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProviderAnalytics {
    pub provider_id: String,
    pub provider_name: String,
    pub status: ProviderStatus,
    pub rate_limits: Vec<RateLimitWindow>,
    pub credit_usage: Option<CreditUsage>,
    pub token_counts: Option<TokenCounts>,
    /// Derived limit / billing state (Claude, Codex, …).
    #[serde(default)]
    pub limit_state: Option<LimitState>,
    /// Provider-specific extra data
    pub extra: HashMap<String, serde_json::Value>,
    /// ISO8601 when this data was fetched
    pub fetched_at: String,
}

/// All providers known to the system
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    /// "auto-detect", "token", "api-key", "device-flow", "cli", "local-file"
    pub auth_type: String,
    /// Short description of what data is available
    pub description: String,
    /// Whether this provider has local data (JSONL, SQLite, XML)
    pub has_local_data: bool,
    /// Whether this provider has an API for live data
    pub has_api: bool,
}

/// Provider registry — all known providers
pub fn all_provider_info() -> Vec<ProviderInfo> {
    vec![
        ProviderInfo {
            id: "claude-code".into(),
            name: "Claude Code".into(),
            auth_type: "auto-detect".into(),
            description: "Rate limits, token usage, session stats".into(),
            has_local_data: true,
            has_api: true,
        },
        ProviderInfo {
            id: "cursor".into(),
            name: "Cursor".into(),
            auth_type: "token".into(),
            description: "Plan usage, AI code attribution, model breakdown".into(),
            has_local_data: true,
            has_api: true,
        },
        ProviderInfo {
            id: "codex".into(),
            name: "Codex (OpenAI)".into(),
            auth_type: "auto-detect".into(),
            description: "Rate limits, credits, token usage".into(),
            has_local_data: true,
            has_api: true,
        },
        ProviderInfo {
            id: "gemini".into(),
            name: "Gemini CLI".into(),
            auth_type: "auto-detect".into(),
            description: "Per-model quota, tier info".into(),
            has_local_data: false,
            has_api: true,
        },
        ProviderInfo {
            id: "claude-desktop".into(),
            name: "Claude Desktop".into(),
            auth_type: "auto-detect".into(),
            description: "Shares rate limits with Claude Code".into(),
            has_local_data: true,
            has_api: true,
        },
        ProviderInfo {
            id: "copilot".into(),
            name: "GitHub Copilot".into(),
            auth_type: "device-flow".into(),
            description: "Premium & chat quotas, plan info".into(),
            has_local_data: false,
            has_api: true,
        },
        ProviderInfo {
            id: "kiro".into(),
            name: "Kiro".into(),
            auth_type: "cli".into(),
            description: "Credits usage, bonus credits".into(),
            has_local_data: false,
            has_api: false,
        },
        ProviderInfo {
            id: "vertex-ai".into(),
            name: "Vertex AI".into(),
            auth_type: "auto-detect".into(),
            description: "Quota usage, Vertex-specific token costs".into(),
            has_local_data: true,
            has_api: true,
        },
        ProviderInfo {
            id: "jetbrains".into(),
            name: "JetBrains AI".into(),
            auth_type: "local-file".into(),
            description: "Credits, refill schedule, detected IDE".into(),
            has_local_data: true,
            has_api: false,
        },
        ProviderInfo {
            id: "openrouter".into(),
            name: "OpenRouter".into(),
            auth_type: "api-key".into(),
            description: "Credits balance, usage, rate limits".into(),
            has_local_data: false,
            has_api: true,
        },
        ProviderInfo {
            id: "augment".into(),
            name: "Augment".into(),
            auth_type: "token".into(),
            description: "Credits remaining, plan, billing cycle".into(),
            has_local_data: false,
            has_api: true,
        },
        ProviderInfo {
            id: "amp".into(),
            name: "Amp".into(),
            auth_type: "token".into(),
            description: "Free tier quota, replenishment rate".into(),
            has_local_data: false,
            has_api: true,
        },
        ProviderInfo {
            id: "droid".into(),
            name: "Droid (Factory)".into(),
            auth_type: "token".into(),
            description: "Standard/premium token usage, billing".into(),
            has_local_data: false,
            has_api: true,
        },
        ProviderInfo {
            id: "kimi".into(),
            name: "Kimi".into(),
            auth_type: "token".into(),
            description: "Weekly quota, 5h rate limit".into(),
            has_local_data: false,
            has_api: true,
        },
        ProviderInfo {
            id: "deepseek".into(),
            name: "DeepSeek".into(),
            auth_type: "api-key".into(),
            description: "Account balance (granted + topped-up), multi-currency".into(),
            has_local_data: false,
            has_api: true,
        },
        ProviderInfo {
            id: "kimi-k2".into(),
            name: "Kimi K2".into(),
            auth_type: "api-key".into(),
            description: "Credits consumed/remaining".into(),
            has_local_data: false,
            has_api: true,
        },
        ProviderInfo {
            id: "zai".into(),
            name: "z.ai".into(),
            auth_type: "api-key".into(),
            description: "Token quota, time limit, per-model breakdown".into(),
            has_local_data: false,
            has_api: true,
        },
        ProviderInfo {
            id: "windsurf".into(),
            name: "Windsurf".into(),
            auth_type: "auto-detect".into(),
            description: "Config status (no usage API available)".into(),
            has_local_data: true,
            has_api: false,
        },
    ]
}
