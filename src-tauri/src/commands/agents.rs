use crate::models::composite_id::to_kebab_slug;
use crate::models::{AgentDefinition, CompositeId};
use crate::utils::markdown::{
    extract_prose_agent, import_model_was_defaulted, parse_agent_md_lenient,
};
use crate::utils::paths::{app_data_dir, atomic_write_str, normalize_line_endings};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

fn get_custom_agents_dir() -> PathBuf {
    crate::utils::paths::app_data_dir().join("agents")
}

#[tauri::command]
pub fn save_agent(agent: AgentDefinition) -> Result<AgentDefinition, String> {
    let agents_dir = get_custom_agents_dir();
    
    if !agents_dir.exists() {
        fs::create_dir_all(&agents_dir)
            .map_err(|e| format!("Failed to create agents directory: {}", e))?;
    }

    let filename = format!("{}.json", agent.id.name);
    let filepath = agents_dir.join(&filename);
    let temp_filepath = agents_dir.join(format!("{}.tmp", agent.id.name));

    let json = serde_json::to_string_pretty(&agent)
        .map_err(|e| format!("Failed to serialize agent: {}", e))?;

    fs::write(&temp_filepath, &json)
        .map_err(|e| format!("Failed to write agent file: {}", e))?;

    fs::rename(&temp_filepath, &filepath)
        .map_err(|e| format!("Failed to save agent file: {}", e))?;

    Ok(agent)
}

#[tauri::command]
pub fn delete_agent(id: String) -> Result<(), String> {
    let agents_dir = get_custom_agents_dir();
    
    let parts: Vec<&str> = id.splitn(2, '/').collect();
    if parts.len() != 2 {
        return Err("Invalid agent ID format".to_string());
    }
    let name = parts[1];

    let filepath = agents_dir.join(format!("{}.json", name));

    if !filepath.exists() {
        return Err("Agent not found".to_string());
    }

    fs::remove_file(&filepath)
        .map_err(|e| format!("Failed to delete agent: {}", e))?;

    Ok(())
}

const MAX_FILE_BYTES: u64 = 256 * 1024;
const WALK_MAX_DEPTH: usize = 6;

/// A candidate agent discovered by scanning a folder, plus where it came from and how it
/// relates to the existing library.
#[derive(Serialize, Deserialize, Clone)]
pub struct ImportableAgent {
    pub agent: AgentDefinition,
    pub source_tool: String,
    pub source_path: String,
    /// "new" | "duplicate-id" | "content-match"
    pub status: String,
    /// True when the source model wasn't an exact haiku/sonnet/opus and we fell back to sonnet.
    pub model_defaulted: bool,
}

#[derive(Serialize, Deserialize)]
pub struct ImportResult {
    pub imported: usize,
    pub skipped: usize,
    pub conflicts: Vec<String>,
    pub agents: Vec<AgentDefinition>,
}

struct Candidate {
    agent: AgentDefinition,
    source_tool: String,
    source_path: String,
    model_defaulted: bool,
    prompt_hash: String,
}

/// Directory imported agents are written to so `get_all_agents` (which reads the registry
/// custom root) can see them. NOTE: this is deliberately different from `save_agent`'s
/// `app_data_dir()/agents`, whose output is invisible to the loader.
fn imported_agents_dir() -> PathBuf {
    app_data_dir()
        .join("registry")
        .join("custom")
        .join("agents")
}

fn prompt_hash(prompt: &str) -> String {
    let normalized = normalize_line_endings(prompt);
    let mut hasher = Sha256::new();
    hasher.update(normalized.trim().as_bytes());
    format!("{:x}", hasher.finalize())
}

fn short_hash(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())[..6].to_string()
}

/// Paths we never treat as agents. The real trap is `.claude/agents` copies bundled inside
/// IDE-extension directories.
fn is_skipped_path(path: &Path) -> bool {
    let s = path.to_string_lossy().replace('\\', "/");
    s.contains("node_modules")
        || s.contains("/extensions/")
        || s.contains(".vscode")
        || s.contains("/.git/")
}

/// Classify a markdown file by which tool-native agents directory it lives under.
fn classify_tool(normalized_path: &str, include_codex: bool) -> Option<&'static str> {
    if normalized_path.contains("/.claude/agents/") {
        Some("claude-code")
    } else if normalized_path.contains("/.cursor/agents/") {
        Some("cursor")
    } else if normalized_path.contains("/.gemini/agents/") {
        Some("gemini")
    } else if include_codex && normalized_path.contains("/.codex/agents/") {
        Some("codex")
    } else {
        None
    }
}

fn scan_candidates(root: &str, include_codex: bool) -> Result<Vec<Candidate>, String> {
    let root_path = PathBuf::from(root);
    if !root_path.exists() {
        return Err(format!("Path does not exist: {}", root));
    }

    let mut candidates: Vec<Candidate> = Vec::new();
    let mut seen_paths: HashSet<String> = HashSet::new();
    let mut seen_prompts: HashSet<String> = HashSet::new();

    let walker = WalkDir::new(&root_path)
        .max_depth(WALK_MAX_DEPTH)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_skipped_path(e.path()));

    for entry in walker.flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        if ext != "md" {
            continue;
        }
        if entry.metadata().map(|m| m.len()).unwrap_or(0) > MAX_FILE_BYTES {
            continue;
        }

        let normalized = path.to_string_lossy().replace('\\', "/");
        let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("agent");

        // Codex prose AGENTS.md files can live anywhere in the tree (opt-in only).
        let is_codex_prose = include_codex && file_name.eq_ignore_ascii_case("AGENTS.md");
        let tool = classify_tool(&normalized, include_codex);
        if tool.is_none() && !is_codex_prose {
            continue;
        }

        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if content.trim().is_empty() {
            continue;
        }

        let (agent, source_tool, model_defaulted) = if is_codex_prose {
            match extract_prose_agent(&content, stem) {
                // Prose has no frontmatter, so the model is always defaulted.
                Some(a) => (a, "codex".to_string(), true),
                None => continue,
            }
        } else {
            let tool = tool.unwrap();
            let mut agent = parse_agent_md_lenient(&content, tool);
            // Frontmatter lacked a usable `name`: derive the id/name from the file stem.
            if agent.id.name == "imported-agent" {
                let slug = to_kebab_slug(stem);
                if let Ok(id) = CompositeId::new("imported", &slug) {
                    agent.name = stem.to_string();
                    agent.id = id;
                }
            }
            let defaulted = import_model_was_defaulted(&content);
            (agent, tool.to_string(), defaulted)
        };

        let canonical = path
            .canonicalize()
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .to_string();

        // Within-scan dedupe by canonical path and by identical prompt.
        if !seen_paths.insert(canonical.clone()) {
            continue;
        }
        let hash = prompt_hash(&agent.prompt);
        if !seen_prompts.insert(hash.clone()) {
            continue;
        }

        candidates.push(Candidate {
            agent,
            source_tool,
            source_path: canonical,
            model_defaulted,
            prompt_hash: hash,
        });
    }

    Ok(candidates)
}

#[tauri::command]
pub fn preview_import_agents(
    path: String,
    include_codex: bool,
) -> Result<Vec<ImportableAgent>, String> {
    let candidates = scan_candidates(&path, include_codex)?;
    let existing = crate::commands::registry::get_all_agents();
    let existing_ids: HashSet<String> = existing.iter().map(|a| a.id.to_string()).collect();
    let existing_prompts: HashSet<String> =
        existing.iter().map(|a| prompt_hash(&a.prompt)).collect();

    Ok(candidates
        .into_iter()
        .map(|c| {
            let status = if existing_ids.contains(&c.agent.id.to_string()) {
                "duplicate-id"
            } else if existing_prompts.contains(&c.prompt_hash) {
                "content-match"
            } else {
                "new"
            };
            ImportableAgent {
                agent: c.agent,
                source_tool: c.source_tool,
                source_path: c.source_path,
                status: status.to_string(),
                model_defaulted: c.model_defaulted,
            }
        })
        .collect())
}

#[tauri::command]
pub fn import_agents_from_dir(
    path: String,
    selected_paths: Vec<String>,
    include_codex: bool,
    rename_on_conflict: bool,
) -> Result<ImportResult, String> {
    // Re-scan and re-parse server-side; never trust a frontend-supplied AgentDefinition.
    let candidates = scan_candidates(&path, include_codex)?;
    let selected: HashSet<String> = selected_paths.into_iter().collect();
    let existing = crate::commands::registry::get_all_agents();
    let mut taken_ids: HashSet<String> = existing.iter().map(|a| a.id.to_string()).collect();

    let mut imported = 0usize;
    let mut skipped = 0usize;
    let mut conflicts: Vec<String> = Vec::new();
    let mut agents: Vec<AgentDefinition> = Vec::new();

    for c in candidates {
        if !selected.contains(&c.source_path) {
            continue;
        }
        let mut agent = c.agent;

        if taken_ids.contains(&agent.id.to_string()) {
            if rename_on_conflict {
                let suffix = short_hash(&c.source_path);
                let new_slug = format!("{}-{}", agent.id.name, suffix);
                match CompositeId::new("imported", &new_slug) {
                    Ok(id) if !taken_ids.contains(&id.to_string()) => {
                        agent.name = format!("{} ({})", agent.name, suffix);
                        agent.id = id;
                    }
                    _ => {
                        skipped += 1;
                        conflicts.push(agent.name.clone());
                        continue;
                    }
                }
            } else {
                skipped += 1;
                conflicts.push(agent.name.clone());
                continue;
            }
        }

        persist_imported_agent(&agent)?;
        taken_ids.insert(agent.id.to_string());
        imported += 1;
        agents.push(agent);
    }

    Ok(ImportResult {
        imported,
        skipped,
        conflicts,
        agents,
    })
}

/// Persist an imported agent to the registry custom root as pretty JSON carrying the
/// top-level `"type": "agent"` field the loader requires.
fn persist_imported_agent(agent: &AgentDefinition) -> Result<(), String> {
    let dir = imported_agents_dir();
    let filename = format!("{}.json", agent.id.name);

    let mut value =
        serde_json::to_value(agent).map_err(|e| format!("Failed to serialize agent: {}", e))?;
    if let Some(obj) = value.as_object_mut() {
        obj.insert("type".to_string(), serde_json::Value::String("agent".to_string()));
    }
    let json = serde_json::to_string_pretty(&value)
        .map_err(|e| format!("Failed to serialize agent: {}", e))?;

    atomic_write_str(&dir.join(filename), &json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AgentModel, AgentColor, MemoryScope, ToolAccess, CompositeId};
    use crate::models::Visibility;

    fn create_test_agent() -> AgentDefinition {
        AgentDefinition {
            id: CompositeId::new("test", "my-agent").unwrap(),
            name: "My Agent".to_string(),
            description: "Test agent".to_string(),
            version: "1.0.0".to_string(),
            author: "test".to_string(),
            visibility: Visibility::Private,
            tags: vec!["test".to_string()],
            model: AgentModel::Sonnet,
            color: AgentColor::Blue,
            memory: MemoryScope::None,
            tools: vec![ToolAccess::All],
            required_capabilities: vec![],
            prompt: "You are a test agent.".to_string(),
            examples: vec![],
        }
    }

    #[test]
    fn test_get_custom_agents_dir() {
        let dir = get_custom_agents_dir();
        assert!(dir.to_string_lossy().contains("com.agentharbor.app"));
        assert!(dir.to_string_lossy().ends_with("agents"));
    }

    #[test]
    fn test_imported_agents_dir_is_registry_custom() {
        let dir = imported_agents_dir();
        let s = dir.to_string_lossy().replace('\\', "/");
        assert!(s.ends_with("registry/custom/agents"));
    }

    #[test]
    fn test_skip_list_excludes_extension_and_node_modules() {
        assert!(is_skipped_path(Path::new(
            "/home/u/.vscode/extensions/some.ext/.claude/agents/x.md"
        )));
        assert!(is_skipped_path(Path::new(
            "/proj/node_modules/pkg/.claude/agents/x.md"
        )));
        assert!(is_skipped_path(Path::new("/proj/.git/agents/x.md")));
        assert!(!is_skipped_path(Path::new("/proj/.claude/agents/x.md")));
    }

    #[test]
    fn test_classify_tool() {
        assert_eq!(
            classify_tool("/p/.claude/agents/a.md", false),
            Some("claude-code")
        );
        assert_eq!(classify_tool("/p/.cursor/agents/a.md", false), Some("cursor"));
        assert_eq!(classify_tool("/p/.gemini/agents/a.md", false), Some("gemini"));
        assert_eq!(classify_tool("/p/.codex/agents/a.md", false), None);
        assert_eq!(classify_tool("/p/.codex/agents/a.md", true), Some("codex"));
        assert_eq!(classify_tool("/p/.cursor/rules/a.mdc", false), None);
    }
}
