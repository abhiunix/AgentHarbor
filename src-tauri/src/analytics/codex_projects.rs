//! Codex Projects: per-project session trees, subagent hierarchies, and
//! per-event timelines read from `~/.codex/sessions/**/rollout-*.jsonl`.
//!
//! Data model notes, established by inspecting the on-disk corpus:
//!
//! * Every rollout begins with a `session_meta` line. A subagent's meta carries
//!   `payload.source.subagent.thread_spawn` with `parent_thread_id`, `depth`,
//!   `agent_path` and `agent_nickname`, which is what lets a run be rebuilt as
//!   a tree. Root sessions have no such block.
//! * `spawn_agent` tool arguments carry a plaintext `task_name` but an
//!   *encrypted* `message`, so subagent detail always comes from the child's
//!   own file rather than the parent's call.
//! * Tool use appears as `custom_tool_call` (almost always `exec`, whose input
//!   is a small JS wrapper around `tools.exec_command`) and `function_call`
//!   (`spawn_agent`, `send_message`, `wait_agent`, `list_agents`).
//! * No MCP or skill events occur in this corpus, so the timeline does not
//!   model them.

use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Timeline pages are capped so a 250 MB session cannot stall the UI.
const MAX_TIMELINE_PAGE: usize = 200;
/// Preview text kept per event; the UI expands on demand.
const PREVIEW_BYTES: usize = 2_000;

#[derive(Serialize, Clone, Debug, Default)]
pub struct CodexAgentNode {
    pub session_id: String,
    pub rollout_path: String,
    /// Subagent nickname (e.g. "James"); absent for root sessions.
    pub nickname: Option<String>,
    /// Spawn path such as `/root/menu_static_trace`.
    pub agent_path: Option<String>,
    pub depth: u32,
    /// Parent thread id for subagents; `None` for root sessions.
    pub parent_id: Option<String>,
    /// Number of timeline events available in this session.
    pub offsets_len: usize,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub model: Option<String>,
    pub tool_calls: u32,
    pub messages: u32,
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub estimated_cost: Option<f64>,
    pub file_size_bytes: u64,
    /// Distinct tool names with call counts, highest first.
    pub tools: Vec<(String, u32)>,
    pub children: Vec<CodexAgentNode>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct CodexProjectSummary {
    pub project_path: String,
    pub project_name: String,
    pub sessions: u32,
    pub subagents: u32,
    pub total_tokens: u64,
    pub estimated_cost: f64,
    pub last_active: Option<String>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct CodexProjectsOverview {
    pub projects: Vec<CodexProjectSummary>,
    pub total_sessions: u32,
    pub total_subagents: u32,
    pub scanned_files: u32,
}

#[derive(Serialize, Clone, Debug)]
pub struct CodexTimelineEvent {
    pub ordinal: u64,
    pub timestamp: Option<String>,
    /// One of: user, assistant, reasoning, tool_call, tool_output, spawn_agent,
    /// send_message, wait_agent, task_started, task_complete, other.
    pub kind: String,
    /// Tool or function name when applicable.
    pub name: Option<String>,
    /// Two-line preview; `truncated` marks that more text exists.
    pub preview: String,
    pub truncated: bool,
    /// Full text, only populated when a single event is requested.
    pub full_text: Option<String>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct CodexTimelinePage {
    pub events: Vec<CodexTimelineEvent>,
    pub total: usize,
    pub offset: usize,
    pub has_more: bool,
}

/// Cached per-file scan, invalidated on `(mtime, size)`.
struct CachedSession {
    mtime: std::time::SystemTime,
    size: u64,
    node: CodexAgentNode,
    project_path: String,
    /// Byte offsets of interesting lines, so the timeline can seek instead of
    /// re-reading the whole file on every page.
    offsets: Vec<u64>,
}

static SESSION_CACHE: Mutex<Option<HashMap<PathBuf, CachedSession>>> = Mutex::new(None);

fn sessions_roots() -> Vec<PathBuf> {
    let Ok(home) = crate::utils::codex_paths::codex_home() else {
        return Vec::new();
    };
    vec![home.join("sessions"), home.join("archived_sessions")]
}

fn json_str(value: &serde_json::Value, key: &str) -> Option<String> {
    value.get(key).and_then(|v| v.as_str()).map(str::to_string)
}

/// Classify one rollout line into a timeline event kind plus display text.
fn classify(payload: &serde_json::Value) -> Option<(String, Option<String>, String)> {
    let ptype = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");
    match ptype {
        "message" | "agent_message" => {
            let role = json_str(payload, "role").unwrap_or_else(|| "assistant".into());
            let text = payload
                .get("content")
                .map(collect_text)
                .unwrap_or_default();
            let kind = if role == "user" { "user" } else { "assistant" };
            Some((kind.to_string(), None, text))
        }
        "reasoning" => Some((
            "reasoning".into(),
            None,
            payload.get("summary").map(collect_text).unwrap_or_default(),
        )),
        "custom_tool_call" | "function_call" => {
            let name = json_str(payload, "name");
            let raw = payload
                .get("input")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .or_else(|| json_str(payload, "arguments"))
                .unwrap_or_default();
            let kind = match name.as_deref() {
                Some("spawn_agent") => "spawn_agent",
                Some("send_message") => "send_message",
                Some("wait_agent") => "wait_agent",
                _ => "tool_call",
            };
            Some((kind.to_string(), name, humanize_tool_input(&raw)))
        }
        "custom_tool_call_output" | "function_call_output" => {
            let text = payload
                .get("output")
                .map(collect_text)
                .unwrap_or_default();
            Some(("tool_output".into(), None, text))
        }
        "task_started" => Some(("task_started".into(), None, String::new())),
        "task_complete" => Some((
            "task_complete".into(),
            None,
            payload
                .get("last_agent_message")
                .map(collect_text)
                .unwrap_or_default(),
        )),
        _ => None,
    }
}

/// Flatten Codex's several content shapes (string, array of parts, object with
/// `text`) into plain display text.
fn collect_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(items) => items
            .iter()
            .map(collect_text)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        serde_json::Value::Object(map) => map
            .get("text")
            .or_else(|| map.get("content"))
            .or_else(|| map.get("summary"))
            .map(collect_text)
            .unwrap_or_default(),
        _ => String::new(),
    }
}

/// `exec` input is a JS wrapper around `tools.exec_command({...})`. Surface the
/// shell command itself, which is what a reader actually wants to see.
fn humanize_tool_input(raw: &str) -> String {
    if let Some(start) = raw.find("\"cmd\":\"") {
        let rest = &raw[start + 7..];
        let mut out = String::new();
        let mut chars = rest.chars();
        while let Some(c) = chars.next() {
            match c {
                '\\' => {
                    if let Some(next) = chars.next() {
                        match next {
                            'n' => out.push('\n'),
                            't' => out.push('\t'),
                            other => out.push(other),
                        }
                    }
                }
                '"' => break,
                other => out.push(other),
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    raw.to_string()
}

fn truncate(text: &str, max: usize) -> (String, bool) {
    if text.len() <= max {
        return (text.to_string(), false);
    }
    let mut end = max;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_string(), true)
}

/// Scan one rollout: header metadata, aggregate counters, and the byte offsets
/// of timeline-worthy lines. Streams the file so size does not drive memory.
fn scan_session(path: &Path) -> Option<(CodexAgentNode, String)> {
    let file = fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    let mut reader = std::io::BufReader::new(file);

    let mut node = CodexAgentNode {
        rollout_path: path.to_string_lossy().to_string(),
        file_size_bytes: size,
        ..Default::default()
    };
    let mut project_path = String::new();
    let mut tools: HashMap<String, u32> = HashMap::new();
    let mut offsets: Vec<u64> = Vec::new();
    let mut position: u64 = 0;
    let mut line = String::new();

    loop {
        line.clear();
        let read = reader.read_line(&mut line).ok()?;
        if read == 0 {
            break;
        }
        let start = position;
        position += read as u64;

        // Cheap prefilter: most bytes in a rollout are tool output.
        let interesting = line.contains("session_meta")
            || line.contains("token_count")
            || line.contains("turn_context")
            || line.contains("\"message\"")
            || line.contains("reasoning")
            || line.contains("_call");
        if !interesting {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let vtype = value.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let payload = value.get("payload").cloned().unwrap_or(serde_json::Value::Null);
        let timestamp = json_str(&value, "timestamp");

        if vtype == "session_meta" {
            node.session_id = json_str(&payload, "id")
                .or_else(|| json_str(&payload, "session_id"))
                .unwrap_or_default();
            project_path = json_str(&payload, "cwd").unwrap_or_default();
            node.started_at = json_str(&payload, "timestamp").or(timestamp.clone());
            node.model = json_str(&payload, "model");
            if let Some(spawn) = payload
                .pointer("/source/subagent/thread_spawn")
                .filter(|v| v.is_object())
            {
                node.nickname = json_str(spawn, "agent_nickname");
                node.agent_path = json_str(spawn, "agent_path");
                node.depth = spawn
                    .get("depth")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1) as u32;
                node.parent_id = json_str(spawn, "parent_thread_id");
            }
            continue;
        }

        if vtype == "turn_context" {
            if let Some(model) = json_str(&payload, "model") {
                node.model = Some(model);
            }
            continue;
        }

        // Cumulative usage: keep the latest snapshot.
        if let Some(usage) = payload
            .pointer("/info/total_token_usage")
            .or_else(|| payload.get("total_token_usage"))
        {
            node.input_tokens = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(node.input_tokens);
            node.cached_input_tokens = usage.get("cached_input_tokens").and_then(|v| v.as_u64()).unwrap_or(node.cached_input_tokens);
            node.output_tokens = usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(node.output_tokens);
            node.total_tokens = usage.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(node.total_tokens);
        }

        if let Some((kind, name, _)) = classify(&payload) {
            offsets.push(start);
            if timestamp.is_some() {
                node.ended_at = timestamp;
            }
            match kind.as_str() {
                "user" | "assistant" => node.messages += 1,
                "tool_call" | "spawn_agent" | "send_message" | "wait_agent" => {
                    node.tool_calls += 1;
                    if let Some(n) = name {
                        *tools.entry(n).or_insert(0) += 1;
                    }
                }
                _ => {}
            }
        }
    }

    if node.session_id.is_empty() {
        return None;
    }
    let mut tool_list: Vec<(String, u32)> = tools.into_iter().collect();
    tool_list.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    node.tools = tool_list;
    node.offsets_len = offsets.len();

    // Cache the offsets so timeline paging can seek directly.
    if let Ok(mut guard) = SESSION_CACHE.lock() {
        let cache = guard.get_or_insert_with(HashMap::new);
        if let Ok(meta) = fs::metadata(path) {
            if let Ok(mtime) = meta.modified() {
                cache.insert(
                    path.to_path_buf(),
                    CachedSession {
                        mtime,
                        size,
                        node: node.clone(),
                        project_path: project_path.clone(),
                        offsets,
                    },
                );
            }
        }
    }
    Some((node, project_path))
}

/// Read a cached scan when the file is unchanged.
fn cached_scan(path: &Path) -> Option<(CodexAgentNode, String)> {
    let meta = fs::metadata(path).ok()?;
    let (mtime, size) = (meta.modified().ok()?, meta.len());
    let guard = SESSION_CACHE.lock().ok()?;
    let cache = guard.as_ref()?;
    let entry = cache.get(path)?;
    if entry.mtime != mtime || entry.size != size {
        return None;
    }
    Some((entry.node.clone(), entry.project_path.clone()))
}

fn scan_or_cached(path: &Path) -> Option<(CodexAgentNode, String)> {
    cached_scan(path).or_else(|| scan_session(path))
}

/// Walk every rollout once and return `(node, project_path)` per session.
fn scan_all() -> Vec<(CodexAgentNode, String)> {
    let mut out = Vec::new();
    for root in sessions_roots() {
        if !root.is_dir() {
            continue;
        }
        for entry in walkdir::WalkDir::new(&root)
            .max_depth(5)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
        {
            if !entry.file_type().is_file() {
                continue;
            }
            if entry.path().extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            if let Some(scanned) = scan_or_cached(entry.path()) {
                out.push(scanned);
            }
        }
    }
    out
}

/// Assemble root sessions with their subagents nested underneath.
fn build_forest(mut scanned: Vec<(CodexAgentNode, String)>) -> Vec<(CodexAgentNode, String)> {
    // Index children by parent id, then attach depth-first.
    let mut children: HashMap<String, Vec<CodexAgentNode>> = HashMap::new();
    let mut roots: Vec<(CodexAgentNode, String)> = Vec::new();

    scanned.sort_by(|a, b| a.0.started_at.cmp(&b.0.started_at));
    for (node, project) in scanned {
        match node.parent_id.clone() {
            Some(parent) if !parent.is_empty() && parent != node.session_id => {
                children.entry(parent).or_default().push(node);
            }
            _ => roots.push((node, project)),
        }
    }

    fn attach(node: &mut CodexAgentNode, children: &mut HashMap<String, Vec<CodexAgentNode>>) {
        if let Some(mut kids) = children.remove(&node.session_id) {
            for kid in &mut kids {
                attach(kid, children);
            }
            node.children = kids;
        }
    }

    for (root, _) in &mut roots {
        attach(root, &mut children);
    }

    // Orphans (parent file missing or pruned) still deserve to be listed.
    for (_, orphans) in children.drain() {
        for orphan in orphans {
            roots.push((orphan, String::new()));
        }
    }
    roots
}

fn rollup(node: &CodexAgentNode) -> (u64, u32) {
    let mut tokens = node.total_tokens;
    let mut agents = node.children.len() as u32;
    for child in &node.children {
        let (t, a) = rollup(child);
        tokens += t;
        agents += a;
    }
    (tokens, agents)
}

/// Project list with session and subagent counts.
#[tauri::command]
pub async fn get_codex_projects_overview() -> Result<CodexProjectsOverview, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let scanned = scan_all();
        let scanned_files = scanned.len() as u32;
        let forest = build_forest(scanned);

        let mut by_project: HashMap<String, CodexProjectSummary> = HashMap::new();
        let mut total_sessions = 0u32;
        let mut total_subagents = 0u32;

        for (root, project) in &forest {
            let key = if project.is_empty() { "Unknown".to_string() } else { project.clone() };
            let (tokens, agents) = rollup(root);
            total_sessions += 1;
            total_subagents += agents;

            let entry = by_project.entry(key.clone()).or_insert_with(|| CodexProjectSummary {
                project_path: key.clone(),
                project_name: Path::new(&key)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| key.clone()),
                ..Default::default()
            });
            entry.sessions += 1;
            entry.subagents += agents;
            entry.total_tokens += tokens;
            if let Some(ts) = root.ended_at.clone().or_else(|| root.started_at.clone()) {
                if entry.last_active.as_deref().is_none_or(|cur| cur < ts.as_str()) {
                    entry.last_active = Some(ts);
                }
            }
        }

        let mut projects: Vec<CodexProjectSummary> = by_project.into_values().collect();
        projects.sort_by(|a, b| b.last_active.cmp(&a.last_active));
        Ok(CodexProjectsOverview { projects, total_sessions, total_subagents, scanned_files })
    })
    .await
    .map_err(|e| format!("Codex projects worker failed: {e}"))?
}

/// Session trees for one project (or all projects when `project_path` is None).
#[tauri::command]
pub async fn get_codex_project_sessions(
    project_path: Option<String>,
) -> Result<Vec<CodexAgentNode>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let forest = build_forest(scan_all());
        let mut out: Vec<CodexAgentNode> = forest
            .into_iter()
            .filter(|(_, project)| match &project_path {
                Some(target) => project == target,
                None => true,
            })
            .map(|(node, _)| node)
            .collect();
        out.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        Ok(out)
    })
    .await
    .map_err(|e| format!("Codex sessions worker failed: {e}"))?
}

/// One page of a session's timeline. Uses cached byte offsets so paging a
/// 250 MB rollout seeks directly instead of re-reading the file.
#[tauri::command]
pub async fn get_codex_session_timeline(
    rollout_path: String,
    offset: Option<usize>,
    limit: Option<usize>,
    expand_ordinal: Option<u64>,
) -> Result<CodexTimelinePage, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = PathBuf::from(&rollout_path);
        // Ensure offsets are populated.
        if cached_scan(&path).is_none() {
            scan_session(&path).ok_or("Session could not be read")?;
        }
        let offsets = {
            let guard = SESSION_CACHE.lock().map_err(|_| "cache poisoned")?;
            guard
                .as_ref()
                .and_then(|c| c.get(&path))
                .map(|e| e.offsets.clone())
                .ok_or("Session not cached")?
        };

        let total = offsets.len();
        let offset = offset.unwrap_or(0).min(total);
        let limit = limit.unwrap_or(100).clamp(1, MAX_TIMELINE_PAGE);
        let end = (offset + limit).min(total);

        let mut file = fs::File::open(&path).map_err(|e| e.to_string())?;
        let mut events = Vec::with_capacity(end - offset);

        for &byte in &offsets[offset..end] {
            file.seek(SeekFrom::Start(byte)).map_err(|e| e.to_string())?;
            let mut reader = std::io::BufReader::new(&mut file);
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            let payload = value.get("payload").cloned().unwrap_or(serde_json::Value::Null);
            let Some((kind, name, text)) = classify(&payload) else {
                continue;
            };
            let ordinal = value.get("ordinal").and_then(|v| v.as_u64()).unwrap_or(byte);
            let (preview, truncated) = truncate(text.trim(), PREVIEW_BYTES);
            let full_text = match expand_ordinal {
                Some(target) if target == ordinal => Some(text),
                _ => None,
            };
            events.push(CodexTimelineEvent {
                ordinal,
                timestamp: json_str(&value, "timestamp"),
                kind,
                name,
                preview,
                truncated,
                full_text,
            });
        }

        Ok(CodexTimelinePage { events, total, offset, has_more: end < total })
    })
    .await
    .map_err(|e| format!("Codex timeline worker failed: {e}"))?
}

