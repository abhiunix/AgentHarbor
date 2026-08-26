//! DeepSeek Harness "Permissions & Control" — model/reasoning-effort
//! switching sourced from `~/.dsh/settings.yaml`'s `agent-default-model`
//! section, plus a read-only per-session policy view sourced from
//! `session_projcache.json`'s `permissions` row (dsh's `permission/preset` /
//! `sandbox/mode` / `approval/policy` session-log events, already folded into
//! that cache). Reuses `deepseek_v2`'s `dsh_root()`, `load_session_metadata()`,
//! `discover_dsh_sessions()`, `decode_session_events()`, and
//! `read_session_permissions()`.
//!
//! Unlike Kimi's `config.toml` (line-targeted edits to dodge the `[providers]`
//! API-key block), `settings.yaml` has no secrets, so writes here are a full
//! `serde_yaml::Value` round-trip that only ever touches the
//! `agent-default-model.model` / `agent-default-model.reasoningEffort` keys —
//! every other key and section is preserved.

use crate::analytics::deepseek_v2::{
    decode_session_events, discover_dsh_sessions, dsh_root, load_session_metadata,
    read_session_permissions, DshSessionMeta, DshSessionPermissions,
};
use crate::utils::paths::atomic_write_str;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Reasoning-effort levels the `llm-deepseek` provider adapter accepts
/// (`Config.reasoningEffort` in `packages/llm/llm-deepseek/src/index.ts`).
/// There is no "medium" tier — just off/low/high/max.
const REASONING_OPTIONS: [&str; 4] = ["off", "low", "high", "max"];

/// Catalog models `llm-deepseek` ships by default, shown even before any
/// session has recorded a `request/context` event.
const DEFAULT_MODEL_OPTIONS: [&str; 3] =
    ["deepseek-v4-flash", "deepseek-v4-pro", "deepseek-v4-flash-vision-exp"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekSessionPolicy {
    pub session_id: String,
    pub workspace_name: String,
    pub permission_preset: Option<String>,
    pub sandbox_mode: Option<String>,
    pub approval_policy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekControlSettings {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub model_options: Vec<String>,
    pub reasoning_options: Vec<String>,
    pub sessions_policies: Vec<DeepSeekSessionPolicy>,
    pub other_settings: Vec<String>,
}

// ── settings.yaml path + read ────────────────────────────────────────────────

fn settings_yaml_path(root: &std::path::Path) -> std::path::PathBuf {
    root.join("settings.yaml")
}

fn read_settings_yaml_text(root: &std::path::Path) -> String {
    std::fs::read_to_string(settings_yaml_path(root)).unwrap_or_default()
}

fn parse_settings_yaml(content: &str) -> serde_yaml::Value {
    if content.trim().is_empty() {
        return serde_yaml::Value::Mapping(serde_yaml::Mapping::new());
    }
    serde_yaml::from_str(content).unwrap_or(serde_yaml::Value::Mapping(serde_yaml::Mapping::new()))
}

fn agent_default_model_field(doc: &serde_yaml::Value, field: &str) -> Option<String> {
    doc.get("agent-default-model")?.get(field)?.as_str().map(String::from)
}

/// Top-level `settings.yaml` keys other than `agent-default-model`, for the
/// page's read-only "other settings" list.
fn other_top_level_keys(doc: &serde_yaml::Value) -> Vec<String> {
    let Some(map) = doc.as_mapping() else { return vec![] };
    map.keys()
        .filter_map(|k| k.as_str())
        .filter(|k| *k != "agent-default-model")
        .map(String::from)
        .collect()
}

// ── model discovery from session logs ────────────────────────────────────────

/// Models actually used, read from every session's `request/context` events
/// (`data.model`).
fn models_seen_in_sessions(root: &std::path::Path) -> HashSet<String> {
    let mut seen = HashSet::new();
    for session in discover_dsh_sessions(root) {
        let Some(events) = decode_session_events(&session.log_path) else { continue };
        for event in &events {
            if event.get("type").and_then(|t| t.as_str()) != Some("request/context") {
                continue;
            }
            if let Some(model) = event.get("data").and_then(|d| d.get("model")).and_then(|m| m.as_str()) {
                seen.insert(model.to_string());
            }
        }
    }
    seen
}

/// Union of {models seen in session logs} ∪ {known defaults} ∪ {current
/// setting}, sorted for a stable picker order.
fn build_model_options(seen: HashSet<String>, current: Option<&str>) -> Vec<String> {
    let mut options = seen;
    for default in DEFAULT_MODEL_OPTIONS {
        options.insert(default.to_string());
    }
    if let Some(cur) = current {
        options.insert(cur.to_string());
    }
    let mut options: Vec<String> = options.into_iter().collect();
    options.sort();
    options
}

/// The harness's fixed effort enum, plus the current setting if it's somehow
/// outside that enum (an older/newer harness) — so the picker never hides the
/// active value.
fn build_reasoning_options(current: Option<&str>) -> Vec<String> {
    let mut options: Vec<String> = REASONING_OPTIONS.iter().map(|s| s.to_string()).collect();
    if let Some(cur) = current {
        if !options.iter().any(|o| o == cur) {
            options.push(cur.to_string());
        }
    }
    options
}

// ── per-session policies ──────────────────────────────────────────────────────

fn build_sessions_policies_from_parts(
    metadata: &HashMap<String, DshSessionMeta>,
    permissions: HashMap<String, DshSessionPermissions>,
) -> Vec<DeepSeekSessionPolicy> {
    let mut policies: Vec<DeepSeekSessionPolicy> = permissions
        .into_iter()
        .map(|(session_id, perm)| {
            let workspace_name = metadata
                .get(&session_id)
                .map(|m| m.workspace_name.clone())
                .unwrap_or_else(|| session_id.clone());
            DeepSeekSessionPolicy {
                session_id,
                workspace_name,
                permission_preset: perm.preset,
                sandbox_mode: perm.sandbox,
                approval_policy: perm.approval,
            }
        })
        .collect();
    policies.sort_by(|a, b| a.session_id.cmp(&b.session_id));
    policies
}

fn build_sessions_policies(root: &std::path::Path) -> Vec<DeepSeekSessionPolicy> {
    let metadata = load_session_metadata(root);
    let permissions = read_session_permissions(root);
    build_sessions_policies_from_parts(&metadata, permissions)
}

// ── settings.yaml writes — targeted nested-key mutation ──────────────────────

fn set_agent_default_model_field(doc: &mut serde_yaml::Value, field: &str, value: String) {
    if !doc.is_mapping() {
        *doc = serde_yaml::Value::Mapping(serde_yaml::Mapping::new());
    }
    let map = doc.as_mapping_mut().expect("just ensured mapping above");
    let section_key = serde_yaml::Value::String("agent-default-model".to_string());
    if !matches!(map.get(&section_key), Some(serde_yaml::Value::Mapping(_))) {
        map.insert(section_key.clone(), serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
    }
    let Some(serde_yaml::Value::Mapping(model_map)) = map.get_mut(&section_key) else {
        unreachable!("just ensured a mapping at agent-default-model above")
    };
    model_map.insert(serde_yaml::Value::String(field.to_string()), serde_yaml::Value::String(value));
}

/// Parse `content` (or start from an empty document), set exactly
/// `agent-default-model.<field>`, and re-serialize — every other key and
/// section round-trips through `serde_yaml::Value` unmodified.
fn apply_agent_default_model_field(content: &str, field: &str, value: &str) -> Result<String, String> {
    let mut doc: serde_yaml::Value = if content.trim().is_empty() {
        serde_yaml::Value::Mapping(serde_yaml::Mapping::new())
    } else {
        serde_yaml::from_str(content).map_err(|e| format!("Failed to parse settings.yaml: {e}"))?
    };
    set_agent_default_model_field(&mut doc, field, value.to_string());
    serde_yaml::to_string(&doc).map_err(|e| format!("Failed to serialize settings.yaml: {e}"))
}

fn validate_reasoning_effort(effort: &str) -> Result<(), String> {
    if REASONING_OPTIONS.contains(&effort) {
        Ok(())
    } else {
        Err(format!("Unknown reasoning effort: {effort}"))
    }
}

// ── build ──────────────────────────────────────────────────────────────────

fn build_control_settings() -> DeepSeekControlSettings {
    let Some(root) = dsh_root() else {
        return DeepSeekControlSettings {
            provider: None,
            model: None,
            reasoning_effort: None,
            model_options: build_model_options(HashSet::new(), None),
            reasoning_options: build_reasoning_options(None),
            sessions_policies: vec![],
            other_settings: vec![],
        };
    };

    let doc = parse_settings_yaml(&read_settings_yaml_text(&root));
    let provider = agent_default_model_field(&doc, "provider");
    let model = agent_default_model_field(&doc, "model");
    let reasoning_effort = agent_default_model_field(&doc, "reasoningEffort");

    DeepSeekControlSettings {
        model_options: build_model_options(models_seen_in_sessions(&root), model.as_deref()),
        reasoning_options: build_reasoning_options(reasoning_effort.as_deref()),
        sessions_policies: build_sessions_policies(&root),
        other_settings: other_top_level_keys(&doc),
        provider,
        model,
        reasoning_effort,
    }
}

// ── Tauri commands ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_deepseek_control_settings() -> Result<DeepSeekControlSettings, String> {
    Ok(build_control_settings())
}

#[tauri::command]
pub fn set_deepseek_default_model(model: String) -> Result<(), String> {
    if model.trim().is_empty() {
        return Err("Model must not be empty".to_string());
    }
    let root = dsh_root().ok_or_else(|| "DeepSeek Harness home directory not found".to_string())?;
    let path = settings_yaml_path(&root);
    let content = read_settings_yaml_text(&root);
    let new_content = apply_agent_default_model_field(&content, "model", &model)?;
    atomic_write_str(&path, &new_content)
}

#[tauri::command]
pub fn set_deepseek_reasoning_effort(effort: String) -> Result<(), String> {
    validate_reasoning_effort(&effort)?;
    let root = dsh_root().ok_or_else(|| "DeepSeek Harness home directory not found".to_string())?;
    let path = settings_yaml_path(&root);
    let content = read_settings_yaml_text(&root);
    let new_content = apply_agent_default_model_field(&content, "reasoningEffort", &effort)?;
    atomic_write_str(&path, &new_content)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::deepseek_v2::parse_session_permissions;

    const SETTINGS_FIXTURE: &str = "ui-onboarding:\n  welcomeNoticeVersion: 2026-08-13.1\nagent-default-model:\n  provider: deepseek-official\n  model: deepseek-v4-pro\n  reasoningEffort: max\n";

    #[test]
    fn apply_default_model_field_changes_model_and_preserves_other_sections() {
        let updated = apply_agent_default_model_field(SETTINGS_FIXTURE, "model", "deepseek-v4-flash").unwrap();
        let doc: serde_yaml::Value = serde_yaml::from_str(&updated).unwrap();

        assert_eq!(agent_default_model_field(&doc, "model").as_deref(), Some("deepseek-v4-flash"));
        assert_eq!(agent_default_model_field(&doc, "provider").as_deref(), Some("deepseek-official"));
        assert_eq!(agent_default_model_field(&doc, "reasoningEffort").as_deref(), Some("max"));
        assert_eq!(
            doc.get("ui-onboarding").and_then(|s| s.get("welcomeNoticeVersion")).and_then(|v| v.as_str()),
            Some("2026-08-13.1"),
            "ui-onboarding section must be preserved untouched"
        );
    }

    #[test]
    fn apply_reasoning_effort_field_changes_effort_and_preserves_model() {
        let updated = apply_agent_default_model_field(SETTINGS_FIXTURE, "reasoningEffort", "low").unwrap();
        let doc: serde_yaml::Value = serde_yaml::from_str(&updated).unwrap();

        assert_eq!(agent_default_model_field(&doc, "reasoningEffort").as_deref(), Some("low"));
        assert_eq!(agent_default_model_field(&doc, "model").as_deref(), Some("deepseek-v4-pro"));
        assert!(doc.get("ui-onboarding").is_some(), "ui-onboarding section must be preserved untouched");
    }

    #[test]
    fn apply_default_model_field_creates_section_when_settings_file_absent() {
        let updated = apply_agent_default_model_field("", "model", "deepseek-v4-pro").unwrap();
        let doc: serde_yaml::Value = serde_yaml::from_str(&updated).unwrap();
        assert_eq!(agent_default_model_field(&doc, "model").as_deref(), Some("deepseek-v4-pro"));
    }

    #[test]
    fn validate_reasoning_effort_accepts_known_values() {
        for effort in REASONING_OPTIONS {
            assert!(validate_reasoning_effort(effort).is_ok());
        }
    }

    #[test]
    fn validate_reasoning_effort_rejects_unknown_value() {
        let err = validate_reasoning_effort("medium").unwrap_err();
        assert!(err.contains("Unknown reasoning effort"));
    }

    #[test]
    fn build_model_options_unions_seen_defaults_and_current() {
        let seen: HashSet<String> = ["deepseek-v4-pro".to_string(), "custom-preview".to_string()].into();
        let options = build_model_options(seen, Some("deepseek-v4-flash"));

        assert!(options.contains(&"deepseek-v4-pro".to_string()));
        assert!(options.contains(&"custom-preview".to_string()));
        assert!(options.contains(&"deepseek-v4-flash".to_string()));
        assert!(options.contains(&"deepseek-v4-flash-vision-exp".to_string()));
        let unique: HashSet<&String> = options.iter().collect();
        assert_eq!(unique.len(), options.len(), "no duplicate model ids");
    }

    #[test]
    fn build_reasoning_options_includes_current_when_outside_enum() {
        let options = build_reasoning_options(Some("ultra"));
        assert!(options.contains(&"ultra".to_string()));
        assert!(options.contains(&"max".to_string()));
    }

    #[test]
    fn build_reasoning_options_has_no_duplicate_when_current_is_known() {
        let options = build_reasoning_options(Some("max"));
        let unique: HashSet<&String> = options.iter().collect();
        assert_eq!(unique.len(), options.len());
    }

    const PROJCACHE_FIXTURE: &str = r#"{
        "tables": {
            "sessions": {
                "session-1": {
                    "identity": { "createdAt": 1787661503700 },
                    "rows": {
                        "permissions": { "ver": 1, "seq": 1, "val": {
                            "preset": "workspace-write",
                            "sandbox": "workspace-write",
                            "approval": "ask"
                        }}
                    }
                },
                "session-2": {
                    "identity": { "createdAt": 1787661503700 },
                    "rows": {}
                }
            }
        }
    }"#;

    #[test]
    fn build_sessions_policies_from_parts_maps_workspace_name_and_falls_back_to_id() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "session-1".to_string(),
            DshSessionMeta {
                workspace_path: "/Users/dev/alpha".to_string(),
                workspace_name: "alpha".to_string(),
                title: Some("t".to_string()),
            },
        );
        let permissions = parse_session_permissions(PROJCACHE_FIXTURE);
        assert_eq!(permissions.len(), 1, "only session-1 has a recorded permissions row");

        let policies = build_sessions_policies_from_parts(&metadata, permissions);
        assert_eq!(policies.len(), 1);
        assert_eq!(policies[0].session_id, "session-1");
        assert_eq!(policies[0].workspace_name, "alpha");
        assert_eq!(policies[0].permission_preset.as_deref(), Some("workspace-write"));
        assert_eq!(policies[0].sandbox_mode.as_deref(), Some("workspace-write"));
        assert_eq!(policies[0].approval_policy.as_deref(), Some("ask"));
    }

    #[test]
    fn build_sessions_policies_from_parts_falls_back_to_session_id_when_workspace_unknown() {
        let permissions = parse_session_permissions(PROJCACHE_FIXTURE);
        let policies = build_sessions_policies_from_parts(&HashMap::new(), permissions);
        assert_eq!(policies.len(), 1);
        assert_eq!(policies[0].workspace_name, "session-1");
    }

    #[test]
    fn other_top_level_keys_excludes_agent_default_model() {
        let doc = parse_settings_yaml(SETTINGS_FIXTURE);
        let keys = other_top_level_keys(&doc);
        assert_eq!(keys, ["ui-onboarding"]);
    }

    #[test]
    fn set_deepseek_default_model_rejects_empty_string() {
        let err = set_deepseek_default_model(String::new()).unwrap_err();
        assert!(err.contains("must not be empty"));
    }

    #[test]
    fn set_deepseek_reasoning_effort_rejects_unknown_value() {
        let err = set_deepseek_reasoning_effort("medium".to_string()).unwrap_err();
        assert!(err.contains("Unknown reasoning effort"));
    }
}
