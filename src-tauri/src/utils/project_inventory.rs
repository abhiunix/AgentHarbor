use std::collections::HashSet;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledItem {
    pub name: String,
    pub item_type: String, // "mcp", "rule", "skill", "hook", "plugin", "agent"
    pub adapter_id: String,
    pub adapter_name: String,
}

/// Deduped counts from on-disk config (MCP/skill/hook/rule/plugin), and distinct agent file names.
pub fn stats_from_installed_items(items: &[InstalledItem]) -> (usize, usize) {
    let mut cap_keys = HashSet::new();
    let mut agent_names = HashSet::new();
    for it in items {
        if it.item_type == "agent" {
            agent_names.insert(it.name.clone());
        } else {
            cap_keys.insert(format!("{}:{}", it.item_type, it.name));
        }
    }
    (cap_keys.len(), agent_names.len())
}

pub fn push_agents_from_dir(items: &mut Vec<InstalledItem>, dir: &Path, adapter_id: &str, adapter_name: &str) {
    if !dir.is_dir() {
        return;
    }
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with(".md") {
                    items.push(InstalledItem {
                        name: name.trim_end_matches(".md").to_string(),
                        item_type: "agent".to_string(),
                        adapter_id: adapter_id.to_string(),
                        adapter_name: adapter_name.to_string(),
                    });
                }
            }
        }
    }
}

pub fn push_skills_from_dir(items: &mut Vec<InstalledItem>, dir: &Path, adapter_id: &str, adapter_name: &str) {
    if !dir.is_dir() {
        return;
    }
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                items.push(InstalledItem {
                    name: name.to_string(),
                    item_type: "skill".to_string(),
                    adapter_id: adapter_id.to_string(),
                    adapter_name: adapter_name.to_string(),
                });
            }
        }
    }
}

pub fn push_rules_from_dir(items: &mut Vec<InstalledItem>, dir: &Path, adapter_id: &str, adapter_name: &str) {
    if !dir.is_dir() {
        return;
    }
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(fname) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !(fname.ends_with(".mdc") || fname.ends_with(".md")) {
                continue;
            }
            let stem = fname
                .trim_end_matches(".mdc")
                .trim_end_matches(".md")
                .to_string();
            if stem.is_empty() {
                continue;
            }
            items.push(InstalledItem {
                name: stem,
                item_type: "rule".to_string(),
                adapter_id: adapter_id.to_string(),
                adapter_name: adapter_name.to_string(),
            });
        }
    }
}

pub fn collect_installed_items(path: &Path) -> Result<Vec<InstalledItem>, String> {
    let mut items = Vec::new();

    // Claude Code adapter - read MCP servers from .mcp.json
    {
        let mcp_json_path = path.join(".mcp.json");
        if mcp_json_path.exists() {
            if let Ok(content) = fs::read_to_string(&mcp_json_path) {
                if let Ok(json) = serde_json::from_str::<Value>(&content) {
                    if let Some(servers) = json.get("mcpServers").and_then(|v| v.as_object()) {
                        for name in servers.keys() {
                            items.push(InstalledItem {
                                name: name.clone(),
                                item_type: "mcp".to_string(),
                                adapter_id: "claude-code".to_string(),
                                adapter_name: "Claude Code".to_string(),
                            });
                        }
                    }
                }
            }
        }

        // Read hooks/plugins from .claude/settings.json and settings.local.json
        let mut seen_claude_hooks = HashSet::new();
        let mut seen_claude_plugins = HashSet::new();
        let claude_settings_files = [
            path.join(".claude").join("settings.json"),
            path.join(".claude").join("settings.local.json"),
        ];
        for settings_path in &claude_settings_files {
            if !settings_path.exists() {
                continue;
            }
            if let Ok(content) = fs::read_to_string(settings_path) {
                if let Ok(json) = serde_json::from_str::<Value>(&content) {
                    if let Some(hooks) = json.get("hooks").and_then(|v| v.as_object()) {
                        for name in hooks.keys() {
                            if seen_claude_hooks.insert(name.clone()) {
                                items.push(InstalledItem {
                                    name: name.clone(),
                                    item_type: "hook".to_string(),
                                    adapter_id: "claude-code".to_string(),
                                    adapter_name: "Claude Code".to_string(),
                                });
                            }
                        }
                    }
                    if let Some(plugins) = json.get("plugins").and_then(|v| v.as_object()) {
                        for name in plugins.keys() {
                            if seen_claude_plugins.insert(name.clone()) {
                                items.push(InstalledItem {
                                    name: name.clone(),
                                    item_type: "plugin".to_string(),
                                    adapter_id: "claude-code".to_string(),
                                    adapter_name: "Claude Code".to_string(),
                                });
                            }
                        }
                    }
                }
            }
        }

        push_skills_from_dir(
            &mut items,
            &path.join(".claude").join("skills"),
            "claude-code",
            "Claude Code",
        );
        push_rules_from_dir(
            &mut items,
            &path.join(".claude").join("rules"),
            "claude-code",
            "Claude Code",
        );
        push_agents_from_dir(
            &mut items,
            &path.join(".claude").join("agents"),
            "claude-code",
            "Claude Code",
        );
    }

    // Cursor adapter
    {
        let cursor_mcp_path = path.join(".cursor").join("mcp.json");
        if cursor_mcp_path.exists() {
            if let Ok(content) = fs::read_to_string(&cursor_mcp_path) {
                if let Ok(json) = serde_json::from_str::<Value>(&content) {
                    if let Some(servers) = json.get("mcpServers").and_then(|v| v.as_object()) {
                        for name in servers.keys() {
                            items.push(InstalledItem {
                                name: name.clone(),
                                item_type: "mcp".to_string(),
                                adapter_id: "cursor".to_string(),
                                adapter_name: "Cursor".to_string(),
                            });
                        }
                    }
                }
            }
        }

        let cursor_hooks_path = path.join(".cursor").join("hooks.json");
        if cursor_hooks_path.exists() {
            if let Ok(content) = fs::read_to_string(&cursor_hooks_path) {
                if let Ok(json) = serde_json::from_str::<Value>(&content) {
                    if let Some(hooks) = json.get("hooks").and_then(|v| v.as_object()) {
                        for name in hooks.keys() {
                            items.push(InstalledItem {
                                name: name.clone(),
                                item_type: "hook".to_string(),
                                adapter_id: "cursor".to_string(),
                                adapter_name: "Cursor".to_string(),
                            });
                        }
                    }
                }
            }
        }

        push_skills_from_dir(
            &mut items,
            &path.join(".cursor").join("skills"),
            "cursor",
            "Cursor",
        );
        push_rules_from_dir(
            &mut items,
            &path.join(".cursor").join("rules"),
            "cursor",
            "Cursor",
        );
        push_agents_from_dir(
            &mut items,
            &path.join(".cursor").join("agents"),
            "cursor",
            "Cursor",
        );
    }

    // Windsurf adapter
    {
        let windsurf_mcp_path = path.join(".windsurf").join("mcp_config.json");
        if windsurf_mcp_path.exists() {
            if let Ok(content) = fs::read_to_string(&windsurf_mcp_path) {
                if let Ok(json) = serde_json::from_str::<Value>(&content) {
                    if let Some(servers) = json.get("mcpServers").and_then(|v| v.as_object()) {
                        for name in servers.keys() {
                            items.push(InstalledItem {
                                name: name.clone(),
                                item_type: "mcp".to_string(),
                                adapter_id: "windsurf".to_string(),
                                adapter_name: "Windsurf".to_string(),
                            });
                        }
                    }
                }
            }
        }

        push_skills_from_dir(
            &mut items,
            &path.join(".windsurf").join("skills"),
            "windsurf",
            "Windsurf",
        );
        push_rules_from_dir(
            &mut items,
            &path.join(".windsurf").join("rules"),
            "windsurf",
            "Windsurf",
        );
    }

    // Root-level shared agents (Claude/Cursor convention)
    push_agents_from_dir(&mut items, &path.join("agents"), "shared", "Shared");

    // Gemini CLI (optional)
    push_agents_from_dir(
        &mut items,
        &path.join(".gemini").join("agents"),
        "gemini",
        "Gemini",
    );
    push_skills_from_dir(
        &mut items,
        &path.join(".gemini").join("skills"),
        "gemini",
        "Gemini",
    );

    Ok(items)
}
