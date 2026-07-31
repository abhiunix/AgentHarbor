use std::fs;
use std::path::PathBuf;

use dirs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorRule {
    pub name: String,
    pub description: String,
    pub globs: String,
    pub always_apply: bool,
    pub file_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorRuleDetail {
    pub name: String,
    pub description: String,
    pub globs: String,
    pub always_apply: bool,
    pub content: String,
    pub file_path: String,
}

/// Parse .mdc frontmatter and body from raw file content.
/// Returns (description, globs, always_apply, body).
fn parse_mdc_content(raw: &str) -> (String, String, bool, String) {
    let mut description = String::new();
    let mut globs = String::new();
    let mut always_apply = false;
    let mut body = String::new();

    // Check for frontmatter delimiters
    let trimmed = raw.trim_start();
    if !trimmed.starts_with("---") {
        // No frontmatter — entire content is the body
        return (description, globs, always_apply, raw.to_string());
    }

    // Find the closing --- delimiter (skip the opening one)
    let after_open = &trimmed[3..];
    if let Some(close_idx) = after_open.find("\n---") {
        let frontmatter = &after_open[..close_idx];
        let after_close = &after_open[close_idx + 4..]; // skip "\n---"
        // Body is everything after the closing ---, stripping one leading newline
        body = after_close.strip_prefix('\n').unwrap_or(after_close).to_string();

        // Parse frontmatter lines
        for line in frontmatter.lines() {
            let line = line.trim();
            if let Some(val) = line.strip_prefix("description:") {
                description = val.trim().to_string();
            } else if let Some(val) = line.strip_prefix("globs:") {
                globs = val.trim().to_string();
            } else if let Some(val) = line.strip_prefix("alwaysApply:") {
                let val = val.trim();
                always_apply = val == "true";
            }
        }
    } else {
        // Opening --- but no closing --- found; treat entire content as body
        body = raw.to_string();
    }

    (description, globs, always_apply, body)
}

/// Generate .mdc file content from parts.
fn generate_mdc_content(description: &str, globs: &str, always_apply: bool, content: &str) -> String {
    format!(
        "---\ndescription: {}\nglobs: {}\nalwaysApply: {}\n---\n{}",
        description,
        globs,
        always_apply,
        content
    )
}

/// List all .mdc rule files from the given rules directory.
fn cursor_rules_dir(project_path: Option<String>) -> Result<PathBuf, String> {
    match project_path {
        Some(p) => {
            if p.contains("..") {
                return Err("Invalid project path".to_string());
            }
            Ok(PathBuf::from(&p).join(".cursor").join("rules"))
        }
        None => {
            let home = dirs::home_dir().ok_or("Could not determine home directory")?;
            Ok(home.join(".cursor").join("rules"))
        }
    }
}

fn list_rules_in_dir(rules_dir: &PathBuf) -> Vec<CursorRule> {
    let mut rules = Vec::new();

    if !rules_dir.is_dir() {
        return rules;
    }

    let entries = match fs::read_dir(rules_dir) {
        Ok(e) => e,
        Err(_) => return rules,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("mdc") {
            continue;
        }
        if !path.is_file() {
            continue;
        }

        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        let raw = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let (description, globs, always_apply, _body) = parse_mdc_content(&raw);

        rules.push(CursorRule {
            name,
            description,
            globs,
            always_apply,
            file_path: path.to_string_lossy().to_string(),
        });
    }

    rules.sort_by(|a, b| a.name.cmp(&b.name));
    rules
}

#[tauri::command]
pub fn list_cursor_rules(project_path: String) -> Result<Vec<CursorRule>, String> {
    if project_path.contains("..") {
        return Err("Invalid project path".to_string());
    }
    let rules_dir = PathBuf::from(&project_path).join(".cursor").join("rules");
    Ok(list_rules_in_dir(&rules_dir))
}

#[tauri::command]
pub fn read_cursor_rule(project_path: Option<String>, rule_name: String) -> Result<CursorRuleDetail, String> {
    if rule_name.contains("..") {
        return Err("Invalid path".to_string());
    }
    let file_path = cursor_rules_dir(project_path)?.join(format!("{}.mdc", rule_name));

    if !file_path.is_file() {
        return Err(format!("Rule '{}' not found", rule_name));
    }

    let raw = fs::read_to_string(&file_path)
        .map_err(|e| format!("Failed to read rule: {}", e))?;

    let (description, globs, always_apply, content) = parse_mdc_content(&raw);

    Ok(CursorRuleDetail {
        name: rule_name,
        description,
        globs,
        always_apply,
        content,
        file_path: file_path.to_string_lossy().to_string(),
    })
}

#[tauri::command]
pub fn write_cursor_rule(
    project_path: Option<String>,
    rule_name: String,
    description: String,
    globs: String,
    always_apply: bool,
    content: String,
) -> Result<(), String> {
    if rule_name.contains("..") {
        return Err("Invalid path".to_string());
    }

    let rules_dir = cursor_rules_dir(project_path)?;
    fs::create_dir_all(&rules_dir)
        .map_err(|e| format!("Failed to create rules directory: {}", e))?;

    let file_path = rules_dir.join(format!("{}.mdc", rule_name));

    let mdc_content = generate_mdc_content(&description, &globs, always_apply, &content);

    crate::utils::paths::atomic_write_str(&file_path, &mdc_content)?;

    Ok(())
}

#[tauri::command]
pub fn delete_cursor_rule(project_path: Option<String>, rule_name: String) -> Result<(), String> {
    if rule_name.contains("..") {
        return Err("Invalid path".to_string());
    }

    let file_path = cursor_rules_dir(project_path)?.join(format!("{}.mdc", rule_name));

    if !file_path.exists() {
        return Err(format!("Rule '{}' not found", rule_name));
    }

    fs::remove_file(&file_path)
        .map_err(|e| format!("Failed to delete rule: {}", e))?;

    Ok(())
}

#[tauri::command]
pub fn list_global_cursor_rules() -> Result<Vec<CursorRule>, String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    let rules_dir = home.join(".cursor").join("rules");
    Ok(list_rules_in_dir(&rules_dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_standard_frontmatter() {
        let raw = "---\ndescription: TypeScript best practices\nglobs: **/*.ts,**/*.tsx\nalwaysApply: false\n---\nAlways use strict TypeScript types.";
        let (desc, globs, always, body) = parse_mdc_content(raw);
        assert_eq!(desc, "TypeScript best practices");
        assert_eq!(globs, "**/*.ts,**/*.tsx");
        assert!(!always);
        assert_eq!(body, "Always use strict TypeScript types.");
    }

    #[test]
    fn test_parse_always_apply_true() {
        let raw = "---\ndescription: Linting\nglobs: \nalwaysApply: true\n---\nRun linter on save.";
        let (desc, globs, always, body) = parse_mdc_content(raw);
        assert_eq!(desc, "Linting");
        assert_eq!(globs, "");
        assert!(always);
        assert_eq!(body, "Run linter on save.");
    }

    #[test]
    fn test_parse_no_frontmatter() {
        let raw = "Just plain content without frontmatter.";
        let (desc, globs, always, body) = parse_mdc_content(raw);
        assert_eq!(desc, "");
        assert_eq!(globs, "");
        assert!(!always);
        assert_eq!(body, "Just plain content without frontmatter.");
    }

    #[test]
    fn test_parse_empty_string() {
        let (desc, globs, always, body) = parse_mdc_content("");
        assert_eq!(desc, "");
        assert_eq!(globs, "");
        assert!(!always);
        assert_eq!(body, "");
    }

    #[test]
    fn test_parse_missing_fields() {
        let raw = "---\ndescription: Only description\n---\nSome body content.";
        let (desc, globs, always, body) = parse_mdc_content(raw);
        assert_eq!(desc, "Only description");
        assert_eq!(globs, "");
        assert!(!always);
        assert_eq!(body, "Some body content.");
    }

    #[test]
    fn test_parse_unclosed_frontmatter() {
        let raw = "---\ndescription: Broken\nglobs: *.rs\nNo closing delimiter here.";
        let (desc, globs, always, body) = parse_mdc_content(raw);
        // No closing ---, treat entire content as body
        assert_eq!(desc, "");
        assert_eq!(globs, "");
        assert!(!always);
        assert_eq!(body, raw);
    }

    #[test]
    fn test_parse_multiline_body() {
        let raw = "---\ndescription: Multi\nglobs: *.py\nalwaysApply: false\n---\nLine 1\nLine 2\nLine 3";
        let (_, _, _, body) = parse_mdc_content(raw);
        assert_eq!(body, "Line 1\nLine 2\nLine 3");
    }

    #[test]
    fn test_generate_mdc_content() {
        let result = generate_mdc_content("My rule", "**/*.rs", true, "Rule body here.");
        let expected = "---\ndescription: My rule\nglobs: **/*.rs\nalwaysApply: true\n---\nRule body here.";
        assert_eq!(result, expected);
    }

    #[test]
    fn test_roundtrip_parse_generate() {
        let description = "Roundtrip test";
        let globs = "src/**/*.ts";
        let always_apply = true;
        let content = "Some rule content\nwith multiple lines.";

        let generated = generate_mdc_content(description, globs, always_apply, content);
        let (d, g, a, b) = parse_mdc_content(&generated);

        assert_eq!(d, description);
        assert_eq!(g, globs);
        assert_eq!(a, always_apply);
        assert_eq!(b, content);
    }

    #[test]
    fn test_list_rules_in_nonexistent_dir() {
        let dir = PathBuf::from("/tmp/agentharbor_test_nonexistent_dir_12345");
        let rules = list_rules_in_dir(&dir);
        assert!(rules.is_empty());
    }

    #[test]
    fn test_write_and_read_rule() {
        let tmp_dir = std::env::temp_dir().join("agentharbor_cursor_rules_test");
        let _ = fs::remove_dir_all(&tmp_dir);
        fs::create_dir_all(&tmp_dir).unwrap();

        let project_path = tmp_dir.to_string_lossy().to_string();

        // Write a rule
        write_cursor_rule(
            Some(project_path.clone()),
            "test-rule".to_string(),
            "Test description".to_string(),
            "**/*.rs".to_string(),
            true,
            "Rule body content.".to_string(),
        )
        .unwrap();

        // Read it back
        let detail = read_cursor_rule(Some(project_path.clone()), "test-rule".to_string()).unwrap();
        assert_eq!(detail.name, "test-rule");
        assert_eq!(detail.description, "Test description");
        assert_eq!(detail.globs, "**/*.rs");
        assert!(detail.always_apply);
        assert_eq!(detail.content, "Rule body content.");

        // List rules
        let rules = list_cursor_rules(project_path.clone()).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].name, "test-rule");

        // Delete
        delete_cursor_rule(Some(project_path.clone()), "test-rule".to_string()).unwrap();
        let rules = list_cursor_rules(project_path.clone()).unwrap();
        assert!(rules.is_empty());

        // Cleanup
        let _ = fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_path_traversal_rejected() {
        assert!(list_cursor_rules("../etc".to_string()).is_err());
        assert!(read_cursor_rule(Some("/tmp".to_string()), "../secret".to_string()).is_err());
        assert!(write_cursor_rule(
            Some("/tmp".to_string()),
            "../escape".to_string(),
            String::new(),
            String::new(),
            false,
            String::new(),
        ).is_err());
        assert!(delete_cursor_rule(Some("/tmp".to_string()), "../bad".to_string()).is_err());
    }
}
