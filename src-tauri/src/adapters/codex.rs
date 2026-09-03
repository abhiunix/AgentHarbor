use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use crate::adapters::traits::*;
use crate::models::{AgentDefinition, CompositeId, Skill, UniversalCapability};

pub struct CodexAdapter;

impl CodexAdapter {
    pub fn new() -> Self {
        Self
    }

    fn codex_dir() -> Result<PathBuf, String> {
        crate::utils::codex_paths::codex_home()
    }

    fn global_agents_md_path() -> Result<PathBuf, String> {
        Ok(Self::codex_dir()?.join("AGENTS.md"))
    }

    fn global_skills_dir(&self) -> Result<PathBuf, String> {
        Ok(Self::codex_dir()?.join("skills"))
    }

    fn is_global_deploy(options: Option<&serde_json::Value>) -> bool {
        options
            .and_then(|value| value.get("global"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
    }

    fn canonical_project_root(project_path: &Path) -> Result<PathBuf, String> {
        let canonical = dunce::canonicalize(project_path)
            .map_err(|error| format!("Failed to resolve project path: {}", error))?;
        if !canonical.is_dir() {
            return Err("Project path is not a directory".to_string());
        }
        Ok(canonical)
    }

    fn checked_project_descendant(project_path: &Path, relative: &Path) -> Result<PathBuf, String> {
        let project = Self::canonical_project_root(project_path)?;
        let candidate = project.join(relative);
        let mut existing_ancestor = candidate.as_path();
        while !existing_ancestor.exists() {
            existing_ancestor = existing_ancestor
                .parent()
                .ok_or_else(|| "Project target has no existing ancestor".to_string())?;
        }
        let canonical_ancestor = dunce::canonicalize(existing_ancestor)
            .map_err(|error| format!("Failed to resolve project target: {}", error))?;
        if !canonical_ancestor.starts_with(&project) {
            return Err(format!(
                "Codex target escapes the selected project: {}",
                candidate.display()
            ));
        }
        Ok(candidate)
    }

    fn deployment_rules_path(
        &self,
        project_path: &Path,
        options: Option<&serde_json::Value>,
    ) -> Result<PathBuf, String> {
        if Self::is_global_deploy(options) {
            Self::global_agents_md_path()
        } else {
            Self::checked_project_descendant(project_path, Path::new("AGENTS.md"))
        }
    }

    fn deployment_skills_dir(
        &self,
        project_path: &Path,
        options: Option<&serde_json::Value>,
    ) -> Result<PathBuf, String> {
        if Self::is_global_deploy(options) {
            self.global_skills_dir()
        } else {
            Self::checked_project_descendant(project_path, Path::new(".agents/skills"))
        }
    }

    fn write_file_atomic(&self, path: &Path, content: &str) -> Result<(), String> {
        write_skill_file_atomic(path, content)
    }

    fn deploy_skills(
        &self,
        project_path: &Path,
        capabilities: &[UniversalCapability],
        options: Option<&serde_json::Value>,
    ) -> Result<Vec<PathBuf>, String> {
        let skills: Vec<&Skill> = capabilities
            .iter()
            .filter_map(|c| match c {
                UniversalCapability::Skill(s) => Some(s),
                _ => None,
            })
            .collect();

        if skills.is_empty() {
            return Ok(vec![]);
        }

        validate_skill_file_paths(capabilities)?;

        let skills_dir = self.deployment_skills_dir(project_path, options)?;
        fs::create_dir_all(&skills_dir)
            .map_err(|e| format!("Failed to create skills dir: {}", e))?;
        let canonical_skills_dir = dunce::canonicalize(&skills_dir)
            .map_err(|e| format!("Failed to resolve skills dir: {}", e))?;

        let mut written = Vec::new();

        for skill in &skills {
            let artifact = skill.id.artifact_name(&skill.name);
            let skill_folder = canonical_skills_dir.join(&artifact);
            if fs::symlink_metadata(&skill_folder)
                .is_ok_and(|metadata| metadata.file_type().is_symlink())
            {
                return Err(format!(
                    "Skill folder must not be a symbolic link: {}",
                    skill_folder.display()
                ));
            }
            fs::create_dir_all(&skill_folder)
                .map_err(|e| format!("Failed to create skill folder: {}", e))?;
            let skill_folder = dunce::canonicalize(&skill_folder)
                .map_err(|e| format!("Failed to resolve skill folder: {}", e))?;
            if !skill_folder.starts_with(&canonical_skills_dir) {
                return Err("Skill folder escapes the Codex skills directory".to_string());
            }

            let frontmatter = build_codex_skill_frontmatter(skill);
            let body = skill
                .files
                .iter()
                .find(|f| {
                    let lower = f.path.to_lowercase();
                    lower == "skill.md" || lower.ends_with("/skill.md")
                })
                .map(|f| f.content.as_str())
                .or_else(|| skill.files.first().map(|f| f.content.as_str()))
                .unwrap_or("");

            let skill_md_content = format!("{}\n{}", frontmatter, body);
            let skill_md_path = prepare_skill_output_path(&skill_folder, Path::new("SKILL.md"))?;
            self.write_file_atomic(&skill_md_path, &skill_md_content)?;
            written.push(skill_md_path);

            // Deploy supporting files
            for file in &skill.files {
                let lower = file.path.to_lowercase();
                if lower == "skill.md" || lower.ends_with("/skill.md") {
                    continue;
                }
                let file_path = prepare_skill_output_path(&skill_folder, Path::new(&file.path))?;
                self.write_file_atomic(&file_path, &file.content)?;
                written.push(file_path);
            }
        }

        Ok(written)
    }
}

fn validate_skill_file_paths(capabilities: &[UniversalCapability]) -> Result<(), String> {
    for skill in capabilities
        .iter()
        .filter_map(|capability| match capability {
            UniversalCapability::Skill(skill) => Some(skill),
            _ => None,
        })
    {
        for file in &skill.files {
            let lower = file.path.to_lowercase();
            if lower == "skill.md" || lower.ends_with("/skill.md") {
                continue;
            }
            validate_skill_relative_path(Path::new(&file.path))?;
        }
    }
    Ok(())
}

fn validate_skill_relative_path(path: &Path) -> Result<PathBuf, String> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(format!(
            "Skill file path must be a non-empty relative path: {}",
            path.display()
        ));
    }

    let mut relative = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => relative.push(part),
            _ => {
                return Err(format!(
                    "Skill file path contains an unsafe component: {}",
                    path.display()
                ));
            }
        }
    }
    if relative.as_os_str().is_empty() {
        return Err("Skill file path must not be empty".to_string());
    }
    Ok(relative)
}

fn write_skill_file_atomic(path: &Path, content: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Skill output path has no parent directory".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|e| format!("Failed to create skill output directory: {}", e))?;
    let canonical_parent = dunce::canonicalize(parent)
        .map_err(|e| format!("Failed to resolve skill output directory: {}", e))?;
    if !canonical_parent.is_dir() {
        return Err("Skill output parent is not a directory".to_string());
    }
    let file_name = path
        .file_name()
        .ok_or_else(|| "Skill output path has no file name".to_string())?;
    let target = canonical_parent.join(file_name);

    let existing_permissions = match fs::symlink_metadata(&target) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(format!(
                "Skill output file must not be a symbolic link: {}",
                target.display()
            ));
        }
        Ok(metadata) if metadata.is_file() => Some(metadata.permissions()),
        Ok(_) => {
            return Err(format!(
                "Skill output file is not a regular file: {}",
                target.display()
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(format!(
                "Failed to inspect skill output file {}: {}",
                target.display(),
                error
            ));
        }
    };

    let temp_path = canonical_parent.join(format!(
        ".agentharbor-codex-skill-{}.tmp",
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
            .map_err(|e| format!("Failed to create skill output temp file: {}", e))?;
        file.write_all(content.as_bytes())
            .map_err(|e| format!("Failed to write skill output: {}", e))?;
        file.sync_all()
            .map_err(|e| format!("Failed to sync skill output: {}", e))?;
        drop(file);

        if let Some(permissions) = existing_permissions {
            fs::set_permissions(&temp_path, permissions)
                .map_err(|e| format!("Failed to preserve skill output permissions: {}", e))?;
        }

        #[cfg(windows)]
        {
            if fs::rename(&temp_path, &target).is_err() {
                if target.exists() {
                    fs::remove_file(&target)
                        .map_err(|e| format!("Failed to replace skill output: {}", e))?;
                }
                fs::rename(&temp_path, &target)
                    .map_err(|e| format!("Failed to install skill output: {}", e))?;
            }
        }
        #[cfg(not(windows))]
        fs::rename(&temp_path, &target)
            .map_err(|e| format!("Failed to install skill output: {}", e))?;

        Ok(())
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    write_result
}

fn prepare_skill_output_path(skill_folder: &Path, relative: &Path) -> Result<PathBuf, String> {
    let relative = validate_skill_relative_path(relative)?;
    let canonical_skill_folder = dunce::canonicalize(skill_folder)
        .map_err(|e| format!("Failed to resolve skill folder: {}", e))?;
    if !canonical_skill_folder.is_dir() {
        return Err("Skill folder is not a directory".to_string());
    }

    let components: Vec<_> = relative.components().collect();
    let (file_component, parent_components) = components
        .split_last()
        .ok_or_else(|| "Skill file path must not be empty".to_string())?;
    let Component::Normal(file_name) = file_component else {
        return Err("Skill file name is invalid".to_string());
    };

    let mut current = canonical_skill_folder.clone();
    for component in parent_components {
        let Component::Normal(part) = component else {
            return Err("Skill directory component is invalid".to_string());
        };
        let next = current.join(part);
        match fs::symlink_metadata(&next) {
            Ok(metadata) if !metadata.is_dir() && !metadata.file_type().is_symlink() => {
                return Err(format!(
                    "Skill output parent is not a directory: {}",
                    next.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&next).map_err(|e| {
                    format!(
                        "Failed to create skill output directory {}: {}",
                        next.display(),
                        e
                    )
                })?;
            }
            Err(error) => {
                return Err(format!(
                    "Failed to inspect skill output directory {}: {}",
                    next.display(),
                    error
                ));
            }
        }

        let canonical_next = dunce::canonicalize(&next)
            .map_err(|e| format!("Failed to resolve skill output directory: {}", e))?;
        if !canonical_next.starts_with(&canonical_skill_folder) {
            return Err(format!(
                "Skill output path escapes its skill folder: {}",
                relative.display()
            ));
        }
        current = canonical_next;
    }

    let output = current.join(file_name);
    match fs::symlink_metadata(&output) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(format!(
                "Skill output file must not be a symbolic link: {}",
                output.display()
            ));
        }
        Ok(metadata) if metadata.is_dir() => {
            return Err(format!(
                "Skill output file is a directory: {}",
                output.display()
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "Failed to inspect skill output file {}: {}",
                output.display(),
                error
            ));
        }
    }
    Ok(output)
}

fn checked_skill_preview_path(skills_dir: &Path, relative: &Path) -> Result<PathBuf, String> {
    let relative = validate_skill_relative_path(relative)?;
    let candidate = skills_dir.join(&relative);
    if !skills_dir.exists() {
        return Ok(candidate);
    }

    let canonical_root = dunce::canonicalize(skills_dir)
        .map_err(|e| format!("Failed to resolve skills directory: {}", e))?;
    let mut existing_ancestor = candidate.as_path();
    while !existing_ancestor.exists() {
        existing_ancestor = existing_ancestor
            .parent()
            .ok_or_else(|| "Skill preview path has no existing ancestor".to_string())?;
    }
    let canonical_ancestor = dunce::canonicalize(existing_ancestor)
        .map_err(|e| format!("Failed to resolve skill preview path: {}", e))?;
    if !canonical_ancestor.starts_with(&canonical_root) {
        return Err(format!(
            "Skill preview path escapes the Codex skills directory: {}",
            candidate.display()
        ));
    }
    Ok(candidate)
}

fn removal_candidates(skills_dir: &Path, id: &CompositeId) -> Result<Vec<String>, String> {
    let mut seen = std::collections::HashSet::new();
    let mut candidates = Vec::new();
    for candidate in [id.artifact_name(&id.name), id.name.clone()] {
        if seen.insert(candidate.clone()) {
            candidates.push(candidate);
        }
    }

    // Private artifacts include the display-name slug, but `remove()` only
    // receives the ID. Discover direct children ending in the same stable ID
    // hash so private skills deployed with any display name can be removed.
    if id.is_private() {
        let hash = id.artifact_name("");
        let suffix = format!("-{}", hash);
        let entries = fs::read_dir(skills_dir).map_err(|e| {
            format!(
                "Failed to read skills directory {}: {}",
                skills_dir.display(),
                e
            )
        })?;
        for entry in entries {
            let entry = entry.map_err(|e| {
                format!(
                    "Failed to inspect a skill in {}: {}",
                    skills_dir.display(),
                    e
                )
            })?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if (name == hash || name.ends_with(&suffix)) && seen.insert(name.clone()) {
                candidates.push(name);
            }
        }
    }

    Ok(candidates)
}

fn remove_skill_from_root(
    skills_dir: &Path,
    id: &CompositeId,
    removed: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if !skills_dir.exists() {
        return Ok(());
    }
    let canonical_root = dunce::canonicalize(skills_dir)
        .map_err(|e| format!("Failed to resolve skills directory: {}", e))?;

    for candidate_name in removal_candidates(skills_dir, id)? {
        let candidate = skills_dir.join(candidate_name);
        let metadata = match fs::symlink_metadata(&candidate) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!(
                    "Failed to inspect skill folder {}: {}",
                    candidate.display(),
                    error
                ));
            }
        };

        if metadata.file_type().is_symlink() {
            fs::remove_file(&candidate).map_err(|e| {
                format!("Failed to remove skill link {}: {}", candidate.display(), e)
            })?;
            removed.push(candidate);
            continue;
        }
        if !metadata.is_dir() {
            return Err(format!(
                "Skill removal target is not a directory: {}",
                candidate.display()
            ));
        }

        let canonical_candidate = dunce::canonicalize(&candidate)
            .map_err(|e| format!("Failed to resolve skill folder: {}", e))?;
        if !canonical_candidate.starts_with(&canonical_root) {
            return Err(format!(
                "Skill removal target escapes its skills directory: {}",
                candidate.display()
            ));
        }
        fs::remove_dir_all(&canonical_candidate).map_err(|e| {
            format!(
                "Failed to remove skill folder {}: {}",
                canonical_candidate.display(),
                e
            )
        })?;
        removed.push(candidate);
    }
    Ok(())
}

fn remove_rules_from_file(
    agents_md: &Path,
    capability_ids: &[CompositeId],
    removed: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if !agents_md.exists() {
        return Ok(());
    }
    let content = fs::read_to_string(agents_md)
        .map_err(|e| format!("Failed to read {}: {}", agents_md.display(), e))?;
    let mut new_content = content.clone();
    for id in capability_ids {
        new_content = crate::utils::rule_block::remove_rule(&new_content, &id.to_string());
    }
    if new_content != content {
        crate::commands::codex::write_codex_config_file(agents_md, &new_content)?;
        removed.push(agents_md.to_path_buf());
    }
    Ok(())
}

fn remove_from_scopes(
    scopes: impl IntoIterator<Item = (PathBuf, PathBuf)>,
    capability_ids: &[CompositeId],
) -> Result<Vec<PathBuf>, String> {
    let mut removed = Vec::new();
    for (skills_dir, agents_md) in scopes {
        for id in capability_ids {
            remove_skill_from_root(&skills_dir, id, &mut removed)?;
        }
        remove_rules_from_file(&agents_md, capability_ids, &mut removed)?;
    }
    Ok(removed)
}

fn build_codex_skill_frontmatter(skill: &Skill) -> String {
    let mut fm = String::from("---\n");
    fm.push_str(&format!("name: {}\n", skill.name));
    fm.push_str(&format!(
        "description: \"{}\"\n",
        skill.description.replace('"', "\\\"")
    ));
    if let Some(ref license) = skill.license {
        fm.push_str(&format!("license: {}\n", license));
    }
    if let Some(ref tools) = skill.allowed_tools {
        if !tools.is_empty() {
            fm.push_str(&format!("allowed-tools: {}\n", tools.join(" ")));
        }
    }
    fm.push_str("metadata:\n");
    fm.push_str(&format!("  author: {}\n", skill.author));
    fm.push_str(&format!("  version: {}\n", skill.version));
    fm.push_str("---\n");
    fm
}

impl AgentAdapter for CodexAdapter {
    fn id(&self) -> &str {
        "codex"
    }

    fn name(&self) -> &str {
        "Codex"
    }

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            mcp: false,
            rules: true,
            skills: true,
            hooks: false,
            plugins: false,
            agents: false,
            custom: false,
        }
    }

    fn detect(&self, _project_path: &Path) -> bool {
        Self::codex_dir().is_ok_and(|codex_dir| codex_dir.exists())
    }

    fn read_config(&self, project_path: &Path) -> Result<AgentConfig, String> {
        let mut config = AgentConfig::default();
        let skills_dir =
            Self::checked_project_descendant(project_path, Path::new(".agents/skills"))?;
        if skills_dir.exists() {
            if let Ok(entries) = fs::read_dir(&skills_dir) {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        if let Some(name) = entry.file_name().to_str() {
                            config.skills.push(name.to_string());
                        }
                    }
                }
            }
        }
        Ok(config)
    }

    fn diff(
        &self,
        project_path: &Path,
        capabilities: &[UniversalCapability],
        _agents: &[AgentDefinition],
        options: Option<&serde_json::Value>,
    ) -> Result<Vec<ConfigDiffEntry>, String> {
        validate_skill_file_paths(capabilities)?;
        let mut entries = Vec::new();

        // Rules target the selected project unless this is an explicit global deploy.
        let rules: Vec<_> = capabilities
            .iter()
            .filter_map(|c| {
                if let UniversalCapability::Rule(r) = c {
                    Some(r)
                } else {
                    None
                }
            })
            .collect();
        if !rules.is_empty() {
            let path = self.deployment_rules_path(project_path, options)?;
            let current = if path.exists() {
                Some(
                    fs::read_to_string(&path)
                        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?,
                )
            } else {
                None
            };
            let mut proposed = current.clone().unwrap_or_default();
            for rule in &rules {
                proposed = crate::utils::rule_block::inject_rule(
                    &proposed,
                    &rule.id.to_string(),
                    &rule.name,
                    &rule.content,
                );
            }
            entries.push(ConfigDiffEntry {
                file_path: path,
                change_type: if current.is_some() {
                    ChangeType::Modify
                } else {
                    ChangeType::Add
                },
                current_content: current,
                proposed_content: proposed,
                merged_content: None,
            });
        }

        let skills: Vec<&Skill> = capabilities
            .iter()
            .filter_map(|c| match c {
                UniversalCapability::Skill(s) => Some(s),
                _ => None,
            })
            .collect();

        if !skills.is_empty() {
            let skills_dir = self.deployment_skills_dir(project_path, options)?;
            for skill in &skills {
                let artifact = skill.id.artifact_name(&skill.name);
                let skill_md_path = checked_skill_preview_path(
                    &skills_dir,
                    &Path::new(&artifact).join("SKILL.md"),
                )?;

                let frontmatter = build_codex_skill_frontmatter(skill);
                let body = skill
                    .files
                    .iter()
                    .find(|f| f.path.to_lowercase() == "skill.md")
                    .map(|f| f.content.as_str())
                    .or_else(|| skill.files.first().map(|f| f.content.as_str()))
                    .unwrap_or("");
                let proposed = format!("{}\n{}", frontmatter, body);

                let current = if skill_md_path.exists() {
                    Some(fs::read_to_string(&skill_md_path).map_err(|e| {
                        format!("Failed to read {}: {}", skill_md_path.display(), e)
                    })?)
                } else {
                    None
                };
                let change_type = if current.is_some() {
                    ChangeType::Modify
                } else {
                    ChangeType::Add
                };

                entries.push(ConfigDiffEntry {
                    file_path: skill_md_path,
                    change_type,
                    current_content: current,
                    proposed_content: proposed,
                    merged_content: None,
                });
            }
        }

        Ok(entries)
    }

    fn deploy(
        &self,
        project_path: &Path,
        capabilities: &[UniversalCapability],
        _agents: &[AgentDefinition],
        _strategy: DeployStrategy,
        options: Option<&serde_json::Value>,
    ) -> Result<DeployResult, String> {
        // Validate registry-provided paths before writing any rule or skill.
        validate_skill_file_paths(capabilities)?;
        let mut all_files = Vec::new();
        let mut all_errors = Vec::new();

        // Rules target the selected project unless this is an explicit global deploy.
        let rules: Vec<_> = capabilities
            .iter()
            .filter_map(|c| {
                if let UniversalCapability::Rule(r) = c {
                    Some(r)
                } else {
                    None
                }
            })
            .collect();
        if !rules.is_empty() {
            let path = self.deployment_rules_path(project_path, options)?;
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
            }
            let mut content = if path.exists() {
                fs::read_to_string(&path)
                    .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?
            } else {
                String::new()
            };
            for rule in rules {
                content = crate::utils::rule_block::inject_rule(
                    &content,
                    &rule.id.to_string(),
                    &rule.name,
                    &rule.content,
                );
            }
            match crate::commands::codex::write_codex_config_file(&path, &content) {
                Ok(()) => all_files.push(path),
                Err(e) => all_errors.push(e),
            }
        }

        match self.deploy_skills(project_path, capabilities, options) {
            Ok(files) => all_files.extend(files),
            Err(e) => all_errors.push(e),
        }

        if all_errors.is_empty() {
            Ok(DeployResult::success(all_files))
        } else {
            Ok(DeployResult {
                success: all_files.len() > 0,
                files_written: all_files,
                errors: all_errors,
            })
        }
    }

    fn remove(
        &self,
        project_path: &Path,
        capability_ids: &[CompositeId],
        _agent_ids: &[CompositeId],
    ) -> Result<RemoveResult, String> {
        let scopes = [
            (
                Self::checked_project_descendant(project_path, Path::new(".agents/skills"))?,
                Self::checked_project_descendant(project_path, Path::new("AGENTS.md"))?,
            ),
            (self.global_skills_dir()?, Self::global_agents_md_path()?),
        ];
        let removed = remove_from_scopes(scopes, capability_ids)?;
        Ok(RemoveResult::success(removed))
    }

    fn managed_paths(&self, project_path: &Path) -> Vec<PathBuf> {
        [Path::new("AGENTS.md"), Path::new(".agents/skills")]
            .into_iter()
            .filter_map(|relative| Self::checked_project_descendant(project_path, relative).ok())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supporting_skill_paths_reject_parent_components() {
        let error =
            validate_skill_relative_path(Path::new("references/../../outside.txt")).unwrap_err();
        assert!(error.contains("unsafe component"));
    }

    #[test]
    fn unsafe_skill_path_is_rejected_before_a_rule_is_written() {
        let project = tempfile::tempdir().unwrap();
        let rule = UniversalCapability::Rule(crate::models::Rule {
            id: CompositeId::new("community", "safe-rule").unwrap(),
            name: "Safe Rule".to_string(),
            description: "Test rule".to_string(),
            version: "1.0.0".to_string(),
            author: "test".to_string(),
            visibility: crate::models::Visibility::Public,
            tags: Vec::new(),
            scope: "project".to_string(),
            content: "Keep this safe.".to_string(),
            env: std::collections::HashMap::new(),
            compatible_agents: vec!["codex".to_string()],
            category: None,
            author_github: None,
            source_info: None,
            stats: None,
        });
        let skill = UniversalCapability::Skill(Skill {
            id: CompositeId::new("community", "unsafe-skill").unwrap(),
            name: "Unsafe Skill".to_string(),
            description: "Test skill".to_string(),
            version: "1.0.0".to_string(),
            author: "test".to_string(),
            visibility: crate::models::Visibility::Public,
            tags: Vec::new(),
            scope: String::new(),
            files: vec![crate::models::SkillFile {
                path: "references/../../outside.txt".to_string(),
                content: "outside".to_string(),
            }],
            env: std::collections::HashMap::new(),
            compatible_agents: vec!["codex".to_string()],
            allowed_tools: None,
            model: None,
            context: None,
            agent: None,
            argument_hint: None,
            license: None,
            category: None,
            author_github: None,
            source_info: None,
            stats: None,
        });

        let error = CodexAdapter::new()
            .deploy(
                project.path(),
                &[rule, skill],
                &[],
                DeployStrategy::Merge,
                None,
            )
            .expect_err("unsafe skill path should fail the deployment");

        assert!(error.contains("unsafe component"));
        assert!(!project.path().join("AGENTS.md").exists());
    }

    #[cfg(unix)]
    #[test]
    fn supporting_skill_paths_reject_absolute_paths() {
        let error = validate_skill_relative_path(Path::new("/tmp/outside.txt")).unwrap_err();
        assert!(error.contains("relative path"));
    }

    #[test]
    fn project_deploy_targets_project_instructions_and_skills() {
        let project = tempfile::tempdir().unwrap();
        let canonical_project = dunce::canonicalize(project.path()).unwrap();
        let adapter = CodexAdapter::new();

        let instructions = adapter.deployment_rules_path(project.path(), None).unwrap();
        let skills = adapter.deployment_skills_dir(project.path(), None).unwrap();

        assert_eq!(instructions, canonical_project.join("AGENTS.md"));
        assert_eq!(skills, canonical_project.join(".agents/skills"));
    }

    #[test]
    fn managed_paths_include_project_instructions_and_skills() {
        let project = tempfile::tempdir().unwrap();
        let canonical_project = dunce::canonicalize(project.path()).unwrap();
        let paths = CodexAdapter::new().managed_paths(project.path());

        assert_eq!(
            paths,
            vec![
                canonical_project.join("AGENTS.md"),
                canonical_project.join(".agents/skills"),
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn project_skills_root_symlink_cannot_escape_project() {
        use std::os::unix::fs::symlink;

        let project = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::create_dir_all(project.path().join(".agents")).unwrap();
        symlink(outside.path(), project.path().join(".agents/skills")).unwrap();

        let adapter = CodexAdapter::new();
        let error = adapter
            .deployment_skills_dir(project.path(), None)
            .unwrap_err();
        assert!(error.contains("escapes the selected project"));
    }

    #[cfg(unix)]
    #[test]
    fn supporting_skill_parent_symlink_cannot_escape_skill_folder() {
        use std::os::unix::fs::symlink;

        let skill_folder = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        symlink(outside.path(), skill_folder.path().join("references")).unwrap();

        let error =
            prepare_skill_output_path(skill_folder.path(), Path::new("references/outside.txt"))
                .unwrap_err();
        assert!(error.contains("escapes its skill folder"));
        assert!(!outside.path().join("outside.txt").exists());
    }

    #[cfg(unix)]
    #[test]
    fn supporting_skill_file_symlink_is_not_replaced() {
        use std::os::unix::fs::symlink;

        let skill_folder = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        symlink(outside.path(), skill_folder.path().join("linked.txt")).unwrap();

        let error =
            prepare_skill_output_path(skill_folder.path(), Path::new("linked.txt")).unwrap_err();
        assert!(error.contains("must not be a symbolic link"));
    }

    #[cfg(unix)]
    #[test]
    fn skill_write_does_not_follow_a_predictable_temp_symlink() {
        use std::os::unix::fs::symlink;

        let skill_folder = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        fs::write(outside.path(), "outside").unwrap();
        symlink(outside.path(), skill_folder.path().join("SKILL.tmp")).unwrap();
        let output = prepare_skill_output_path(skill_folder.path(), Path::new("SKILL.md")).unwrap();

        write_skill_file_atomic(&output, "inside").unwrap();

        assert_eq!(fs::read_to_string(&output).unwrap(), "inside");
        assert_eq!(fs::read_to_string(outside.path()).unwrap(), "outside");
    }

    #[cfg(unix)]
    #[test]
    fn preview_rejects_an_existing_skill_folder_symlink_escape() {
        use std::os::unix::fs::symlink;

        let skills = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("SKILL.md"), "outside").unwrap();
        symlink(outside.path(), skills.path().join("escaped")).unwrap();

        let error =
            checked_skill_preview_path(skills.path(), Path::new("escaped/SKILL.md")).unwrap_err();
        assert!(error.contains("escapes the Codex skills directory"));
    }

    #[test]
    fn removal_checks_project_and_global_targets() {
        let project = tempfile::tempdir().unwrap();
        let global = tempfile::tempdir().unwrap();
        let id = CompositeId::new("community", "sample-skill").unwrap();

        let project_skills = project.path().join("skills");
        let global_skills = global.path().join("skills");
        fs::create_dir_all(project_skills.join("sample-skill")).unwrap();
        fs::create_dir_all(global_skills.join("sample-skill")).unwrap();

        let project_agents = project.path().join("AGENTS.md");
        let global_agents = global.path().join("AGENTS.md");
        let rule = crate::utils::rule_block::inject_rule(
            "",
            &id.to_string(),
            "Sample rule",
            "Sample content",
        );
        fs::write(&project_agents, &rule).unwrap();
        fs::write(&global_agents, &rule).unwrap();

        let removed = remove_from_scopes(
            [
                (project_skills.clone(), project_agents.clone()),
                (global_skills.clone(), global_agents.clone()),
            ],
            std::slice::from_ref(&id),
        )
        .unwrap();

        assert!(!project_skills.join("sample-skill").exists());
        assert!(!global_skills.join("sample-skill").exists());
        assert!(!fs::read_to_string(&project_agents)
            .unwrap()
            .contains(&id.to_string()));
        assert!(!fs::read_to_string(&global_agents)
            .unwrap()
            .contains(&id.to_string()));
        assert_eq!(removed.len(), 4);
    }

    #[test]
    fn private_skill_removal_finds_the_deployed_display_name_artifact() {
        let skills = tempfile::tempdir().unwrap();
        let id = CompositeId::new("private-author", "private-skill").unwrap();
        let deployed_artifact = id.artifact_name("Friendly Display Name");
        assert_ne!(deployed_artifact, id.artifact_name(&id.name));
        fs::create_dir_all(skills.path().join(&deployed_artifact)).unwrap();
        let mut removed = Vec::new();

        remove_skill_from_root(skills.path(), &id, &mut removed).unwrap();

        assert!(!skills.path().join(&deployed_artifact).exists());
        assert_eq!(removed, vec![skills.path().join(deployed_artifact)]);
    }

    #[cfg(unix)]
    #[test]
    fn removal_unlinks_skill_symlink_without_deleting_target() {
        use std::os::unix::fs::symlink;

        let skills = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let protected = outside.path().join("keep.txt");
        fs::write(&protected, "keep").unwrap();
        symlink(outside.path(), skills.path().join("linked-skill")).unwrap();
        let id = CompositeId::new("community", "linked-skill").unwrap();
        let mut removed = Vec::new();

        remove_skill_from_root(skills.path(), &id, &mut removed).unwrap();

        assert!(!skills.path().join("linked-skill").exists());
        assert_eq!(fs::read_to_string(protected).unwrap(), "keep");
        assert_eq!(removed.len(), 1);
    }
}
