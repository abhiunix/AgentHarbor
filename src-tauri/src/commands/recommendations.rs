use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::adapters::AdapterRegistry;
use crate::ai::{client, context};
use crate::models::UniversalCapability;
use crate::registry::{
    get_bundled_registry_path, get_community_registry_path, load_capabilities,
};

const CACHE_TTL_SECS: i64 = 30 * 60;

fn cache_path() -> PathBuf {
    crate::utils::paths::app_data_dir().join("recommendations-cache.json")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    pub id: String,
    /// "deploy" | "remove" | "info"
    pub action: String,
    pub capability_id: Option<String>,
    pub capability_name: Option<String>,
    pub capability_type: Option<String>,
    pub target_adapter_id: Option<String>,
    pub target_adapter_name: Option<String>,
    pub reason: String,
    /// "high" | "medium" | "low"
    pub priority: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendationsPayload {
    pub recommendations: Vec<Recommendation>,
    pub generated_at: String,
    pub from_cache: bool,
    /// Brief plain-language summary of why these were chosen, shown above the list.
    pub summary: String,
}

#[derive(Debug, Deserialize)]
struct ModelRecommendation {
    #[serde(default)]
    action: String,
    #[serde(default)]
    capability_id: Option<String>,
    #[serde(default)]
    target_adapter_id: Option<String>,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    priority: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModelOutput {
    #[serde(default)]
    summary: String,
    #[serde(default)]
    recommendations: Vec<ModelRecommendation>,
}

const SYSTEM_PROMPT: &str = r#"You are an expert AI tooling advisor for "AgentHarbor", a control plane for developers who use multiple AI coding tools (Claude Code, Cursor, Codex, Gemini CLI, Windsurf, GitHub Copilot, Antigravity, VS Code).

Given a JSON snapshot of which AI tools the user has installed, which projects they have, which capabilities (MCP servers, rules, skills, hooks) are already deployed, and which capabilities are available in the registry, recommend 3-6 high-leverage actions that will measurably improve the user's setup.

Rules:
- Only recommend capability IDs that exist in `available_capabilities`. Never invent IDs.
- Only target adapters that exist in `adapters` and have `detected_anywhere: true`. Skip recommendations otherwise.
- Each recommendation must include a concrete `reason` (1-2 sentences) that references a real signal from the input (a detected adapter, a missing-but-popular tag, a tool the user is using heavily, etc.). Avoid generic statements.
- Prefer actions that span tools (e.g. "deploy across detected adapters") when compatible.
- Prioritize: cost savings > security > productivity > nice-to-have.

Return ONLY a JSON object with this exact shape, no prose, no markdown:

{
  "summary": "One-sentence overview of why these picks fit this user.",
  "recommendations": [
    {
      "action": "deploy",
      "capability_id": "community/<id-from-available_capabilities>",
      "target_adapter_id": "<adapter id from `adapters`>",
      "reason": "...",
      "priority": "high"
    }
  ]
}
"#;

fn registry_lookup() -> std::collections::HashMap<String, UniversalCapability> {
    let mut dirs = vec![get_bundled_registry_path()];
    let community = get_community_registry_path();
    if community.exists() {
        dirs.push(community);
    }
    let custom = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.agentharbor.app")
        .join("registry")
        .join("custom");
    if custom.exists() {
        dirs.push(custom);
    }

    let load = load_capabilities(&dirs);
    load.items
        .into_iter()
        .map(|c| (c.id().to_string(), c))
        .collect()
}

fn validate_recommendations(model_out: ModelOutput) -> RecommendationsPayload {
    let registry = registry_lookup();
    let adapters = AdapterRegistry::new();
    let known_adapter_ids: HashSet<String> =
        adapters.all().iter().map(|a| a.id().to_string()).collect();

    let mut out: Vec<Recommendation> = Vec::new();
    for (i, r) in model_out.recommendations.into_iter().enumerate() {
        if r.action.is_empty() || r.reason.is_empty() {
            continue;
        }

        let (cap_name, cap_type) = match &r.capability_id {
            Some(id) => match registry.get(id) {
                Some(cap) => {
                    let t = match cap {
                        UniversalCapability::Mcp(_) => "mcp",
                        UniversalCapability::Rule(_) => "rule",
                        UniversalCapability::Skill(_) => "skill",
                        UniversalCapability::Hook(_) => "hook",
                        UniversalCapability::Plugin(_) => "plugin",
                        UniversalCapability::Custom(_) => "custom",
                    };
                    (Some(cap.name().to_string()), Some(t.to_string()))
                }
                None => {
                    // Hallucinated capability — skip.
                    continue;
                }
            },
            None => (None, None),
        };

        let target_adapter_name = r
            .target_adapter_id
            .as_ref()
            .and_then(|id| {
                if known_adapter_ids.contains(id) {
                    adapters.get(id).map(|a| a.name().to_string())
                } else {
                    None
                }
            });

        if r.target_adapter_id.is_some() && target_adapter_name.is_none() {
            continue;
        }

        out.push(Recommendation {
            id: format!("rec-{}", i + 1),
            action: r.action,
            capability_id: r.capability_id,
            capability_name: cap_name,
            capability_type: cap_type,
            target_adapter_id: r.target_adapter_id,
            target_adapter_name,
            reason: r.reason,
            priority: r.priority.unwrap_or_else(|| "medium".to_string()),
        });
    }

    RecommendationsPayload {
        recommendations: out,
        generated_at: Utc::now().to_rfc3339(),
        from_cache: false,
        summary: model_out.summary,
    }
}

fn read_cache() -> Option<RecommendationsPayload> {
    let path = cache_path();
    if !path.exists() {
        return None;
    }
    let content = fs::read_to_string(&path).ok()?;
    let mut payload: RecommendationsPayload = serde_json::from_str(&content).ok()?;
    let generated = chrono::DateTime::parse_from_rfc3339(&payload.generated_at).ok()?;
    let age_secs = Utc::now().signed_duration_since(generated).num_seconds();
    if age_secs > CACHE_TTL_SECS {
        return None;
    }
    payload.from_cache = true;
    Some(payload)
}

fn write_cache(payload: &RecommendationsPayload) -> Result<(), String> {
    let path = cache_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(payload).map_err(|e| e.to_string())?;
    crate::utils::paths::atomic_write_str(&path, &json)
}

fn generate_now() -> Result<RecommendationsPayload, String> {
    if !client::has_api_key() {
        return Err(
            "Anthropic API key not set. Open Settings → Secrets and add a key with name 'anthropic_api_key'."
                .to_string(),
        );
    }

    let ctx = context::build_context();
    let ctx_json = serde_json::to_string(&ctx)
        .map_err(|e| format!("Failed to serialize context: {}", e))?;

    let user_prompt = format!(
        "Here is the user's AgentHarbor snapshot as JSON. Recommend actions. Respond with the JSON object only.\n\nSNAPSHOT:\n{}",
        ctx_json
    );

    let response = client::complete(SYSTEM_PROMPT, &user_prompt)?;

    let json_slice = extract_json_object(&response)
        .ok_or_else(|| format!("Model response was not valid JSON. Raw: {}", response))?;

    let model_out: ModelOutput = serde_json::from_str(json_slice)
        .map_err(|e| format!("Failed to parse model JSON: {} (raw: {})", e, json_slice))?;

    let payload = validate_recommendations(model_out);
    let _ = write_cache(&payload);
    Ok(payload)
}

fn extract_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let mut depth = 0i32;
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        match b {
            b'{' => depth += 1,
            b'}' => {
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

#[tauri::command]
pub fn get_recommendations(force_refresh: bool) -> Result<RecommendationsPayload, String> {
    if !force_refresh {
        if let Some(cached) = read_cache() {
            return Ok(cached);
        }
    }
    generate_now()
}

#[tauri::command]
pub fn get_cached_recommendations_count() -> u32 {
    read_cache()
        .map(|p| p.recommendations.len() as u32)
        .unwrap_or(0)
}

#[tauri::command]
pub fn has_anthropic_api_key() -> bool {
    client::has_api_key()
}

#[tauri::command]
pub fn clear_recommendations_cache() -> Result<(), String> {
    let path = cache_path();
    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_object_plain() {
        let s = r#"{"a": 1}"#;
        assert_eq!(extract_json_object(s), Some(r#"{"a": 1}"#));
    }

    #[test]
    fn test_extract_json_object_with_prose() {
        let s = "Here is JSON:\n```\n{\"x\": [1,2], \"y\": {}}\n```";
        assert_eq!(extract_json_object(s), Some(r#"{"x": [1,2], "y": {}}"#));
    }

    #[test]
    fn test_extract_json_object_none() {
        assert_eq!(extract_json_object("no braces"), None);
    }
}
