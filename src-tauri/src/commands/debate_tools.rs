//! Sandboxed read-only filesystem tools exposed to the Anthropic / OpenAI
//! debate models. Each tool operates strictly within the user-picked project
//! root: paths are canonicalized and verified to be prefixed by the project
//! root's canonical form, symlinks that resolve outside the root are
//! rejected, and nothing here is allowed to write, execute, or open a network
//! socket.
//!
//! The shape of each tool is:
//!   - `name` — what the model invokes.
//!   - `description` — given to the model in the tool spec.
//!   - `input_schema` — JSON Schema for the model's arguments.
//!   - `execute` — synchronous, returns `Result<String, String>` where the
//!     string is exactly what we feed back into the model as the
//!     `tool_result`. Errors are returned as `Err(...)` so callers can flag
//!     them as `is_error: true` for the wire event and the Anthropic
//!     `tool_result.is_error` flag.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use walkdir::WalkDir;

// ── Limits ──────────────────────────────────────────────────────────────────

/// `read_file` truncation point. UTF-8 characters past 64 KiB are dropped,
/// the model is told the file was longer.
pub const READ_FILE_MAX_BYTES: usize = 64 * 1024;

/// `list_directory` cap. We sort dirs-then-files alphabetically, then trim.
pub const LIST_DIRECTORY_MAX_ENTRIES: usize = 500;

/// `grep` default and hard cap on the number of `{file, line, text}` rows.
pub const GREP_DEFAULT_MAX_RESULTS: u32 = 50;
pub const GREP_HARD_CAP_RESULTS: u32 = 200;

/// Recursive walk depth for `grep`.
pub const GREP_MAX_DEPTH: usize = 6;

/// Skip files bigger than this when grepping — large binaries / lockfiles /
/// minified bundles would dominate runtime without paying for themselves.
pub const GREP_MAX_FILE_BYTES: u64 = 1024 * 1024;

/// Clamp each grep result line to this many chars to keep the model's
/// `tool_result` small. The trimmed line is what we feed back.
pub const GREP_MAX_LINE_CHARS: usize = 200;

/// Subdirectory / file names skipped during list/grep walks.
const SKIP_NAMES: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".venv",
    "__pycache__",
    ".DS_Store",
];

// ── Tool catalog ────────────────────────────────────────────────────────────

/// Tool kinds. Keeps the dispatch + UI labels in one enum so we can't drift.
#[derive(Debug, Clone, Copy)]
pub enum DebateTool {
    ReadFile,
    ListDirectory,
    Grep,
}

impl DebateTool {
    pub fn name(self) -> &'static str {
        match self {
            DebateTool::ReadFile => "read_file",
            DebateTool::ListDirectory => "list_directory",
            DebateTool::Grep => "grep",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            DebateTool::ReadFile =>
                "Read a UTF-8 file from inside the project. Use to verify what a file actually \
contains before claiming it does or doesn't. The path is relative to the project root. \
Output is capped at 64 KiB; long files are truncated with a marker.",
            DebateTool::ListDirectory =>
                "List the entries (files and subdirectories) inside a directory of the project. \
Pass an empty path to list the project root. Returns up to 500 entries, with directories first. \
Common build/cache folders (.git, node_modules, target, dist, build, .next, .venv, __pycache__) \
are skipped.",
            DebateTool::Grep =>
                "Case-insensitive literal substring search across the project. Walks up to 6 \
levels deep, skipping common build/cache folders and files over 1 MiB. Returns matching \
line objects {file, line, text}.",
        }
    }

    pub fn input_schema(self) -> Value {
        match self {
            DebateTool::ReadFile => json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path relative to the project root. Use forward slashes."
                    }
                },
                "required": ["path"],
                "additionalProperties": false,
            }),
            DebateTool::ListDirectory => json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path relative to the project root. Empty string lists the project root."
                    }
                },
                "required": ["path"],
                "additionalProperties": false,
            }),
            DebateTool::Grep => json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Literal substring to search for (case-insensitive)."
                    },
                    "path": {
                        "type": "string",
                        "description": "Optional subdirectory of the project to scope the search. Defaults to project root."
                    },
                    "max_results": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 200,
                        "description": "Max rows to return. Default 50, hard cap 200."
                    }
                },
                "required": ["pattern"],
                "additionalProperties": false,
            }),
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "read_file" => Some(DebateTool::ReadFile),
            "list_directory" => Some(DebateTool::ListDirectory),
            "grep" => Some(DebateTool::Grep),
            _ => None,
        }
    }
}

/// All three tools, in a stable order. Used to feed the provider request.
pub const ALL_TOOLS: &[DebateTool] = &[
    DebateTool::ReadFile,
    DebateTool::ListDirectory,
    DebateTool::Grep,
];

/// Anthropic Messages API tool format.
pub fn anthropic_tool_specs() -> Vec<Value> {
    ALL_TOOLS
        .iter()
        .copied()
        .map(|t| {
            json!({
                "name": t.name(),
                "description": t.description(),
                "input_schema": t.input_schema(),
            })
        })
        .collect()
}

/// OpenAI Chat Completions tool format (function-tool wrapper).
pub fn openai_tool_specs() -> Vec<Value> {
    ALL_TOOLS
        .iter()
        .copied()
        .map(|t| {
            json!({
                "type": "function",
                "function": {
                    "name": t.name(),
                    "description": t.description(),
                    "parameters": t.input_schema(),
                }
            })
        })
        .collect()
}

// ── Dispatch ────────────────────────────────────────────────────────────────

/// Execute a tool by name. Input is parsed JSON. `project_dir` is the
/// user-picked project root.
pub fn execute_tool(name: &str, input: &Value, project_dir: &str) -> Result<String, String> {
    let Some(tool) = DebateTool::from_name(name) else {
        return Err(format!("unknown_tool: {}", name));
    };
    match tool {
        DebateTool::ReadFile => {
            let path = input
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing_arg: path".to_string())?;
            read_file_impl(project_dir, path)
        }
        DebateTool::ListDirectory => {
            let path = input
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            list_directory_impl(project_dir, path)
        }
        DebateTool::Grep => {
            let pattern = input
                .get("pattern")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing_arg: pattern".to_string())?;
            let path = input
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let max_results = input
                .get("max_results")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32)
                .unwrap_or(GREP_DEFAULT_MAX_RESULTS)
                .clamp(1, GREP_HARD_CAP_RESULTS);
            grep_impl(project_dir, pattern, path, max_results)
        }
    }
}

// ── Sandboxing ──────────────────────────────────────────────────────────────

/// Resolve `rel` against `project_dir` and verify the canonical result stays
/// inside the canonical project root. Returns the canonical absolute path on
/// success. Used as the entry point for every tool — a `..` or symlink that
/// escapes the project triggers `Err("path_escapes_project: <rel>")`.
fn resolve_within_project(project_dir: &str, rel: &str) -> Result<PathBuf, String> {
    let project = PathBuf::from(project_dir);
    let canonical_project = project
        .canonicalize()
        .map_err(|e| format!("project_unavailable: {}", e))?;
    // Strip a leading `/` or `\` so absolute-looking inputs from the model
    // still join inside the project.
    let trimmed = rel.trim_start_matches(['/', '\\']);
    let target = if trimmed.is_empty() {
        project.clone()
    } else {
        project.join(trimmed)
    };
    let canonical_target = target
        .canonicalize()
        .map_err(|_| format!("not_found: {}", rel))?;
    if !canonical_target.starts_with(&canonical_project) {
        return Err(format!("path_escapes_project: {}", rel));
    }
    Ok(canonical_target)
}

/// True if a path entry should be ignored by list/grep walks based on its
/// file/dir basename.
fn is_skipped_name(name: &str) -> bool {
    SKIP_NAMES.contains(&name)
}

// ── read_file ───────────────────────────────────────────────────────────────

fn read_file_impl(project_dir: &str, rel: &str) -> Result<String, String> {
    let path = resolve_within_project(project_dir, rel)?;
    if !path.is_file() {
        return Err(format!("not_a_file: {}", rel));
    }
    let bytes = fs::read(&path).map_err(|e| format!("read_error: {}", e))?;
    let total = bytes.len();
    if total <= READ_FILE_MAX_BYTES {
        return String::from_utf8(bytes).map_err(|e| format!("not_utf8: {}", e));
    }
    // Truncate to a valid UTF-8 boundary at or before the limit. Otherwise
    // mid-codepoint truncation would yield `Err(Utf8Error)`.
    let mut cut = READ_FILE_MAX_BYTES;
    while cut > 0 && (bytes[cut] & 0b1100_0000) == 0b1000_0000 {
        cut -= 1;
    }
    let head = String::from_utf8(bytes[..cut].to_vec())
        .map_err(|e| format!("not_utf8: {}", e))?;
    Ok(format!("{head}\n[truncated, file was {total} bytes]"))
}

// ── list_directory ─────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct DirEntryOut {
    name: String,
    is_dir: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    size: Option<u64>,
}

fn list_directory_impl(project_dir: &str, rel: &str) -> Result<String, String> {
    let path = resolve_within_project(project_dir, rel)?;
    if !path.is_dir() {
        return Err(format!("not_a_directory: {}", rel));
    }
    let read = fs::read_dir(&path).map_err(|e| format!("read_dir_error: {}", e))?;
    let mut entries: Vec<DirEntryOut> = Vec::new();
    for entry in read.flatten() {
        let name = match entry.file_name().to_str() {
            Some(s) => s.to_string(),
            None => continue,
        };
        if is_skipped_name(&name) {
            continue;
        }
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        let is_dir = ft.is_dir();
        let size = if is_dir {
            None
        } else {
            entry.metadata().ok().map(|m| m.len())
        };
        entries.push(DirEntryOut { name, is_dir, size });
    }
    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.cmp(&b.name),
    });
    if entries.len() > LIST_DIRECTORY_MAX_ENTRIES {
        entries.truncate(LIST_DIRECTORY_MAX_ENTRIES);
    }
    serde_json::to_string(&entries).map_err(|e| format!("serialize_error: {}", e))
}

// ── grep ────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct GrepHit {
    file: String,
    line: u32,
    text: String,
}

fn grep_impl(
    project_dir: &str,
    pattern: &str,
    rel: &str,
    max_results: u32,
) -> Result<String, String> {
    if pattern.is_empty() {
        return Err("empty_pattern".to_string());
    }
    let needle_lower = pattern.to_ascii_lowercase();
    let root = resolve_within_project(project_dir, rel)?;
    let canonical_project = PathBuf::from(project_dir)
        .canonicalize()
        .map_err(|e| format!("project_unavailable: {}", e))?;

    let mut hits: Vec<GrepHit> = Vec::new();
    let walker = WalkDir::new(&root)
        .max_depth(GREP_MAX_DEPTH)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !is_skipped_name(&name)
        });

    for entry in walker.flatten() {
        if hits.len() >= max_results as usize {
            break;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        // Defense in depth: re-verify each walked file stays inside the project
        // (in case some weird symlink slipped past WalkDir's follow_links flag).
        let path = entry.path();
        let Ok(canonical) = path.canonicalize() else {
            continue;
        };
        if !canonical.starts_with(&canonical_project) {
            continue;
        }
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.len() > GREP_MAX_FILE_BYTES {
            continue;
        }
        let contents = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue, // non-UTF8 / unreadable — silently skip
        };
        let rel_path = match path.strip_prefix(&canonical_project) {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => path.to_string_lossy().to_string(),
        };
        for (idx, line) in contents.lines().enumerate() {
            if hits.len() >= max_results as usize {
                break;
            }
            if line.to_ascii_lowercase().contains(&needle_lower) {
                hits.push(GrepHit {
                    file: rel_path.clone(),
                    line: (idx + 1) as u32,
                    text: clamp_chars(line.trim(), GREP_MAX_LINE_CHARS),
                });
            }
        }
    }

    serde_json::to_string(&hits).map_err(|e| format!("serialize_error: {}", e))
}

fn clamp_chars(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let mut out = String::with_capacity(n);
    for (i, c) in s.chars().enumerate() {
        if i >= n {
            break;
        }
        out.push(c);
    }
    out
}

// ── Wire-event preview helpers ─────────────────────────────────────────────

/// Take the first N chars of a JSON value's compact serialization. Used for
/// `input_preview` on the `debate:tool_call` event.
pub fn preview_input(input: &Value, n: usize) -> String {
    let serialized = serde_json::to_string(input).unwrap_or_default();
    clamp_chars(&serialized, n)
}

/// Take the first N chars of `s`, trimmed. Used for `output_preview`.
pub fn preview_output(s: &str, n: usize) -> String {
    clamp_chars(s.trim(), n)
}

/// Same as `preview_output` but for the persisted record, which carries more
/// of the result so users can inspect it later.
pub fn truncate_for_record(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    // Slice on UTF-8 boundary.
    let mut cut = max_bytes;
    let bytes = s.as_bytes();
    while cut > 0 && (bytes[cut] & 0b1100_0000) == 0b1000_0000 {
        cut -= 1;
    }
    let mut out = String::from_utf8(bytes[..cut].to_vec()).unwrap_or_default();
    out.push_str("\n[truncated]");
    out
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::Path;

    fn make_project() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let s = root.to_string_lossy().to_string();
        (dir, s)
    }

    fn write_file(root: &str, rel: &str, contents: &[u8]) -> PathBuf {
        let p = Path::new(root).join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut f = fs::File::create(&p).unwrap();
        f.write_all(contents).unwrap();
        p
    }

    #[test]
    fn read_file_returns_contents() {
        let (_d, root) = make_project();
        write_file(&root, "hello.txt", b"hi there");
        let out = read_file_impl(&root, "hello.txt").unwrap();
        assert_eq!(out, "hi there");
    }

    #[test]
    fn read_file_truncates_large_input() {
        let (_d, root) = make_project();
        let big = vec![b'a'; READ_FILE_MAX_BYTES + 10];
        write_file(&root, "big.txt", &big);
        let out = read_file_impl(&root, "big.txt").unwrap();
        assert!(out.contains("[truncated, file was "));
        // Body must not exceed the limit by more than the marker length.
        let head_len = out.find("\n[truncated").unwrap();
        assert!(head_len <= READ_FILE_MAX_BYTES);
    }

    #[test]
    fn read_file_rejects_escape_with_dotdot() {
        let (_d, root) = make_project();
        write_file(&root, "inside.txt", b"x");
        let err = read_file_impl(&root, "../etc/passwd").unwrap_err();
        // Either canonical resolves outside (→ path_escapes_project) or the
        // file doesn't exist (→ not_found). Both are safe outcomes; assert we
        // didn't successfully return contents.
        assert!(err.starts_with("path_escapes_project") || err.starts_with("not_found"));
    }

    #[test]
    fn list_directory_sorts_dirs_first() {
        let (_d, root) = make_project();
        write_file(&root, "z.txt", b"");
        write_file(&root, "a.txt", b"");
        fs::create_dir_all(Path::new(&root).join("b_dir")).unwrap();
        let out = list_directory_impl(&root, "").unwrap();
        let parsed: Vec<DirEntryOut> = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed[0].name, "b_dir");
        assert!(parsed[0].is_dir);
        let file_names: Vec<&str> = parsed.iter().filter(|e| !e.is_dir).map(|e| e.name.as_str()).collect();
        assert_eq!(file_names, vec!["a.txt", "z.txt"]);
    }

    #[test]
    fn list_directory_skips_well_known_names() {
        let (_d, root) = make_project();
        fs::create_dir_all(Path::new(&root).join("node_modules")).unwrap();
        fs::create_dir_all(Path::new(&root).join(".git")).unwrap();
        write_file(&root, "keep.txt", b"");
        let out = list_directory_impl(&root, "").unwrap();
        assert!(!out.contains("node_modules"));
        assert!(!out.contains(".git"));
        assert!(out.contains("keep.txt"));
    }

    #[test]
    fn grep_finds_substring_case_insensitive() {
        let (_d, root) = make_project();
        write_file(&root, "src/foo.rs", b"fn Bar() {}\nfn baz() {}\n");
        let out = grep_impl(&root, "bar", "", 50).unwrap();
        let parsed: Vec<GrepHit> = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].line, 1);
        assert!(parsed[0].text.contains("Bar"));
    }

    #[test]
    fn grep_respects_max_results() {
        let (_d, root) = make_project();
        let mut body = String::new();
        for _ in 0..30 {
            body.push_str("needle here\n");
        }
        write_file(&root, "f.txt", body.as_bytes());
        let out = grep_impl(&root, "needle", "", 5).unwrap();
        let parsed: Vec<GrepHit> = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed.len(), 5);
    }

    #[test]
    fn grep_skips_skipped_dirs() {
        let (_d, root) = make_project();
        write_file(&root, "node_modules/leaked.txt", b"needle\n");
        write_file(&root, "src/real.txt", b"needle\n");
        let out = grep_impl(&root, "needle", "", 50).unwrap();
        let parsed: Vec<GrepHit> = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed.len(), 1);
        assert!(parsed[0].file.contains("real.txt"));
    }

    #[test]
    fn execute_tool_dispatches_by_name() {
        let (_d, root) = make_project();
        write_file(&root, "hello.txt", b"hi");
        let v = execute_tool("read_file", &json!({"path": "hello.txt"}), &root).unwrap();
        assert_eq!(v, "hi");
    }

    #[test]
    fn execute_tool_unknown_name() {
        let (_d, root) = make_project();
        let err = execute_tool("rm_rf", &json!({}), &root).unwrap_err();
        assert!(err.starts_with("unknown_tool"));
    }

    #[test]
    fn truncate_for_record_clamps() {
        let big = "a".repeat(3000);
        let out = truncate_for_record(&big, 2048);
        assert!(out.len() <= 2048 + "\n[truncated]".len());
        assert!(out.ends_with("[truncated]"));
    }

    #[test]
    fn truncate_for_record_passes_short_through() {
        let s = "hi";
        assert_eq!(truncate_for_record(s, 2048), "hi");
    }
}
