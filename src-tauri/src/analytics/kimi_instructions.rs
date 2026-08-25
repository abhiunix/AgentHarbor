//! Kimi Code Instructions — view/edit per-project `AGENTS.md` files.
//! Kimi's CLI loads instructions from, per project directory:
//!   `<dir>/.kimi/AGENTS.md`, `<dir>/AGENTS.md`, `<dir>/agents.md` (uppercase
//!   preferred). There is no global `~/.kimi/AGENTS.md` equivalent.
//! Reuses `analytics::kimi_v2`'s `build_dir_map()` for the user's project list
//! (`~/.kimi/kimi.json` → `work_dirs[].path`), matching `kimi_plans.rs`'s
//! reuse pattern.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KimiInstructionFile {
    pub project_path: String,
    pub project_name: String,
    pub abs_path: String,
    pub exists: bool,
    pub size_bytes: Option<u64>,
    pub modified: Option<String>,
}

fn project_name_from_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

fn metadata_to_rfc3339(m: &std::fs::Metadata) -> Option<String> {
    m.modified().ok().map(|t| {
        let dt: chrono::DateTime<chrono::Utc> = t.into();
        dt.to_rfc3339()
    })
}

fn build_entry(project_path: &str, project_name: &str, path: &Path) -> KimiInstructionFile {
    let metadata = std::fs::metadata(path).ok();
    let exists = metadata.is_some();
    let size_bytes = metadata.as_ref().map(|m| m.len());
    let modified = metadata.as_ref().and_then(metadata_to_rfc3339);
    KimiInstructionFile {
        project_path: project_path.to_string(),
        project_name: project_name.to_string(),
        abs_path: path.to_string_lossy().to_string(),
        exists,
        size_bytes,
        modified,
    }
}

/// The user's Kimi project directories, from `~/.kimi/kimi.json`'s
/// `work_dirs[].path` (deduped).
fn work_dir_paths() -> Vec<String> {
    let mut paths: Vec<String> = crate::analytics::kimi_v2::build_dir_map()
        .into_values()
        .map(|(path, _)| path)
        .collect();
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

/// For each work dir: the canonical `<dir>/AGENTS.md` (exists or not), plus
/// any already-existing `<dir>/.kimi/AGENTS.md` or `<dir>/agents.md`.
/// Newest-modified (or existing) entries sort first.
fn list_instruction_files_for(work_dirs: &[String]) -> Vec<KimiInstructionFile> {
    let mut out = Vec::new();
    for project_path in work_dirs {
        let project_name = project_name_from_path(project_path);
        let base = Path::new(project_path);
        let canonical = base.join("AGENTS.md");

        out.push(build_entry(project_path, &project_name, &canonical));

        for extra in [base.join(".kimi").join("AGENTS.md"), base.join("agents.md")] {
            if extra.is_file() && !same_file(&extra, &canonical) {
                out.push(build_entry(project_path, &project_name, &extra));
            }
        }
    }
    out.sort_by(|a, b| b.exists.cmp(&a.exists).then_with(|| b.modified.cmp(&a.modified)));
    out
}

/// Path guard: the basename must be exactly `AGENTS.md` or `agents.md`, AND
/// `abs_path` must be one of the allowed shapes under a known work dir
/// (`<dir>/AGENTS.md`, `<dir>/agents.md`, `<dir>/.kimi/AGENTS.md`). Compared
/// directly against the known work dirs — never canonicalized, so a path
/// that doesn't exist yet (create flow) still validates correctly.
fn validate_path_against(abs_path: &str, work_dirs: &[String]) -> Result<(), String> {
    if abs_path.contains("..") {
        return Err("Invalid instructions file path".to_string());
    }
    let path = Path::new(abs_path);
    let basename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if basename != "AGENTS.md" && basename != "agents.md" {
        return Err("Invalid instructions file name".to_string());
    }

    for project_path in work_dirs {
        let base = Path::new(project_path);
        let allowed = [
            base.join("AGENTS.md"),
            base.join("agents.md"),
            base.join(".kimi").join("AGENTS.md"),
        ];
        if allowed.iter().any(|p| p == path) {
            return Ok(());
        }
    }
    Err("Path is outside known Kimi project directories".to_string())
}

fn read_instruction_at(abs_path: &str, work_dirs: &[String]) -> Result<String, String> {
    validate_path_against(abs_path, work_dirs)?;
    let path = Path::new(abs_path);
    if !path.is_file() {
        return Ok(String::new());
    }
    std::fs::read_to_string(path).map_err(|e| e.to_string())
}

fn write_instruction_at(abs_path: &str, content: &str, work_dirs: &[String]) -> Result<(), String> {
    validate_path_against(abs_path, work_dirs)?;
    crate::utils::paths::atomic_write_str(Path::new(abs_path), content)
}

// ── Tauri Commands ───────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_kimi_instruction_files() -> Vec<KimiInstructionFile> {
    list_instruction_files_for(&work_dir_paths())
}

#[tauri::command]
pub fn read_kimi_instruction(abs_path: String) -> Result<String, String> {
    read_instruction_at(&abs_path, &work_dir_paths())
}

#[tauri::command]
pub fn write_kimi_instruction(abs_path: String, content: String) -> Result<(), String> {
    write_instruction_at(&abs_path, &content, &work_dir_paths())
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn lists_canonical_file_and_existing_variants() {
        // Note: on a case-insensitive filesystem (the macOS/Windows default),
        // `AGENTS.md` and `agents.md` in the SAME directory are one file, so
        // this fixture keeps the two uppercase/lowercase names in different
        // directories to stay meaningful across filesystems.
        let temp = TempDir::new().unwrap();
        let proj_a = temp.path().join("proj-a"); // canonical AGENTS.md exists
        let proj_b = temp.path().join("proj-b"); // only .kimi/AGENTS.md exists
        let proj_c = temp.path().join("proj-c"); // no instructions file at all
        std::fs::create_dir_all(&proj_a).unwrap();
        std::fs::create_dir_all(proj_b.join(".kimi")).unwrap();
        std::fs::create_dir_all(&proj_c).unwrap();

        std::fs::write(proj_a.join("AGENTS.md"), "# a").unwrap();
        std::fs::write(proj_b.join(".kimi").join("AGENTS.md"), "# b kimi").unwrap();

        let work_dirs = [
            proj_a.to_string_lossy().to_string(),
            proj_b.to_string_lossy().to_string(),
            proj_c.to_string_lossy().to_string(),
        ];
        let files = list_instruction_files_for(&work_dirs);

        let a_entries: Vec<&KimiInstructionFile> =
            files.iter().filter(|f| f.project_path == work_dirs[0]).collect();
        assert_eq!(a_entries.len(), 1);
        assert!(a_entries[0].exists);
        assert!(a_entries[0].abs_path.ends_with("AGENTS.md"));

        let b_entries: Vec<&KimiInstructionFile> =
            files.iter().filter(|f| f.project_path == work_dirs[1]).collect();
        assert_eq!(b_entries.len(), 2);
        let missing_count = b_entries.iter().filter(|f| !f.exists).count();
        assert_eq!(missing_count, 1);
        let present_count = b_entries.iter().filter(|f| f.exists).count();
        assert_eq!(present_count, 1);

        let c_entries: Vec<&KimiInstructionFile> =
            files.iter().filter(|f| f.project_path == work_dirs[2]).collect();
        assert_eq!(c_entries.len(), 1);
        assert!(!c_entries[0].exists);
    }

    #[test]
    fn guard_accepts_allowed_shapes_and_rejects_others() {
        let temp = TempDir::new().unwrap();
        let proj = temp.path().join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        let work_dirs = [proj.to_string_lossy().to_string()];

        let canonical = proj.join("AGENTS.md");
        let lower = proj.join("agents.md");
        let kimi_dir = proj.join(".kimi").join("AGENTS.md");

        assert!(validate_path_against(&canonical.to_string_lossy(), &work_dirs).is_ok());
        assert!(validate_path_against(&lower.to_string_lossy(), &work_dirs).is_ok());
        assert!(validate_path_against(&kimi_dir.to_string_lossy(), &work_dirs).is_ok());

        let wrong_name = proj.join("NOTES.md");
        assert!(validate_path_against(&wrong_name.to_string_lossy(), &work_dirs).is_err());

        let outside = temp.path().join("other").join("AGENTS.md");
        assert!(validate_path_against(&outside.to_string_lossy(), &work_dirs).is_err());
    }

    #[test]
    fn write_then_read_round_trip() {
        let temp = TempDir::new().unwrap();
        let proj = temp.path().join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        let work_dirs = [proj.to_string_lossy().to_string()];
        let target = proj.join("AGENTS.md");
        let abs_path = target.to_string_lossy().to_string();

        assert_eq!(read_instruction_at(&abs_path, &work_dirs).unwrap(), "");

        write_instruction_at(&abs_path, "# hello agents", &work_dirs).unwrap();
        assert_eq!(read_instruction_at(&abs_path, &work_dirs).unwrap(), "# hello agents");

        // .kimi/AGENTS.md subpath: parent dir must be auto-created on write.
        let nested = proj.join(".kimi").join("AGENTS.md");
        let nested_path = nested.to_string_lossy().to_string();
        write_instruction_at(&nested_path, "# nested", &work_dirs).unwrap();
        assert_eq!(read_instruction_at(&nested_path, &work_dirs).unwrap(), "# nested");
    }
}
