//! Model routing recommendations: classify recent Claude prompts by complexity and
//! estimate $ saved if cheaper-model-suitable prompts had been routed to Haiku/Sonnet.
//!
//! Walks ~/.claude/projects/**/*.jsonl directly so we can pair each assistant message
//! with the immediately-preceding user prompt text (used as the row preview in the UI).

use crate::analytics::cost_engine::{estimate_cost, TokensForCost};
use crate::commands::usage::decode_claude_project_path;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
use walkdir::WalkDir;

const HAIKU_MODEL: &str = "claude-haiku-4-5";
const SONNET_MODEL: &str = "claude-sonnet-4-5";
const PROMPT_PREVIEW_MAX_CHARS: usize = 220;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRoutingMessage {
    pub timestamp: String,
    pub model: Option<String>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub has_thinking: bool,
    pub tool_count: u32,
    pub project: Option<String>,
    pub current_cost: f64,
    pub haiku_cost: f64,
    pub sonnet_cost: f64,
    pub prompt_preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRoutingAnalysis {
    pub period_days: u32,
    pub messages: Vec<ModelRoutingMessage>,
    pub total_current_cost: f64,
    pub total_messages: u64,
    pub generated_at: String,
}

fn is_haiku(model: Option<&str>) -> bool {
    model.map(|m| m.to_ascii_lowercase().contains("haiku")).unwrap_or(false)
}

/// Pull readable text out of a `message.content` field that may be either a plain string
/// or an array of content blocks ({type: "text", text: "..."}, etc).
fn extract_user_text(message: &Value) -> Option<String> {
    let content = message.get("content")?;
    if let Some(s) = content.as_str() {
        return Some(s.to_string());
    }
    if let Some(arr) = content.as_array() {
        let mut buf = String::new();
        for item in arr {
            let ty = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
            match ty {
                "text" => {
                    if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                        if !buf.is_empty() {
                            buf.push(' ');
                        }
                        buf.push_str(t);
                    }
                }
                "tool_result" => {
                    // Skip tool_result echoes — they're not user prompts, they're tool outputs
                    // being fed back to the model. Showing them as the "prompt" is misleading.
                    continue;
                }
                _ => {}
            }
        }
        if !buf.is_empty() {
            return Some(buf);
        }
    }
    None
}

fn truncate_preview(s: &str) -> String {
    let cleaned: String = s.chars().map(|c| if c == '\n' || c == '\r' { ' ' } else { c }).collect();
    let trimmed = cleaned.trim();
    if trimmed.chars().count() <= PROMPT_PREVIEW_MAX_CHARS {
        return trimmed.to_string();
    }
    let truncated: String = trimmed.chars().take(PROMPT_PREVIEW_MAX_CHARS).collect();
    format!("{}…", truncated)
}

fn extract_tools_from_msg(message: &Value) -> Vec<String> {
    let content = message.get("content").and_then(|c| c.as_array());
    let mut out = Vec::new();
    if let Some(items) = content {
        for item in items {
            if item.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                    out.push(name.to_string());
                }
            }
        }
    }
    out
}

fn has_thinking_in_msg(message: &Value) -> bool {
    let content = message.get("content").and_then(|c| c.as_array());
    if let Some(items) = content {
        for item in items {
            if item.get("type").and_then(|t| t.as_str()) == Some("thinking") {
                return true;
            }
        }
    }
    false
}

fn parse_tokens(usage: &Value) -> (u64, u64, u64, u64) {
    let input = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
    let output = usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
    let cache_read = usage.get("cache_read_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
    let cache_write = usage.get("cache_creation_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
    (input, output, cache_read, cache_write)
}

struct FileWalkCtx<'a> {
    cutoff: DateTime<Utc>,
    project: Option<String>,
    out: &'a mut Vec<ModelRoutingMessage>,
}

fn walk_jsonl_file(path: &Path, ctx: &mut FileWalkCtx<'_>) -> std::io::Result<()> {
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);

    let mut last_user_text: Option<String> = None;
    let mut seen_keys: HashSet<String> = HashSet::new();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let json: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let ty = json.get("type").and_then(|v| v.as_str()).unwrap_or("");

        if ty == "user" {
            // Capture latest user prompt text so the next assistant message can claim it.
            if let Some(msg) = json.get("message") {
                if let Some(text) = extract_user_text(msg) {
                    last_user_text = Some(truncate_preview(&text));
                }
            }
            continue;
        }

        if ty != "assistant" {
            continue;
        }

        // Dedup streaming chunks (Claude emits multiple lines per message with cumulative tokens).
        let message_id = json.get("message").and_then(|m| m.get("id")).and_then(|v| v.as_str());
        let request_id = json.get("requestId").and_then(|v| v.as_str());
        if let (Some(mid), Some(rid)) = (message_id, request_id) {
            let key = format!("{}:{}", mid, rid);
            if seen_keys.contains(&key) {
                continue;
            }
            seen_keys.insert(key);
        }

        let timestamp = json.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
        if timestamp.is_empty() {
            continue;
        }
        let ts = match DateTime::parse_from_rfc3339(timestamp) {
            Ok(t) => t.with_timezone(&Utc),
            Err(_) => continue,
        };
        if ts < ctx.cutoff {
            continue;
        }

        let message = match json.get("message") {
            Some(m) => m,
            None => continue,
        };

        let usage = match message.get("usage") {
            Some(u) => u,
            None => continue,
        };
        let (input, output, cache_read, cache_write) = parse_tokens(usage);
        if input == 0 && output == 0 && cache_read == 0 && cache_write == 0 {
            continue;
        }

        let model = message
            .get("model")
            .and_then(|v| v.as_str())
            .or_else(|| json.get("model").and_then(|v| v.as_str()))
            .map(String::from);

        if is_haiku(model.as_deref()) {
            // Already on cheapest model — no savings to find. Skip but keep the prompt
            // around for subsequent non-Haiku turns in the same conversation.
            continue;
        }

        let tools = extract_tools_from_msg(message);
        let has_thinking = has_thinking_in_msg(message);

        let tokens = TokensForCost { input, output, cache_read, cache_write };
        let current_cost = estimate_cost(model.as_deref(), &tokens);
        let haiku_cost = estimate_cost(Some(HAIKU_MODEL), &tokens);
        let sonnet_cost = estimate_cost(Some(SONNET_MODEL), &tokens);

        ctx.out.push(ModelRoutingMessage {
            timestamp: timestamp.to_string(),
            model,
            input_tokens: input,
            output_tokens: output,
            cache_read,
            cache_write,
            has_thinking,
            tool_count: tools.len() as u32,
            project: ctx.project.clone(),
            current_cost,
            haiku_cost,
            sonnet_cost,
            prompt_preview: last_user_text.clone(),
        });
    }

    Ok(())
}

#[tauri::command]
pub async fn analyze_model_routing(days: Option<u32>) -> Result<ModelRoutingAnalysis, String> {
    let period_days = days.unwrap_or(30);

    tauri::async_runtime::spawn_blocking(move || {
        let home = dirs::home_dir().ok_or_else(|| "Could not determine home directory".to_string())?;
        let projects_dir = home.join(".claude").join("projects");

        let cutoff = Utc::now() - Duration::days(period_days as i64);
        let mut messages: Vec<ModelRoutingMessage> = Vec::new();
        let mut total_current_cost = 0.0_f64;

        if projects_dir.exists() {
            for entry in WalkDir::new(&projects_dir).follow_links(false).into_iter().filter_map(|e| e.ok()) {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }
                let project = path
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .map(decode_claude_project_path)
                    .filter(|s| !s.is_empty());

                let mut ctx = FileWalkCtx { cutoff, project, out: &mut messages };
                let _ = walk_jsonl_file(path, &mut ctx);
            }
        }

        for m in &messages {
            total_current_cost += m.current_cost;
        }

        messages.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        Ok(ModelRoutingAnalysis {
            period_days,
            total_messages: messages.len() as u64,
            messages,
            total_current_cost,
            generated_at: Utc::now().to_rfc3339(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
