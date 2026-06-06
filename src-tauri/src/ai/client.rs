use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

use crate::utils::keychain;

// Match the canonical casing used elsewhere (debate worker, debate page UI,
// existing Secrets manager) so users don't end up with two keychain entries.
pub const ANTHROPIC_KEY_NAME: &str = "ANTHROPIC_API_KEY";
const ANTHROPIC_MESSAGES_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_MODEL: &str = "claude-haiku-4-5-20251001";
const DEFAULT_MAX_TOKENS: u32 = 2048;
const TIMEOUT_SECS: u64 = 45;

#[derive(Serialize)]
struct AnthropicMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct AnthropicRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    messages: Vec<AnthropicMessage<'a>>,
}

#[derive(Deserialize)]
struct AnthropicContent {
    #[serde(rename = "type")]
    content_type: String,
    #[serde(default)]
    text: String,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
}

pub fn has_api_key() -> bool {
    matches!(
        keychain::get_secret(ANTHROPIC_KEY_NAME),
        Ok(Some(ref v)) if !v.trim().is_empty()
    )
}

pub fn complete(system_prompt: &str, user_prompt: &str) -> Result<String, String> {
    let api_key = keychain::get_secret(ANTHROPIC_KEY_NAME)?
        .ok_or_else(|| "Anthropic API key not set. Add it in Settings.".to_string())?;

    let mut headers = HeaderMap::new();
    headers.insert(
        "x-api-key",
        HeaderValue::from_str(&api_key)
            .map_err(|e| format!("Invalid API key format: {}", e))?,
    );
    headers.insert(
        "anthropic-version",
        HeaderValue::from_static(ANTHROPIC_VERSION),
    );
    headers.insert(
        "content-type",
        HeaderValue::from_static("application/json"),
    );

    let body = AnthropicRequest {
        model: DEFAULT_MODEL,
        max_tokens: DEFAULT_MAX_TOKENS,
        system: system_prompt,
        messages: vec![AnthropicMessage {
            role: "user",
            content: user_prompt,
        }],
    };

    let client = Client::builder()
        .timeout(Duration::from_secs(TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("HTTP client build failed: {}", e))?;

    let resp = client
        .post(ANTHROPIC_MESSAGES_URL)
        .headers(headers)
        .json(&body)
        .send()
        .map_err(|e| format!("Anthropic API request failed: {}", e))?;

    let status = resp.status();
    if !status.is_success() {
        let body_text = resp.text().unwrap_or_default();
        return Err(humanize_api_error(status.as_u16(), &body_text));
    }

    let parsed: AnthropicResponse = resp
        .json()
        .map_err(|e| format!("Failed to parse Anthropic response: {}", e))?;

    let text = parsed
        .content
        .into_iter()
        .find(|c| c.content_type == "text")
        .map(|c| c.text)
        .ok_or_else(|| "Anthropic response had no text content".to_string())?;

    Ok(text)
}

/// Turn a raw Anthropic HTTP error into a short, human-readable message.
/// The raw body (e.g. `{"type":"error","error":{"type":"authentication_error",
/// "message":"invalid x-api-key"}}`) is useless to end users.
fn humanize_api_error(status: u16, body: &str) -> String {
    // Best-effort: pull the structured error type/message if present.
    let err_type = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.get("type")).and_then(|t| t.as_str()).map(String::from));

    match status {
        401 | 403 => {
            "Your ANTHROPIC_API_KEY is invalid or unauthorized. Open the Secrets Manager and update it (get a key at console.anthropic.com).".to_string()
        }
        429 => {
            "Anthropic is rate-limiting requests right now. Wait a moment and try again.".to_string()
        }
        529 => {
            "Anthropic's API is temporarily overloaded. Try again in a bit.".to_string()
        }
        500..=599 => {
            "Anthropic's API is having trouble right now. Try again shortly.".to_string()
        }
        400 if err_type.as_deref() == Some("invalid_request_error") => {
            "Anthropic rejected the request as invalid. If this persists, please report it.".to_string()
        }
        _ => format!(
            "Anthropic API error ({}). Check your ANTHROPIC_API_KEY in the Secrets Manager and try again.",
            status
        ),
    }
}

/// Extract the first JSON array from a string. The model sometimes wraps its
/// JSON output in prose or ``` fences; pull the array out so we can parse it.
pub fn extract_json_array(text: &str) -> Option<&str> {
    let start = text.find('[')?;
    let mut depth = 0i32;
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        match b {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

#[allow(dead_code)]
pub fn parse_json_array<T: for<'de> Deserialize<'de>>(text: &str) -> Result<Vec<T>, String> {
    let slice = extract_json_array(text)
        .ok_or_else(|| "No JSON array found in model response".to_string())?;
    serde_json::from_str::<Vec<T>>(slice)
        .map_err(|e| format!("Failed to parse JSON array: {} (raw: {})", e, slice))
}

#[allow(dead_code)]
pub fn parse_json_value(text: &str) -> Result<Value, String> {
    let slice = extract_json_array(text)
        .ok_or_else(|| "No JSON array found in model response".to_string())?;
    serde_json::from_str::<Value>(slice)
        .map_err(|e| format!("Failed to parse JSON: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_array_plain() {
        let s = "[1, 2, 3]";
        assert_eq!(extract_json_array(s), Some("[1, 2, 3]"));
    }

    #[test]
    fn test_extract_json_array_with_prose() {
        let s = "Here is the result:\n```json\n[{\"a\": 1}]\n```\nDone.";
        assert_eq!(extract_json_array(s), Some("[{\"a\": 1}]"));
    }

    #[test]
    fn test_extract_json_array_nested() {
        let s = "prefix [[1,2],[3,4]] suffix";
        assert_eq!(extract_json_array(s), Some("[[1,2],[3,4]]"));
    }

    #[test]
    fn test_extract_json_array_none() {
        assert_eq!(extract_json_array("no brackets here"), None);
        assert_eq!(extract_json_array("only [ open"), None);
    }

    #[test]
    fn test_humanize_api_error_401() {
        let raw = r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#;
        let msg = humanize_api_error(401, raw);
        assert!(msg.contains("invalid or unauthorized"));
        assert!(!msg.contains("x-api-key")); // no raw body leaked
    }

    #[test]
    fn test_humanize_api_error_429_and_5xx() {
        assert!(humanize_api_error(429, "").contains("rate-limiting"));
        assert!(humanize_api_error(503, "").contains("trouble"));
    }
}
