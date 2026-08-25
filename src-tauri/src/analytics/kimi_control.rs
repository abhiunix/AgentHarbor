//! Kimi "Permissions & Control" — read/write control settings sourced from
//! `~/.kimi/config.toml` (default model, default flags, loop control) plus a
//! read-only per-session approval view from each session's `state.json`.
//! Reuses `kimi_v2`'s `kimi_root()`, `parse_config`, and `build_dir_map()`.
//!
//! Config writes are targeted line-replacements that never touch the
//! `[providers]` block (which holds the user's API key) or any other line —
//! see `replace_or_insert_top_level` for the safety-critical logic.

use crate::analytics::kimi_v2::{self, build_dir_map, kimi_root, KimiModelInfo};
use crate::utils::paths::{atomic_write_str, read_with_sharing};
use serde::{Deserialize, Serialize};

/// Top-level boolean flags this page is allowed to toggle. Anything else is
/// rejected before the config file is even read.
const CONTROL_FLAGS: [&str; 3] = ["default_yolo", "default_thinking", "default_plan_mode"];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KimiLoopControl {
    pub max_steps_per_turn: Option<u64>,
    pub max_retries_per_step: Option<u64>,
    pub reserved_context_size: Option<u64>,
    pub compaction_trigger_ratio: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KimiSessionApproval {
    pub session_id: String,
    pub project_name: String,
    pub yolo: bool,
    pub afk: bool,
    pub auto_approve_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KimiControlSettings {
    pub default_model: Option<String>,
    pub default_yolo: bool,
    pub default_thinking: bool,
    pub default_plan_mode: bool,
    pub loop_control: KimiLoopControl,
    pub models: Vec<KimiModelInfo>,
    pub sessions_approval: Vec<KimiSessionApproval>,
}

// ── config.toml — top-level flags + [loop_control] (read-only parse) ────────

struct ControlExtras {
    default_yolo: bool,
    default_thinking: bool,
    default_plan_mode: bool,
    loop_control: KimiLoopControl,
}

/// `key = value` → the trimmed right-hand side, or `None` if `line` isn't an
/// assignment to exactly `key`.
fn scalar_rhs<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(key)?.trim_start();
    rest.strip_prefix('=').map(|v| v.trim())
}

fn parse_control_extras(content: &str) -> ControlExtras {
    let mut extras = ControlExtras {
        default_yolo: false,
        default_thinking: false,
        default_plan_mode: false,
        loop_control: KimiLoopControl::default(),
    };
    let mut past_top_level = false;
    let mut in_loop_control = false;

    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('[') {
            past_top_level = true;
            let header = rest.trim_end_matches(']').trim();
            in_loop_control = header == "loop_control";
            continue;
        }
        if !past_top_level {
            if let Some(v) = scalar_rhs(line, "default_yolo") {
                extras.default_yolo = v == "true";
            } else if let Some(v) = scalar_rhs(line, "default_thinking") {
                extras.default_thinking = v == "true";
            } else if let Some(v) = scalar_rhs(line, "default_plan_mode") {
                extras.default_plan_mode = v == "true";
            }
            continue;
        }
        if in_loop_control {
            if let Some(v) = scalar_rhs(line, "max_steps_per_turn") {
                extras.loop_control.max_steps_per_turn = v.parse().ok();
            } else if let Some(v) = scalar_rhs(line, "max_retries_per_step") {
                extras.loop_control.max_retries_per_step = v.parse().ok();
            } else if let Some(v) = scalar_rhs(line, "reserved_context_size") {
                extras.loop_control.reserved_context_size = v.parse().ok();
            } else if let Some(v) = scalar_rhs(line, "compaction_trigger_ratio") {
                extras.loop_control.compaction_trigger_ratio = v.parse().ok();
            }
        }
    }
    extras
}

fn read_config_text() -> String {
    kimi_root()
        .and_then(|r| std::fs::read_to_string(r.join("config.toml")).ok())
        .unwrap_or_default()
}

// ── Per-session approval (state.json) ────────────────────────────────────────

struct SessionDirInfo {
    project_path: String,
    session_id: String,
    dir: std::path::PathBuf,
}

/// Mirrors `kimi_plans::discover_session_dirs` / `kimi_transcripts`'s
/// session-walking pattern.
fn discover_session_dirs() -> Vec<SessionDirInfo> {
    let Some(root) = kimi_root() else { return vec![] };
    let dir_map = build_dir_map();
    let sessions_root = root.join("sessions");
    let mut out = Vec::new();

    let Ok(md5_dirs) = std::fs::read_dir(&sessions_root) else { return out };
    for md5_entry in md5_dirs.flatten() {
        if !md5_entry.path().is_dir() {
            continue;
        }
        let md5_name = md5_entry.file_name().to_string_lossy().to_string();
        let project_path = dir_map
            .get(&md5_name)
            .map(|(p, _)| p.clone())
            .unwrap_or_else(|| md5_name.clone());

        let Ok(session_dirs) = std::fs::read_dir(md5_entry.path()) else { continue };
        for sess_entry in session_dirs.flatten() {
            let sess_path = sess_entry.path();
            if !sess_path.is_dir() {
                continue;
            }
            out.push(SessionDirInfo {
                project_path: project_path.clone(),
                session_id: sess_entry.file_name().to_string_lossy().to_string(),
                dir: sess_path,
            });
        }
    }
    out
}

fn dir_modified_unix(dir: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(dir).and_then(|m| m.modified()).ok()
}

fn project_name_from_path(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

#[derive(Debug, Clone, Deserialize, Default)]
struct KimiApproval {
    #[serde(default)]
    yolo: bool,
    #[serde(default)]
    afk: bool,
    #[serde(default)]
    auto_approve_actions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct KimiSessionStateApproval {
    #[serde(default)]
    approval: Option<KimiApproval>,
}

fn read_session_approval(dir: &std::path::Path) -> Option<KimiApproval> {
    let text = std::fs::read_to_string(dir.join("state.json")).ok()?;
    let state: KimiSessionStateApproval = serde_json::from_str(&text).ok()?;
    state.approval
}

fn approval_to_session(session_id: &str, project_path: &str, approval: KimiApproval) -> KimiSessionApproval {
    KimiSessionApproval {
        session_id: session_id.to_string(),
        project_name: project_name_from_path(project_path),
        yolo: approval.yolo,
        afk: approval.afk,
        auto_approve_actions: approval.auto_approve_actions,
    }
}

fn read_sessions_approval() -> Vec<KimiSessionApproval> {
    let mut sessions = discover_session_dirs();
    sessions.sort_by_key(|sd| std::cmp::Reverse(dir_modified_unix(&sd.dir)));

    sessions
        .iter()
        .filter_map(|sd| {
            let approval = read_session_approval(&sd.dir)?;
            Some(approval_to_session(&sd.session_id, &sd.project_path, approval))
        })
        .collect()
}

fn build_control_settings() -> KimiControlSettings {
    let content = read_config_text();
    let (default_model, models, _providers) = kimi_v2::parse_config(&content);
    let extras = parse_control_extras(&content);
    KimiControlSettings {
        default_model,
        default_yolo: extras.default_yolo,
        default_thinking: extras.default_thinking,
        default_plan_mode: extras.default_plan_mode,
        loop_control: extras.loop_control,
        models,
        sessions_approval: read_sessions_approval(),
    }
}

// ── Config writes — targeted line-replace, never a full round-trip ──────────

/// `(leading_whitespace, existing_rhs)` if `line` assigns exactly `key`.
fn scalar_line_bounds<'a>(line: &'a str, key: &str) -> Option<(&'a str, &'a str)> {
    let trimmed = line.trim_start();
    let rhs = scalar_rhs(trimmed, key)?;
    let leading_ws = &line[..line.len() - trimmed.len()];
    Some((leading_ws, rhs))
}

/// The quote character already used for `key`'s value, defaulting to `"`
/// when the key isn't present yet.
fn detect_quote_char(content: &str, key: &str) -> char {
    for line in content.lines() {
        if line.trim_start().starts_with('[') {
            break;
        }
        if let Some((_, rhs)) = scalar_line_bounds(line, key) {
            if let Some(c) = rhs.chars().next() {
                if c == '"' || c == '\'' {
                    return c;
                }
            }
        }
    }
    '"'
}

/// Replace the first top-level (pre-`[section]`) line assigning `key` with
/// `key = new_rhs`, preserving every other line byte-for-byte. Inserts the
/// key at the top if it isn't present yet. Never looks past the first
/// section header, so a same-named key inside a table (e.g. `[providers.*]`
/// `api_key`) is never touched.
fn replace_or_insert_top_level(content: &str, key: &str, new_rhs: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut replaced = false;
    let mut past_top_level = false;

    for line in content.lines() {
        if !replaced && !past_top_level {
            if line.trim_start().starts_with('[') {
                past_top_level = true;
            } else if let Some((leading_ws, _)) = scalar_line_bounds(line, key) {
                lines.push(format!("{leading_ws}{key} = {new_rhs}"));
                replaced = true;
                continue;
            }
        }
        lines.push(line.to_string());
    }

    if !replaced {
        lines.insert(0, format!("{key} = {new_rhs}"));
    }

    let mut result = lines.join("\n");
    if content.ends_with('\n') {
        result.push('\n');
    }
    result
}

fn apply_default_model(content: &str, model_id: &str) -> String {
    let quote = detect_quote_char(content, "default_model");
    let rhs = format!("{quote}{model_id}{quote}");
    replace_or_insert_top_level(content, "default_model", &rhs)
}

fn apply_control_flag(content: &str, flag: &str, value: bool) -> String {
    let rhs = if value { "true" } else { "false" };
    replace_or_insert_top_level(content, flag, rhs)
}

// ── Tauri Commands ───────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_kimi_control_settings() -> Result<KimiControlSettings, String> {
    Ok(build_control_settings())
}

#[tauri::command]
pub fn set_kimi_default_model(model_id: String) -> Result<(), String> {
    let root = kimi_root().ok_or_else(|| "Kimi home directory not found".to_string())?;
    let config_path = root.join("config.toml");
    let content = read_with_sharing(&config_path)?;

    let (_default_model, models, _providers) = kimi_v2::parse_config(&content);
    if !models.iter().any(|m| m.id == model_id) {
        return Err(format!("Unknown Kimi model: {model_id}"));
    }

    let new_content = apply_default_model(&content, &model_id);
    atomic_write_str(&config_path, &new_content)
}

#[tauri::command]
pub fn set_kimi_control_flag(flag: String, value: bool) -> Result<(), String> {
    if !CONTROL_FLAGS.contains(&flag.as_str()) {
        return Err(format!("Unknown control flag: {flag}"));
    }
    let root = kimi_root().ok_or_else(|| "Kimi home directory not found".to_string())?;
    let config_path = root.join("config.toml");
    let content = read_with_sharing(&config_path)?;

    let new_content = apply_control_flag(&content, &flag, value);
    atomic_write_str(&config_path, &new_content)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_default_model_preserves_api_key_and_other_lines() {
        let content = r#"default_model = "moonshot-ai/kimi-k2.7-code"
theme = "dark"

[models."moonshot-ai/kimi-k2.7-code"]
provider = "managed:moonshot-ai"
model = "kimi-k2.7-code"

[models."moonshot-ai/kimi-k3"]
provider = "managed:moonshot-ai"
model = "kimi-k3"

[providers."managed:moonshot-ai"]
type = "kimi"
api_key = "sk-secret"
"#;
        let updated = apply_default_model(content, "moonshot-ai/kimi-k3");

        assert!(updated.contains("default_model = \"moonshot-ai/kimi-k3\""));
        assert!(updated.contains("api_key = \"sk-secret\""));

        let orig_lines: Vec<&str> = content.lines().collect();
        let new_lines: Vec<&str> = updated.lines().collect();
        assert_eq!(orig_lines.len(), new_lines.len());
        for (i, (o, n)) in orig_lines.iter().zip(new_lines.iter()).enumerate() {
            if i == 0 {
                assert_eq!(*n, "default_model = \"moonshot-ai/kimi-k3\"");
            } else {
                assert_eq!(o, n, "line {i} should be unchanged");
            }
        }
    }

    #[test]
    fn apply_default_model_inserts_key_when_absent() {
        let content = "theme = \"dark\"\n\n[models.\"a\"]\nprovider = \"x\"\n";
        let updated = apply_default_model(content, "a");
        assert_eq!(updated.lines().next(), Some("default_model = \"a\""));
        assert!(updated.contains("theme = \"dark\""));
        assert!(updated.contains("[models.\"a\"]"));
    }

    #[test]
    fn apply_control_flag_toggles_single_line_only() {
        let content = r#"default_model = "moonshot-ai/kimi-k2.7-code"
default_yolo = true
default_thinking = false

[loop_control]
max_steps_per_turn = 40
"#;
        let updated = apply_control_flag(content, "default_yolo", false);

        let orig_lines: Vec<&str> = content.lines().collect();
        let new_lines: Vec<&str> = updated.lines().collect();
        assert_eq!(orig_lines.len(), new_lines.len());
        assert_eq!(new_lines[1], "default_yolo = false");
        for i in [0usize, 2, 3, 4, 5] {
            assert_eq!(orig_lines[i], new_lines[i]);
        }
    }

    #[test]
    fn set_kimi_control_flag_rejects_non_allowlisted_flag() {
        let err = set_kimi_control_flag("api_key".to_string(), true).unwrap_err();
        assert!(err.contains("Unknown control flag"));
    }

    #[test]
    fn parse_control_extras_reads_top_level_flags_and_loop_control() {
        let content = r#"default_model = "moonshot-ai/kimi-k2.7-code"
default_yolo = true
default_thinking = false
default_plan_mode = true

[loop_control]
max_steps_per_turn = 40
max_retries_per_step = 3
reserved_context_size = 8192
compaction_trigger_ratio = 0.85

[models."moonshot-ai/kimi-k2.7-code"]
provider = "managed:moonshot-ai"
"#;
        let extras = parse_control_extras(content);
        assert!(extras.default_yolo);
        assert!(!extras.default_thinking);
        assert!(extras.default_plan_mode);
        assert_eq!(extras.loop_control.max_steps_per_turn, Some(40));
        assert_eq!(extras.loop_control.max_retries_per_step, Some(3));
        assert_eq!(extras.loop_control.reserved_context_size, Some(8192));
        assert_eq!(extras.loop_control.compaction_trigger_ratio, Some(0.85));
    }

    #[test]
    fn parse_control_extras_defaults_when_absent() {
        let content = "default_model = \"x\"\n\n[models.\"x\"]\nprovider = \"y\"\n";
        let extras = parse_control_extras(content);
        assert!(!extras.default_yolo);
        assert!(!extras.default_thinking);
        assert!(!extras.default_plan_mode);
        assert_eq!(extras.loop_control.max_steps_per_turn, None);
        assert_eq!(extras.loop_control.compaction_trigger_ratio, None);
    }

    #[test]
    fn approval_to_session_maps_state_fields() {
        let approval = KimiApproval {
            yolo: true,
            afk: false,
            auto_approve_actions: ["Bash".to_string(), "Edit".to_string()].into(),
        };
        let session = approval_to_session("sess-1", "/Users/test/my-project", approval);
        assert_eq!(session.session_id, "sess-1");
        assert_eq!(session.project_name, "my-project");
        assert!(session.yolo);
        assert!(!session.afk);
        assert_eq!(session.auto_approve_actions, ["Bash", "Edit"]);
    }

    #[test]
    fn parses_approval_json_shape_from_state_file() {
        let text = r#"{"approval": {"yolo": true, "afk": false, "auto_approve_actions": ["Bash(*)"]}}"#;
        let state: KimiSessionStateApproval = serde_json::from_str(text).unwrap();
        let approval = state.approval.expect("approval present");
        assert!(approval.yolo);
        assert!(!approval.afk);
        assert_eq!(approval.auto_approve_actions, ["Bash(*)"]);
    }

    #[test]
    fn parses_approval_json_shape_when_missing() {
        let text = r#"{"version": 1}"#;
        let state: KimiSessionStateApproval = serde_json::from_str(text).unwrap();
        assert!(state.approval.is_none());
    }
}
