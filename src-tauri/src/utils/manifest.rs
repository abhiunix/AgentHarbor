use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::utils::project_inventory::{collect_installed_items, InstalledItem};

pub const MANIFEST_START: &str = "<!-- AgentHarbor: Deployed Capabilities -->";
pub const MANIFEST_END: &str = "<!-- /AgentHarbor -->";

/// Replace (or append/remove) the manifest section bounded by MANIFEST_START..MANIFEST_END.
pub fn replace_manifest_section(content: &str, manifest: &str) -> String {
    let start_idx = content.find(MANIFEST_START);
    let end_idx = content.find(MANIFEST_END);

    match (start_idx, end_idx, manifest.is_empty()) {
        // Remove section entirely when manifest is empty and section exists
        (Some(si), Some(ei), true) => {
            let before = content[..si].trim_end_matches('\n');
            let after_end = ei + MANIFEST_END.len();
            let after = content[after_end..].trim_start_matches('\n');
            if before.is_empty() && after.is_empty() {
                String::new()
            } else if before.is_empty() {
                after.to_string()
            } else if after.is_empty() {
                before.to_string()
            } else {
                format!("{}\n{}", before, after)
            }
        }
        // Remove: start found but no end, manifest empty
        (Some(si), None, true) => {
            let before = content[..si].trim_end_matches('\n');
            before.to_string()
        }
        // Replace existing section (start and end found)
        (Some(si), Some(ei), false) => {
            let before = &content[..si];
            let after_end = ei + MANIFEST_END.len();
            let after = &content[after_end..];
            format!("{}{}{}", before, manifest, after)
        }
        // Start found but no end marker -- replace from start to EOF
        (Some(si), None, false) => {
            let before = &content[..si];
            format!("{}{}", before, manifest)
        }
        // No start marker, manifest empty -- return content as-is
        (None, _, true) => content.to_string(),
        // No start marker, append manifest
        (None, _, false) => {
            if content.is_empty() {
                manifest.to_string()
            } else {
                format!("{}\n\n{}", content.trim_end(), manifest)
            }
        }
    }
}

/// Build a capability manifest block from installed items.
///
/// Groups by type (MCP, Rule, Skill, Hook, Plugin, Custom, Agent),
/// sorts alphabetically within each group, deduplicates by (type, name).
/// Returns empty string if no items.
pub fn build_capability_manifest(items: &[InstalledItem]) -> String {
    if items.is_empty() {
        return String::new();
    }

    // Define the ordering of types
    let type_order: &[&str] = &["mcp", "rule", "skill", "hook", "plugin", "custom", "agent"];

    // Group items by type, dedup by (type, name) keeping first adapter encountered
    let mut groups: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();

    for item in items {
        let key = (item.item_type.clone(), item.name.clone());
        if seen.contains(&key) {
            continue;
        }
        seen.insert(key);
        groups
            .entry(item.item_type.clone())
            .or_default()
            .insert(item.name.clone(), item.adapter_name.clone());
    }

    let mut lines = Vec::new();
    lines.push(MANIFEST_START.to_string());

    for &typ in type_order {
        if let Some(entries) = groups.get(typ) {
            let display_type = match typ {
                "mcp" => "MCP",
                "rule" => "Rule",
                "skill" => "Skill",
                "hook" => "Hook",
                "plugin" => "Plugin",
                "custom" => "Custom",
                "agent" => "Agent",
                other => other,
            };
            for (name, adapter) in entries {
                lines.push(format!("- **{}: {}** (via {})", display_type, name, adapter));
            }
        }
    }

    // Handle any types not in the predefined order
    for (typ, entries) in &groups {
        if !type_order.contains(&typ.as_str()) {
            let display_type = typ.as_str();
            for (name, adapter) in entries {
                lines.push(format!("- **{}: {}** (via {})", display_type, name, adapter));
            }
        }
    }

    lines.push(MANIFEST_END.to_string());

    // If only markers and no actual items, return empty
    if lines.len() <= 2 {
        return String::new();
    }

    lines.join("\n")
}

/// Write the capability manifest into AGENTS.md at the project root.
pub fn write_agents_md(project_path: &Path, items: &[InstalledItem]) -> Result<PathBuf, String> {
    let manifest = build_capability_manifest(items);
    let agents_md_path = project_path.join("AGENTS.md");

    let existing = if agents_md_path.exists() {
        std::fs::read_to_string(&agents_md_path)
            .map_err(|e| format!("Failed to read AGENTS.md: {}", e))?
    } else {
        String::new()
    };

    let updated = replace_manifest_section(&existing, &manifest);
    crate::utils::paths::atomic_write_str(&agents_md_path, &updated)?;
    Ok(agents_md_path)
}

/// Rebuild manifests in all applicable instruction files for a project.
///
/// Updates:
/// - AGENTS.md (always)
/// - CLAUDE.md (if .claude/ dir or CLAUDE.md exists)
/// - GEMINI.md (if .gemini/ dir or GEMINI.md exists)
/// - .cursorrules (if .cursor/ dir or .cursorrules exists)
/// - .cursor/rules/agentharbor-manifest.mdc (if .cursor/rules/ dir exists)
pub fn rebuild_all_manifests(project_path: &Path) -> Result<Vec<PathBuf>, String> {
    let items = collect_installed_items(project_path)?;
    let manifest = build_capability_manifest(&items);
    let mut written = Vec::new();

    // AGENTS.md -- always
    {
        let path = project_path.join("AGENTS.md");
        let existing = if path.exists() {
            std::fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read AGENTS.md: {}", e))?
        } else {
            String::new()
        };
        let updated = replace_manifest_section(&existing, &manifest);
        crate::utils::paths::atomic_write_str(&path, &updated)?;
        written.push(path);
    }

    // CLAUDE.md
    {
        let claude_md = project_path.join("CLAUDE.md");
        let claude_dir = project_path.join(".claude");
        if claude_md.exists() || claude_dir.is_dir() {
            let existing = if claude_md.exists() {
                std::fs::read_to_string(&claude_md)
                    .map_err(|e| format!("Failed to read CLAUDE.md: {}", e))?
            } else {
                String::new()
            };
            let updated = replace_manifest_section(&existing, &manifest);
            crate::utils::paths::atomic_write_str(&claude_md, &updated)?;
            written.push(claude_md);
        }
    }

    // GEMINI.md
    {
        let gemini_md = project_path.join("GEMINI.md");
        let gemini_dir = project_path.join(".gemini");
        if gemini_md.exists() || gemini_dir.is_dir() {
            let existing = if gemini_md.exists() {
                std::fs::read_to_string(&gemini_md)
                    .map_err(|e| format!("Failed to read GEMINI.md: {}", e))?
            } else {
                String::new()
            };
            let updated = replace_manifest_section(&existing, &manifest);
            crate::utils::paths::atomic_write_str(&gemini_md, &updated)?;
            written.push(gemini_md);
        }
    }

    // .cursorrules
    {
        let cursorrules = project_path.join(".cursorrules");
        let cursor_dir = project_path.join(".cursor");
        if cursorrules.exists() || cursor_dir.is_dir() {
            let existing = if cursorrules.exists() {
                std::fs::read_to_string(&cursorrules)
                    .map_err(|e| format!("Failed to read .cursorrules: {}", e))?
            } else {
                String::new()
            };
            let updated = replace_manifest_section(&existing, &manifest);
            crate::utils::paths::atomic_write_str(&cursorrules, &updated)?;
            written.push(cursorrules);
        }
    }

    // .cursor/rules/agentharbor-manifest.mdc
    {
        let cursor_rules_dir = project_path.join(".cursor").join("rules");
        if cursor_rules_dir.is_dir() {
            let mdc_path = cursor_rules_dir.join("agentharbor-manifest.mdc");
            let mdc_content = if manifest.is_empty() {
                String::new()
            } else {
                format!(
                    "---\ndescription: \"AgentHarbor deployed capabilities manifest\"\nglobs: \n---\n\n{}",
                    manifest
                )
            };
            crate::utils::paths::atomic_write_str(&mdc_path, &mdc_content)?;
            written.push(mdc_path);
        }
    }

    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replace_manifest_append_to_empty() {
        let result = replace_manifest_section("", "<!-- AgentHarbor: Deployed Capabilities -->\n- **MCP: foo** (via Claude Code)\n<!-- /AgentHarbor -->");
        assert!(result.starts_with(MANIFEST_START));
        assert!(result.ends_with(MANIFEST_END));
    }

    #[test]
    fn test_replace_manifest_append_to_existing() {
        let result = replace_manifest_section("# My Project\n\nSome content.", "<!-- AgentHarbor: Deployed Capabilities -->\n- item\n<!-- /AgentHarbor -->");
        assert!(result.starts_with("# My Project\n\nSome content."));
        assert!(result.contains(MANIFEST_START));
        assert!(result.ends_with(MANIFEST_END));
    }

    #[test]
    fn test_replace_manifest_replaces_existing() {
        let content = "before\n<!-- AgentHarbor: Deployed Capabilities -->\nold stuff\n<!-- /AgentHarbor -->\nafter";
        let result = replace_manifest_section(content, "<!-- AgentHarbor: Deployed Capabilities -->\nnew stuff\n<!-- /AgentHarbor -->");
        assert!(result.contains("before"));
        assert!(result.contains("new stuff"));
        assert!(result.contains("after"));
        assert!(!result.contains("old stuff"));
    }

    #[test]
    fn test_replace_manifest_no_end_marker() {
        let content = "before\n<!-- AgentHarbor: Deployed Capabilities -->\ntrailing stuff";
        let result = replace_manifest_section(content, "<!-- AgentHarbor: Deployed Capabilities -->\nnew\n<!-- /AgentHarbor -->");
        assert!(result.contains("before"));
        assert!(result.contains("new"));
        assert!(!result.contains("trailing stuff"));
    }

    #[test]
    fn test_replace_manifest_remove_section() {
        let content = "before\n<!-- AgentHarbor: Deployed Capabilities -->\nold stuff\n<!-- /AgentHarbor -->\nafter";
        let result = replace_manifest_section(content, "");
        assert!(result.contains("before"));
        assert!(result.contains("after"));
        assert!(!result.contains(MANIFEST_START));
    }

    #[test]
    fn test_replace_manifest_remove_empty_leaves_nothing() {
        let content = "<!-- AgentHarbor: Deployed Capabilities -->\nold stuff\n<!-- /AgentHarbor -->";
        let result = replace_manifest_section(content, "");
        assert_eq!(result, "");
    }

    #[test]
    fn test_replace_manifest_no_marker_empty_manifest() {
        let content = "just content";
        let result = replace_manifest_section(content, "");
        assert_eq!(result, "just content");
    }

    #[test]
    fn test_build_manifest_empty() {
        let result = build_capability_manifest(&[]);
        assert_eq!(result, "");
    }

    #[test]
    fn test_build_manifest_groups_and_sorts() {
        let items = vec![
            InstalledItem {
                name: "zebra-mcp".to_string(),
                item_type: "mcp".to_string(),
                adapter_id: "claude-code".to_string(),
                adapter_name: "Claude Code".to_string(),
            },
            InstalledItem {
                name: "alpha-mcp".to_string(),
                item_type: "mcp".to_string(),
                adapter_id: "cursor".to_string(),
                adapter_name: "Cursor".to_string(),
            },
            InstalledItem {
                name: "my-agent".to_string(),
                item_type: "agent".to_string(),
                adapter_id: "shared".to_string(),
                adapter_name: "Shared".to_string(),
            },
            InstalledItem {
                name: "lint-rule".to_string(),
                item_type: "rule".to_string(),
                adapter_id: "claude-code".to_string(),
                adapter_name: "Claude Code".to_string(),
            },
        ];
        let result = build_capability_manifest(&items);
        assert!(result.contains(MANIFEST_START));
        assert!(result.contains(MANIFEST_END));
        // MCP items should come before Rule, which comes before Agent
        let mcp_pos = result.find("MCP: alpha-mcp").unwrap();
        let rule_pos = result.find("Rule: lint-rule").unwrap();
        let agent_pos = result.find("Agent: my-agent").unwrap();
        assert!(mcp_pos < rule_pos);
        assert!(rule_pos < agent_pos);
        // Alpha sorts within MCP group
        let alpha_pos = result.find("alpha-mcp").unwrap();
        let zebra_pos = result.find("zebra-mcp").unwrap();
        assert!(alpha_pos < zebra_pos);
    }

    #[test]
    fn test_build_manifest_deduplicates() {
        let items = vec![
            InstalledItem {
                name: "shared-mcp".to_string(),
                item_type: "mcp".to_string(),
                adapter_id: "claude-code".to_string(),
                adapter_name: "Claude Code".to_string(),
            },
            InstalledItem {
                name: "shared-mcp".to_string(),
                item_type: "mcp".to_string(),
                adapter_id: "cursor".to_string(),
                adapter_name: "Cursor".to_string(),
            },
        ];
        let result = build_capability_manifest(&items);
        // Should appear only once (first adapter wins)
        assert_eq!(result.matches("shared-mcp").count(), 1);
        assert!(result.contains("via Claude Code"));
    }
}
