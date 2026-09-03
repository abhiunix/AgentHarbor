use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const MAX_CONFIG_BYTES: usize = 2 * 1024 * 1024;
const MAX_SKILL_FILE_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexConfigSnapshot {
    pub content: String,
    pub path: String,
    pub exists: bool,
    pub revision: String,
}

pub(crate) fn codex_home() -> Result<PathBuf, String> {
    crate::utils::codex_paths::codex_home()
}

// ── Skill struct ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexSkill {
    pub name: String,
    pub file_path: String,
    pub has_scripts: bool,
    pub has_resources: bool,
    pub scope: String,
    pub source_root: String,
}

// ── List skills ─────────────────────────────────────────────────────────────

fn canonical_project_path(project_path: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(project_path);
    if !path.is_absolute() {
        return Err("Project path must be absolute".to_string());
    }

    let canonical =
        dunce::canonicalize(&path).map_err(|e| format!("Failed to resolve project path: {}", e))?;
    if !canonical.is_dir() {
        return Err("Project path is not a directory".to_string());
    }
    Ok(canonical)
}

fn checked_project_skill_root(project: &Path, relative: &Path) -> Result<PathBuf, String> {
    let candidate = project.join(relative);
    let mut existing_ancestor = candidate.as_path();
    while !existing_ancestor.exists() {
        existing_ancestor = existing_ancestor
            .parent()
            .ok_or_else(|| "Project skill root has no existing ancestor".to_string())?;
    }

    let canonical_ancestor = dunce::canonicalize(existing_ancestor)
        .map_err(|e| format!("Failed to resolve project skill root: {}", e))?;
    if !canonical_ancestor.starts_with(project) {
        return Err(format!(
            "Project skill root escapes the selected project: {}",
            candidate.display()
        ));
    }
    Ok(candidate)
}

fn configured_skill_roots(project_path: Option<&str>) -> Result<Vec<(PathBuf, String)>, String> {
    if let Some(project_path) = project_path {
        let project = canonical_project_path(project_path)?;
        return Ok(vec![
            (
                checked_project_skill_root(&project, Path::new(".agents/skills"))?,
                "project".to_string(),
            ),
            (
                checked_project_skill_root(&project, Path::new(".codex/skills"))?,
                "project-legacy".to_string(),
            ),
        ]);
    }

    let mut roots = vec![(codex_home()?.join("skills"), "global".to_string())];
    if let Some(home) = dirs::home_dir() {
        roots.push((home.join(".agents").join("skills"), "user".to_string()));
    }
    Ok(roots)
}

fn collect_skills_from_root(
    root: &Path,
    scope: &str,
    seen: &mut HashSet<PathBuf>,
    skills: &mut Vec<CodexSkill>,
) {
    let canonical_root = match dunce::canonicalize(root) {
        Ok(path) if path.is_dir() => path,
        _ => return,
    };

    let entries = match fs::read_dir(&canonical_root) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let entry_path = match dunce::canonicalize(entry.path()) {
            Ok(path) if path.is_dir() && path.starts_with(&canonical_root) => path,
            _ => continue,
        };
        if !seen.insert(entry_path.clone()) {
            continue;
        }

        let skill_md = entry_path.join("SKILL.md");
        if !skill_md.is_file() {
            continue;
        }
        let Some(name) = entry_path.file_name() else {
            continue;
        };

        skills.push(CodexSkill {
            name: name.to_string_lossy().to_string(),
            file_path: entry_path.to_string_lossy().to_string(),
            has_scripts: entry_path.join("scripts").is_dir(),
            has_resources: entry_path.join("references").is_dir()
                || entry_path.join("resources").is_dir()
                || entry_path.join("assets").is_dir(),
            scope: scope.to_string(),
            source_root: canonical_root.to_string_lossy().to_string(),
        });
    }
}

#[tauri::command]
pub fn list_codex_skills(project_path: Option<String>) -> Result<Vec<CodexSkill>, String> {
    let mut skills = Vec::new();
    let mut seen = HashSet::new();
    for (root, scope) in configured_skill_roots(project_path.as_deref())? {
        collect_skills_from_root(&root, &scope, &mut seen, &mut skills);
    }

    skills.sort_by(|a, b| a.name.cmp(&b.name).then(a.file_path.cmp(&b.file_path)));
    Ok(skills)
}

// ── Read skill file ─────────────────────────────────────────────────────────

#[tauri::command]
pub fn read_codex_skill_file(
    file_path: String,
    project_path: Option<String>,
) -> Result<String, String> {
    let requested = dunce::canonicalize(&file_path)
        .map_err(|e| format!("Failed to resolve skill file: {}", e))?;
    if !requested.is_file() {
        return Err("Skill path is not a file".to_string());
    }

    let allowed = configured_skill_roots(project_path.as_deref())?
        .into_iter()
        .filter_map(|(root, _)| dunce::canonicalize(root).ok())
        .any(|root| requested.starts_with(root));
    if !allowed {
        return Err("Skill file is outside the allowed Codex skill roots".to_string());
    }

    read_utf8_file_with_limit(&requested, MAX_SKILL_FILE_BYTES, "Codex skill file")
}

// ── Config read/write ───────────────────────────────────────────────────────

#[tauri::command]
pub fn get_codex_home_path() -> Result<String, String> {
    Ok(codex_home()?.to_string_lossy().to_string())
}

#[tauri::command]
pub fn read_codex_config() -> Result<String, String> {
    let path = codex_home()?.join("config.toml");
    if !path.exists() {
        return Ok(String::new());
    }
    read_utf8_file_with_limit(&path, MAX_CONFIG_BYTES as u64, "Codex config")
}

fn config_revision(exists: bool, content: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(if exists {
        &b"file\0"[..]
    } else {
        &b"missing\0"[..]
    });
    digest.update(content.as_bytes());
    format!("{:x}", digest.finalize())
}

fn read_codex_config_snapshot_sync() -> Result<CodexConfigSnapshot, String> {
    let path = codex_home()?.join("config.toml");
    let exists = path.is_file();
    let content = if exists {
        read_utf8_file_with_limit(&path, MAX_CONFIG_BYTES as u64, "Codex config")?
    } else {
        String::new()
    };
    Ok(CodexConfigSnapshot {
        revision: config_revision(exists, &content),
        content,
        path: path.to_string_lossy().to_string(),
        exists,
    })
}

#[tauri::command]
pub async fn read_codex_config_snapshot() -> Result<CodexConfigSnapshot, String> {
    tauri::async_runtime::spawn_blocking(read_codex_config_snapshot_sync)
        .await
        .map_err(|error| format!("Codex config task failed: {error}"))?
}

fn read_utf8_file_with_limit(path: &Path, max_bytes: u64, label: &str) -> Result<String, String> {
    let metadata = fs::metadata(path)
        .map_err(|e| format!("Failed to inspect {}: {}", label.to_lowercase(), e))?;
    if metadata.len() > max_bytes {
        return Err(format!(
            "{} exceeds the {} MiB safety limit",
            label,
            max_bytes / (1024 * 1024)
        ));
    }
    fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {}", label.to_lowercase(), e))
}

#[tauri::command]
pub fn write_codex_config(content: String) -> Result<(), String> {
    validate_codex_config(&content)?;
    let path = codex_home()?.join("config.toml");
    write_codex_config_file(&path, &content)
}

fn write_codex_config_snapshot_sync(
    content: String,
    expected_revision: String,
) -> Result<CodexConfigSnapshot, String> {
    validate_codex_config(&content)?;
    let current = read_codex_config_snapshot_sync()?;
    if current.revision != expected_revision {
        return Err(
            "Codex config changed outside AgentHarbor. Refresh before saving so those changes are not overwritten."
                .into(),
        );
    }
    write_codex_config_file(Path::new(&current.path), &content)?;
    read_codex_config_snapshot_sync()
}

#[tauri::command]
pub async fn write_codex_config_snapshot(
    content: String,
    expected_revision: String,
) -> Result<CodexConfigSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        write_codex_config_snapshot_sync(content, expected_revision)
    })
    .await
    .map_err(|error| format!("Codex config task failed: {error}"))?
}

fn resolved_config_write_target(path: &Path) -> Result<PathBuf, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            let target = dunce::canonicalize(path)
                .map_err(|e| format!("Failed to resolve Codex config symlink: {}", e))?;
            if !target.is_file() {
                return Err("Codex config symlink must point to a file".to_string());
            }
            Ok(target)
        }
        Ok(metadata) if metadata.is_file() => Ok(path.to_path_buf()),
        Ok(_) => Err("Codex config path is not a file".to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(path.to_path_buf()),
        Err(error) => Err(format!("Failed to inspect Codex config: {}", error)),
    }
}

pub(crate) fn write_codex_config_file(path: &Path, content: &str) -> Result<(), String> {
    let target = resolved_config_write_target(path)?;
    let parent = target
        .parent()
        .ok_or_else(|| "Codex config path has no parent directory".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|e| format!("Failed to create Codex config directory: {}", e))?;

    let existing_permissions = fs::metadata(&target)
        .ok()
        .map(|metadata| metadata.permissions());
    let file_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config.toml");
    let temp_path = parent.join(format!(
        ".{}.agentharbor-{}.tmp",
        file_name,
        uuid::Uuid::new_v4()
    ));

    let write_result = (|| -> Result<(), String> {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            let mode = existing_permissions
                .as_ref()
                .map(|permissions| permissions.mode() & 0o7777)
                .unwrap_or(0o600);
            options.mode(mode);
        }

        let mut file = options
            .open(&temp_path)
            .map_err(|e| format!("Failed to create Codex config temp file: {}", e))?;
        file.write_all(content.as_bytes())
            .map_err(|e| format!("Failed to write Codex config: {}", e))?;
        file.sync_all()
            .map_err(|e| format!("Failed to sync Codex config: {}", e))?;
        drop(file);

        if let Some(permissions) = existing_permissions {
            fs::set_permissions(&temp_path, permissions)
                .map_err(|e| format!("Failed to preserve Codex config permissions: {}", e))?;
        }

        #[cfg(windows)]
        {
            if fs::rename(&temp_path, &target).is_err() {
                if target.exists() {
                    fs::remove_file(&target)
                        .map_err(|e| format!("Failed to replace Codex config: {}", e))?;
                }
                fs::rename(&temp_path, &target)
                    .map_err(|e| format!("Failed to install Codex config: {}", e))?;
            }
        }
        #[cfg(not(windows))]
        fs::rename(&temp_path, &target)
            .map_err(|e| format!("Failed to install Codex config: {}", e))?;

        Ok(())
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    write_result
}

fn validate_codex_config(content: &str) -> Result<(), String> {
    if content.len() > MAX_CONFIG_BYTES {
        return Err("Codex config exceeds the 2 MiB safety limit".to_string());
    }
    content
        .parse::<toml_edit::DocumentMut>()
        .map(|_| ())
        .map_err(|e| format!("Invalid Codex TOML: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_toml_without_rewriting_it() {
        let config = "# keep this comment\nmodel = \"gpt-5.6-sol\"\n";
        assert!(validate_codex_config(config).is_ok());
    }

    #[test]
    fn rejects_invalid_toml() {
        assert!(validate_codex_config("model = [\n").is_err());
    }

    #[test]
    fn rejects_oversized_config() {
        let config = "x".repeat(MAX_CONFIG_BYTES + 1);
        assert!(validate_codex_config(&config).is_err());
    }

    #[test]
    fn config_revision_tracks_content_and_missing_state() {
        let existing = config_revision(true, "same");
        assert_ne!(existing, config_revision(true, "changed"));
        assert_ne!(existing, config_revision(false, "same"));
        assert_eq!(existing.len(), 64);
    }

    #[test]
    fn rejects_oversized_skill_file_reads() {
        let file = tempfile::NamedTempFile::new().unwrap();
        file.as_file().set_len(MAX_SKILL_FILE_BYTES + 1).unwrap();

        let error =
            read_utf8_file_with_limit(file.path(), MAX_SKILL_FILE_BYTES, "Codex skill file")
                .unwrap_err();

        assert!(error.contains("4 MiB safety limit"));
    }

    #[test]
    fn project_skill_roots_use_documented_agents_directory() {
        let dir = tempfile::tempdir().unwrap();
        let roots = configured_skill_roots(Some(dir.path().to_str().unwrap())).unwrap();
        let canonical = dunce::canonicalize(dir.path()).unwrap();
        assert_eq!(roots[0].0, canonical.join(".agents/skills"));
        assert_eq!(roots[0].1, "project");
    }

    #[cfg(unix)]
    #[test]
    fn project_skill_root_symlink_cannot_escape_project() {
        use std::os::unix::fs::symlink;

        let project = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::create_dir_all(project.path().join(".agents")).unwrap();
        symlink(outside.path(), project.path().join(".agents/skills")).unwrap();

        let error = configured_skill_roots(Some(project.path().to_str().unwrap())).unwrap_err();
        assert!(error.contains("escapes the selected project"));
    }

    #[cfg(unix)]
    #[test]
    fn canonical_root_rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let project = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let skills = project.path().join(".agents/skills");
        fs::create_dir_all(&skills).unwrap();
        fs::write(outside.path().join("SKILL.md"), "outside").unwrap();
        symlink(outside.path(), skills.join("escaped")).unwrap();

        let mut seen = HashSet::new();
        let mut found = Vec::new();
        collect_skills_from_root(&skills, "project", &mut seen, &mut found);
        assert!(found.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn config_write_preserves_private_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        fs::write(&path, "model = \"old\"\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

        write_codex_config_file(&path, "model = \"new\"\n").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "model = \"new\"\n");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[cfg(unix)]
    #[test]
    fn config_write_keeps_symlink_and_updates_its_target() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("managed-config.toml");
        let link = directory.path().join("config.toml");
        fs::write(&target, "model = \"old\"\n").unwrap();
        symlink(&target, &link).unwrap();

        write_codex_config_file(&link, "model = \"new\"\n").unwrap();

        assert!(fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(fs::read_to_string(&target).unwrap(), "model = \"new\"\n");
    }

    #[cfg(unix)]
    #[test]
    fn new_config_is_private_by_default() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        write_codex_config_file(&path, "model = \"new\"\n").unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
