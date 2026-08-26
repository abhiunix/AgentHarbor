//! DeepSeek Harness (`dsh`) Instructions — view/edit per-workspace `AGENTS.md`
//! files. dsh reads instructions from `<workspace>/AGENTS.md` (uppercase
//! preferred; `agents.md` is an accepted alt-casing). There is no global
//! `~/.dsh/AGENTS.md` equivalent. Workspace directories come from
//! `storages/workspace.json`'s `tables.workspaces.*.path`, plus any session's
//! `identity.cwd` in `storages/session_projcache.json` not already covered.
//! Modeled on `kimi_instructions.rs`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::analytics::deepseek_v2::dsh_root;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekInstructionFile {
    pub project_path: String,
    pub project_name: String,
    pub abs_path: String,
    pub exists: bool,
    pub size_bytes: Option<u64>,
    pub modified: Option<String>,
}

fn project_name_from_path(path: &str) -> String {
    Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string())
}

fn metadata_to_rfc3339(m: &std::fs::Metadata) -> Option<String> {
    m.modified().ok().map(|t| {
        let dt: chrono::DateTime<chrono::Utc> = t.into();
        dt.to_rfc3339()
    })
}

fn build_entry(project_path: &str, project_name: &str, path: &Path) -> DeepSeekInstructionFile {
    let metadata = std::fs::metadata(path).ok();
    let exists = metadata.is_some();
    let size_bytes = metadata.as_ref().map(|m| m.len());
    let modified = metadata.as_ref().and_then(metadata_to_rfc3339);
    DeepSeekInstructionFile {
        project_path: project_path.to_string(),
        project_name: project_name.to_string(),
        abs_path: path.to_string_lossy().to_string(),
        exists,
        size_bytes,
        modified,
    }
}

// ── Workspace path discovery (workspace.json + session_projcache.json) ─────

#[derive(Debug, Deserialize)]
struct WorkspaceFile {
    tables: WorkspaceTables,
}

#[derive(Debug, Default, Deserialize)]
struct WorkspaceTables {
    #[serde(default)]
    workspaces: HashMap<String, WorkspaceEntryRaw>,
}

#[derive(Debug, Deserialize)]
struct WorkspaceEntryRaw {
    path: String,
}

#[derive(Debug, Deserialize)]
struct SessionCacheFile {
    tables: SessionCacheTables,
}

#[derive(Debug, Default, Deserialize)]
struct SessionCacheTables {
    #[serde(default)]
    sessions: HashMap<String, SessionEntryRaw>,
}

#[derive(Debug, Default, Deserialize)]
struct SessionEntryRaw {
    #[serde(default)]
    identity: IdentityRaw,
}

#[derive(Debug, Default, Deserialize)]
struct IdentityRaw {
    #[serde(default)]
    cwd: Option<String>,
}

/// Every `workspace.json` path, plus any session `identity.cwd` not already
/// covered by one — deduped and sorted.
fn workspace_paths(root: &Path) -> Vec<String> {
    let mut paths: Vec<String> = std::fs::read_to_string(root.join("storages").join("workspace.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<WorkspaceFile>(&text).ok())
        .map(|f| f.tables.workspaces.into_values().map(|w| w.path).collect())
        .unwrap_or_default();

    if let Ok(text) = std::fs::read_to_string(root.join("storages").join("session_projcache.json")) {
        if let Ok(cache) = serde_json::from_str::<SessionCacheFile>(&text) {
            for entry in cache.tables.sessions.into_values() {
                if let Some(cwd) = entry.identity.cwd {
                    paths.push(cwd);
                }
            }
        }
    }

    paths.sort();
    paths.dedup();
    paths
}

/// Whether two paths resolve to the same file on disk — used to avoid
/// listing `AGENTS.md` and `agents.md` twice on case-insensitive filesystems
/// (the macOS/Windows default), where they're the same underlying file.
fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => false,
    }
}

/// For each workspace: the canonical `<workspace>/AGENTS.md` (exists or not),
/// plus an already-existing `<workspace>/agents.md`. Newest-modified (or
/// existing) entries sort first.
fn list_instruction_files_for(workspaces: &[String]) -> Vec<DeepSeekInstructionFile> {
    let mut out = Vec::new();
    for project_path in workspaces {
        let project_name = project_name_from_path(project_path);
        let base = Path::new(project_path);
        let canonical = base.join("AGENTS.md");
        out.push(build_entry(project_path, &project_name, &canonical));

        let lower = base.join("agents.md");
        if lower.is_file() && !same_file(&lower, &canonical) {
            out.push(build_entry(project_path, &project_name, &lower));
        }
    }
    out.sort_by(|a, b| b.exists.cmp(&a.exists).then_with(|| b.modified.cmp(&a.modified)));
    out
}

/// Path guard: the basename must be exactly `AGENTS.md` or `agents.md`, AND
/// `abs_path` must be `<workspace>/AGENTS.md` or `<workspace>/agents.md` for
/// a known workspace. Compared directly against the known workspaces — never
/// canonicalized, so a path that doesn't exist yet (create flow) still
/// validates correctly.
fn validate_path_against(abs_path: &str, workspaces: &[String]) -> Result<(), String> {
    if abs_path.contains("..") {
        return Err("Invalid instructions file path".to_string());
    }
    let path = Path::new(abs_path);
    let basename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if basename != "AGENTS.md" && basename != "agents.md" {
        return Err("Invalid instructions file name".to_string());
    }

    for project_path in workspaces {
        let base = Path::new(project_path);
        let allowed = [base.join("AGENTS.md"), base.join("agents.md")];
        if allowed.iter().any(|p| p == path) {
            return Ok(());
        }
    }
    Err("Path is outside known DeepSeek workspace directories".to_string())
}

fn read_instruction_at(abs_path: &str, workspaces: &[String]) -> Result<String, String> {
    validate_path_against(abs_path, workspaces)?;
    let path = Path::new(abs_path);
    if !path.is_file() {
        return Ok(String::new());
    }
    std::fs::read_to_string(path).map_err(|e| e.to_string())
}

fn write_instruction_at(abs_path: &str, content: &str, workspaces: &[String]) -> Result<(), String> {
    validate_path_against(abs_path, workspaces)?;
    crate::utils::paths::atomic_write_str(Path::new(abs_path), content)
}

// ── Tauri Commands ───────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_deepseek_instruction_files() -> Vec<DeepSeekInstructionFile> {
    let Some(root) = dsh_root() else { return Vec::new() };
    list_instruction_files_for(&workspace_paths(&root))
}

#[tauri::command]
pub fn read_deepseek_instruction(abs_path: String) -> Result<String, String> {
    let Some(root) = dsh_root() else {
        return Err("DeepSeek Harness home (~/.dsh) not found".to_string());
    };
    read_instruction_at(&abs_path, &workspace_paths(&root))
}

#[tauri::command]
pub fn write_deepseek_instruction(abs_path: String, content: String) -> Result<(), String> {
    let Some(root) = dsh_root() else {
        return Err("DeepSeek Harness home (~/.dsh) not found".to_string());
    };
    write_instruction_at(&abs_path, &content, &workspace_paths(&root))
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn lists_canonical_file_and_existing_lowercase_variant() {
        let temp = TempDir::new().unwrap();
        let proj_a = temp.path().join("proj-a"); // canonical AGENTS.md exists
        let proj_b = temp.path().join("proj-b"); // no instructions file at all
        std::fs::create_dir_all(&proj_a).unwrap();
        std::fs::create_dir_all(&proj_b).unwrap();
        std::fs::write(proj_a.join("AGENTS.md"), "# a").unwrap();

        let workspaces = [proj_a.to_string_lossy().to_string(), proj_b.to_string_lossy().to_string()];
        let files = list_instruction_files_for(&workspaces);

        let a_entries: Vec<&DeepSeekInstructionFile> =
            files.iter().filter(|f| f.project_path == workspaces[0]).collect();
        assert_eq!(a_entries.len(), 1);
        assert!(a_entries[0].exists);
        assert!(a_entries[0].abs_path.ends_with("AGENTS.md"));

        let b_entries: Vec<&DeepSeekInstructionFile> =
            files.iter().filter(|f| f.project_path == workspaces[1]).collect();
        assert_eq!(b_entries.len(), 1);
        assert!(!b_entries[0].exists);
    }

    #[test]
    fn guard_accepts_allowed_shapes_and_rejects_others() {
        let temp = TempDir::new().unwrap();
        let proj = temp.path().join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        let workspaces = [proj.to_string_lossy().to_string()];

        let canonical = proj.join("AGENTS.md");
        let lower = proj.join("agents.md");

        assert!(validate_path_against(&canonical.to_string_lossy(), &workspaces).is_ok());
        assert!(validate_path_against(&lower.to_string_lossy(), &workspaces).is_ok());

        let wrong_name = proj.join("NOTES.md");
        assert!(validate_path_against(&wrong_name.to_string_lossy(), &workspaces).is_err());

        let outside = temp.path().join("other").join("AGENTS.md");
        assert!(validate_path_against(&outside.to_string_lossy(), &workspaces).is_err());
    }

    #[test]
    fn write_then_read_round_trip() {
        let temp = TempDir::new().unwrap();
        let proj = temp.path().join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        let workspaces = [proj.to_string_lossy().to_string()];
        let target = proj.join("AGENTS.md");
        let abs_path = target.to_string_lossy().to_string();

        assert_eq!(read_instruction_at(&abs_path, &workspaces).unwrap(), "");

        write_instruction_at(&abs_path, "# hello agents", &workspaces).unwrap();
        assert_eq!(read_instruction_at(&abs_path, &workspaces).unwrap(), "# hello agents");
    }

    #[test]
    fn workspace_paths_merges_workspace_json_and_session_cwds_deduped() {
        let temp = TempDir::new().unwrap();
        let storages = temp.path().join("storages");
        std::fs::create_dir_all(&storages).unwrap();
        std::fs::write(
            storages.join("workspace.json"),
            r#"{"tables":{"workspaces":{"w1":{"path":"/proj/alpha"}}}}"#,
        )
        .unwrap();
        std::fs::write(
            storages.join("session_projcache.json"),
            r#"{"tables":{"sessions":{
                "s1":{"identity":{"cwd":"/proj/alpha"}},
                "s2":{"identity":{"cwd":"/proj/beta"}}
            }}}"#,
        )
        .unwrap();

        let paths = workspace_paths(temp.path());
        assert_eq!(paths, ["/proj/alpha".to_string(), "/proj/beta".to_string()]);
    }
}
