use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::models::{AgentDefinition, UniversalCapability};

#[derive(Debug)]
pub struct LoadResult<T> {
    pub items: Vec<T>,
    pub errors: Vec<LoadError>,
}

#[derive(Debug)]
pub struct LoadError {
    pub path: PathBuf,
    pub message: String,
}

pub fn load_capabilities(dirs: &[PathBuf]) -> LoadResult<UniversalCapability> {
    let mut capabilities: HashMap<String, UniversalCapability> = HashMap::new();
    let mut errors: Vec<LoadError> = Vec::new();

    for dir in dirs {
        if !dir.exists() {
            continue;
        }

        let subdirs = ["mcps", "rules", "skills", "hooks", "plugins", "customs"];
        for subdir in &subdirs {
            let subdir_path = dir.join("capabilities").join(subdir);
            if !subdir_path.exists() {
                continue;
            }

            let entries = match fs::read_dir(&subdir_path) {
                Ok(entries) => entries,
                Err(e) => {
                    errors.push(LoadError {
                        path: subdir_path.clone(),
                        message: format!("Failed to read directory: {}", e),
                    });
                    continue;
                }
            };

            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                // Skip index/manifest files
                if path.file_name().and_then(|s| s.to_str()) == Some("index.json") {
                    continue;
                }

                match load_capability_file(&path) {
                    Ok(capability) => {
                        let id = capability.id().to_string();
                        capabilities.insert(id, capability);
                    }
                    Err(e) => {
                        errors.push(LoadError {
                            path: path.clone(),
                            message: e,
                        });
                    }
                }
            }
        }

        // Scan skills/ directory recursively — supports any nesting depth:
        // skills/<skill>/SKILL.md (flat)
        // skills/<category>/<skill>/SKILL.md (one level)
        // skills/<cat1>/<cat2>/<skill>/SKILL.md (multi-level)
        let skills_root = dir.join("skills");
        if skills_root.exists() {
            fn find_skill_dirs(dir: &std::path::Path, capabilities: &mut HashMap<String, UniversalCapability>, errors: &mut Vec<LoadError>) {
                let entries = match fs::read_dir(dir) {
                    Ok(e) => e,
                    Err(_) => return,
                };
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.is_dir() {
                        continue;
                    }
                    let skill_md = path.join("SKILL.md");
                    if skill_md.exists() {
                        match load_skill_directory(&path) {
                            Ok(capability) => {
                                let id = capability.id().to_string();
                                capabilities.insert(id, capability);
                            }
                            Err(e) => {
                                errors.push(LoadError { path: skill_md, message: e });
                            }
                        }
                    } else {
                        // Recurse into subdirectory (category folder)
                        find_skill_dirs(&path, capabilities, errors);
                    }
                }
            }
            find_skill_dirs(&skills_root, &mut capabilities, &mut errors);
        }

        // Scan new community registry format: type folders at root with category subdirectories
        // e.g., mcps/devtools/github-mcp.json, rules/code-quality/general-code-review.json
        let new_format_types = ["mcps", "rules", "hooks"];
        for type_name in &new_format_types {
            let type_dir = dir.join(type_name);
            // Skip if this is the capabilities/ subfolder (already handled above)
            if !type_dir.exists() || dir.join("capabilities").join(type_name).exists() && type_dir == dir.join(type_name) {
                // Only skip if the capabilities/<type> path exists AND would collide
                // Actually, we should always scan root-level type dirs for the new format
            }
            if !type_dir.exists() {
                continue;
            }
            if let Ok(cat_entries) = fs::read_dir(&type_dir) {
                for cat_entry in cat_entries.flatten() {
                    let cat_path = cat_entry.path();
                    if cat_path.is_dir() {
                        // Category subdirectory: scan JSON files inside
                        if let Ok(file_entries) = fs::read_dir(&cat_path) {
                            for file_entry in file_entries.flatten() {
                                let file_path = file_entry.path();
                                if file_path.extension().and_then(|s| s.to_str()) != Some("json") {
                                    continue;
                                }
                                if file_path.file_name().and_then(|s| s.to_str()) == Some("index.json") {
                                    continue;
                                }
                                match load_capability_file(&file_path) {
                                    Ok(capability) => {
                                        let id = capability.id().to_string();
                                        capabilities.insert(id, capability);
                                    }
                                    Err(e) => {
                                        errors.push(LoadError {
                                            path: file_path,
                                            message: e,
                                        });
                                    }
                                }
                            }
                        }
                    } else if cat_path.extension().and_then(|s| s.to_str()) == Some("json")
                        && cat_path.file_name().and_then(|s| s.to_str()) != Some("index.json")
                    {
                        // JSON file directly in type folder (no category), skip index.json
                        match load_capability_file(&cat_path) {
                            Ok(capability) => {
                                let id = capability.id().to_string();
                                capabilities.insert(id, capability);
                            }
                            Err(e) => {
                                errors.push(LoadError {
                                    path: cat_path,
                                    message: e,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    LoadResult {
        items: capabilities.into_values().collect(),
        errors,
    }
}

/// Load a skill from an agentskills.io directory (skills/<name>/SKILL.md + supporting files)
fn load_skill_directory(skill_dir: &std::path::Path) -> Result<UniversalCapability, String> {
    let skill_md_path = skill_dir.join("SKILL.md");
    let content = fs::read_to_string(&skill_md_path)
        .map_err(|e| format!("Failed to read SKILL.md: {}", e))?;

    // Parse YAML frontmatter
    let trimmed = content.trim();
    let (meta, body) = if let Some(after_first) = trimmed.strip_prefix("---") {
        if let Some(end) = after_first.find("---") {
            let frontmatter = &after_first[..end];
            let body = after_first[end + 3..].trim_start().to_string();
            (frontmatter.to_string(), body)
        } else {
            (String::new(), content.clone())
        }
    } else {
        (String::new(), content.clone())
    };

    // Parse frontmatter fields
    let mut name = String::new();
    let mut description = String::new();
    let mut license: Option<String> = None;
    let mut allowed_tools: Option<Vec<String>> = None;

    for line in meta.lines() {
        let line = line.trim();
        if let Some(pos) = line.find(':') {
            let key = line[..pos].trim();
            let value = line[pos + 1..].trim().trim_matches('"').to_string();
            match key {
                "name" => name = value,
                "description" => description = value,
                "license" => license = Some(value),
                "allowed-tools" | "allowed_tools" => {
                    allowed_tools = Some(value.split_whitespace().map(String::from).collect());
                }
                _ => {}
            }
        }
    }

    // Fall back to directory name for the skill name
    if name.is_empty() {
        name = skill_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
    }

    // Read metadata.json if present for author, stars, tags, etc.
    let metadata_path = skill_dir.join("metadata.json");
    let metadata: Option<serde_json::Value> = if metadata_path.exists() {
        fs::read_to_string(&metadata_path)
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok())
    } else {
        None
    };

    // Extract metadata fields
    let author = metadata.as_ref()
        .and_then(|m| m.get("author").or_else(|| m.get("author_github")))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("community")
        .to_string();

    let author_github = metadata.as_ref()
        .and_then(|m| m.get("author_github"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let display_name = metadata.as_ref()
        .and_then(|m| m.get("display_name"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let tags: Vec<String> = metadata.as_ref()
        .and_then(|m| m.get("tags"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let category = metadata.as_ref()
        .and_then(|m| m.get("category"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let version = metadata.as_ref()
        .and_then(|m| m.get("version"))
        .and_then(|v| v.as_str())
        .unwrap_or("1.0.0")
        .to_string();

    let meta_license = metadata.as_ref()
        .and_then(|m| m.get("license"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let compatible_adapters: Vec<String> = metadata.as_ref()
        .and_then(|m| m.get("compatible_adapters"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_else(|| vec![
            "claude-code".to_string(),
            "cursor".to_string(),
            "windsurf".to_string(),
        ]);

    let source_info = metadata.as_ref()
        .and_then(|m| m.get("source"))
        .and_then(|v| serde_json::from_value::<crate::models::CapabilitySource>(v.clone()).ok())
        .and_then(|mut s| {
            if s.url.as_deref() == Some("") { s.url = None; }
            if s.repo.as_deref() == Some("") { s.repo = None; }
            if s.url.is_none() && s.repo.is_none() { None } else { Some(s) }
        });

    let stats = metadata.as_ref()
        .and_then(|m| m.get("stats"))
        .and_then(|v| serde_json::from_value::<crate::models::CapabilityStats>(v.clone()).ok());

    // Use metadata description if SKILL.md description is empty
    if description.is_empty() {
        description = metadata.as_ref()
            .and_then(|m| m.get("description"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
    }

    // Generate composite ID using real author
    let id_str = format!("{}/{}", author, name);
    let id: crate::models::CompositeId = id_str
        .parse()
        .map_err(|_| format!("Invalid composite ID: {}", id_str))?;

    // Collect all files in the skill directory
    let mut files = vec![crate::models::SkillFile {
        path: "SKILL.md".to_string(),
        content: body,
    }];

    // Walk subdirectories for supporting files (scripts/, references/, assets/)
    fn collect_files(dir: &std::path::Path, base: &std::path::Path, files: &mut Vec<crate::models::SkillFile>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    collect_files(&path, base, files);
                } else if path.file_name().map(|n| n != "SKILL.md" && n != "metadata.json").unwrap_or(true) {
                    if let Ok(content) = fs::read_to_string(&path) {
                        let rel = path.strip_prefix(base).unwrap_or(&path);
                        files.push(crate::models::SkillFile {
                            path: rel.to_string_lossy().to_string(),
                            content,
                        });
                    }
                }
            }
        }
    }
    collect_files(skill_dir, skill_dir, &mut files);

    // Use display_name from metadata, or title-case the skill name
    let final_name = display_name.unwrap_or_else(|| {
        name.replace('-', " ").split_whitespace()
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    None => String::new(),
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    });

    Ok(UniversalCapability::Skill(crate::models::Skill {
        id,
        name: final_name,
        description,
        version,
        author,
        visibility: crate::models::Visibility::Public,
        tags,
        scope: String::new(),
        files,
        env: std::collections::HashMap::new(),
        compatible_agents: compatible_adapters,
        allowed_tools,
        model: None,
        context: None,
        agent: None,
        argument_hint: None,
        license: license.or(meta_license),
        category,
        author_github,
        source_info,
        stats,
    }))
}

fn load_capability_file(path: &Path) -> Result<UniversalCapability, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read file: {}", e))?;

    if let Ok(cap) = serde_json::from_str::<UniversalCapability>(&content) {
        return Ok(cap);
    }

    parse_community_format(&content, path)
}

fn parse_community_format(content: &str, path: &Path) -> Result<UniversalCapability, String> {
    let raw: serde_json::Value = serde_json::from_str(content)
        .map_err(|e| format!("Failed to parse JSON: {}", e))?;

    let obj = raw.as_object().ok_or("JSON is not an object")?;

    let name = obj.get("name")
        .or_else(|| obj.get("display_name"))
        .and_then(|v| v.as_str()).unwrap_or("").to_string();
    let description = obj.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let version = obj.get("version").and_then(|v| v.as_str()).unwrap_or("1.0.0").to_string();
    let author = obj.get("author")
        .or_else(|| obj.get("author_github"))
        .and_then(|v| v.as_str()).unwrap_or("community").to_string();
    // Generate composite ID: use explicit "id" field, or derive from author + name/filename
    let id_str = if let Some(id_val) = obj.get("id").and_then(|v| v.as_str()) {
        id_val.to_string()
    } else {
        let slug = if !name.is_empty() {
            name.to_lowercase().replace(' ', "-")
        } else {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string()
        };
        format!("{}/{}", author, slug)
    };
    let visibility_str = obj.get("visibility").and_then(|v| v.as_str()).unwrap_or("public");
    let visibility = if visibility_str == "private" {
        crate::models::Visibility::Private
    } else {
        crate::models::Visibility::Public
    };
    let tags: Vec<String> = obj.get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let compatible_agents: Vec<String> = obj.get("adapters")
        .or_else(|| obj.get("compatible_agents"))
        .or_else(|| obj.get("compatible_adapters"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let author_github = obj.get("author_github")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let category = obj.get("category")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let source_info = obj.get("source")
        .and_then(|v| serde_json::from_value::<crate::models::CapabilitySource>(v.clone()).ok())
        .and_then(|mut s| {
            // Treat empty strings as None
            if s.url.as_deref() == Some("") { s.url = None; }
            if s.repo.as_deref() == Some("") { s.repo = None; }
            if s.url.is_none() && s.repo.is_none() { None } else { Some(s) }
        });
    let stats = obj.get("stats")
        .and_then(|v| serde_json::from_value::<crate::models::CapabilityStats>(v.clone()).ok());

    let id: crate::models::CompositeId = id_str.parse()
        .map_err(|_| format!("Invalid composite ID: {}", &id_str))?;

    if let Some(mcp) = obj.get("mcp") {
        let command = mcp.get("command").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let args: Vec<String> = mcp.get("args")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let mut env = std::collections::HashMap::new();
        if let Some(env_val) = mcp.get("env") {
            if let Some(arr) = env_val.as_array() {
                for item in arr {
                    if let Some(key) = item.get("key").and_then(|v| v.as_str()) {
                        env.insert(key.to_string(), crate::models::EnvVariable {
                            var_type: "secret".to_string(),
                            label: item.get("label").and_then(|v| v.as_str()).unwrap_or(key).to_string(),
                            required: item.get("required").and_then(|v| v.as_bool()).unwrap_or(false),
                            value: item.get("value").and_then(|v| v.as_str()).map(String::from),
                        });
                    }
                }
            } else if let Some(obj_env) = env_val.as_object() {
                for (key, val) in obj_env {
                    env.insert(key.clone(), crate::models::EnvVariable {
                        var_type: val.get("type").and_then(|v| v.as_str()).unwrap_or("secret").to_string(),
                        label: val.get("label").and_then(|v| v.as_str()).unwrap_or(key).to_string(),
                        required: val.get("required").and_then(|v| v.as_bool()).unwrap_or(false),
                        value: val.get("value").and_then(|v| v.as_str()).map(String::from),
                    });
                }
            }
        }
        return Ok(UniversalCapability::Mcp(crate::models::McpServer {
            id, name, description, version, author, visibility, tags,
            transport: "stdio".to_string(), command, args, url: String::new(), env, compatible_agents,
            disabled: None, always_allow: None, disabled_tools: None, tool_list: None,
            category: category.clone(), author_github: author_github.clone(),
            source_info: source_info.clone(), stats: stats.clone(),
        }));
    }

    if let Some(hook) = obj.get("hook") {
        let command = hook.get("command").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let event = hook.get("trigger").or_else(|| hook.get("event"))
            .and_then(|v| v.as_str()).unwrap_or("PostToolUse").to_string();
        let matcher = hook.get("matcher").and_then(|v| v.as_str()).unwrap_or("*").to_string();
        let timeout_ms = hook.get("timeout_ms").and_then(|v| v.as_u64()).unwrap_or(10000) as u32;
        return Ok(UniversalCapability::Hook(crate::models::Hook {
            id, name, description, version, author, visibility, tags,
            event, matcher, command, timeout_ms,
            env: std::collections::HashMap::new(),
            compatible_agents,
            adapter_configs: std::collections::HashMap::new(),
            category: category.clone(), author_github: author_github.clone(),
            source_info: source_info.clone(), stats: stats.clone(),
        }));
    }

    if let Some(skill) = obj.get("skill") {
        let files: Vec<crate::models::SkillFile> = skill.get("files")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|f| {
                let p = f.get("path")?.as_str()?.to_string();
                let c = f.get("content")?.as_str()?.to_string();
                Some(crate::models::SkillFile { path: p, content: c })
            }).collect())
            .unwrap_or_default();
        let allowed_tools: Option<Vec<String>> = skill.get("allowed_tools")
            .or_else(|| skill.get("allowed-tools"))
            .and_then(|v| {
                if let Some(arr) = v.as_array() {
                    Some(arr.iter().filter_map(|s| s.as_str().map(String::from)).collect())
                } else { v.as_str().map(|s| s.split_whitespace().map(String::from).collect()) }
            });
        let model = skill.get("model").and_then(|v| v.as_str()).map(String::from);
        let context = skill.get("context").and_then(|v| v.as_str()).map(String::from);
        let agent = skill.get("agent").and_then(|v| v.as_str()).map(String::from);
        let argument_hint = skill.get("argument_hint")
            .or_else(|| skill.get("argument-hint"))
            .and_then(|v| v.as_str()).map(String::from);
        let license = skill.get("license").and_then(|v| v.as_str()).map(String::from);
        return Ok(UniversalCapability::Skill(crate::models::Skill {
            id, name, description, version, author, visibility, tags,
            scope: String::new(), files,
            env: std::collections::HashMap::new(),
            compatible_agents,
            allowed_tools, model, context, agent, argument_hint, license,
            category: category.clone(), author_github: author_github.clone(),
            source_info: source_info.clone(), stats: stats.clone(),
        }));
    }

    if let Some(rule) = obj.get("rule") {
        let content_str = rule.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let scope = rule.get("scope").and_then(|v| v.as_str()).unwrap_or("project").to_string();
        return Ok(UniversalCapability::Rule(crate::models::Rule {
            id, name, description, version, author, visibility, tags,
            scope, content: content_str,
            env: std::collections::HashMap::new(),
            compatible_agents,
            category: category.clone(), author_github: author_github.clone(),
            source_info: source_info.clone(), stats: stats.clone(),
        }));
    }

    if let Some(plugin) = obj.get("plugin") {
        let install_command = plugin.get("install_command").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let config: std::collections::HashMap<String, serde_json::Value> = plugin.get("config")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        return Ok(UniversalCapability::Plugin(crate::models::Plugin {
            id, name, description, version, author, visibility, tags,
            install_command, config,
            env: std::collections::HashMap::new(),
            compatible_agents,
        }));  // Plugin has no extra metadata fields yet
    }

    // NEW FORMAT: fields at root level (no "mcp"/"rule"/"hook" wrapper)
    // Detect by presence of type-specific fields at root
    if obj.contains_key("transport") || (obj.contains_key("command") && obj.contains_key("args")) {
        // It's an MCP server (new format)
        let command = obj.get("command").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let args: Vec<String> = obj.get("args")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let transport = obj.get("transport").and_then(|v| v.as_str()).unwrap_or("stdio").to_string();
        let url = obj.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let mut env = std::collections::HashMap::new();
        if let Some(env_val) = obj.get("env") {
            if let Some(arr) = env_val.as_array() {
                for item in arr {
                    if let Some(key) = item.get("key").and_then(|v| v.as_str()) {
                        env.insert(key.to_string(), crate::models::EnvVariable {
                            var_type: "secret".to_string(),
                            label: item.get("label").and_then(|v| v.as_str()).unwrap_or(key).to_string(),
                            required: item.get("required").and_then(|v| v.as_bool()).unwrap_or(false),
                            value: item.get("value").and_then(|v| v.as_str()).map(String::from),
                        });
                    }
                }
            } else if let Some(obj_env) = env_val.as_object() {
                for (key, val) in obj_env {
                    if val.is_string() {
                        // Simple string env values (new format)
                        env.insert(key.clone(), crate::models::EnvVariable {
                            var_type: "secret".to_string(),
                            label: key.clone(),
                            required: true,
                            value: val.as_str().map(String::from),
                        });
                    } else {
                        env.insert(key.clone(), crate::models::EnvVariable {
                            var_type: val.get("type").and_then(|v| v.as_str()).unwrap_or("secret").to_string(),
                            label: val.get("label").and_then(|v| v.as_str()).unwrap_or(key).to_string(),
                            required: val.get("required").and_then(|v| v.as_bool()).unwrap_or(false),
                            value: val.get("value").and_then(|v| v.as_str()).map(String::from),
                        });
                    }
                }
            }
        }
        return Ok(UniversalCapability::Mcp(crate::models::McpServer {
            id, name, description, version, author, visibility, tags,
            transport, command, args, url, env, compatible_agents,
            disabled: obj.get("disabled").and_then(|v| v.as_bool()),
            always_allow: obj.get("always_allow").and_then(|v| v.as_array()).map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect()),
            disabled_tools: obj.get("disabled_tools").and_then(|v| v.as_array()).map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect()),
            tool_list: None,
            category: category.clone(), author_github: author_github.clone(),
            source_info: source_info.clone(), stats: stats.clone(),
        }));
    }

    if obj.contains_key("scope") && obj.contains_key("content") {
        // It's a rule (new format)
        let content_str = obj.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let scope = obj.get("scope").and_then(|v| v.as_str()).unwrap_or("project").to_string();
        return Ok(UniversalCapability::Rule(crate::models::Rule {
            id, name, description, version, author, visibility, tags,
            scope, content: content_str,
            env: std::collections::HashMap::new(),
            compatible_agents,
            category: category.clone(), author_github: author_github.clone(),
            source_info: source_info.clone(), stats: stats.clone(),
        }));
    }

    if obj.contains_key("trigger") {
        // It's a hook (new format)
        let command = obj.get("command").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let event = obj.get("trigger").and_then(|v| v.as_str()).unwrap_or("PostToolUse").to_string();
        let matcher = obj.get("matcher").and_then(|v| v.as_str()).unwrap_or("*").to_string();
        let timeout_ms = obj.get("timeout_ms").and_then(|v| v.as_u64()).unwrap_or(10000) as u32;
        return Ok(UniversalCapability::Hook(crate::models::Hook {
            id, name, description, version, author, visibility, tags,
            event, matcher, command, timeout_ms,
            env: std::collections::HashMap::new(),
            compatible_agents,
            adapter_configs: std::collections::HashMap::new(),
            category: category.clone(), author_github: author_github.clone(),
            source_info: source_info.clone(), stats: stats.clone(),
        }));
    }

    let parent_dir = path.parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("");

    match parent_dir {
        "mcps" => Ok(UniversalCapability::Mcp(crate::models::McpServer {
            id, name, description, version, author, visibility, tags,
            transport: "stdio".to_string(), command: String::new(), args: vec![],
            url: String::new(), env: std::collections::HashMap::new(), compatible_agents,
            disabled: None, always_allow: None, disabled_tools: None, tool_list: None,
            category, author_github, source_info, stats,
        })),
        "rules" => Ok(UniversalCapability::Rule(crate::models::Rule {
            id, name, description, version, author, visibility, tags,
            scope: "project".to_string(), content: String::new(),
            env: std::collections::HashMap::new(),
            compatible_agents,
            category, author_github, source_info, stats,
        })),
        "skills" => Ok(UniversalCapability::Skill(crate::models::Skill {
            id, name, description, version, author, visibility, tags,
            scope: String::new(), files: vec![],
            env: std::collections::HashMap::new(),
            compatible_agents,
            allowed_tools: None, model: None, context: None,
            agent: None, argument_hint: None, license: None,
            category, author_github, source_info, stats,
        })),
        "hooks" => Ok(UniversalCapability::Hook(crate::models::Hook {
            id, name, description, version, author, visibility, tags,
            event: "PostToolUse".to_string(), matcher: "*".to_string(),
            command: String::new(), timeout_ms: 10000,
            env: std::collections::HashMap::new(),
            compatible_agents,
            adapter_configs: std::collections::HashMap::new(),
            category, author_github, source_info, stats,
        })),
        "plugins" => Ok(UniversalCapability::Plugin(crate::models::Plugin {
            id, name, description, version, author, visibility, tags,
            install_command: String::new(), config: std::collections::HashMap::new(),
            env: std::collections::HashMap::new(),
            compatible_agents,
        })),
        "customs" => Ok(UniversalCapability::Custom(crate::models::Custom {
            id, name, description, version, author, visibility, tags,
            env: std::collections::HashMap::new(),
            compatible_agents,
            adapter_configs: std::collections::HashMap::new(),
        })),
        _ => Err(format!("Cannot determine capability type from directory: {}", parent_dir)),
    }
}

pub fn load_agents(dirs: &[PathBuf]) -> LoadResult<AgentDefinition> {
    let mut agents: HashMap<String, AgentDefinition> = HashMap::new();
    let mut errors: Vec<LoadError> = Vec::new();

    for dir in dirs {
        let agents_dir = dir.join("agents");
        if !agents_dir.exists() {
            continue;
        }

        let entries = match fs::read_dir(&agents_dir) {
            Ok(entries) => entries,
            Err(e) => {
                errors.push(LoadError {
                    path: agents_dir.clone(),
                    message: format!("Failed to read directory: {}", e),
                });
                continue;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_dir() {
                // Category subdirectory (new format: agents/<category>/<agent-name>.json + .md)
                if let Ok(cat_entries) = fs::read_dir(&path) {
                    for cat_entry in cat_entries.flatten() {
                        let cat_path = cat_entry.path();
                        let ext = cat_path.extension().and_then(|s| s.to_str()).unwrap_or("none");
                        if ext == "json" {
                            // New format: try loading JSON metadata + companion .md for prompt
                            match load_agent_json_with_md(&cat_path) {
                                Ok(agent) => {
                                    let id = agent.id.to_string();
                                    agents.insert(id, agent);
                                }
                                Err(e) => {
                                    errors.push(LoadError {
                                        path: cat_path.clone(),
                                        message: e,
                                    });
                                }
                            }
                        } else if ext == "md" {
                            // Only load .md if there's no companion .json (avoid double-loading)
                            let companion_json = cat_path.with_extension("json");
                            if !companion_json.exists() {
                                match load_agent_file(&cat_path) {
                                    Ok(agent) => {
                                        let id = agent.id.to_string();
                                        agents.insert(id, agent);
                                    }
                                    Err(e) => {
                                        errors.push(LoadError {
                                            path: cat_path.clone(),
                                            message: e,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
                continue;
            }

            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("none");
            if ext != "json" && ext != "md" {
                continue;
            }

            // Skip non-agent files that often live alongside agent definitions
            // (registry README, manifest index, license, contributing guide).
            let fname = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            let stem_upper = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_ascii_uppercase();
            if fname == "index.json"
                || stem_upper == "README"
                || stem_upper == "CONTRIBUTING"
                || stem_upper == "LICENSE"
                || stem_upper == "CHANGELOG"
            {
                continue;
            }

            match load_agent_file(&path) {
                Ok(agent) => {
                    let id = agent.id.to_string();
                    agents.insert(id, agent);
                }
                Err(e) => {
                    errors.push(LoadError {
                        path: path.clone(),
                        message: e,
                    });
                }
            }
        }
    }

    LoadResult {
        items: agents.into_values().collect(),
        errors,
    }
}

fn load_agent_file(path: &Path) -> Result<AgentDefinition, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read file: {}", e))?;

    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    if ext == "md" {
        return parse_community_agent_md(&content, path);
    }

    #[derive(serde::Deserialize)]
    struct AgentWithType {
        #[serde(rename = "type")]
        _type: String,
        #[serde(flatten)]
        agent: AgentDefinition,
    }

    let parsed: AgentWithType = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse JSON: {}", e))?;

    Ok(parsed.agent)
}

/// Load an agent from a JSON metadata file + companion .md file (new community format)
/// The .json provides metadata, the .md (same stem) provides the prompt content.
fn load_agent_json_with_md(json_path: &Path) -> Result<AgentDefinition, String> {
    use crate::models::{AgentColor, AgentModel, MemoryScope, ToolAccess, Visibility};

    let content = fs::read_to_string(json_path)
        .map_err(|e| format!("Failed to read file: {}", e))?;

    // Try standard deserialization first
    #[derive(serde::Deserialize)]
    struct AgentWithType {
        #[serde(rename = "type")]
        _type: Option<String>,
        #[serde(flatten)]
        agent: AgentDefinition,
    }

    if let Ok(parsed) = serde_json::from_str::<AgentWithType>(&content) {
        let mut agent = parsed.agent;
        // If there's a companion .md file, use it for the prompt
        let md_path = json_path.with_extension("md");
        if md_path.exists() {
            if let Ok(md_content) = fs::read_to_string(&md_path) {
                let prompt = md_content.trim().to_string();
                if !prompt.is_empty() {
                    agent.prompt = prompt;
                }
            }
        }
        return Ok(agent);
    }

    // Fall back to manual parsing for community format
    let raw: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse JSON: {}", e))?;
    let obj = raw.as_object().ok_or("JSON is not an object")?;

    let name = obj.get("name")
        .or_else(|| obj.get("display_name"))
        .and_then(|v| v.as_str()).unwrap_or("").to_string();
    let author = obj.get("author")
        .or_else(|| obj.get("author_github"))
        .and_then(|v| v.as_str()).unwrap_or("community").to_string();
    let description = obj.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let version = obj.get("version").and_then(|v| v.as_str()).unwrap_or("1.0.0").to_string();
    let tags: Vec<String> = obj.get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let model_str = obj.get("model").and_then(|v| v.as_str()).unwrap_or("sonnet").to_lowercase();
    let model = if model_str.contains("opus") { AgentModel::Opus }
                else if model_str.contains("haiku") { AgentModel::Haiku }
                else { AgentModel::Sonnet };

    let color_str = obj.get("color").and_then(|v| v.as_str()).unwrap_or("blue").to_lowercase();
    let color = match color_str.as_str() {
        "red" => AgentColor::Red, "green" => AgentColor::Green,
        "yellow" => AgentColor::Yellow, "purple" => AgentColor::Purple,
        "orange" => AgentColor::Orange, "pink" => AgentColor::Pink,
        "cyan" => AgentColor::Cyan, _ => AgentColor::Blue,
    };

    let memory_str = obj.get("memory").and_then(|v| v.as_str()).unwrap_or("none").to_lowercase();
    let memory = match memory_str.as_str() {
        "project" => MemoryScope::Project, "user" => MemoryScope::User,
        _ => MemoryScope::None,
    };

    let slug = if !name.is_empty() {
        name.to_lowercase().replace(' ', "-")
    } else {
        json_path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown").to_string()
    };
    let id_str = if let Some(id_val) = obj.get("id").and_then(|v| v.as_str()) {
        id_val.to_string()
    } else {
        format!("{}/{}", author, slug)
    };
    let id = id_str.parse().map_err(|_| format!("Invalid composite ID: {}", id_str))?;

    // Read prompt from companion .md file
    let mut prompt = String::new();
    let md_path = json_path.with_extension("md");
    if md_path.exists() {
        if let Ok(md_content) = fs::read_to_string(&md_path) {
            prompt = md_content.trim().to_string();
        }
    }
    // Fall back to "prompt" field in JSON
    if prompt.is_empty() {
        prompt = obj.get("prompt").and_then(|v| v.as_str()).unwrap_or("").to_string();
    }

    let required_capabilities: Vec<crate::models::CompositeId> = obj.get("required_capabilities")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()?.parse().ok()).collect())
        .unwrap_or_default();

    Ok(AgentDefinition {
        id, name, description, version, author,
        visibility: Visibility::Public, tags, model, color, memory,
        tools: vec![ToolAccess::All], required_capabilities,
        prompt, examples: vec![],
    })
}

fn parse_community_agent_md(content: &str, path: &Path) -> Result<AgentDefinition, String> {
    use crate::models::{AgentColor, AgentModel, MemoryScope, ToolAccess, Visibility};

    if content.trim().starts_with("---") {
        return crate::utils::markdown::parse_agent_md(content)
            .map_err(|e| format!("Failed to parse agent markdown: {}", e));
    }

    let lines: Vec<&str> = content.lines().collect();

    let mut name = String::new();
    let mut author = "community".to_string();
    let mut version = "1.0.0".to_string();
    let mut tags: Vec<String> = Vec::new();
    let mut model = AgentModel::Sonnet;
    let mut color = AgentColor::Blue;
    let mut memory = MemoryScope::None;
    let mut description = String::new();
    let mut prompt = String::new();

    let mut section = "header";

    for line in &lines {
        let trimmed = line.trim();

        if trimmed.starts_with("# ") && name.is_empty() {
            name = trimmed.trim_start_matches("# ").trim().to_string();
            if name.ends_with(" Agent") {
                name = name.trim_end_matches(" Agent").to_string();
            }
            continue;
        }

        if trimmed == "## Description" {
            section = "description";
            continue;
        }
        if trimmed == "## System Prompt" || trimmed == "## Prompt" {
            section = "prompt";
            continue;
        }
        if trimmed.starts_with("## ") && section != "prompt" {
            section = "other";
            continue;
        }

        if trimmed.starts_with("**Author:**") {
            author = trimmed.replace("**Author:**", "").trim().to_string();
        } else if trimmed.starts_with("**Version:**") {
            version = trimmed.replace("**Version:**", "").trim().to_string();
        } else if trimmed.starts_with("**Tags:**") {
            tags = trimmed.replace("**Tags:**", "").trim()
                .split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
        } else if trimmed.starts_with("**Model:**") {
            let m = trimmed.replace("**Model:**", "").trim().to_lowercase();
            model = if m.contains("opus") { AgentModel::Opus }
                    else if m.contains("haiku") { AgentModel::Haiku }
                    else { AgentModel::Sonnet };
        } else if trimmed.starts_with("**Color:**") {
            let c = trimmed.replace("**Color:**", "").trim().to_lowercase();
            color = match c.as_str() {
                "red" => AgentColor::Red, "green" => AgentColor::Green,
                "yellow" => AgentColor::Yellow, "purple" => AgentColor::Purple,
                "orange" => AgentColor::Orange, "pink" => AgentColor::Pink,
                "cyan" => AgentColor::Cyan, _ => AgentColor::Blue,
            };
        } else if trimmed.starts_with("**Memory:**") {
            let m = trimmed.replace("**Memory:**", "").trim().to_lowercase();
            memory = match m.as_str() {
                "project" => MemoryScope::Project, "user" => MemoryScope::User,
                "session" => MemoryScope::None, _ => MemoryScope::None,
            };
        } else {
            match section {
                "description"
                    if !trimmed.is_empty() => {
                        if !description.is_empty() { description.push(' '); }
                        description.push_str(trimmed);
                    }
                "prompt" => {
                    prompt.push_str(line);
                    prompt.push('\n');
                }
                _ => {}
            }
        }
    }

    let prompt = prompt.trim().to_string();
    if name.is_empty() {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown");
        name = stem.replace('-', " ").split_whitespace()
            .map(|w| { let mut c = w.chars(); match c.next() { None => String::new(), Some(f) => f.to_uppercase().collect::<String>() + c.as_str() }})
            .collect::<Vec<_>>().join(" ");
    }

    let slug = name.to_lowercase().replace(' ', "-");
    let id_str = format!("{}/{}", author, slug);
    let id = id_str.parse().map_err(|_| format!("Invalid composite ID: {}", id_str))?;

    Ok(AgentDefinition {
        id, name, description, version, author,
        visibility: Visibility::Public, tags, model, color, memory,
        tools: vec![ToolAccess::All], required_capabilities: vec![],
        prompt, examples: vec![],
    })
}

pub fn get_bundled_registry_path() -> PathBuf {
    let exe_path = std::env::current_exe().unwrap_or_default();
    let default_dir = PathBuf::from(".");
    let exe_dir = exe_path.parent().unwrap_or(&default_dir);
    
    #[cfg(target_os = "macos")]
    {
        if exe_dir.ends_with("MacOS") {
            // Tauri bundles resources into .app/Contents/Resources/
            let resources_path = exe_dir
                .parent() // Contents
                .map(|p| p.join("Resources").join("registry"));
            if let Some(ref path) = resources_path {
                if path.exists() {
                    return path.clone();
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let resources = exe_dir.join("registry");
        if resources.exists() {
            return resources;
        }
        // Tauri on Windows may put resources in a resources/ subdirectory
        let resources = exe_dir.join("resources").join("registry");
        if resources.exists() {
            return resources;
        }
    }

    #[cfg(target_os = "linux")]
    {
        let resources = exe_dir.join("registry");
        if resources.exists() {
            return resources;
        }
        // AppImage/deb may use ../share/ path
        if let Some(parent) = exe_dir.parent() {
            let resources = parent.join("share").join("agentharbor").join("registry");
            if resources.exists() {
                return resources;
            }
        }
    }

    // Dev mode: CARGO_MANIFEST_DIR is src-tauri, registry is at ../registry
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let dev_registry = PathBuf::from(&manifest_dir).parent()
            .map(|p| p.join("registry"));
        if let Some(ref path) = dev_registry {
            if path.exists() {
                return path.clone();
            }
        }
    }
    
    let relative_registry = PathBuf::from("../registry");
    if relative_registry.exists() {
        return relative_registry;
    }
    
    let cwd_registry = std::env::current_dir()
        .map(|p| p.join("registry"))
        .unwrap_or_else(|_| PathBuf::from("registry"));
    if cwd_registry.exists() {
        return cwd_registry;
    }
    
    let parent_registry = std::env::current_dir()
        .map(|p| p.parent().map(|pp| pp.join("registry")).unwrap_or_else(|| PathBuf::from("registry")))
        .unwrap_or_else(|_| PathBuf::from("registry"));
    if parent_registry.exists() {
        return parent_registry;
    }
    
    exe_dir
        .parent()
        .map(|p| p.join("registry"))
        .unwrap_or_else(|| PathBuf::from("registry"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    fn create_test_capability_json(content: &str, dir: &Path, subdir: &str, filename: &str) {
        let cap_dir = dir.join("capabilities").join(subdir);
        fs::create_dir_all(&cap_dir).unwrap();
        let file_path = cap_dir.join(filename);
        let mut file = File::create(file_path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }

    fn create_test_agent_json(content: &str, dir: &Path, filename: &str) {
        let agents_dir = dir.join("agents");
        fs::create_dir_all(&agents_dir).unwrap();
        let file_path = agents_dir.join(filename);
        let mut file = File::create(file_path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn test_load_valid_mcp_capability() {
        let temp = tempdir().unwrap();
        let dir = temp.path().to_path_buf();

        let json = r#"{
            "type": "mcp",
            "id": "community/test-mcp",
            "name": "Test MCP",
            "description": "Test description",
            "version": "1.0.0",
            "author": "community",
            "visibility": "public",
            "tags": ["test"],
            "transport": "stdio",
            "command": "npx",
            "args": [],
            "env": {},
            "compatible_agents": ["claude-code"]
        }"#;

        create_test_capability_json(json, &dir, "mcps", "test.json");

        let result = load_capabilities(&[dir]);
        assert_eq!(result.items.len(), 1);
        assert!(result.errors.is_empty());
        assert_eq!(result.items[0].id().to_string(), "community/test-mcp");
    }

    #[test]
    fn test_load_invalid_json_skipped() {
        let temp = tempdir().unwrap();
        let dir = temp.path().to_path_buf();

        let invalid_json = r#"{ invalid json }"#;
        create_test_capability_json(invalid_json, &dir, "mcps", "invalid.json");

        let result = load_capabilities(&[dir]);
        assert!(result.items.is_empty());
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0].message.contains("Failed to parse JSON"));
    }

    #[test]
    fn test_duplicate_id_resolution() {
        let temp1 = tempdir().unwrap();
        let temp2 = tempdir().unwrap();
        let dir1 = temp1.path().to_path_buf();
        let dir2 = temp2.path().to_path_buf();

        let json1 = r#"{
            "type": "mcp",
            "id": "community/same-id",
            "name": "First MCP",
            "description": "First",
            "version": "1.0.0",
            "author": "community",
            "visibility": "public",
            "tags": [],
            "transport": "stdio",
            "command": "first",
            "args": [],
            "env": {},
            "compatible_agents": []
        }"#;

        let json2 = r#"{
            "type": "mcp",
            "id": "community/same-id",
            "name": "Second MCP",
            "description": "Second",
            "version": "2.0.0",
            "author": "community",
            "visibility": "public",
            "tags": [],
            "transport": "stdio",
            "command": "second",
            "args": [],
            "env": {},
            "compatible_agents": []
        }"#;

        create_test_capability_json(json1, &dir1, "mcps", "first.json");
        create_test_capability_json(json2, &dir2, "mcps", "second.json");

        let result = load_capabilities(&[dir1, dir2]);
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].name(), "Second MCP");
    }

    #[test]
    fn test_load_valid_agent() {
        let temp = tempdir().unwrap();
        let dir = temp.path().to_path_buf();

        let json = r#"{
            "type": "agent",
            "id": "community/test-agent",
            "name": "Test Agent",
            "description": "Test description",
            "version": "1.0.0",
            "author": "community",
            "visibility": "public",
            "tags": ["test"],
            "model": "haiku",
            "color": "blue",
            "memory": "project",
            "tools": ["all"],
            "required_capabilities": [],
            "prompt": "You are a test agent.",
            "examples": []
        }"#;

        create_test_agent_json(json, &dir, "test.json");

        let result = load_agents(&[dir]);
        assert_eq!(result.items.len(), 1);
        assert!(result.errors.is_empty());
        assert_eq!(result.items[0].id.to_string(), "community/test-agent");
    }

    #[test]
    fn test_nonexistent_directory_ignored() {
        let result = load_capabilities(&[PathBuf::from("/nonexistent/path")]);
        assert!(result.items.is_empty());
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_load_multiple_capability_types() {
        let temp = tempdir().unwrap();
        let dir = temp.path().to_path_buf();

        let mcp_json = r#"{
            "type": "mcp",
            "id": "community/test-mcp",
            "name": "Test MCP",
            "description": "Test",
            "version": "1.0.0",
            "author": "community",
            "visibility": "public",
            "tags": [],
            "transport": "stdio",
            "command": "npx",
            "args": [],
            "env": {},
            "compatible_agents": []
        }"#;

        let rule_json = r#"{
            "type": "rule",
            "id": "community/test-rule",
            "name": "Test Rule",
            "description": "Test",
            "version": "1.0.0",
            "author": "community",
            "visibility": "public",
            "tags": [],
            "scope": "project",
            "content": "Test content",
            "compatible_agents": []
        }"#;

        create_test_capability_json(mcp_json, &dir, "mcps", "mcp.json");
        create_test_capability_json(rule_json, &dir, "rules", "rule.json");

        let result = load_capabilities(&[dir]);
        assert_eq!(result.items.len(), 2);
        assert!(result.errors.is_empty());
    }
}
