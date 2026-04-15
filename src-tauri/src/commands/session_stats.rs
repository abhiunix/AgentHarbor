use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LongestSession {
    #[serde(alias = "sessionId", default)]
    pub session_id: String,
    #[serde(default)]
    pub duration: u64,
    #[serde(alias = "messageCount", default)]
    pub message_count: u64,
    #[serde(default)]
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DailyActivity {
    #[serde(default)]
    pub date: String,
    #[serde(alias = "messageCount", default)]
    pub message_count: u64,
    #[serde(alias = "sessionCount", default)]
    pub session_count: u64,
    #[serde(alias = "toolCallCount", default)]
    pub tool_call_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelUsageEntry {
    #[serde(alias = "inputTokens", default)]
    pub input_tokens: u64,
    #[serde(alias = "outputTokens", default)]
    pub output_tokens: u64,
    #[serde(alias = "cacheReadInputTokens", default)]
    pub cache_read_input_tokens: u64,
    #[serde(alias = "cacheCreationInputTokens", default)]
    pub cache_creation_input_tokens: u64,
    #[serde(alias = "costUSD", default)]
    pub cost_usd: f64,
    #[serde(alias = "contextWindow", default)]
    pub context_window: u64,
    #[serde(alias = "maxOutputTokens", default)]
    pub max_output_tokens: u64,
    #[serde(alias = "webSearchRequests", default)]
    pub web_search_requests: u64,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawStatsCache {
    #[serde(default)]
    version: u32,
    #[serde(alias = "totalSessions", default)]
    total_sessions: u64,
    #[serde(alias = "totalMessages", default)]
    total_messages: u64,
    #[serde(alias = "longestSession")]
    longest_session: Option<LongestSession>,
    #[serde(alias = "hourCounts", default)]
    hour_counts: HashMap<String, u64>,
    #[serde(alias = "dailyActivity", default)]
    daily_activity: Vec<DailyActivity>,
    #[serde(alias = "modelUsage", default)]
    model_usage: HashMap<String, ModelUsageEntry>,
    #[serde(alias = "firstSessionDate", default)]
    first_session_date: Option<String>,
    #[serde(alias = "lastComputedDate", default)]
    last_computed_date: Option<String>,
    #[serde(alias = "totalSpeculationTimeSavedMs", default)]
    total_speculation_time_saved_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStats {
    pub total_sessions: u64,
    pub total_messages: u64,
    pub longest_session: Option<LongestSession>,
    pub hour_counts: Vec<u64>,
    pub daily_activity: Vec<DailyActivity>,
    pub model_usage: HashMap<String, ModelUsageEntry>,
    pub first_session_date: Option<String>,
    pub total_cost_usd: f64,
}

#[tauri::command]
pub fn get_claude_session_stats() -> Result<SessionStats, String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let stats_path = home.join(".claude").join("stats-cache.json");

    if !stats_path.exists() {
        return Ok(SessionStats {
            total_sessions: 0,
            total_messages: 0,
            longest_session: None,
            hour_counts: vec![0; 24],
            daily_activity: vec![],
            model_usage: HashMap::new(),
            first_session_date: None,
            total_cost_usd: 0.0,
        });
    }

    let content = std::fs::read_to_string(&stats_path).map_err(|e| e.to_string())?;
    let raw: RawStatsCache = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse stats-cache.json: {}", e))?;

    let mut hour_counts = vec![0u64; 24];
    for (key, value) in &raw.hour_counts {
        if let Ok(hour) = key.parse::<usize>() {
            if hour < 24 {
                hour_counts[hour] = *value;
            }
        }
    }

    let total_cost_usd: f64 = raw.model_usage.values().map(|m| m.cost_usd).sum();

    Ok(SessionStats {
        total_sessions: raw.total_sessions,
        total_messages: raw.total_messages,
        longest_session: raw.longest_session,
        hour_counts,
        daily_activity: raw.daily_activity,
        model_usage: raw.model_usage,
        first_session_date: raw.first_session_date,
        total_cost_usd,
    })
}
