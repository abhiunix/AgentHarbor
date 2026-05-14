//! Cursor Analytics V2 — comprehensive dashboard using Cursor's dashboard APIs.
//! Auth: auto-detect from Cursor's local SQLite DB → user-provided token fallback.
//! APIs: cursor.com/api/dashboard/*, /api/v2/analytics/*, /api/auth/*, /api/usage-summary

use crate::analytics::cursor::{cookie_header, resolve_token};
use crate::analytics::http;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

// ── API Response Types ──────────────────────────────────────────────────────

// Auth & Identity

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CursorAuthMe {
    pub email: Option<String>,
    pub name: Option<String>,
    pub sub: Option<String>,
    pub id: Option<i64>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub picture: Option<String>,
    pub email_verified: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CursorStripeInfo {
    pub membership_type: Option<String>,
    pub payment_id: Option<String>,
    pub is_team_member: Option<bool>,
    pub team_id: Option<i64>,
    pub team_membership_type: Option<String>,
    pub individual_membership_type: Option<String>,
    pub is_on_billable_auto: Option<bool>,
    pub is_yearly_plan: Option<bool>,
    pub last_payment_failed: Option<bool>,
    pub pending_cancellation_date: Option<String>,
    pub customer_balance: Option<f64>,
    pub verified_student: Option<bool>,
}

// Usage Summary (new nested structure)

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UsagePlanBreakdown {
    pub included: Option<f64>,
    pub bonus: Option<f64>,
    pub total: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UsagePlan {
    pub enabled: Option<bool>,
    pub used: Option<f64>,       // cents
    pub limit: Option<f64>,      // cents
    pub remaining: Option<f64>,  // cents
    pub breakdown: Option<UsagePlanBreakdown>,
    pub auto_percent_used: Option<f64>,
    pub api_percent_used: Option<f64>,
    pub total_percent_used: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UsageOnDemand {
    pub enabled: Option<bool>,
    pub used: Option<f64>,       // cents
    pub limit: Option<f64>,      // cents (null if unlimited)
    pub remaining: Option<f64>,  // cents (null if unlimited)
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct IndividualUsage {
    pub plan: Option<UsagePlan>,
    pub on_demand: Option<UsageOnDemand>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TeamUsage {
    pub on_demand: Option<UsageOnDemand>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CursorUsageSummary {
    pub billing_cycle_start: Option<String>,
    pub billing_cycle_end: Option<String>,
    pub membership_type: Option<String>,
    pub limit_type: Option<String>,
    pub is_unlimited: Option<bool>,
    pub individual_usage: Option<IndividualUsage>,
    pub team_usage: Option<TeamUsage>,
}

// Usage Events

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_write_tokens: Option<i64>,
    pub cache_read_tokens: Option<i64>,
    pub total_cents: Option<f64>,
}

// Note: TokenUsage now has rename_all = camelCase to match the API response
// (inputTokens, outputTokens, cacheWriteTokens, cacheReadTokens, totalCents)

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CursorUsageEvent {
    pub timestamp: Option<String>,  // epoch ms string
    pub model: Option<String>,
    pub kind: Option<String>,
    pub max_mode: Option<bool>,
    pub requests_costs: Option<f64>,
    pub usage_based_costs: Option<String>,  // e.g. "$0.96"
    pub is_token_based_call: Option<bool>,
    pub token_usage: Option<TokenUsage>,
    pub owning_user: Option<String>,
    pub owning_team: Option<String>,
    pub cursor_token_fee: Option<f64>,
    pub is_chargeable: Option<bool>,
    pub is_headless: Option<bool>,
    pub charged_cents: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UsageEventsResponse {
    pub total_usage_events_count: Option<i64>,
    pub usage_events_display: Option<Vec<CursorUsageEvent>>,
}

// Hard Limit

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CursorHardLimit {
    pub hard_limit: Option<f64>,  // cents
    pub is_dynamic_team_limit: Option<bool>,
}

// Teams

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CursorTeamInfo {
    pub name: Option<String>,
    pub id: Option<i64>,
    pub role: Option<String>,
    pub seats: Option<i64>,
    pub has_billing: Option<bool>,
    pub privacy_mode_forced: Option<bool>,
    pub subscription_status: Option<String>,
    pub pricing_strategy: Option<String>,
    pub billing_cycle_start: Option<String>,  // epoch ms string
    pub billing_cycle_end: Option<String>,    // epoch ms string
    pub sso_enabled: Option<bool>,
    pub dashboard_analytics_requires_admin: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct TeamsResponse {
    teams: Option<Vec<CursorTeamInfo>>,
}

// Team Spend

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TeamMemberSpend {
    pub user_id: Option<i64>,
    pub spend_cents: Option<f64>,
    pub name: Option<String>,
    pub email: Option<String>,
    pub role: Option<String>,
    pub included_spend_cents: Option<f64>,
    pub profile_picture_url: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TeamSpendResponse {
    pub team_member_spend: Option<Vec<TeamMemberSpend>>,
    pub total_members: Option<i64>,
    pub total_pages: Option<i64>,
    pub max_user_spend_cents: Option<f64>,
}

// V2 Analytics — ClickHouse-style responses

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AiCommitsAggregated {
    pub total_commits: Option<i64>,
    pub total_lines_added: Option<i64>,
    pub ai_lines_added: Option<i64>,
    pub ai_impact_percentage: Option<f64>,
    pub unique_repos: Option<i64>,
    pub avg_ai_lines_per_commit: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AnalyticsMetaField {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AnalyticsTimeseriesResponse {
    pub meta: Option<Vec<AnalyticsMetaField>>,
    pub data: Option<Vec<HashMap<String, serde_json::Value>>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModelBreakdownEntry {
    pub requests: Option<i64>,
    pub users: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModelUsageDay {
    pub date: String,
    pub model_breakdown: HashMap<String, ModelBreakdownEntry>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModelUsageResponse {
    pub meta: Option<Vec<AnalyticsMetaField>>,
    pub data: Option<Vec<ModelUsageDayRaw>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModelUsageDayRaw {
    pub date: Option<String>,
    pub model_breakdown: Option<HashMap<String, HashMap<String, serde_json::Value>>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModelAggregatedEntry {
    pub model_intent: Option<String>,
    pub total_requests: Option<i64>,
    pub total_unique_users: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModelAggregatedResponse {
    pub meta: Option<Vec<AnalyticsMetaField>>,
    pub data: Option<Vec<ModelAggregatedEntry>>,
}

// ── Combined Overview ───────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CursorV2Overview {
    pub auth: Option<CursorAuthMe>,
    pub stripe: Option<CursorStripeInfo>,
    pub usage_summary: Option<CursorUsageSummary>,
    pub hard_limit: Option<CursorHardLimit>,
    pub ai_commits: Option<AiCommitsAggregated>,
    pub model_aggregated: Option<Vec<ModelAggregatedEntry>>,
    pub team: Option<CursorTeamInfo>,
    // New enriched fields
    pub composer_stats: Option<serde_json::Value>,
    pub tab_stats: Option<serde_json::Value>,
    pub top_files: Option<serde_json::Value>,
    pub request_breakdown: Option<serde_json::Value>,
    pub ai_commits_by_repo: Option<serde_json::Value>,
    pub sessions: Option<serde_json::Value>,
    pub team_members: Option<serde_json::Value>,
    pub connection_method: String,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CursorV2UsageEventsPage {
    pub events: Vec<CursorUsageEvent>,
    pub total_count: i64,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CursorV2ConnectionStatus {
    pub connected: bool,
    pub connection_method: String,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub team_name: Option<String>,
    pub team_id: Option<i64>,
    pub error: Option<String>,
}

// ── Cache ───────────────────────────────────────────────────────────────────

struct CacheEntry<T> {
    data: T,
    fetched_at: Instant,
    fetched_at_iso: String,
}

impl<T> CacheEntry<T> {
    fn new(data: T) -> Self {
        Self {
            data,
            fetched_at: Instant::now(),
            fetched_at_iso: chrono::Utc::now().to_rfc3339(),
        }
    }

    fn is_valid(&self, ttl_seconds: u64) -> bool {
        self.fetched_at.elapsed() < Duration::from_secs(ttl_seconds)
    }
}

struct CursorV2Cache {
    overview: HashMap<String, CacheEntry<CursorV2Overview>>,       // keyed by time_range
    events: HashMap<String, CacheEntry<CursorV2UsageEventsPage>>,  // keyed by "timeRange:page:pageSize"
    ai_commits: HashMap<String, CacheEntry<AnalyticsTimeseriesResponse>>,  // keyed by time_range
    model_usage: HashMap<String, CacheEntry<ModelUsageResponse>>,  // keyed by time_range
    composer_tabs: HashMap<String, CacheEntry<serde_json::Value>>, // keyed by time_range
    team_spend: HashMap<String, CacheEntry<TeamSpendResponse>>,    // keyed by "page:pageSize"
    request_breakdown: HashMap<String, CacheEntry<AnalyticsTimeseriesResponse>>, // keyed by time_range
    ttl_seconds: u64,
}

lazy_static! {
    static ref CACHE: Mutex<CursorV2Cache> = Mutex::new(CursorV2Cache {
        overview: HashMap::new(),
        events: HashMap::new(),
        ai_commits: HashMap::new(),
        model_usage: HashMap::new(),
        composer_tabs: HashMap::new(),
        team_spend: HashMap::new(),
        request_breakdown: HashMap::new(),
        ttl_seconds: 300, // 5 minutes default
    });
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CursorV2CacheInfo {
    pub last_refreshed: Option<String>,
    pub ttl_seconds: u64,
    pub entries_count: u32,
}

// ── Internal helpers ────────────────────────────────────────────────────────

fn post_headers() -> reqwest::header::HeaderMap {
    http::headers(&[("Origin", "https://cursor.com")])
}

fn time_range_to_epoch_ms(range: &str) -> (String, String) {
    let now = chrono::Utc::now();
    let end = now.timestamp_millis().to_string();
    let start = match range {
        "1d" => (now - chrono::Duration::days(1)).timestamp_millis(),
        "7d" => (now - chrono::Duration::days(7)).timestamp_millis(),
        "30d" => (now - chrono::Duration::days(30)).timestamp_millis(),
        "90d" => (now - chrono::Duration::days(90)).timestamp_millis(),
        "all" => (now - chrono::Duration::days(3650)).timestamp_millis(),
        _ => (now - chrono::Duration::days(30)).timestamp_millis(),
    };
    (start.to_string(), end)
}

fn time_range_to_dates(range: &str) -> (String, String) {
    let now = chrono::Utc::now();
    let end = now.format("%Y-%m-%d").to_string();
    let start = match range {
        "1d" => (now - chrono::Duration::days(1)).format("%Y-%m-%d").to_string(),
        "7d" => (now - chrono::Duration::days(7)).format("%Y-%m-%d").to_string(),
        "30d" => (now - chrono::Duration::days(30)).format("%Y-%m-%d").to_string(),
        "90d" => (now - chrono::Duration::days(90)).format("%Y-%m-%d").to_string(),
        "all" => (now - chrono::Duration::days(3650)).format("%Y-%m-%d").to_string(),
        _ => (now - chrono::Duration::days(30)).format("%Y-%m-%d").to_string(),
    };
    (start, end)
}

/// Fetch the team ID from /api/auth/stripe
fn fetch_team_id(cookie: &str) -> Option<i64> {
    let stripe: CursorStripeInfo = http::cookie_get(
        "https://cursor.com/api/auth/stripe",
        cookie,
        None,
    ).ok()?;
    stripe.team_id
}

// ── Internal sync functions (run on background thread) ──────────────────────

fn _connection_status() -> CursorV2ConnectionStatus {
    let (token, method) = match resolve_token() {
        Ok(t) => t,
        Err(e) => {
            return CursorV2ConnectionStatus {
                connected: false,
                connection_method: "none".into(),
                email: None, plan: None, team_name: None, team_id: None,
                error: Some(e),
            };
        }
    };

    let cookie = cookie_header(&token);

    let auth: Result<CursorAuthMe, String> = http::cookie_get(
        "https://cursor.com/api/auth/me", &cookie, None,
    )
    .map_err(String::from);
    let auth = auth.ok();

    let stripe: Option<CursorStripeInfo> = http::cookie_get(
        "https://cursor.com/api/auth/stripe", &cookie, None,
    ).ok();

    CursorV2ConnectionStatus {
        connected: auth.is_some(),
        connection_method: method,
        email: auth.as_ref().and_then(|a| a.email.clone()),
        plan: stripe.as_ref().and_then(|s| s.membership_type.clone()),
        team_name: None,
        team_id: stripe.as_ref().and_then(|s| s.team_id),
        error: if auth.is_none() { Some("Token may be expired — try signing in again".into()) } else { None },
    }
}

fn _overview(time_range: &str) -> Result<CursorV2Overview, String> {
    let (token, method) = resolve_token()?;
    let cookie = cookie_header(&token);

    let auth: Option<CursorAuthMe> = http::cookie_get(
        "https://cursor.com/api/auth/me", &cookie, None,
    ).ok();

    let stripe: Option<CursorStripeInfo> = http::cookie_get(
        "https://cursor.com/api/auth/stripe", &cookie, None,
    ).ok();

    let usage_summary: Option<CursorUsageSummary> = http::cookie_get(
        "https://cursor.com/api/usage-summary", &cookie, None,
    ).ok();

    let team_id = stripe.as_ref().and_then(|s| s.team_id);

    let hard_limit = team_id.and_then(|tid| {
        http::cookie_post::<CursorHardLimit>(
            "https://cursor.com/api/dashboard/get-hard-limit",
            &cookie,
            &serde_json::json!({"teamId": tid}),
            Some(post_headers()),
        ).ok()
    });

    let team = team_id.and_then(|tid| {
        let resp: TeamsResponse = http::cookie_post(
            "https://cursor.com/api/dashboard/teams",
            &cookie,
            &serde_json::json!({}),
            Some(post_headers()),
        ).ok()?;
        resp.teams?.into_iter().find(|t| t.id == Some(tid))
    });

    let (start_date, end_date) = time_range_to_dates(time_range);
    let ai_commits = team_id.and_then(|tid| {
        let url = format!(
            "https://cursor.com/api/v2/analytics/team/ai-commits/aggregated?startDate={}&endDate={}&teamId={}",
            start_date, end_date, tid
        );
        http::cookie_get::<AiCommitsAggregated>(&url, &cookie, None).ok()
    });

    let model_aggregated = team_id.and_then(|tid| {
        let url = format!(
            "https://cursor.com/api/v2/analytics/team/models/aggregated?startDate={}&endDate={}&teamId={}",
            start_date, end_date, tid
        );
        let resp: ModelAggregatedResponse = http::cookie_get(&url, &cookie, None).ok()?;
        resp.data
    });

    // Composer stats (agent edit accept/reject)
    let composer_stats = team_id.and_then(|tid| {
        let url = format!(
            "https://cursor.com/api/v2/analytics/team/composer?startDate={}&endDate={}&teamId={}",
            start_date, end_date, tid
        );
        http::cookie_get::<serde_json::Value>(&url, &cookie, None).ok()
    });

    // Tab completion stats
    let tab_stats = team_id.and_then(|tid| {
        let url = format!(
            "https://cursor.com/api/v2/analytics/team/tabs?startDate={}&endDate={}&teamId={}",
            start_date, end_date, tid
        );
        http::cookie_get::<serde_json::Value>(&url, &cookie, None).ok()
    });

    // Top file extensions
    let top_files = team_id.and_then(|tid| {
        let url = format!(
            "https://cursor.com/api/v2/analytics/team/top-files?startDate={}&endDate={}&teamId={}",
            start_date, end_date, tid
        );
        http::cookie_get::<serde_json::Value>(&url, &cookie, None).ok()
    });

    // Request breakdown (daily usage by type)
    let request_breakdown = team_id.and_then(|tid| {
        let url = format!(
            "https://cursor.com/api/v2/analytics/team/usage?startDate={}&endDate={}&teamId={}",
            start_date, end_date, tid
        );
        http::cookie_get::<serde_json::Value>(&url, &cookie, None).ok()
    });

    // AI commits by repo
    let ai_commits_by_repo = team_id.and_then(|tid| {
        let url = format!(
            "https://cursor.com/api/v2/analytics/team/ai-commits/by-repo?startDate={}&endDate={}&teamId={}",
            start_date, end_date, tid
        );
        http::cookie_get::<serde_json::Value>(&url, &cookie, None).ok()
    });

    // Active sessions
    let sessions: Option<serde_json::Value> = http::cookie_get(
        "https://cursor.com/api/auth/sessions", &cookie, None,
    ).ok();

    // Team members
    let team_members: Option<serde_json::Value> = team_id.and_then(|tid| {
        http::cookie_post(
            "https://cursor.com/api/dashboard/team",
            &cookie,
            &serde_json::json!({"teamId": tid}),
            Some(post_headers()),
        ).ok()
    });

    Ok(CursorV2Overview {
        auth, stripe, usage_summary, hard_limit, ai_commits, model_aggregated, team,
        composer_stats, tab_stats, top_files, request_breakdown, ai_commits_by_repo, sessions,
        team_members, connection_method: method, error: None,
    })
}

fn _usage_events(time_range: &str, page: u32, page_size: u32) -> Result<CursorV2UsageEventsPage, String> {
    let (token, _) = resolve_token()?;
    let cookie = cookie_header(&token);
    let team_id = fetch_team_id(&cookie).ok_or("Cannot determine team ID")?;
    let (start, end) = time_range_to_epoch_ms(time_range);

    let resp: UsageEventsResponse = http::cookie_post(
        "https://cursor.com/api/dashboard/get-filtered-usage-events",
        &cookie,
        &serde_json::json!({"teamId": team_id, "startDate": start, "endDate": end, "page": page, "pageSize": page_size}),
        Some(post_headers()),
    )?;

    Ok(CursorV2UsageEventsPage {
        events: resp.usage_events_display.unwrap_or_default(),
        total_count: resp.total_usage_events_count.unwrap_or(0),
        page, page_size,
    })
}

fn _ai_commits(time_range: &str) -> Result<AnalyticsTimeseriesResponse, String> {
    let (token, _) = resolve_token()?;
    let cookie = cookie_header(&token);
    let team_id = fetch_team_id(&cookie).ok_or("Cannot determine team ID")?;
    let (start, end) = time_range_to_dates(time_range);
    let url = format!(
        "https://cursor.com/api/v2/analytics/team/ai-commits/timeseries?startDate={}&endDate={}&teamId={}",
        start, end, team_id
    );
    http::cookie_get(&url, &cookie, None).map_err(String::from)
}

fn _model_usage(time_range: &str) -> Result<ModelUsageResponse, String> {
    let (token, _) = resolve_token()?;
    let cookie = cookie_header(&token);
    let team_id = fetch_team_id(&cookie).ok_or("Cannot determine team ID")?;
    let (start, end) = time_range_to_dates(time_range);
    let url = format!(
        "https://cursor.com/api/v2/analytics/team/models?startDate={}&endDate={}&teamId={}",
        start, end, team_id
    );
    http::cookie_get(&url, &cookie, None).map_err(String::from)
}

fn _composer_tabs(time_range: &str) -> Result<serde_json::Value, String> {
    let (token, _) = resolve_token()?;
    let cookie = cookie_header(&token);
    let team_id = fetch_team_id(&cookie).ok_or("Cannot determine team ID")?;
    let (start, end) = time_range_to_dates(time_range);

    let composer: Option<serde_json::Value> = http::cookie_get(
        &format!("https://cursor.com/api/v2/analytics/team/composer?startDate={}&endDate={}&teamId={}", start, end, team_id),
        &cookie, None,
    ).ok();
    let tabs: Option<serde_json::Value> = http::cookie_get(
        &format!("https://cursor.com/api/v2/analytics/team/tabs?startDate={}&endDate={}&teamId={}", start, end, team_id),
        &cookie, None,
    ).ok();
    let top_files: Option<serde_json::Value> = http::cookie_get(
        &format!("https://cursor.com/api/v2/analytics/team/top-files?startDate={}&endDate={}&teamId={}", start, end, team_id),
        &cookie, None,
    ).ok();

    Ok(serde_json::json!({"composer": composer, "tabs": tabs, "topFiles": top_files}))
}

fn _team_spend(page: u32, page_size: u32) -> Result<TeamSpendResponse, String> {
    let (token, _) = resolve_token()?;
    let cookie = cookie_header(&token);
    let team_id = fetch_team_id(&cookie).ok_or("Cannot determine team ID")?;
    http::cookie_post(
        "https://cursor.com/api/dashboard/get-team-spend",
        &cookie,
        &serde_json::json!({"teamId": team_id, "page": page, "pageSize": page_size, "sortBy": "spend", "sortDirection": "desc"}),
        Some(post_headers()),
    )
    .map_err(String::from)
}

fn _request_breakdown(time_range: &str) -> Result<AnalyticsTimeseriesResponse, String> {
    let (token, _) = resolve_token()?;
    let cookie = cookie_header(&token);
    let team_id = fetch_team_id(&cookie).ok_or("Cannot determine team ID")?;
    let (start, end) = time_range_to_dates(time_range);
    let url = format!(
        "https://cursor.com/api/v2/analytics/team/usage?startDate={}&endDate={}&teamId={}",
        start, end, team_id
    );
    http::cookie_get(&url, &cookie, None).map_err(String::from)
}

// ── Tauri Commands (async — run blocking HTTP on background thread) ─────────

#[tauri::command]
pub async fn get_cursor_v2_connection_status() -> Result<CursorV2ConnectionStatus, String> {
    // Connection status is NOT cached — it's fast (just reads SQLite)
    tokio::task::spawn_blocking(_connection_status)
        .await
        .map_err(|e| format!("Task failed: {}", e))
}

#[tauri::command]
pub async fn get_cursor_v2_overview(time_range: String, force_refresh: Option<bool>) -> Result<CursorV2Overview, String> {
    let force = force_refresh.unwrap_or(false);
    if !force {
        if let Ok(cache) = CACHE.lock() {
            if let Some(entry) = cache.overview.get(&time_range) {
                if entry.is_valid(cache.ttl_seconds) {
                    return Ok(entry.data.clone());
                }
            }
        }
    }
    let tr = time_range.clone();
    let result = tokio::task::spawn_blocking(move || _overview(&tr))
        .await
        .map_err(|e| format!("Task failed: {}", e))??;
    if let Ok(mut cache) = CACHE.lock() {
        cache.overview.insert(time_range, CacheEntry::new(result.clone()));
    }
    Ok(result)
}

#[tauri::command]
pub async fn get_cursor_v2_usage_events(time_range: String, page: u32, page_size: u32, force_refresh: Option<bool>) -> Result<CursorV2UsageEventsPage, String> {
    let force = force_refresh.unwrap_or(false);
    let cache_key = format!("{}:{}:{}", time_range, page, page_size);
    if !force {
        if let Ok(cache) = CACHE.lock() {
            if let Some(entry) = cache.events.get(&cache_key) {
                if entry.is_valid(cache.ttl_seconds) {
                    return Ok(entry.data.clone());
                }
            }
        }
    }
    let tr = time_range.clone();
    let result = tokio::task::spawn_blocking(move || _usage_events(&tr, page, page_size))
        .await
        .map_err(|e| format!("Task failed: {}", e))??;
    if let Ok(mut cache) = CACHE.lock() {
        cache.events.insert(cache_key, CacheEntry::new(result.clone()));
    }
    Ok(result)
}

#[tauri::command]
pub async fn get_cursor_v2_ai_commits(time_range: String, force_refresh: Option<bool>) -> Result<AnalyticsTimeseriesResponse, String> {
    let force = force_refresh.unwrap_or(false);
    if !force {
        if let Ok(cache) = CACHE.lock() {
            if let Some(entry) = cache.ai_commits.get(&time_range) {
                if entry.is_valid(cache.ttl_seconds) {
                    return Ok(entry.data.clone());
                }
            }
        }
    }
    let tr = time_range.clone();
    let result = tokio::task::spawn_blocking(move || _ai_commits(&tr))
        .await
        .map_err(|e| format!("Task failed: {}", e))??;
    if let Ok(mut cache) = CACHE.lock() {
        cache.ai_commits.insert(time_range, CacheEntry::new(result.clone()));
    }
    Ok(result)
}

#[tauri::command]
pub async fn get_cursor_v2_model_usage(time_range: String, force_refresh: Option<bool>) -> Result<ModelUsageResponse, String> {
    let force = force_refresh.unwrap_or(false);
    if !force {
        if let Ok(cache) = CACHE.lock() {
            if let Some(entry) = cache.model_usage.get(&time_range) {
                if entry.is_valid(cache.ttl_seconds) {
                    return Ok(entry.data.clone());
                }
            }
        }
    }
    let tr = time_range.clone();
    let result = tokio::task::spawn_blocking(move || _model_usage(&tr))
        .await
        .map_err(|e| format!("Task failed: {}", e))??;
    if let Ok(mut cache) = CACHE.lock() {
        cache.model_usage.insert(time_range, CacheEntry::new(result.clone()));
    }
    Ok(result)
}

#[tauri::command]
pub async fn get_cursor_v2_composer_tabs(time_range: String, force_refresh: Option<bool>) -> Result<serde_json::Value, String> {
    let force = force_refresh.unwrap_or(false);
    if !force {
        if let Ok(cache) = CACHE.lock() {
            if let Some(entry) = cache.composer_tabs.get(&time_range) {
                if entry.is_valid(cache.ttl_seconds) {
                    return Ok(entry.data.clone());
                }
            }
        }
    }
    let tr = time_range.clone();
    let result = tokio::task::spawn_blocking(move || _composer_tabs(&tr))
        .await
        .map_err(|e| format!("Task failed: {}", e))??;
    if let Ok(mut cache) = CACHE.lock() {
        cache.composer_tabs.insert(time_range, CacheEntry::new(result.clone()));
    }
    Ok(result)
}

#[tauri::command]
pub async fn get_cursor_v2_team_spend(page: u32, page_size: u32, force_refresh: Option<bool>) -> Result<TeamSpendResponse, String> {
    let force = force_refresh.unwrap_or(false);
    let cache_key = format!("{}:{}", page, page_size);
    if !force {
        if let Ok(cache) = CACHE.lock() {
            if let Some(entry) = cache.team_spend.get(&cache_key) {
                if entry.is_valid(cache.ttl_seconds) {
                    return Ok(entry.data.clone());
                }
            }
        }
    }
    let result = tokio::task::spawn_blocking(move || _team_spend(page, page_size))
        .await
        .map_err(|e| format!("Task failed: {}", e))??;
    if let Ok(mut cache) = CACHE.lock() {
        cache.team_spend.insert(cache_key, CacheEntry::new(result.clone()));
    }
    Ok(result)
}

#[tauri::command]
pub async fn get_cursor_v2_request_breakdown(time_range: String, force_refresh: Option<bool>) -> Result<AnalyticsTimeseriesResponse, String> {
    let force = force_refresh.unwrap_or(false);
    if !force {
        if let Ok(cache) = CACHE.lock() {
            if let Some(entry) = cache.request_breakdown.get(&time_range) {
                if entry.is_valid(cache.ttl_seconds) {
                    return Ok(entry.data.clone());
                }
            }
        }
    }
    let tr = time_range.clone();
    let result = tokio::task::spawn_blocking(move || _request_breakdown(&tr))
        .await
        .map_err(|e| format!("Task failed: {}", e))??;
    if let Ok(mut cache) = CACHE.lock() {
        cache.request_breakdown.insert(time_range, CacheEntry::new(result.clone()));
    }
    Ok(result)
}

#[tauri::command]
pub async fn set_cursor_v2_cache_ttl(seconds: u64) -> Result<(), String> {
    let mut cache = CACHE.lock().map_err(|e| format!("Cache lock error: {}", e))?;
    cache.ttl_seconds = seconds;
    Ok(())
}

#[tauri::command]
pub fn get_cursor_v2_cache_info() -> Result<CursorV2CacheInfo, String> {
    let cache = CACHE.lock().map_err(|e| format!("Cache lock error: {}", e))?;
    // Find the most recent fetched_at_iso across all cache entries
    let mut last_refreshed: Option<String> = None;
    let mut latest_instant: Option<Instant> = None;
    for entry in cache.overview.values() {
        if latest_instant.is_none_or(|li| entry.fetched_at > li) {
            latest_instant = Some(entry.fetched_at);
            last_refreshed = Some(entry.fetched_at_iso.clone());
        }
    }
    for entry in cache.events.values() {
        if latest_instant.is_none_or(|li| entry.fetched_at > li) {
            latest_instant = Some(entry.fetched_at);
            last_refreshed = Some(entry.fetched_at_iso.clone());
        }
    }
    for entry in cache.ai_commits.values() {
        if latest_instant.is_none_or(|li| entry.fetched_at > li) {
            latest_instant = Some(entry.fetched_at);
            last_refreshed = Some(entry.fetched_at_iso.clone());
        }
    }
    for entry in cache.model_usage.values() {
        if latest_instant.is_none_or(|li| entry.fetched_at > li) {
            latest_instant = Some(entry.fetched_at);
            last_refreshed = Some(entry.fetched_at_iso.clone());
        }
    }
    let entries_count = (cache.overview.len() + cache.events.len() + cache.ai_commits.len()
        + cache.model_usage.len() + cache.composer_tabs.len()
        + cache.team_spend.len() + cache.request_breakdown.len()) as u32;
    Ok(CursorV2CacheInfo {
        last_refreshed,
        ttl_seconds: cache.ttl_seconds,
        entries_count,
    })
}

#[tauri::command]
pub fn disconnect_cursor_v2() -> Result<(), String> {
    // Clear cache on disconnect
    if let Ok(mut cache) = CACHE.lock() {
        cache.overview.clear();
        cache.events.clear();
        cache.ai_commits.clear();
        cache.model_usage.clear();
        cache.composer_tabs.clear();
        cache.team_spend.clear();
        cache.request_breakdown.clear();
    }
    crate::analytics::token_store::delete_provider_token("cursor", "session-token")
}
