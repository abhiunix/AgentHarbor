use super::codex_app_server;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item};

const MAX_TEXT_FILE_BYTES: u64 = 4 * 1024 * 1024;
const DEFAULT_PROJECT_DOC_MAX_BYTES: usize = 32 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexInstructionSource {
    pub path: String,
    pub kind: String,
    pub exists: bool,
    pub loaded: bool,
    pub truncated: bool,
    #[serde(skip)]
    byte_len: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexInstructionsSnapshot {
    pub scope: String,
    pub path: String,
    pub content: String,
    pub exists: bool,
    pub revision: String,
    pub instruction_sources: Vec<CodexInstructionSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexPermissionProfile {
    pub id: String,
    pub name: String,
    pub description: String,
    pub allowed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexControlSnapshot {
    pub scope: String,
    pub source_path: String,
    pub model: String,
    pub model_reasoning_effort: String,
    pub approval_policy: String,
    pub sandbox_mode: String,
    pub web_search: bool,
    pub network_access: bool,
    pub source: String,
    pub layers: Vec<Value>,
    pub warnings: Vec<String>,
    pub permission_profiles: Vec<CodexPermissionProfile>,
    pub app_server_available: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodexControlUpdates {
    pub model: Option<String>,
    pub model_reasoning_effort: Option<String>,
    pub approval_policy: Option<String>,
    pub sandbox_mode: Option<String>,
    pub web_search: Option<bool>,
    pub network_access: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexReasoningEffortOption {
    pub reasoning_effort: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexModelInfo {
    pub id: String,
    pub model: String,
    pub display_name: String,
    pub description: String,
    pub hidden: bool,
    pub supported_reasoning_efforts: Vec<CodexReasoningEffortOption>,
    pub reasoning_efforts: Vec<String>,
    pub default_reasoning_effort: String,
    pub input_modalities: Vec<String>,
    pub supports_personality: bool,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexModelList {
    pub models: Vec<CodexModelInfo>,
    pub app_server_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configured_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configured_reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexModelUpdateResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configured_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configured_reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

#[derive(Debug, Clone)]
struct ConfigEdit {
    key_path: &'static str,
    value: Value,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct ConfigWriteOutcome {
    warning: Option<String>,
}

fn canonical_project_root(project_path: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(project_path);
    if !path.is_absolute() {
        return Err("Project path must be absolute".into());
    }
    let canonical = dunce::canonicalize(&path)
        .map_err(|error| format!("Could not resolve project path: {error}"))?;
    if !canonical.is_dir() {
        return Err("Project path must be an existing directory".into());
    }
    Ok(canonical)
}

/// Resolve a file below an allowed root and reject symlink escapes. Existing
/// file symlinks are rejected because atomic replacement would otherwise
/// replace the link itself rather than update its target.
fn safe_file_below(root: &Path, relative: &Path, create_parent: bool) -> Result<PathBuf, String> {
    if !root.is_absolute() || relative.is_absolute() {
        return Err("Safe path resolution requires an absolute root and relative child".into());
    }
    if relative.components().any(|component| {
        matches!(
            component,
            std::path::Component::ParentDir | std::path::Component::RootDir
        )
    }) {
        return Err("Path escapes its allowed root".into());
    }

    if create_parent {
        fs::create_dir_all(root).map_err(|error| error.to_string())?;
    }
    let canonical_root = if root.exists() {
        dunce::canonicalize(root).map_err(|error| error.to_string())?
    } else {
        root.to_path_buf()
    };
    let candidate = canonical_root.join(relative);
    let parent = candidate
        .parent()
        .ok_or_else(|| "Target has no parent directory".to_string())?;

    if create_parent {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    if parent.exists() {
        let canonical_parent = dunce::canonicalize(parent).map_err(|error| error.to_string())?;
        if !canonical_parent.starts_with(&canonical_root) {
            return Err("Target parent resolves outside its allowed root".into());
        }
    }
    if candidate.exists() {
        let metadata = fs::symlink_metadata(&candidate).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            return Err("Target file must not be a symbolic link".into());
        }
        let canonical_target =
            dunce::canonicalize(&candidate).map_err(|error| error.to_string())?;
        if !canonical_target.starts_with(&canonical_root) {
            return Err("Target resolves outside its allowed root".into());
        }
    }
    Ok(candidate)
}

fn read_small_utf8(path: &Path) -> Result<String, String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if metadata.len() > MAX_TEXT_FILE_BYTES {
        return Err(format!(
            "{} is larger than the 4 MiB editor limit",
            path.display()
        ));
    }
    fs::read_to_string(path).map_err(|error| error.to_string())
}

fn instruction_source(path: PathBuf, kind: &str) -> Result<CodexInstructionSource, String> {
    let exists = path.is_file();
    Ok(CodexInstructionSource {
        path: path.to_string_lossy().to_string(),
        kind: kind.to_string(),
        exists,
        loaded: false,
        truncated: false,
        byte_len: 0,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InstructionDiscoveryConfig {
    fallback_filenames: Vec<String>,
    max_bytes: usize,
}

impl Default for InstructionDiscoveryConfig {
    fn default() -> Self {
        Self {
            fallback_filenames: Vec::new(),
            max_bytes: DEFAULT_PROJECT_DOC_MAX_BYTES,
        }
    }
}

fn safe_instruction_filename(name: &str) -> bool {
    let path = Path::new(name);
    !name.is_empty()
        && !path.is_absolute()
        && path.components().count() == 1
        && path.file_name().is_some_and(|file_name| file_name == name)
}

fn apply_instruction_config_json(config: &Value, settings: &mut InstructionDiscoveryConfig) {
    if let Some(max_bytes) = config
        .get("project_doc_max_bytes")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
    {
        settings.max_bytes = max_bytes.min(MAX_TEXT_FILE_BYTES as usize);
    }
    if let Some(filenames) = config
        .get("project_doc_fallback_filenames")
        .and_then(Value::as_array)
    {
        settings.fallback_filenames = filenames
            .iter()
            .filter_map(Value::as_str)
            .filter(|name| safe_instruction_filename(name))
            .map(ToString::to_string)
            .collect();
    }
}

fn apply_instruction_config_toml(
    content: &str,
    settings: &mut InstructionDiscoveryConfig,
) -> Result<(), String> {
    let document = content
        .parse::<DocumentMut>()
        .map_err(|error| format!("Could not parse Codex config: {error}"))?;
    if let Some(max_bytes) = document
        .get("project_doc_max_bytes")
        .and_then(Item::as_integer)
        .and_then(|value| usize::try_from(value).ok())
    {
        settings.max_bytes = max_bytes.min(MAX_TEXT_FILE_BYTES as usize);
    }
    if let Some(filenames) = document
        .get("project_doc_fallback_filenames")
        .and_then(Item::as_array)
    {
        settings.fallback_filenames = filenames
            .iter()
            .filter_map(|value| value.as_str())
            .filter(|name| safe_instruction_filename(name))
            .map(ToString::to_string)
            .collect();
    }
    Ok(())
}

fn read_utf8_prefix(path: &Path, max_bytes: usize) -> Result<String, String> {
    if max_bytes == 0 {
        return Ok(String::new());
    }
    let mut bytes = Vec::with_capacity(max_bytes.min(DEFAULT_PROJECT_DOC_MAX_BYTES));
    fs::File::open(path)
        .map_err(|error| error.to_string())?
        .take(max_bytes as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    match String::from_utf8(bytes) {
        Ok(content) => Ok(content),
        Err(error) if error.utf8_error().error_len().is_none() => {
            let valid = error.utf8_error().valid_up_to();
            String::from_utf8(error.into_bytes()[..valid].to_vec())
                .map_err(|error| error.to_string())
        }
        Err(error) => Err(format!("{} is not valid UTF-8: {error}", path.display())),
    }
}

fn first_non_empty_instruction_at(
    directory: &Path,
    fallback_filenames: &[String],
    max_bytes: usize,
    normal_kind: &str,
    override_kind: &str,
) -> Result<Option<CodexInstructionSource>, String> {
    let mut candidates = vec![
        ("AGENTS.override.md", override_kind),
        ("AGENTS.md", normal_kind),
    ];
    candidates.extend(
        fallback_filenames
            .iter()
            .map(|name| (name.as_str(), "projectFallback")),
    );
    for (filename, kind) in candidates {
        let path = directory.join(filename);
        if !path.is_file() {
            continue;
        }
        let file_size = fs::metadata(&path)
            .map_err(|error| error.to_string())?
            .len();
        let content = read_utf8_prefix(&path, max_bytes)?;
        if content.trim().is_empty() {
            continue;
        }
        return Ok(Some(CodexInstructionSource {
            path: path.to_string_lossy().to_string(),
            kind: kind.to_string(),
            exists: true,
            loaded: true,
            truncated: file_size > max_bytes as u64,
            byte_len: content.len(),
        }));
    }
    Ok(None)
}

fn repository_root_or_self(project_root: &Path) -> PathBuf {
    project_root
        .ancestors()
        .find(|candidate| candidate.join(".git").exists())
        .unwrap_or(project_root)
        .to_path_buf()
}

fn directories_from_root(root: &Path, leaf: &Path) -> Vec<PathBuf> {
    let mut directories = Vec::new();
    let mut cursor = Some(leaf);
    while let Some(directory) = cursor {
        directories.push(directory.to_path_buf());
        if directory == root {
            break;
        }
        cursor = directory.parent();
    }
    directories.reverse();
    directories
}

fn instruction_discovery_config(
    project_root: Option<&Path>,
) -> Result<InstructionDiscoveryConfig, String> {
    let params = json!({
        "includeLayers": false,
        "cwd": project_root.map(|path| path.to_string_lossy().to_string())
    });
    if let Ok(result) = codex_app_server::request("config/read", params) {
        let mut settings = InstructionDiscoveryConfig::default();
        if let Some(config) = result.get("config") {
            apply_instruction_config_json(config, &mut settings);
        }
        return Ok(settings);
    }

    let mut settings = InstructionDiscoveryConfig::default();
    let global = config_file_for_scope(None, false)?;
    if global.is_file() {
        apply_instruction_config_toml(&read_small_utf8(&global)?, &mut settings)?;
    }
    if let Some(root) = project_root {
        let project = config_file_for_scope(Some(root), false)?;
        if project.is_file() {
            apply_instruction_config_toml(&read_small_utf8(&project)?, &mut settings)?;
        }
    }
    Ok(settings)
}

fn read_codex_instructions_impl(
    project_path: Option<String>,
) -> Result<CodexInstructionsSnapshot, String> {
    let home = crate::utils::codex_paths::codex_home()?;
    let global_target = safe_file_below(&home, Path::new("AGENTS.md"), false)?;
    let project_root = project_path
        .as_deref()
        .map(canonical_project_root)
        .transpose()?;
    let target = match &project_root {
        Some(root) => safe_file_below(root, Path::new("AGENTS.md"), false)?,
        None => global_target.clone(),
    };

    let exists = target.is_file();
    let content = if exists {
        read_small_utf8(&target)?
    } else {
        String::new()
    };
    let mut sources = Vec::new();

    if let Some(global) = first_non_empty_instruction_at(
        &home,
        &[],
        MAX_TEXT_FILE_BYTES as usize,
        "global",
        "globalOverride",
    )? {
        sources.push(global);
    } else if project_root.is_none() {
        sources.push(instruction_source(global_target.clone(), "global")?);
    }

    if let Some(root) = &project_root {
        let discovery = instruction_discovery_config(Some(root))?;
        let repository_root = repository_root_or_self(root);
        let mut remaining = discovery.max_bytes;
        for directory in directories_from_root(&repository_root, root) {
            if remaining == 0 {
                break;
            }
            if let Some(source) = first_non_empty_instruction_at(
                &directory,
                &discovery.fallback_filenames,
                remaining,
                "project",
                "projectOverride",
            )? {
                remaining = remaining.saturating_sub(source.byte_len);
                sources.push(source);
            }
        }
        if !sources
            .iter()
            .any(|source| source.path == target.to_string_lossy())
        {
            sources.push(instruction_source(target.clone(), "projectTarget")?);
        }
    }

    Ok(CodexInstructionsSnapshot {
        scope: if project_root.is_some() {
            "project".into()
        } else {
            "global".into()
        },
        path: target.to_string_lossy().to_string(),
        exists,
        revision: instruction_revision(exists, &content),
        content,
        instruction_sources: sources,
    })
}

#[tauri::command]
pub async fn read_codex_instructions(
    project_path: Option<String>,
) -> Result<CodexInstructionsSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || read_codex_instructions_impl(project_path))
        .await
        .map_err(|error| format!("Codex instructions task failed: {error}"))?
}

fn write_codex_instructions_impl(
    project_path: Option<String>,
    content: String,
    expected_revision: Option<String>,
) -> Result<CodexInstructionsSnapshot, String> {
    if content.len() as u64 > MAX_TEXT_FILE_BYTES {
        return Err("Instructions exceed the 4 MiB editor limit".into());
    }
    let target = match project_path.as_deref() {
        Some(project) => {
            let root = canonical_project_root(project)?;
            safe_file_below(&root, Path::new("AGENTS.md"), true)?
        }
        None => {
            let home = crate::utils::codex_paths::codex_home()?;
            safe_file_below(&home, Path::new("AGENTS.md"), true)?
        }
    };
    if let Some(expected_revision) = expected_revision {
        let current_exists = target.is_file();
        let current_content = if current_exists {
            read_small_utf8(&target)?
        } else {
            String::new()
        };
        if instruction_revision(current_exists, &current_content) != expected_revision {
            return Err(
                "Codex instructions changed outside AgentHarbor. Refresh before saving so those changes are not overwritten."
                    .into(),
            );
        }
    }
    crate::commands::codex::write_codex_config_file(&target, &content)?;
    read_codex_instructions_impl(project_path)
}

#[tauri::command]
pub async fn write_codex_instructions(
    project_path: Option<String>,
    content: String,
    expected_revision: Option<String>,
) -> Result<CodexInstructionsSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        write_codex_instructions_impl(project_path, content, expected_revision)
    })
    .await
    .map_err(|error| format!("Codex instructions task failed: {error}"))?
}

fn instruction_revision(exists: bool, content: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(if exists {
        &b"file\0"[..]
    } else {
        &b"missing\0"[..]
    });
    digest.update(content.as_bytes());
    format!("{:x}", digest.finalize())
}

fn string_field(config: &Value, key: &str) -> Option<String> {
    config.get(key)?.as_str().map(ToString::to_string)
}

fn relevant_config(value: &Value) -> Value {
    let mut output = Map::new();
    for key in [
        "model",
        "model_reasoning_effort",
        "approval_policy",
        "sandbox_mode",
        "web_search",
    ] {
        if let Some(field) = value.get(key) {
            output.insert(key.to_string(), field.clone());
        }
    }
    if let Some(network) = value.pointer("/sandbox_workspace_write/network_access") {
        output.insert(
            "sandbox_workspace_write".into(),
            json!({ "network_access": network }),
        );
    }
    Value::Object(output)
}

fn sanitize_app_server_layers(result: &Value) -> Vec<Value> {
    result
        .get("layers")
        .and_then(Value::as_array)
        .map(|layers| {
            layers
                .iter()
                .map(|layer| {
                    json!({
                        "name": layer.get("name").cloned().unwrap_or(Value::Null),
                        "version": layer.get("version").cloned().unwrap_or(Value::Null),
                        "config": relevant_config(layer.get("config").unwrap_or(&Value::Null)),
                        "disabledReason": layer.get("disabledReason").cloned().unwrap_or(Value::Null)
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn snapshot_from_app_server(
    result: Value,
    project_root: Option<&Path>,
) -> Result<CodexControlSnapshot, String> {
    let config = result
        .get("config")
        .ok_or_else(|| "App Server config/read response has no config field".to_string())?;
    let mut warnings = Vec::new();
    let approval_policy = match config.get("approval_policy") {
        Some(Value::String(value)) => Some(value.clone()),
        Some(Value::Object(_)) => {
            warnings.push(
                "A granular approval policy is active; this editor will not overwrite it unless a new policy is selected."
                    .into(),
            );
            Some("granular".into())
        }
        _ => None,
    };
    let web_search_mode = string_field(config, "web_search");
    if web_search_mode.as_deref() == Some("cached") {
        warnings.push(
            "Web search is set to 'cached'. The toggle shows it as enabled; saving it after toggling will use 'live' or 'disabled'."
                .into(),
        );
    } else if let Some(mode) = web_search_mode.as_deref() {
        if !matches!(mode, "disabled" | "indexed" | "live") {
            warnings.push(format!(
                "Web search uses the custom mode '{mode}'. The toggle represents every non-disabled mode as enabled."
            ));
        }
    }
    let source_path = config_file_for_scope(project_root, false)?;

    Ok(CodexControlSnapshot {
        scope: if project_root.is_some() {
            "project"
        } else {
            "global"
        }
        .into(),
        source_path: source_path.to_string_lossy().to_string(),
        model: string_field(config, "model").unwrap_or_default(),
        model_reasoning_effort: string_field(config, "model_reasoning_effort").unwrap_or_default(),
        approval_policy: approval_policy.unwrap_or_default(),
        sandbox_mode: string_field(config, "sandbox_mode").unwrap_or_default(),
        web_search: web_search_mode
            .as_deref()
            .is_some_and(|mode| mode != "disabled"),
        network_access: config
            .pointer("/sandbox_workspace_write/network_access")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        source: "app-server".into(),
        layers: sanitize_app_server_layers(&result),
        warnings,
        permission_profiles: Vec::new(),
        app_server_available: true,
    })
}

fn parse_permission_profiles(result: &Value) -> Vec<CodexPermissionProfile> {
    result
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|profile| {
            Some(CodexPermissionProfile {
                id: profile.get("id")?.as_str()?.to_string(),
                name: profile.get("id")?.as_str()?.to_string(),
                description: profile
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                allowed: profile
                    .get("allowed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect()
}

fn list_permission_profiles(
    project_root: Option<&Path>,
) -> Result<(Vec<CodexPermissionProfile>, bool), codex_app_server::AppServerError> {
    let cwd = project_root.map(|path| path.to_string_lossy().to_string());
    let mut cursor: Option<String> = None;
    let mut profiles = Vec::new();
    let mut seen = HashSet::new();

    for _ in 0..5 {
        let result = codex_app_server::request(
            "permissionProfile/list",
            json!({
                "cursor": cursor.clone(),
                "limit": 100,
                "cwd": cwd.clone()
            }),
        )?;
        for profile in parse_permission_profiles(&result) {
            if seen.insert(profile.id.clone()) {
                profiles.push(profile);
            }
        }
        cursor = result
            .get("nextCursor")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);
        if cursor.is_none() {
            return Ok((profiles, false));
        }
    }

    Ok((profiles, cursor.is_some()))
}

fn strip_inline_comment(value: &str) -> &str {
    let mut single_quote = false;
    let mut double_quote = false;
    let mut escaped = false;
    for (index, character) in value.char_indices() {
        if double_quote && escaped {
            escaped = false;
            continue;
        }
        if double_quote && character == '\\' {
            escaped = true;
            continue;
        }
        match character {
            '\'' if !double_quote => single_quote = !single_quote,
            '"' if !single_quote => double_quote = !double_quote,
            '#' if !single_quote && !double_quote => return &value[..index],
            _ => {}
        }
    }
    value
}

fn parse_toml_scalar(raw: &str) -> Option<Value> {
    let value = strip_inline_comment(raw).trim();
    if value.eq_ignore_ascii_case("true") {
        return Some(Value::Bool(true));
    }
    if value.eq_ignore_ascii_case("false") {
        return Some(Value::Bool(false));
    }
    if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
        return serde_json::from_str::<String>(value)
            .ok()
            .map(Value::String);
    }
    if value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2 {
        return Some(Value::String(value[1..value.len() - 1].to_string()));
    }
    (!value.is_empty()).then(|| Value::String(value.to_string()))
}

fn parse_relevant_toml(content: &str) -> HashMap<String, Value> {
    let mut section = String::new();
    let mut values = HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') && !trimmed.starts_with("[[") {
            section = trimmed[1..trimmed.len() - 1].trim().to_string();
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, raw_value)) = trimmed.split_once('=') else {
            continue;
        };
        let key = key.trim().trim_matches(['"', '\'']);
        let key_path = if section.is_empty() || key.contains('.') {
            key.to_string()
        } else {
            format!("{section}.{key}")
        };
        if matches!(
            key_path.as_str(),
            "model"
                | "model_reasoning_effort"
                | "approval_policy"
                | "sandbox_mode"
                | "web_search"
                | "sandbox_workspace_write.network_access"
        ) {
            if let Some(value) = parse_toml_scalar(raw_value) {
                values.insert(key_path, value);
            }
        }
    }
    values
}

fn merge_values(target: &mut HashMap<String, Value>, source: &HashMap<String, Value>) {
    for (key, value) in source {
        target.insert(key.clone(), value.clone());
    }
}

fn fallback_layer(kind: &str, path: &Path, values: &HashMap<String, Value>) -> Value {
    let mut config = Map::new();
    for (key, value) in values {
        if key == "sandbox_workspace_write.network_access" {
            config.insert(
                "sandbox_workspace_write".into(),
                json!({ "network_access": value }),
            );
        } else {
            config.insert(key.clone(), value.clone());
        }
    }
    json!({
        "name": { "type": kind, "file": path.to_string_lossy() },
        "version": "file-fallback",
        "config": config,
        "disabledReason": null
    })
}

fn config_file_for_scope(
    project_root: Option<&Path>,
    create_parent: bool,
) -> Result<PathBuf, String> {
    match project_root {
        Some(root) => safe_file_below(root, Path::new(".codex/config.toml"), create_parent),
        // Global Codex configs are commonly symlinked between machines or a
        // dotfiles checkout. The shared writer deliberately preserves that
        // link and atomically replaces its resolved target. Project configs
        // remain contained inside the selected project above.
        None => Ok(crate::utils::codex_paths::codex_home()?.join("config.toml")),
    }
}

fn read_config_values(path: &Path) -> Result<HashMap<String, Value>, String> {
    if !path.exists() {
        return Ok(HashMap::new());
    }
    read_small_utf8(path).map(|content| parse_relevant_toml(&content))
}

fn fallback_control_snapshot(
    project_root: Option<&Path>,
    initial_warning: Option<String>,
) -> Result<CodexControlSnapshot, String> {
    let global_path = config_file_for_scope(None, false)?;
    let global_values = read_config_values(&global_path)?;
    let mut effective = global_values.clone();
    let mut layers = Vec::new();
    if global_path.exists() {
        layers.push(fallback_layer("user", &global_path, &global_values));
    }

    if let Some(root) = project_root {
        let project_path = config_file_for_scope(Some(root), false)?;
        let project_values = read_config_values(&project_path)?;
        merge_values(&mut effective, &project_values);
        if project_path.exists() {
            layers.push(fallback_layer("project", &project_path, &project_values));
        }
    }

    let as_string = |key: &str| {
        effective
            .get(key)
            .and_then(Value::as_str)
            .map(ToString::to_string)
    };
    let web_search_mode = as_string("web_search");
    let mut warnings: Vec<String> = initial_warning.into_iter().collect();
    warnings.push(
        "The config-file fallback cannot report managed configuration, trust decisions, or permission profiles."
            .into(),
    );
    if web_search_mode.as_deref() == Some("cached") {
        warnings.push(
            "Web search is set to 'cached'. The toggle shows it as enabled; saving it after toggling will use 'live' or 'disabled'."
                .into(),
        );
    } else if let Some(mode) = web_search_mode.as_deref() {
        if !matches!(mode, "disabled" | "indexed" | "live") {
            warnings.push(format!(
                "Web search uses the custom mode '{mode}'. The toggle represents every non-disabled mode as enabled."
            ));
        }
    }
    let source_path = config_file_for_scope(project_root, false)?;
    Ok(CodexControlSnapshot {
        scope: if project_root.is_some() {
            "project"
        } else {
            "global"
        }
        .into(),
        source_path: source_path.to_string_lossy().to_string(),
        model: as_string("model").unwrap_or_default(),
        model_reasoning_effort: as_string("model_reasoning_effort").unwrap_or_default(),
        approval_policy: as_string("approval_policy").unwrap_or_default(),
        sandbox_mode: as_string("sandbox_mode").unwrap_or_default(),
        web_search: web_search_mode
            .as_deref()
            .is_some_and(|mode| mode != "disabled"),
        network_access: effective
            .get("sandbox_workspace_write.network_access")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        source: "file-fallback".into(),
        layers,
        warnings,
        permission_profiles: Vec::new(),
        app_server_available: false,
    })
}

fn get_codex_control_snapshot_impl(
    project_path: Option<String>,
) -> Result<CodexControlSnapshot, String> {
    let project_root = project_path
        .as_deref()
        .map(canonical_project_root)
        .transpose()?;
    let config_params = json!({
        "includeLayers": true,
        "cwd": project_root.as_ref().map(|path| path.to_string_lossy().to_string())
    });

    match codex_app_server::request("config/read", config_params) {
        Ok(result) => {
            let mut snapshot = match snapshot_from_app_server(result, project_root.as_deref()) {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    return fallback_control_snapshot(
                        project_root.as_deref(),
                        Some(format!(
                            "Using config-file fallback because the App Server response was incompatible: {error}."
                        )),
                    )
                }
            };
            match list_permission_profiles(project_root.as_deref()) {
                Ok((profiles, truncated)) => {
                    snapshot.permission_profiles = profiles;
                    if truncated {
                        snapshot.warnings.push(
                            "The permission profile list exceeded 500 entries and was truncated."
                                .into(),
                        );
                    }
                }
                Err(error) => snapshot.warnings.push(format!(
                    "Permission profiles are unavailable in this Codex version: {error}"
                )),
            }
            Ok(snapshot)
        }
        Err(error) => fallback_control_snapshot(
            project_root.as_deref(),
            Some(format!("Using config-file fallback because {error}.")),
        ),
    }
}

#[tauri::command]
pub async fn get_codex_control_snapshot(
    project_path: Option<String>,
) -> Result<CodexControlSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || get_codex_control_snapshot_impl(project_path))
        .await
        .map_err(|error| format!("Codex control task failed: {error}"))?
}

fn validate_model(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 128 {
        return Err("Model must be between 1 and 128 characters".into());
    }
    if value.chars().any(|character| {
        character.is_control() || character.is_whitespace() || !character.is_ascii()
    }) {
        return Err("Model may contain only non-whitespace ASCII characters".into());
    }
    Ok(value.to_string())
}

fn validate_reasoning_effort(value: &str) -> Result<String, String> {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty() || value.len() > 64 {
        return Err("Reasoning effort must be between 1 and 64 characters".into());
    }
    if value.chars().any(|character| {
        character.is_control() || character.is_whitespace() || !character.is_ascii()
    }) {
        return Err("Reasoning effort may contain only non-whitespace ASCII characters".into());
    }
    Ok(value)
}

fn validate_choice(value: &str, label: &str, allowed: &[&str]) -> Result<String, String> {
    let value = value.trim().to_ascii_lowercase();
    if allowed.contains(&value.as_str()) {
        Ok(value)
    } else {
        Err(format!(
            "Unsupported {label} '{value}'. Expected one of: {}",
            allowed.join(", ")
        ))
    }
}

fn edits_from_updates(updates: CodexControlUpdates) -> Result<Vec<ConfigEdit>, String> {
    let mut edits = Vec::new();
    if let Some(model) = updates.model {
        edits.push(ConfigEdit {
            key_path: "model",
            value: Value::String(validate_model(&model)?),
        });
    }
    if let Some(effort) = updates.model_reasoning_effort {
        edits.push(ConfigEdit {
            key_path: "model_reasoning_effort",
            value: Value::String(validate_reasoning_effort(&effort)?),
        });
    }
    if let Some(policy) = updates.approval_policy {
        edits.push(ConfigEdit {
            key_path: "approval_policy",
            value: Value::String(validate_choice(
                &policy,
                "approval policy",
                &["untrusted", "on-request", "never"],
            )?),
        });
    }
    if let Some(mode) = updates.sandbox_mode {
        edits.push(ConfigEdit {
            key_path: "sandbox_mode",
            value: Value::String(validate_choice(
                &mode,
                "sandbox mode",
                &["read-only", "workspace-write", "danger-full-access"],
            )?),
        });
    }
    if let Some(enabled) = updates.web_search {
        edits.push(ConfigEdit {
            key_path: "web_search",
            value: Value::String(if enabled { "live" } else { "disabled" }.into()),
        });
    }
    if let Some(network) = updates.network_access {
        edits.push(ConfigEdit {
            key_path: "sandbox_workspace_write.network_access",
            value: Value::Bool(network),
        });
    }
    if edits.is_empty() {
        return Err("At least one control update is required".into());
    }
    Ok(edits)
}

fn render_toml_scalar(value: &Value) -> Result<String, String> {
    match value {
        Value::String(value) => serde_json::to_string(value).map_err(|error| error.to_string()),
        Value::Bool(value) => Ok(value.to_string()),
        _ => Err("Only string and boolean control values can be written".into()),
    }
}

fn assignment_key(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') {
        return None;
    }
    let (key, _) = trimmed.split_once('=')?;
    Some(key.trim().trim_matches(['"', '\'']).to_string())
}

fn section_name(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.starts_with('[') && trimmed.ends_with(']') && !trimmed.starts_with("[[") {
        Some(trimmed[1..trimmed.len() - 1].trim().to_string())
    } else {
        None
    }
}

fn comment_offset(raw_value: &str) -> Option<usize> {
    let mut single_quote = false;
    let mut double_quote = false;
    let mut escaped = false;
    for (index, character) in raw_value.char_indices() {
        if double_quote && escaped {
            escaped = false;
            continue;
        }
        if double_quote && character == '\\' {
            escaped = true;
            continue;
        }
        match character {
            '\'' if !double_quote => single_quote = !single_quote,
            '"' if !single_quote => double_quote = !double_quote,
            '#' if !single_quote && !double_quote => return Some(index),
            _ => {}
        }
    }
    None
}

fn replace_assignment_value(line: &str, rendered: &str) -> Result<String, String> {
    let equals = line
        .find('=')
        .ok_or_else(|| "Target config assignment has no equals sign".to_string())?;
    let prefix = &line[..=equals];
    let raw_value = &line[equals + 1..];
    let leading_len = raw_value.len() - raw_value.trim_start().len();
    let leading = &raw_value[..leading_len];
    let comment = comment_offset(raw_value)
        .map(|offset| raw_value[offset..].trim_start())
        .unwrap_or("");
    if comment.is_empty() {
        Ok(format!("{prefix}{leading}{rendered}"))
    } else {
        Ok(format!("{prefix}{leading}{rendered}  {comment}"))
    }
}

fn apply_one_toml_edit(lines: &mut Vec<String>, edit: &ConfigEdit) -> Result<(), String> {
    let (target_section, target_key) = edit
        .key_path
        .rsplit_once('.')
        .map(|(section, key)| (section, key))
        .unwrap_or(("", edit.key_path));
    let rendered = render_toml_scalar(&edit.value)?;
    let mut section = String::new();
    let mut matches = Vec::new();

    for (index, line) in lines.iter().enumerate() {
        if let Some(next_section) = section_name(line) {
            section = next_section;
            continue;
        }
        let Some(key) = assignment_key(line) else {
            continue;
        };
        let direct_dotted_match = section.is_empty() && key == edit.key_path;
        if direct_dotted_match || (section == target_section && key == target_key) {
            matches.push(index);
        }
    }
    if matches.len() > 1 {
        return Err(format!(
            "Config contains duplicate assignments for {}",
            edit.key_path
        ));
    }
    if let Some(index) = matches.first().copied() {
        lines[index] = replace_assignment_value(&lines[index], &rendered)?;
        return Ok(());
    }

    if target_section.is_empty() {
        let insert_at = lines
            .iter()
            .position(|line| section_name(line).is_some())
            .unwrap_or(lines.len());
        lines.insert(insert_at, format!("{target_key} = {rendered}"));
        return Ok(());
    }

    let section_start = lines
        .iter()
        .position(|line| section_name(line).as_deref() == Some(target_section));
    if let Some(start) = section_start {
        let insert_at = lines[start + 1..]
            .iter()
            .position(|line| section_name(line).is_some())
            .map(|offset| start + 1 + offset)
            .unwrap_or(lines.len());
        lines.insert(insert_at, format!("{target_key} = {rendered}"));
    } else {
        if !lines.is_empty() && !lines.last().is_some_and(|line| line.is_empty()) {
            lines.push(String::new());
        }
        lines.push(format!("[{target_section}]"));
        lines.push(format!("{target_key} = {rendered}"));
    }
    Ok(())
}

fn apply_comment_preserving_edits(content: &str, edits: &[ConfigEdit]) -> Result<String, String> {
    let newline = if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let normalized = content.replace("\r\n", "\n");
    let had_final_newline = normalized.ends_with('\n');
    let mut lines: Vec<String> = normalized
        .strip_suffix('\n')
        .unwrap_or(&normalized)
        .split('\n')
        .map(ToString::to_string)
        .collect();
    if content.is_empty() {
        lines.clear();
    }
    for edit in edits {
        apply_one_toml_edit(&mut lines, edit)?;
    }
    let mut output = lines.join(newline);
    if had_final_newline || !output.is_empty() {
        output.push_str(newline);
    }
    Ok(output)
}

fn write_config_fallback(path: &Path, edits: &[ConfigEdit]) -> Result<(), String> {
    let current = if path.exists() {
        read_small_utf8(path)?
    } else {
        String::new()
    };
    if !current.trim().is_empty() {
        current
            .parse::<DocumentMut>()
            .map_err(|error| format!("Refusing to update malformed Codex config: {error}"))?;
    }
    let updated = apply_comment_preserving_edits(&current, edits)?;
    updated
        .parse::<DocumentMut>()
        .map_err(|error| format!("Generated Codex config is invalid: {error}"))?;
    crate::commands::codex::write_codex_config_file(path, &updated)
}

fn compact_server_message(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(400)
        .collect()
}

fn config_write_outcome(result: &Value) -> Result<ConfigWriteOutcome, String> {
    if result.get("filePath").and_then(Value::as_str).is_none()
        || result.get("version").and_then(Value::as_str).is_none()
    {
        return Err(
            "App Server config/batchWrite response has invalid filePath or version fields".into(),
        );
    }
    match result.get("status").and_then(Value::as_str) {
        Some("ok") => Ok(ConfigWriteOutcome::default()),
        Some("okOverridden") => {
            let detail = result
                .pointer("/overriddenMetadata/message")
                .and_then(Value::as_str)
                .map(compact_server_message)
                .filter(|message| !message.is_empty());
            let mut warning =
                "Codex saved the file, but a higher-priority configuration overrides the effective value."
                    .to_string();
            if let Some(detail) = detail {
                warning.push(' ');
                warning.push_str(&detail);
            }
            Ok(ConfigWriteOutcome {
                warning: Some(warning),
            })
        }
        Some(status) => Err(format!(
            "App Server config/batchWrite returned unsupported status '{status}'"
        )),
        None => Err("App Server config/batchWrite response has no status field".into()),
    }
}

fn write_config_edits(
    project_root: Option<&Path>,
    edits: &[ConfigEdit],
) -> Result<ConfigWriteOutcome, String> {
    let target = config_file_for_scope(project_root, false)?;
    let app_edits: Vec<Value> = edits
        .iter()
        .map(|edit| {
            json!({
                "keyPath": edit.key_path,
                "value": edit.value,
                "mergeStrategy": "replace"
            })
        })
        .collect();
    let mut params = json!({
        "edits": app_edits,
        "expectedVersion": null,
        "reloadUserConfig": true
    });
    if project_root.is_some() {
        params["filePath"] = Value::String(target.to_string_lossy().to_string());
    }

    match codex_app_server::request("config/batchWrite", params) {
        Ok(result) => config_write_outcome(&result),
        Err(error) if error.permits_file_fallback() => {
            let safe_target = config_file_for_scope(project_root, true)?;
            write_config_fallback(&safe_target, edits)?;
            Ok(ConfigWriteOutcome::default())
        }
        Err(error) => Err(error.to_string()),
    }
}

fn update_codex_control_impl(
    updates: CodexControlUpdates,
    project_path: Option<String>,
) -> Result<CodexControlSnapshot, String> {
    let edits = edits_from_updates(updates)?;
    let project_root = project_path
        .as_deref()
        .map(canonical_project_root)
        .transpose()?;
    let outcome = write_config_edits(project_root.as_deref(), &edits)?;
    let mut snapshot = get_codex_control_snapshot_impl(project_path)?;
    if let Some(warning) = outcome.warning {
        snapshot.warnings.insert(0, warning);
    }
    Ok(snapshot)
}

#[tauri::command]
pub async fn update_codex_control(
    updates: CodexControlUpdates,
    project_path: Option<String>,
) -> Result<CodexControlSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || update_codex_control_impl(updates, project_path))
        .await
        .map_err(|error| format!("Codex control task failed: {error}"))?
}

fn model_from_app_server(value: &Value) -> Option<CodexModelInfo> {
    let id = value.get("id")?.as_str()?.to_string();
    let model = value
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or(&id)
        .to_string();
    let supported_reasoning_efforts: Vec<CodexReasoningEffortOption> = value
        .get("supportedReasoningEfforts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|effort| {
            Some(CodexReasoningEffortOption {
                reasoning_effort: effort.get("reasoningEffort")?.as_str()?.to_string(),
                description: effort
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            })
        })
        .collect();
    let reasoning_efforts = supported_reasoning_efforts
        .iter()
        .map(|effort| effort.reasoning_effort.clone())
        .collect();
    let display_name = value
        .get("displayName")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .unwrap_or_else(|| id.clone());
    Some(CodexModelInfo {
        id,
        model,
        display_name,
        description: value
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        hidden: value
            .get("hidden")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        supported_reasoning_efforts,
        reasoning_efforts,
        default_reasoning_effort: value
            .get("defaultReasoningEffort")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        input_modalities: value
            .get("inputModalities")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(ToString::to_string)
            .collect(),
        supports_personality: value
            .get("supportsPersonality")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_default: value
            .get("isDefault")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

#[derive(Debug, Deserialize, Default)]
struct CachedModels {
    #[serde(default)]
    models: Vec<CachedModel>,
}

#[derive(Debug, Deserialize, Default)]
struct CachedModel {
    slug: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    visibility: Option<String>,
    #[serde(default)]
    supported_reasoning_levels: Vec<CachedReasoningEffort>,
    #[serde(default)]
    default_reasoning_level: Option<String>,
    #[serde(default)]
    input_modalities: Vec<String>,
    #[serde(default)]
    supports_personality: bool,
}

#[derive(Debug, Deserialize, Default)]
struct CachedReasoningEffort {
    effort: String,
    #[serde(default)]
    description: String,
}

fn cached_models(include_hidden: bool) -> Result<Vec<CodexModelInfo>, String> {
    let path = safe_file_below(
        &crate::utils::codex_paths::codex_home()?,
        Path::new("models_cache.json"),
        false,
    )?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let parsed: CachedModels = serde_json::from_str(&read_small_utf8(&path)?)
        .map_err(|error| format!("Could not parse {}: {error}", path.display()))?;
    let configured_model = config_file_for_scope(None, false)
        .ok()
        .and_then(|path| read_config_values(&path).ok())
        .and_then(|values| {
            values
                .get("model")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        });

    Ok(parsed
        .models
        .into_iter()
        .filter_map(|model| {
            let hidden = matches!(
                model.visibility.as_deref(),
                Some("hidden") | Some("hide") | Some("internal")
            );
            if hidden && !include_hidden {
                return None;
            }
            let efforts: Vec<CodexReasoningEffortOption> = model
                .supported_reasoning_levels
                .into_iter()
                .map(|effort| CodexReasoningEffortOption {
                    reasoning_effort: effort.effort,
                    description: effort.description,
                })
                .collect();
            let reasoning_efforts = efforts
                .iter()
                .map(|effort| effort.reasoning_effort.clone())
                .collect();
            let default_reasoning_effort =
                model.default_reasoning_level.clone().unwrap_or_default();
            let is_default = configured_model.as_deref() == Some(model.slug.as_str());
            Some(CodexModelInfo {
                id: model.slug.clone(),
                model: model.slug.clone(),
                display_name: model.display_name.unwrap_or(model.slug),
                description: model.description.unwrap_or_default(),
                hidden,
                supported_reasoning_efforts: efforts,
                reasoning_efforts,
                default_reasoning_effort,
                input_modalities: model.input_modalities,
                supports_personality: model.supports_personality,
                is_default,
            })
        })
        .collect())
}

fn configured_model_settings_from_file() -> (Option<String>, Option<String>) {
    let values = config_file_for_scope(None, false)
        .ok()
        .and_then(|path| read_config_values(&path).ok())
        .unwrap_or_default();
    (
        values
            .get("model")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        values
            .get("model_reasoning_effort")
            .and_then(Value::as_str)
            .map(ToString::to_string),
    )
}

fn configured_model_settings() -> (Option<String>, Option<String>) {
    if let Ok(result) = codex_app_server::request(
        "config/read",
        json!({ "includeLayers": false, "cwd": null }),
    ) {
        if let Some(config) = result.get("config") {
            return (
                string_field(config, "model"),
                string_field(config, "model_reasoning_effort"),
            );
        }
    }
    configured_model_settings_from_file()
}

fn apply_model_settings(
    models: &mut [CodexModelInfo],
    configured_model: Option<String>,
    configured_effort: Option<String>,
) {
    let Some(configured_model) = configured_model else {
        return;
    };
    for model in models {
        model.is_default = model.id == configured_model || model.model == configured_model;
        if model.is_default {
            if let Some(effort) = configured_effort.as_deref() {
                if model
                    .supported_reasoning_efforts
                    .iter()
                    .any(|option| option.reasoning_effort == effort)
                {
                    model.default_reasoning_effort = effort.to_string();
                }
            }
        }
    }
}

fn fallback_model_list(
    include_hidden: bool,
    reason: impl Into<String>,
) -> Result<CodexModelList, String> {
    let reason = reason.into();
    match cached_models(include_hidden) {
        Ok(mut models) => {
            let (configured_model, configured_effort) = configured_model_settings_from_file();
            apply_model_settings(
                &mut models,
                configured_model.clone(),
                configured_effort.clone(),
            );
            Ok(CodexModelList {
                models,
                app_server_available: false,
                configured_model,
                configured_reasoning_effort: configured_effort,
                warning: Some(format!(
                    "Using the local Codex model cache because {reason}."
                )),
            })
        }
        Err(cache_error) => {
            let (configured_model, configured_effort) = configured_model_settings_from_file();
            Ok(CodexModelList {
                models: Vec::new(),
                app_server_available: false,
                configured_model,
                configured_reasoning_effort: configured_effort,
                warning: Some(format!(
                    "The Codex model catalog is unavailable because {reason}. The local cache also failed: {cache_error}"
                )),
            })
        }
    }
}

fn list_codex_models_impl(include_hidden: bool) -> Result<CodexModelList, String> {
    let mut cursor: Option<String> = None;
    let mut models = Vec::new();
    for _ in 0..5 {
        let params = json!({
            "cursor": cursor,
            "limit": 100,
            "includeHidden": include_hidden
        });
        let result = match codex_app_server::request("model/list", params) {
            Ok(result) => result,
            Err(error) => return fallback_model_list(include_hidden, error.to_string()),
        };
        let Some(data) = result.get("data").and_then(Value::as_array) else {
            return fallback_model_list(
                include_hidden,
                "the App Server model/list response had no data array",
            );
        };
        models.extend(data.iter().filter_map(model_from_app_server));
        cursor = result
            .get("nextCursor")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        if cursor.is_none() {
            break;
        }
    }
    let (configured_model, configured_effort) = configured_model_settings();
    apply_model_settings(
        &mut models,
        configured_model.clone(),
        configured_effort.clone(),
    );
    Ok(CodexModelList {
        models,
        app_server_available: true,
        configured_model,
        configured_reasoning_effort: configured_effort,
        warning: cursor
            .is_some()
            .then(|| "The Codex model catalog exceeded 500 entries and was truncated.".to_string()),
    })
}

#[tauri::command]
pub async fn list_codex_models() -> Result<CodexModelList, String> {
    tauri::async_runtime::spawn_blocking(|| list_codex_models_impl(false))
        .await
        .map_err(|error| format!("Codex model task failed: {error}"))?
}

fn update_codex_model_settings_impl(
    model: String,
    reasoning_effort: String,
) -> Result<CodexModelUpdateResult, String> {
    let model = validate_model(&model)?;
    let reasoning_effort = validate_reasoning_effort(&reasoning_effort)?;
    let catalog = list_codex_models_impl(true)?;
    let mut configured_model = model.clone();
    if !catalog.models.is_empty() {
        let selected = catalog
            .models
            .iter()
            .find(|entry| entry.id == model || entry.model == model)
            .ok_or_else(|| format!("Model '{model}' is not in the installed Codex catalog"))?;
        if !selected.reasoning_efforts.is_empty()
            && !selected
                .reasoning_efforts
                .iter()
                .any(|effort| effort == &reasoning_effort)
        {
            return Err(format!(
                "Model '{}' does not support reasoning effort '{}'",
                selected.display_name, reasoning_effort
            ));
        }
        configured_model = selected.model.clone();
    }
    let edits = vec![
        ConfigEdit {
            key_path: "model",
            value: Value::String(configured_model),
        },
        ConfigEdit {
            key_path: "model_reasoning_effort",
            value: Value::String(reasoning_effort.clone()),
        },
    ];
    let outcome = write_config_edits(None, &edits)?;
    let (configured_model, configured_reasoning_effort) = configured_model_settings();
    Ok(CodexModelUpdateResult {
        configured_model,
        configured_reasoning_effort,
        warning: outcome.warning,
    })
}

#[tauri::command]
pub async fn update_codex_model_settings(
    model: String,
    reasoning_effort: String,
) -> Result<CodexModelUpdateResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        update_codex_model_settings_impl(model, reasoning_effort)
    })
    .await
    .map_err(|error| format!("Codex model task failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_config_write_without_override() {
        let outcome = config_write_outcome(&json!({
            "filePath": "/tmp/config.toml",
            "version": "v2",
            "status": "ok",
            "overriddenMetadata": null
        }))
        .unwrap();
        assert_eq!(outcome, ConfigWriteOutcome::default());
    }

    #[test]
    fn preserves_config_override_warning() {
        let outcome = config_write_outcome(&json!({
            "filePath": "/tmp/config.toml",
            "version": "v2",
            "status": "okOverridden",
            "overriddenMetadata": {
                "message": "Managed policy\nkeeps the effective model."
            }
        }))
        .unwrap();
        let warning = outcome.warning.unwrap();
        assert!(warning.contains("higher-priority configuration"));
        assert!(warning.contains("Managed policy keeps the effective model."));
        assert!(!warning.contains('\n'));
    }

    #[test]
    fn rejects_incompatible_config_write_response() {
        assert!(config_write_outcome(&json!({})).is_err());
        assert!(config_write_outcome(&json!({
            "filePath": "/tmp/config.toml",
            "version": "v2",
            "status": "futureStatus"
        }))
        .is_err());
    }

    #[test]
    fn parses_and_layers_relevant_config_values() {
        let global = parse_relevant_toml(
            r#"
model = "gpt-global"
approval_policy = "on-request"
[sandbox_workspace_write]
network_access = false
"#,
        );
        let project = parse_relevant_toml(
            r#"
model = "gpt-project"
web_search = "live"
"#,
        );
        let mut effective = global;
        merge_values(&mut effective, &project);
        assert_eq!(effective.get("model"), Some(&json!("gpt-project")));
        assert_eq!(effective.get("approval_policy"), Some(&json!("on-request")));
        assert_eq!(
            effective.get("sandbox_workspace_write.network_access"),
            Some(&json!(false))
        );
        assert_eq!(effective.get("web_search"), Some(&json!("live")));
    }

    #[test]
    fn comment_preserving_update_keeps_unrelated_content() {
        let input = r#"# keep this comment
model = "old" # keep inline
unknown = "untouched"

[mcp_servers.demo]
command = "demo"
"#;
        let output = apply_comment_preserving_edits(
            input,
            &[
                ConfigEdit {
                    key_path: "model",
                    value: json!("new"),
                },
                ConfigEdit {
                    key_path: "approval_policy",
                    value: json!("never"),
                },
                ConfigEdit {
                    key_path: "sandbox_workspace_write.network_access",
                    value: json!(true),
                },
            ],
        )
        .unwrap();
        assert!(output.contains("# keep this comment"));
        assert!(output.contains("model = \"new\"  # keep inline"));
        assert!(output.contains("unknown = \"untouched\""));
        assert!(output.contains("approval_policy = \"never\""));
        assert!(output.contains("[sandbox_workspace_write]\nnetwork_access = true"));
        assert!(output.contains("[mcp_servers.demo]\ncommand = \"demo\""));
    }

    #[test]
    fn comment_preserving_update_rejects_duplicate_target_keys() {
        let error = apply_comment_preserving_edits(
            "model = \"one\"\nmodel = \"two\"\n",
            &[ConfigEdit {
                key_path: "model",
                value: json!("three"),
            }],
        )
        .unwrap_err();
        assert!(error.contains("duplicate assignments"));
    }

    #[test]
    fn parses_app_server_model_shape() {
        let model = model_from_app_server(&json!({
            "id": "gpt-test",
            "model": "gpt-test",
            "displayName": "GPT Test",
            "description": "Test model",
            "hidden": false,
            "supportedReasoningEfforts": [
                { "reasoningEffort": "low", "description": "Fast" },
                { "reasoningEffort": "high", "description": "Deep" }
            ],
            "defaultReasoningEffort": "low",
            "inputModalities": ["text", "image"],
            "supportsPersonality": true,
            "isDefault": true
        }))
        .unwrap();
        assert_eq!(model.id, "gpt-test");
        assert_eq!(model.reasoning_efforts, vec!["low", "high"]);
        assert!(model.is_default);
    }

    #[test]
    fn configured_model_and_effort_override_catalog_defaults() {
        let mut models = vec![model_from_app_server(&json!({
            "id": "gpt-test",
            "model": "gpt-test",
            "supportedReasoningEfforts": [
                { "reasoningEffort": "low" },
                { "reasoningEffort": "high" }
            ],
            "defaultReasoningEffort": "low",
            "isDefault": false
        }))
        .unwrap()];
        apply_model_settings(&mut models, Some("gpt-test".into()), Some("high".into()));
        assert!(models[0].is_default);
        assert_eq!(models[0].default_reasoning_effort, "high");
    }

    #[test]
    fn parses_permission_profiles_defensively() {
        let profiles = parse_permission_profiles(&json!({
            "data": [
                { "id": "read-only", "description": "Read files", "allowed": true },
                { "description": "missing id", "allowed": true }
            ]
        }));
        assert_eq!(
            profiles,
            vec![CodexPermissionProfile {
                id: "read-only".into(),
                name: "read-only".into(),
                description: "Read files".into(),
                allowed: true
            }]
        );
    }

    #[test]
    fn serializes_frontend_contracts_in_camel_case() {
        let snapshot = CodexControlSnapshot {
            scope: "global".into(),
            source_path: "/tmp/config.toml".into(),
            model: "gpt-test".into(),
            model_reasoning_effort: "high".into(),
            approval_policy: "on-request".into(),
            sandbox_mode: "workspace-write".into(),
            web_search: true,
            network_access: false,
            source: "app-server".into(),
            layers: Vec::new(),
            warnings: Vec::new(),
            permission_profiles: Vec::new(),
            app_server_available: true,
        };
        let value = serde_json::to_value(snapshot).unwrap();
        assert_eq!(value["sourcePath"], json!("/tmp/config.toml"));
        assert_eq!(value["modelReasoningEffort"], json!("high"));
        assert_eq!(value["webSearch"], json!(true));
        assert_eq!(value["appServerAvailable"], json!(true));
        assert!(value.get("source_path").is_none());

        let models = CodexModelList {
            models: vec![model_from_app_server(&json!({
                "id": "gpt-test",
                "supportedReasoningEfforts": [
                    { "reasoningEffort": "high", "description": "Deep" }
                ]
            }))
            .unwrap()],
            app_server_available: false,
            configured_model: Some("gpt-test".into()),
            configured_reasoning_effort: Some("high".into()),
            warning: Some("fallback".into()),
        };
        let value = serde_json::to_value(models).unwrap();
        assert_eq!(value["appServerAvailable"], json!(false));
        assert_eq!(value["configuredModel"], json!("gpt-test"));
        assert_eq!(value["configuredReasoningEffort"], json!("high"));
        assert_eq!(
            value["models"][0]["supportedReasoningEfforts"][0]["reasoningEffort"],
            json!("high")
        );

        let updates: CodexControlUpdates = serde_json::from_value(json!({
            "approvalPolicy": "never",
            "sandboxMode": "danger-full-access",
            "webSearch": false,
            "networkAccess": true
        }))
        .unwrap();
        assert_eq!(updates.approval_policy.as_deref(), Some("never"));
        assert_eq!(updates.web_search, Some(false));
        assert_eq!(updates.network_access, Some(true));
    }

    #[test]
    fn instruction_config_uses_fallback_names_and_byte_limit() {
        let mut settings = InstructionDiscoveryConfig::default();
        apply_instruction_config_toml(
            "project_doc_fallback_filenames = [\"TEAM.md\", \"../escape.md\"]\nproject_doc_max_bytes = 3\n",
            &mut settings,
        )
        .unwrap();
        assert_eq!(settings.fallback_filenames, vec!["TEAM.md"]);
        assert_eq!(settings.max_bytes, 3);

        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("AGENTS.override.md"), "  \n").unwrap();
        fs::write(directory.path().join("AGENTS.md"), "").unwrap();
        fs::write(directory.path().join("TEAM.md"), "abcdef").unwrap();
        let source = first_non_empty_instruction_at(
            directory.path(),
            &settings.fallback_filenames,
            settings.max_bytes,
            "project",
            "projectOverride",
        )
        .unwrap()
        .unwrap();
        assert_eq!(Path::new(&source.path).file_name().unwrap(), "TEAM.md");
        assert_eq!(source.kind, "projectFallback");
        assert_eq!(source.byte_len, 3);
        assert!(source.loaded);
        assert!(source.truncated);

        let serialized = serde_json::to_value(source).unwrap();
        assert_eq!(serialized["loaded"], json!(true));
        assert_eq!(serialized["truncated"], json!(true));
        assert!(serialized.get("content").is_none());

        let mut oversized = InstructionDiscoveryConfig::default();
        apply_instruction_config_toml("project_doc_max_bytes = 999999999\n", &mut oversized)
            .unwrap();
        assert_eq!(oversized.max_bytes, MAX_TEXT_FILE_BYTES as usize);
        apply_instruction_config_json(
            &json!({ "project_doc_max_bytes": 999999999_u64 }),
            &mut oversized,
        );
        assert_eq!(oversized.max_bytes, MAX_TEXT_FILE_BYTES as usize);
    }

    #[test]
    fn instruction_revision_tracks_content_and_missing_state() {
        let existing = instruction_revision(true, "same");
        assert_ne!(existing, instruction_revision(true, "changed"));
        assert_ne!(existing, instruction_revision(false, "same"));
        assert_eq!(existing.len(), 64);
    }

    #[test]
    fn web_search_boolean_writes_documented_modes() {
        let enabled = edits_from_updates(CodexControlUpdates {
            web_search: Some(true),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(enabled[0].value, json!("live"));
        let disabled = edits_from_updates(CodexControlUpdates {
            web_search: Some(false),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(disabled[0].value, json!("disabled"));
    }

    #[test]
    fn validates_control_enums() {
        assert!(validate_choice(
            "workspace-write",
            "sandbox mode",
            &["read-only", "workspace-write", "danger-full-access"]
        )
        .is_ok());
        assert!(validate_choice(
            "everything",
            "sandbox mode",
            &["read-only", "workspace-write", "danger-full-access"]
        )
        .is_err());
        assert!(validate_reasoning_effort("xhigh").is_ok());
        assert_eq!(
            validate_reasoning_effort("future-tier").unwrap(),
            "future-tier"
        );
        assert!(validate_reasoning_effort("not valid").is_err());
    }

    #[test]
    fn safe_file_rejects_parent_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let error = safe_file_below(dir.path(), Path::new("../outside"), false).unwrap_err();
        assert!(error.contains("escapes"));
    }

    #[cfg(unix)]
    #[test]
    fn safe_file_rejects_symlinked_target() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        symlink(outside.path(), dir.path().join("AGENTS.md")).unwrap();
        let error = safe_file_below(dir.path(), Path::new("AGENTS.md"), false).unwrap_err();
        assert!(error.contains("symbolic link"));
    }

    #[cfg(unix)]
    #[test]
    fn project_config_rejects_symlinked_parent_escape() {
        use std::os::unix::fs::symlink;

        let project = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        symlink(outside.path(), project.path().join(".codex")).unwrap();
        let error =
            safe_file_below(project.path(), Path::new(".codex/config.toml"), false).unwrap_err();
        assert!(error.contains("outside"));
    }

    #[cfg(unix)]
    #[test]
    fn new_fallback_config_is_private() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        write_config_fallback(
            &path,
            &[ConfigEdit {
                key_path: "model",
                value: json!("gpt-test"),
            }],
        )
        .unwrap();
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
