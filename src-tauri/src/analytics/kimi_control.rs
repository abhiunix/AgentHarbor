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

/// One editable (or read-only) scalar entry within a `KimiConfigSection`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KimiConfigEntry {
    pub key: String,
    /// "bool" | "int" | "float" | "string" | "array".
    pub value_type: String,
    /// Rendered current value: bools as "true"/"false", numbers as-is,
    /// strings without their surrounding quotes, arrays as the raw `[...]`.
    pub value: String,
    pub editable: bool,
}

/// A group of editable scalars, either the top-level of config.toml
/// (`section: None`) or a `[section]` table (e.g. `Some("loop_control")`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KimiConfigSection {
    pub section: Option<String>,
    pub entries: Vec<KimiConfigEntry>,
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

// ── Generic config.toml parse — every editable scalar, grouped by section ───

/// `key = value` → `(key, raw_rhs)`, or `None` if `line` isn't a scalar
/// assignment.
fn split_key_value(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('[') {
        return None;
    }
    let (key, rhs) = trimmed.split_once('=')?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    Some((key, rhs.trim()))
}

fn is_int_token(s: &str) -> bool {
    let digits = s.strip_prefix('-').unwrap_or(s);
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
}

fn is_float_token(s: &str) -> bool {
    let digits = s.strip_prefix('-').unwrap_or(s);
    let Some((int_part, frac_part)) = digits.split_once('.') else { return false };
    !frac_part.is_empty()
        && frac_part.chars().all(|c| c.is_ascii_digit())
        && int_part.chars().all(|c| c.is_ascii_digit())
}

/// Infer `(value_type, rendered_value, editable)` from a raw TOML token, or
/// `None` if the token isn't one of the shapes this editor understands
/// (bool / int / float / quoted string / array).
fn build_entry(key: &str, rhs: &str) -> Option<KimiConfigEntry> {
    let (value_type, value, editable) = if rhs == "true" || rhs == "false" {
        ("bool", rhs.to_string(), true)
    } else if rhs.starts_with('[') {
        ("array", rhs.to_string(), false)
    } else if rhs.len() >= 2
        && ((rhs.starts_with('"') && rhs.ends_with('"'))
            || (rhs.starts_with('\'') && rhs.ends_with('\'')))
    {
        ("string", rhs[1..rhs.len() - 1].to_string(), true)
    } else if is_int_token(rhs) {
        ("int", rhs.to_string(), true)
    } else if is_float_token(rhs) {
        ("float", rhs.to_string(), true)
    } else {
        return None;
    };
    Some(KimiConfigEntry { key: key.to_string(), value_type: value_type.to_string(), value, editable })
}

/// Parse config.toml into editable sections: top-level (`section: None`),
/// then each `[section]` table in file order, excluding `models`/`providers`
/// tables (and, for the nested `[providers]` shape, their bare sub-headers)
/// entirely. Mirrors `parse_config`'s providers-block handling so a table
/// like `[providers."managed:x"]` or `[providers]` + `[managed:x]` is never
/// surfaced here.
fn parse_config_sections(content: &str) -> Vec<KimiConfigSection> {
    let mut sections: Vec<KimiConfigSection> = vec![KimiConfigSection { section: None, entries: vec![] }];
    let mut past_top_level = false;
    let mut in_providers_block = false;
    let mut skip_current = false;

    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('[') {
            past_top_level = true;
            let header = rest.trim_end_matches(']').trim();
            let header_unquoted = header.trim_matches(|c| c == '"' || c == '\'');

            if header_unquoted == "providers" {
                in_providers_block = true;
                skip_current = true;
                continue;
            }
            if header_unquoted.starts_with("providers.")
                || header_unquoted == "models"
                || header_unquoted.starts_with("models.")
            {
                in_providers_block = false;
                skip_current = true;
                continue;
            }
            if in_providers_block {
                skip_current = true;
                continue;
            }
            skip_current = false;
            sections.push(KimiConfigSection { section: Some(header_unquoted.to_string()), entries: vec![] });
            continue;
        }

        let Some((key, rhs)) = split_key_value(line) else { continue };
        if !past_top_level && key == "default_model" {
            continue;
        }
        if past_top_level && skip_current {
            continue;
        }
        if let Some(entry) = build_entry(key, rhs) {
            sections.last_mut().expect("top-level section always present").entries.push(entry);
        }
    }

    sections
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

// ── Generic config write — section-scoped, error (never append) if missing ──

/// Backslash/quote/control-char escape for a TOML basic string body.
fn escape_toml_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

/// Replace a top-level (pre-`[section]`) `key`'s value, preserving every
/// other line byte-for-byte. Unlike `replace_or_insert_top_level` (used by
/// the model/flag setters), this errors instead of inserting when the key
/// isn't found — the generic editor should never silently create new keys.
fn replace_scalar_top_level(content: &str, key: &str, new_rhs: &str) -> Result<String, String> {
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
        return Err(format!("Key '{key}' not found at top level of config.toml"));
    }
    let mut result = lines.join("\n");
    if content.ends_with('\n') {
        result.push('\n');
    }
    Ok(result)
}

/// Replace `key`'s value within `[section]` only (stopping at the next
/// header), preserving every other line byte-for-byte. Errors instead of
/// inserting when the key isn't found in that scope.
fn replace_scalar_in_section(content: &str, section: &str, key: &str, new_rhs: &str) -> Result<String, String> {
    let mut lines: Vec<String> = Vec::new();
    let mut replaced = false;
    let mut in_section = false;

    for line in content.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix('[') {
            let header = rest.trim_end_matches(']').trim();
            let header_unquoted = header.trim_matches(|c| c == '"' || c == '\'');
            in_section = header_unquoted == section;
            lines.push(line.to_string());
            continue;
        }
        if !replaced && in_section {
            if let Some((leading_ws, _)) = scalar_line_bounds(line, key) {
                lines.push(format!("{leading_ws}{key} = {new_rhs}"));
                replaced = true;
                continue;
            }
        }
        lines.push(line.to_string());
    }

    if !replaced {
        return Err(format!("Key '{key}' not found in [{section}]"));
    }
    let mut result = lines.join("\n");
    if content.ends_with('\n') {
        result.push('\n');
    }
    Ok(result)
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

/// The full generic list of editable/read-only scalars in config.toml,
/// grouped by section, for the "Permissions & Control" editor. Excludes
/// `default_model` (stays in the Model picker) and the `models`/`providers`
/// tables entirely.
#[tauri::command]
pub fn get_kimi_config_tunables() -> Result<Vec<KimiConfigSection>, String> {
    Ok(parse_config_sections(&read_config_text()))
}

/// Generic scalar setter behind the config editor. Rejects the
/// `providers`/`models` tables and the `api_key` key outright, validates
/// `raw_value` against the declared `value_type`, then does a targeted,
/// section-scoped line replace — erroring (never appending) if the key isn't
/// found in that exact scope.
#[tauri::command]
pub fn set_kimi_config_value(
    section: Option<String>,
    key: String,
    raw_value: String,
    value_type: String,
) -> Result<(), String> {
    if let Some(sec) = &section {
        if sec == "providers" || sec.starts_with("providers.") || sec == "models" || sec.starts_with("models.") {
            return Err(format!("Cannot edit section '{sec}'"));
        }
    }
    if key == "api_key" {
        return Err("Cannot edit api_key".to_string());
    }

    let new_rhs = match value_type.as_str() {
        "bool" => {
            if raw_value != "true" && raw_value != "false" {
                return Err(format!("Invalid bool value: {raw_value}"));
            }
            raw_value
        }
        "int" => {
            raw_value.parse::<i64>().map_err(|_| format!("Invalid int value: {raw_value}"))?;
            raw_value
        }
        "float" => {
            raw_value.parse::<f64>().map_err(|_| format!("Invalid float value: {raw_value}"))?;
            raw_value
        }
        "string" => format!("\"{}\"", escape_toml_string(&raw_value)),
        other => return Err(format!("Unsupported value type: {other}")),
    };

    let root = kimi_root().ok_or_else(|| "Kimi home directory not found".to_string())?;
    let config_path = root.join("config.toml");
    let content = read_with_sharing(&config_path)?;

    let new_content = match &section {
        Some(sec) => replace_scalar_in_section(&content, sec, &key, &new_rhs)?,
        None => replace_scalar_top_level(&content, &key, &new_rhs)?,
    };
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

    // ── Generic config editor ────────────────────────────────────────────────

    const FIXTURE: &str = r#"default_model = "moonshot-ai/kimi-k2.7-code"
default_thinking = false
default_yolo = true
default_plan_mode = false
default_editor = "vim"
theme = "dark"
telemetry = true
hooks = []
extra_skill_dirs = []

[loop_control]
max_steps_per_turn = 40
max_retries_per_step = 3
reserved_context_size = 8192
compaction_trigger_ratio = 0.85

[background]
max_running_tasks = 4
keep_alive_on_exit = false

[models."moonshot-ai/kimi-k2.7-code"]
provider = "managed:moonshot-ai"
model = "kimi-k2.7-code"

[providers."managed:moonshot-ai"]
type = "kimi"
api_key = "sk-secret"
"#;

    #[test]
    fn parse_config_sections_infers_types_and_excludes_models_and_providers() {
        let sections = parse_config_sections(FIXTURE);

        fn find_entry<'a>(entries: &'a [KimiConfigEntry], key: &str) -> Option<&'a KimiConfigEntry> {
            entries.iter().find(|e| e.key == key)
        }

        let top = sections.iter().find(|s| s.section.is_none()).expect("top-level section present");
        assert!(find_entry(&top.entries, "default_model").is_none(), "default_model must stay out of tunables");

        let yolo = find_entry(&top.entries, "default_yolo").expect("default_yolo present");
        assert_eq!(yolo.value_type, "bool");
        assert_eq!(yolo.value, "true");
        assert!(yolo.editable);

        let editor = find_entry(&top.entries, "default_editor").expect("default_editor present");
        assert_eq!(editor.value_type, "string");
        assert_eq!(editor.value, "vim");
        assert!(editor.editable);

        let hooks = find_entry(&top.entries, "hooks").expect("hooks present");
        assert_eq!(hooks.value_type, "array");
        assert_eq!(hooks.value, "[]");
        assert!(!hooks.editable);

        let loop_control = sections
            .iter()
            .find(|s| s.section.as_deref() == Some("loop_control"))
            .expect("loop_control present");
        let steps = find_entry(&loop_control.entries, "max_steps_per_turn").expect("max_steps_per_turn present");
        assert_eq!(steps.value_type, "int");
        assert_eq!(steps.value, "40");
        let ratio = find_entry(&loop_control.entries, "compaction_trigger_ratio").expect("ratio present");
        assert_eq!(ratio.value_type, "float");
        assert_eq!(ratio.value, "0.85");

        let background = sections
            .iter()
            .find(|s| s.section.as_deref() == Some("background"))
            .expect("background present");
        assert!(find_entry(&background.entries, "max_running_tasks").is_some());

        assert!(sections.iter().all(|s| s.section.as_deref() != Some("models")));
        assert!(sections.iter().all(|s| !s.section.as_deref().unwrap_or("").starts_with("models.")));
        assert!(sections.iter().all(|s| s.section.as_deref() != Some("providers")));
        assert!(sections.iter().all(|s| !s.section.as_deref().unwrap_or("").starts_with("providers.")));
        assert!(sections.iter().flat_map(|s| &s.entries).all(|e| e.key != "api_key"));
    }

    #[test]
    fn replace_scalar_top_level_changes_only_target_line_and_keeps_api_key() {
        let updated = replace_scalar_top_level(FIXTURE, "default_yolo", "false").unwrap();

        let orig_lines: Vec<&str> = FIXTURE.lines().collect();
        let new_lines: Vec<&str> = updated.lines().collect();
        assert_eq!(orig_lines.len(), new_lines.len());
        for (o, n) in orig_lines.iter().zip(new_lines.iter()) {
            if o.trim_start().starts_with("default_yolo") {
                assert_eq!(*n, "default_yolo = false");
            } else {
                assert_eq!(o, n, "unrelated line should be byte-identical");
            }
        }
        assert!(updated.contains(r#"api_key = "sk-secret""#));
    }

    #[test]
    fn replace_scalar_in_section_scopes_int_and_float_to_loop_control_only() {
        let content = r#"[loop_control]
max_steps_per_turn = 40
compaction_trigger_ratio = 0.85

[background]
max_steps_per_turn = 999
"#;
        let updated = replace_scalar_in_section(content, "loop_control", "max_steps_per_turn", "60").unwrap();
        assert!(updated.contains("[loop_control]\nmax_steps_per_turn = 60"));
        assert!(updated.contains("[background]\nmax_steps_per_turn = 999"), "unrelated section untouched");

        let updated = replace_scalar_in_section(&updated, "loop_control", "compaction_trigger_ratio", "0.5").unwrap();
        assert!(updated.contains("compaction_trigger_ratio = 0.5"));
        assert!(updated.contains("[background]\nmax_steps_per_turn = 999"), "unrelated section still untouched");
    }

    #[test]
    fn replace_scalar_top_level_string_is_quoted_and_escaped() {
        let content = "default_editor = \"vim\"\n\n[loop_control]\nmax_steps_per_turn = 40\n";
        let new_rhs = format!("\"{}\"", escape_toml_string(r#"code --wait "x""#));
        let updated = replace_scalar_top_level(content, "default_editor", &new_rhs).unwrap();
        assert!(updated.contains(r#"default_editor = "code --wait \"x\"""#));
    }

    #[test]
    fn set_kimi_config_value_rejects_providers_section() {
        let err = set_kimi_config_value(
            Some("providers.\"managed:moonshot-ai\"".to_string()),
            "type".to_string(),
            "x".to_string(),
            "string".to_string(),
        )
        .unwrap_err();
        assert!(err.contains("Cannot edit section"));
    }

    #[test]
    fn set_kimi_config_value_rejects_models_section() {
        let err = set_kimi_config_value(
            Some("models.\"moonshot-ai/kimi-k3\"".to_string()),
            "provider".to_string(),
            "x".to_string(),
            "string".to_string(),
        )
        .unwrap_err();
        assert!(err.contains("Cannot edit section"));
    }

    #[test]
    fn set_kimi_config_value_rejects_api_key_regardless_of_section() {
        // A key literally named `api_key` is rejected even in a section that
        // isn't `providers`/`models` — the guard is independent of section.
        let err = set_kimi_config_value(
            Some("loop_control".to_string()),
            "api_key".to_string(),
            "sk-new".to_string(),
            "string".to_string(),
        )
        .unwrap_err();
        assert!(err.contains("Cannot edit api_key"));
    }

    #[test]
    fn set_kimi_config_value_rejects_bogus_value_type() {
        let err = set_kimi_config_value(None, "theme".to_string(), "dark".to_string(), "bogus".to_string())
            .unwrap_err();
        assert!(err.contains("Unsupported value type"));
    }

    #[test]
    fn set_kimi_config_value_rejects_non_parsing_raw_value() {
        let err = set_kimi_config_value(
            Some("loop_control".to_string()),
            "max_steps_per_turn".to_string(),
            "not-a-number".to_string(),
            "int".to_string(),
        )
        .unwrap_err();
        assert!(err.contains("Invalid int value"));
    }
}
