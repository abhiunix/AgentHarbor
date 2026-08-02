use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageData {
    pub input_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    // V2 fields
    pub cache_ephemeral_1h_tokens: Option<u64>,
    pub cache_ephemeral_5m_tokens: Option<u64>,
    pub web_search_requests: Option<u64>,
    pub web_fetch_requests: Option<u64>,
    pub service_tier: Option<String>,
    pub inference_geo: Option<String>,
    pub speed: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectUsageRecord {
    pub uuid: String,
    pub timestamp: String,
    pub model: Option<String>,
    pub usage: Option<UsageData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    // V2 fields
    pub session_id: Option<String>,
    pub git_branch: Option<String>,
    pub tools_used: Vec<String>,
    pub has_thinking: bool,
    pub message_type: Option<String>,
    pub claude_version: Option<String>,
}

fn parse_usage_from_value(v: &serde_json::Value) -> Option<UsageData> {
    let obj = v.as_object()?;
    // Parse cache_creation sub-object for ephemeral tokens
    let cache_creation = obj.get("cache_creation");
    let ephemeral_1h = cache_creation
        .and_then(|c| c.get("ephemeral_1h_input_tokens"))
        .and_then(|x| x.as_u64());
    let ephemeral_5m = cache_creation
        .and_then(|c| c.get("ephemeral_5m_input_tokens"))
        .and_then(|x| x.as_u64());

    // Parse server_tool_use
    let server_tool_use = obj.get("server_tool_use");
    let web_search = server_tool_use
        .and_then(|s| s.get("web_search_requests"))
        .and_then(|x| x.as_u64());
    let web_fetch = server_tool_use
        .and_then(|s| s.get("web_fetch_requests"))
        .and_then(|x| x.as_u64());

    Some(UsageData {
        input_tokens: obj.get("input_tokens").and_then(|x| x.as_u64()),
        cache_read_input_tokens: obj.get("cache_read_input_tokens").and_then(|x| x.as_u64()),
        cache_creation_input_tokens: obj.get("cache_creation_input_tokens").and_then(|x| x.as_u64()),
        output_tokens: obj.get("output_tokens").and_then(|x| x.as_u64()),
        cache_ephemeral_1h_tokens: ephemeral_1h,
        cache_ephemeral_5m_tokens: ephemeral_5m,
        web_search_requests: web_search,
        web_fetch_requests: web_fetch,
        service_tier: obj.get("service_tier").and_then(|x| x.as_str()).map(String::from),
        inference_geo: obj.get("inference_geo").and_then(|x| x.as_str()).map(String::from),
        speed: obj.get("speed").and_then(|x| x.as_str()).map(String::from),
    })
}

/// Extract tool names from message.content[] where type == "tool_use"
fn extract_tools(json: &serde_json::Value) -> Vec<String> {
    let content = json
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array());
    let mut tools = Vec::new();
    if let Some(items) = content {
        for item in items {
            if item.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                    tools.push(name.to_string());
                }
            }
        }
    }
    tools
}

/// Check if any content block is type == "thinking"
fn has_thinking(json: &serde_json::Value) -> bool {
    let content = json
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array());
    if let Some(items) = content {
        for item in items {
            if item.get("type").and_then(|t| t.as_str()) == Some("thinking") {
                return true;
            }
        }
    }
    false
}

fn extract_record(line: &str, project_path: Option<String>) -> Option<ProjectUsageRecord> {
    let json: serde_json::Value = serde_json::from_str(line).ok()?;
    let uuid = json.get("uuid").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let timestamp = json.get("timestamp").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if uuid.is_empty() || timestamp.is_empty() {
        return None;
    }

    // Message type
    let message_type = json.get("type").and_then(|v| v.as_str()).map(String::from);

    let model = json
        .get("model")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| {
            json.get("message")
                .and_then(|m| m.get("model"))
                .and_then(|v| v.as_str())
                .map(String::from)
        });

    let usage = json
        .get("usage")
        .and_then(parse_usage_from_value)
        .or_else(|| {
            json.get("message")
                .and_then(|m| m.get("usage"))
                .and_then(parse_usage_from_value)
        });

    // V2 fields — extract even if no usage data (user messages have metadata)
    let session_id = json.get("sessionId").and_then(|v| v.as_str()).map(String::from);
    let git_branch = json.get("gitBranch").and_then(|v| v.as_str()).map(String::from);
    let claude_version = json.get("version").and_then(|v| v.as_str()).map(String::from);
    let tools = extract_tools(&json);
    let thinking = has_thinking(&json);

    // For records with usage data, require non-zero tokens
    if let Some(ref u) = usage {
        let input = u.input_tokens.unwrap_or(0);
        let output = u.output_tokens.unwrap_or(0);
        let cache_read = u.cache_read_input_tokens.unwrap_or(0);
        let cache_write = u.cache_creation_input_tokens.unwrap_or(0);
        if input == 0 && output == 0 && cache_read == 0 && cache_write == 0 {
            return None;
        }
    } else {
        // No usage data — skip unless it's a user message (for prompt counting)
        if message_type.as_deref() != Some("user") {
            return None;
        }
    }

    Some(ProjectUsageRecord {
        uuid,
        timestamp,
        model,
        usage,
        project_path,
        session_id,
        git_branch,
        tools_used: tools,
        has_thinking: thinking,
        message_type,
        claude_version,
    })
}

/// Decode Claude Code project dir name to absolute path.
/// Encoding: path with "/" (or "\") and spaces replaced by "-".
/// Unix: Users-foo-Downloads-proj -> /Users/foo/Downloads/proj
/// Windows: C-Users-foo-proj -> C:\Users\foo\proj (drive letter detection)
pub fn decode_claude_project_path(dir_name: &str) -> String {
    let s = dir_name.trim_start_matches('-');
    if s.is_empty() {
        return String::new();
    }

    #[cfg(target_os = "windows")]
    {
        let parts: Vec<&str> = s.splitn(2, '-').collect();
        if parts.len() == 2
            && parts[0].len() == 1
            && parts[0].chars().all(|c| c.is_ascii_alphabetic())
        {
            return format!("{}:\\{}", parts[0], parts[1].replace('-', "\\"));
        }
        format!("\\{}", s.replace('-', "\\"))
    }

    #[cfg(not(target_os = "windows"))]
    format!("/{}", s.replace('-', "/"))
}

lazy_static::lazy_static! {
    /// Encoded project dir -> real cwd. A dir's cwd never changes, so a permanent
    /// per-process memo is safe and keeps the JSONL probe off the hot path.
    static ref PROJECT_PATH_CACHE: Mutex<HashMap<PathBuf, Option<String>>> =
        Mutex::new(HashMap::new());
}

/// How many leading lines of a transcript to scan before giving up on it: the first
/// lines ("mode", "file-history-snapshot", …) carry no `cwd`.
const CWD_SCAN_MAX_LINES: usize = 50;
/// How many transcripts (newest first) to probe per project dir.
const CWD_SCAN_MAX_FILES: usize = 2;

/// Resolve a Claude Code project dir to the absolute cwd it was recorded for.
///
/// Claude encodes the cwd by replacing every `/` with `-`, which is lossy for folder
/// names that contain dashes. The transcripts inside the dir carry the original `cwd`,
/// so prefer that; fall back to a filesystem-validated decode, then the lossy decode.
pub fn resolve_project_dir_path(encoded_dir: &Path) -> Option<String> {
    if let Ok(cache) = PROJECT_PATH_CACHE.lock() {
        if let Some(hit) = cache.get(encoded_dir) {
            return hit.clone();
        }
    }
    let resolved = compute_project_dir_path(encoded_dir);
    if let Ok(mut cache) = PROJECT_PATH_CACHE.lock() {
        cache.insert(encoded_dir.to_path_buf(), resolved.clone());
    }
    resolved
}

/// Record a cwd observed while streaming a transcript, upgrading a dir that the
/// bounded probe could not resolve. Never overwrites an already-resolved entry.
fn seed_project_dir_path(encoded_dir: &Path, cwd: &str) {
    if cwd.is_empty() {
        return;
    }
    if let Ok(mut cache) = PROJECT_PATH_CACHE.lock() {
        match cache.get(encoded_dir) {
            Some(Some(_)) => {}
            _ => {
                cache.insert(encoded_dir.to_path_buf(), Some(cwd.to_string()));
            }
        }
    }
}

fn compute_project_dir_path(encoded_dir: &Path) -> Option<String> {
    if let Some(cwd) = probe_cwd_from_transcripts(encoded_dir) {
        return Some(cwd);
    }
    let dir_name = encoded_dir.file_name().and_then(|n| n.to_str())?;
    if dir_name.is_empty() {
        return None;
    }
    let fs_decoded = super::transcripts::decode_project_path(dir_name);
    if Path::new(&fs_decoded).is_dir() {
        return Some(fs_decoded);
    }
    let lossy = decode_claude_project_path(dir_name);
    if lossy.is_empty() {
        None
    } else {
        Some(lossy)
    }
}

fn probe_cwd_from_transcripts(dir: &Path) -> Option<String> {
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("jsonl"))
        .filter_map(|e| {
            let modified = e.metadata().ok()?.modified().ok()?;
            Some((modified, e.path()))
        })
        .collect();
    files.sort_by(|a, b| b.0.cmp(&a.0));
    files
        .into_iter()
        .take(CWD_SCAN_MAX_FILES)
        .find_map(|(_, path)| scan_file_for_cwd(&path))
}

fn scan_file_for_cwd(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let reader = BufReader::new(file);
    for line in reader.lines().take(CWD_SCAN_MAX_LINES).map_while(Result::ok) {
        let json: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(cwd) = json.get("cwd").and_then(|v| v.as_str()) {
            if !cwd.is_empty() {
                return Some(cwd.to_string());
            }
        }
    }
    None
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProjectWithMcp {
    pub path: String,
    pub name: String,
}

/// List projects that have MCP presence (from ~/.claude/projects - Claude Code known projects).
#[tauri::command]
pub fn list_projects_with_mcp() -> Result<Vec<ProjectWithMcp>, String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let projects_dir = home.join(".claude").join("projects");
    if !projects_dir.exists() {
        return Ok(vec![]);
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&projects_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let dir_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        if dir_name.is_empty() || dir_name.starts_with('.') {
            continue;
        }
        let decoded = match resolve_project_dir_path(&path) {
            Some(d) if d.len() >= 2 => d,
            _ => continue,
        };
        let name = std::path::Path::new(&decoded)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&dir_name)
            .to_string();
        out.push(ProjectWithMcp {
            path: decoded,
            name,
        });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

#[tauri::command]
pub fn read_project_usage_files() -> Result<Vec<ProjectUsageRecord>, String> {
    read_project_usage_files_with_mtime_floor(None)
}

/// When `min_modified` is `Some(floor)`, skip JSONL files whose metadata `modified` time is before
/// `floor` — same rule as tray Claude stats (`enrich_with_today_stats`) so V2 **`today`** (IST) data
/// matches the menu bar.
pub fn read_project_usage_files_with_mtime_floor(
    min_modified: Option<std::time::SystemTime>,
) -> Result<Vec<ProjectUsageRecord>, String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let projects_dir = home.join(".claude").join("projects");
    if !projects_dir.exists() {
        return Ok(vec![]);
    }

    let mut records = Vec::new();
    for entry in WalkDir::new(&projects_dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "jsonl" {
                    if let Some(floor) = min_modified {
                        let modified_ok = fs::metadata(path)
                            .and_then(|m| m.modified())
                            .map(|t| t >= floor)
                            .unwrap_or(false);
                        if !modified_ok {
                            continue;
                        }
                    }
                    let project_path = path
                        .parent()
                        .and_then(resolve_project_dir_path)
                        .filter(|s| !s.is_empty());
                    if let Err(e) = read_jsonl_file(path, project_path, &mut records) {
                        eprintln!("Warning: failed to read {}: {}", path.display(), e);
                    }
                }
            }
        }
    }
    Ok(records)
}

fn read_jsonl_file(
    path: &std::path::Path,
    project_path: Option<String>,
    records: &mut Vec<ProjectUsageRecord>,
) -> Result<(), String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let reader = BufReader::new(file);
    // Deduplicate streaming chunks per file: Claude sends multiple JSONL lines
    // for a single assistant message (streaming), each with cumulative token counts.
    // We keep only the first occurrence per (message.id, requestId) pair.
    // Matches CodexBar's deduplication logic.
    let mut seen_keys: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut seeded = project_path.is_some();
    for line in reader.lines() {
        let line = line.map_err(|e| e.to_string())?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Check for dedup key before full extraction
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
            // Every line is parsed anyway — seed the path memo at no extra parse cost
            // for dirs whose bounded probe came up empty.
            if !seeded {
                if let (Some(dir), Some(cwd)) =
                    (path.parent(), json.get("cwd").and_then(|v| v.as_str()))
                {
                    seed_project_dir_path(dir, cwd);
                    seeded = true;
                }
            }
            let message_id = json
                .get("message")
                .and_then(|m| m.get("id"))
                .and_then(|v| v.as_str());
            let request_id = json.get("requestId").and_then(|v| v.as_str());
            if let (Some(mid), Some(rid)) = (message_id, request_id) {
                let key = format!("{}:{}", mid, rid);
                if seen_keys.contains(&key) {
                    continue; // Skip duplicate streaming chunk
                }
                seen_keys.insert(key);
            }
            // If either ID is missing (older logs), treat each line as distinct
        }
        if let Some(record) = extract_record(line, project_path.clone()) {
            records.push(record);
        }
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Mirror Claude's encoding: every "/" of the cwd becomes "-".
    fn encode(real: &Path) -> String {
        real.to_string_lossy().replace('/', "-")
    }

    fn make_project_dir(root: &Path, real: &Path, lines: &str) -> PathBuf {
        let dir = root.join("projects").join(encode(real));
        fs::create_dir_all(&dir).unwrap();
        if !lines.is_empty() {
            fs::write(dir.join("session.jsonl"), lines).unwrap();
        }
        dir
    }

    #[test]
    fn resolves_dashed_folder_name_from_cwd() {
        let tmp = TempDir::new().unwrap();
        let real = tmp.path().join("incometax-24-25");
        fs::create_dir_all(&real).unwrap();
        let lines = format!(
            "{{\"type\":\"user\",\"cwd\":\"{}\",\"uuid\":\"u1\"}}\n",
            real.display()
        );
        let dir = make_project_dir(tmp.path(), &real, &lines);

        assert_eq!(
            resolve_project_dir_path(&dir).as_deref(),
            Some(real.to_string_lossy().as_ref())
        );
        // Lossy decode is what this fixes: it would drop the dashed segments.
        assert_ne!(
            decode_claude_project_path(dir.file_name().unwrap().to_str().unwrap()),
            real.to_string_lossy()
        );
    }

    #[test]
    fn finds_cwd_after_leading_lines_without_it() {
        let tmp = TempDir::new().unwrap();
        let real = tmp.path().join("agentharbor-app");
        fs::create_dir_all(&real).unwrap();
        let lines = format!(
            "{{\"type\":\"mode\"}}\n{{\"type\":\"file-history-snapshot\",\"messageId\":\"m1\"}}\nnot json\n{{\"type\":\"user\",\"cwd\":\"{}\"}}\n",
            real.display()
        );
        let dir = make_project_dir(tmp.path(), &real, &lines);

        assert_eq!(
            resolve_project_dir_path(&dir).as_deref(),
            Some(real.to_string_lossy().as_ref())
        );
    }

    #[test]
    fn falls_back_to_fs_validated_decode_without_cwd() {
        let tmp = TempDir::new().unwrap();
        let real = tmp.path().join("work").join("my-app-name");
        fs::create_dir_all(&real).unwrap();
        let dir = make_project_dir(tmp.path(), &real, "{\"type\":\"mode\"}\n");

        assert_eq!(
            resolve_project_dir_path(&dir).as_deref(),
            Some(real.to_string_lossy().as_ref())
        );
    }

    #[test]
    fn memoizes_resolution_per_dir() {
        let tmp = TempDir::new().unwrap();
        let real = tmp.path().join("cached-project-name");
        fs::create_dir_all(&real).unwrap();
        let lines = format!("{{\"cwd\":\"{}\"}}\n", real.display());
        let dir = make_project_dir(tmp.path(), &real, &lines);

        let first = resolve_project_dir_path(&dir);
        assert_eq!(first.as_deref(), Some(real.to_string_lossy().as_ref()));

        // Remove every source of truth: only the memo can still answer.
        fs::remove_dir_all(&dir).unwrap();
        fs::remove_dir_all(&real).unwrap();
        assert_eq!(resolve_project_dir_path(&dir), first);
    }
}
