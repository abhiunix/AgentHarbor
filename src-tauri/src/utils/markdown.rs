use serde::{Deserialize, Serialize};

use crate::models::composite_id::to_kebab_slug;
use crate::models::{
    AgentColor, AgentDefinition, AgentModel, CompositeId, MemoryScope, ToolAccess, Visibility,
};

#[derive(Debug)]
pub enum MarkdownError {
    InvalidFrontmatter(String),
    MissingField(String),
    ParseError(String),
}

impl std::fmt::Display for MarkdownError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MarkdownError::InvalidFrontmatter(msg) => write!(f, "Invalid frontmatter: {}", msg),
            MarkdownError::MissingField(field) => write!(f, "Missing required field: {}", field),
            MarkdownError::ParseError(msg) => write!(f, "Parse error: {}", msg),
        }
    }
}

impl std::error::Error for MarkdownError {}

#[derive(Serialize, Deserialize, Debug)]
struct AgentFrontmatter {
    name: String,
    description: String,
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    memory: Option<String>,
}

pub fn generate_agent_md(agent: &AgentDefinition) -> String {
    let mut lines = Vec::new();
    
    lines.push("---".to_string());
    lines.push(format!("name: {}", agent.id.name));
    lines.push(format!("description: \"{}\"", escape_yaml_string(&agent.description)));
    lines.push(format!("model: {}", model_to_string(&agent.model)));
    
    if agent.color != AgentColor::Blue {
        lines.push(format!("color: {}", color_to_string(&agent.color)));
    }
    
    if agent.memory != MemoryScope::None {
        lines.push(format!("memory: {}", memory_to_string(&agent.memory)));
    }
    
    lines.push("---".to_string());
    lines.push(String::new());
    lines.push(agent.prompt.clone());
    
    lines.join("\n")
}

pub fn generate_agent_mdc(agent: &AgentDefinition) -> String {
    let mut lines = Vec::new();

    lines.push("---".to_string());
    lines.push(format!(
        "description: \"{}\"",
        escape_yaml_string(&agent.description)
    ));
    lines.push("---".to_string());
    lines.push(String::new());
    lines.push(agent.prompt.clone());

    lines.join("\n")
}

pub fn parse_agent_md(content: &str) -> Result<AgentDefinition, MarkdownError> {
    let content = content.trim();
    
    if !content.starts_with("---") {
        return Err(MarkdownError::InvalidFrontmatter(
            "Content must start with ---".to_string(),
        ));
    }
    
    let rest = &content[3..];
    let end_marker = rest.find("\n---");
    
    if end_marker.is_none() {
        return Err(MarkdownError::InvalidFrontmatter(
            "Missing closing --- for frontmatter".to_string(),
        ));
    }
    
    let frontmatter_end = end_marker.unwrap();
    let frontmatter_str = &rest[..frontmatter_end];
    let body_start = frontmatter_end + 4;
    let body = if body_start < rest.len() {
        rest[body_start..].trim()
    } else {
        ""
    };
    
    let frontmatter: AgentFrontmatter = serde_yaml::from_str(frontmatter_str)
        .map_err(|e| MarkdownError::ParseError(e.to_string()))?;
    
    let model = parse_model(&frontmatter.model)?;
    let color = frontmatter
        .color
        .as_ref()
        .map(|c| parse_color(c))
        .transpose()?
        .unwrap_or(AgentColor::Blue);
    let memory = frontmatter
        .memory
        .as_ref()
        .map(|m| parse_memory(m))
        .transpose()?
        .unwrap_or(MemoryScope::None);
    
    let description = frontmatter.description
        .trim_matches('"')
        .to_string();
    
    let id_str = format!("unknown/{}", frontmatter.name);
    let id = id_str.parse().map_err(|e: crate::models::CompositeIdError| {
        MarkdownError::ParseError(e.to_string())
    })?;
    
    Ok(AgentDefinition {
        id,
        name: frontmatter.name.replace('-', " ").split_whitespace()
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
        description,
        version: "1.0.0".to_string(),
        author: "unknown".to_string(),
        visibility: Visibility::Private,
        tags: vec![],
        model,
        color,
        memory,
        tools: vec![ToolAccess::All],
        required_capabilities: vec![],
        prompt: body.to_string(),
        examples: vec![],
    })
}

/// Lenient frontmatter for importing foreign agent files. Every field is optional and
/// unknown keys (Claude's disallowedTools/permissionMode/skills, Cursor's readonly/
/// is_background, Gemini's kind/tools/mcpServers/temperature, etc.) are silently dropped.
#[derive(Deserialize, Debug, Default)]
struct LenientFrontmatter {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    memory: Option<String>,
}

/// Split a `---\n<frontmatter>\n---\n<body>` document. Returns None when there is no
/// closing frontmatter delimiter.
fn split_frontmatter(content: &str) -> Option<(&str, &str)> {
    let content = content.trim_start();
    if !content.starts_with("---") {
        return None;
    }
    let rest = &content[3..];
    let end_marker = rest.find("\n---")?;
    let frontmatter_str = &rest[..end_marker];
    let body_start = end_marker + 4;
    let body = if body_start < rest.len() {
        rest[body_start..].trim()
    } else {
        ""
    };
    Some((frontmatter_str, body))
}

/// Map an arbitrary model string to the three-value enum. Anything that isn't an exact
/// haiku/sonnet/opus match (full model ids, inherit, fable, absent) defaults to sonnet
/// and reports `defaulted = true`.
fn map_import_model(raw: Option<&str>) -> (AgentModel, bool) {
    match raw.map(|s| s.trim().to_lowercase()).as_deref() {
        Some("haiku") => (AgentModel::Haiku, false),
        Some("sonnet") => (AgentModel::Sonnet, false),
        Some("opus") => (AgentModel::Opus, false),
        _ => (AgentModel::Sonnet, true),
    }
}

fn map_import_color(raw: Option<&str>) -> AgentColor {
    match raw.map(|s| s.trim().to_lowercase()).as_deref() {
        Some("red") => AgentColor::Red,
        Some("green") => AgentColor::Green,
        Some("yellow") => AgentColor::Yellow,
        Some("purple") => AgentColor::Purple,
        Some("orange") => AgentColor::Orange,
        Some("pink") => AgentColor::Pink,
        Some("cyan") => AgentColor::Cyan,
        _ => AgentColor::Blue,
    }
}

fn map_import_memory(raw: Option<&str>) -> MemoryScope {
    match raw.map(|s| s.trim().to_lowercase()).as_deref() {
        Some("local") => MemoryScope::Project,
        _ => MemoryScope::None,
    }
}

fn import_id(display_name: &str, fallback_name: &str) -> CompositeId {
    let mut slug = to_kebab_slug(display_name);
    if slug.is_empty() {
        slug = to_kebab_slug(fallback_name);
    }
    if slug.is_empty() {
        slug = "imported-agent".to_string();
    }
    CompositeId::new("imported", &slug)
        .unwrap_or_else(|_| CompositeId::new("imported", "imported-agent").unwrap())
}

/// Parse a tool-native agent markdown file (Claude/Cursor/Gemini shape: YAML frontmatter +
/// markdown body) leniently into the universal AgentDefinition. Author is set to "imported".
/// When the frontmatter omits `name` the id/name fall back to "imported-agent"; the caller
/// is expected to override with a better fallback (e.g. the file stem).
pub fn parse_agent_md_lenient(content: &str, _source_tool: &str) -> AgentDefinition {
    let (frontmatter_str, body) = split_frontmatter(content).unwrap_or(("", content.trim()));

    let fm: LenientFrontmatter = if frontmatter_str.trim().is_empty() {
        LenientFrontmatter::default()
    } else {
        serde_yaml::from_str(frontmatter_str).unwrap_or_default()
    };

    let display_name = fm
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("imported-agent")
        .to_string();

    let (model, _) = map_import_model(fm.model.as_deref());
    let description = fm
        .description
        .map(|d| d.trim().trim_matches('"').to_string())
        .unwrap_or_default();

    AgentDefinition {
        id: import_id(&display_name, "imported-agent"),
        name: display_name,
        description,
        version: "1.0.0".to_string(),
        author: "imported".to_string(),
        visibility: Visibility::Private,
        tags: vec![],
        model,
        color: map_import_color(fm.color.as_deref()),
        memory: map_import_memory(fm.memory.as_deref()),
        tools: vec![ToolAccess::All],
        required_capabilities: vec![],
        prompt: body.to_string(),
        examples: vec![],
    }
}

/// True when the file's frontmatter model wasn't an exact haiku/sonnet/opus match (or was
/// absent) and [`parse_agent_md_lenient`] therefore defaulted it to sonnet.
pub fn import_model_was_defaulted(content: &str) -> bool {
    let (frontmatter_str, _) = split_frontmatter(content).unwrap_or(("", ""));
    let fm: LenientFrontmatter = serde_yaml::from_str(frontmatter_str).unwrap_or_default();
    map_import_model(fm.model.as_deref()).1
}

/// Extract an agent from a prose `AGENTS.md` (Codex) that has no frontmatter: the first
/// heading becomes the name, the first paragraph the description, and the whole document
/// the prompt (model always defaults to sonnet). Returns None for empty files and
/// AgentHarbor manifest-stub files (which only contain the deployed-capabilities marker).
pub fn extract_prose_agent(content: &str, fallback_name: &str) -> Option<AgentDefinition> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Skip manifest stubs: files whose only real content is the AgentHarbor marker/comments.
    let meaningful: String = trimmed
        .lines()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with("<!--") && !t.contains("AgentHarbor:")
        })
        .collect::<Vec<_>>()
        .join("\n");
    if meaningful.trim().is_empty() {
        return None;
    }

    let name = trimmed
        .lines()
        .find(|l| l.trim_start().starts_with('#'))
        .map(|l| l.trim_start().trim_start_matches('#').trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback_name.to_string());

    let mut description = String::new();
    for para in trimmed.split("\n\n") {
        let joined = para
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();
        if !joined.is_empty() {
            description = joined;
            break;
        }
    }

    Some(AgentDefinition {
        id: import_id(&name, fallback_name),
        name,
        description,
        version: "1.0.0".to_string(),
        author: "imported".to_string(),
        visibility: Visibility::Private,
        tags: vec![],
        model: AgentModel::Sonnet,
        color: AgentColor::Blue,
        memory: MemoryScope::None,
        tools: vec![ToolAccess::All],
        required_capabilities: vec![],
        prompt: trimmed.to_string(),
        examples: vec![],
    })
}

fn escape_yaml_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn model_to_string(model: &AgentModel) -> &'static str {
    match model {
        AgentModel::Haiku => "haiku",
        AgentModel::Sonnet => "sonnet",
        AgentModel::Opus => "opus",
    }
}

fn color_to_string(color: &AgentColor) -> &'static str {
    match color {
        AgentColor::Red => "red",
        AgentColor::Blue => "blue",
        AgentColor::Green => "green",
        AgentColor::Yellow => "yellow",
        AgentColor::Purple => "purple",
        AgentColor::Orange => "orange",
        AgentColor::Pink => "pink",
        AgentColor::Cyan => "cyan",
    }
}

fn memory_to_string(memory: &MemoryScope) -> &'static str {
    match memory {
        MemoryScope::Project => "project",
        MemoryScope::User => "user",
        MemoryScope::None => "none",
    }
}

fn parse_model(s: &str) -> Result<AgentModel, MarkdownError> {
    match s.to_lowercase().as_str() {
        "haiku" => Ok(AgentModel::Haiku),
        "sonnet" => Ok(AgentModel::Sonnet),
        "opus" => Ok(AgentModel::Opus),
        _ => Err(MarkdownError::ParseError(format!("Invalid model: {}", s))),
    }
}

fn parse_color(s: &str) -> Result<AgentColor, MarkdownError> {
    match s.to_lowercase().as_str() {
        "red" => Ok(AgentColor::Red),
        "blue" => Ok(AgentColor::Blue),
        "green" => Ok(AgentColor::Green),
        "yellow" => Ok(AgentColor::Yellow),
        "purple" => Ok(AgentColor::Purple),
        "orange" => Ok(AgentColor::Orange),
        "pink" => Ok(AgentColor::Pink),
        "cyan" => Ok(AgentColor::Cyan),
        _ => Err(MarkdownError::ParseError(format!("Invalid color: {}", s))),
    }
}

fn parse_memory(s: &str) -> Result<MemoryScope, MarkdownError> {
    match s.to_lowercase().as_str() {
        "project" => Ok(MemoryScope::Project),
        "user" => Ok(MemoryScope::User),
        "none" => Ok(MemoryScope::None),
        _ => Err(MarkdownError::ParseError(format!("Invalid memory scope: {}", s))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::AgentExample;

    fn create_test_agent() -> AgentDefinition {
        AgentDefinition {
            id: "community/api-test-runner".parse().unwrap(),
            name: "API Test Runner".to_string(),
            description: "Use this agent when new API endpoints need testing.".to_string(),
            version: "1.0.0".to_string(),
            author: "community".to_string(),
            visibility: Visibility::Public,
            tags: vec!["testing".to_string()],
            model: AgentModel::Haiku,
            color: AgentColor::Green,
            memory: MemoryScope::Project,
            tools: vec![ToolAccess::All],
            required_capabilities: vec![],
            prompt: "You are an elite API testing engineer.".to_string(),
            examples: vec![],
        }
    }

    #[test]
    fn test_generate_agent_md_basic() {
        let agent = create_test_agent();
        let md = generate_agent_md(&agent);
        
        assert!(md.starts_with("---\n"));
        assert!(md.contains("name: api-test-runner"));
        assert!(md.contains("description: \"Use this agent when new API endpoints need testing.\""));
        assert!(md.contains("model: haiku"));
        assert!(md.contains("color: green"));
        assert!(md.contains("memory: project"));
        assert!(md.contains("You are an elite API testing engineer."));
    }

    #[test]
    fn test_generate_agent_md_default_color_omitted() {
        let mut agent = create_test_agent();
        agent.color = AgentColor::Blue;
        
        let md = generate_agent_md(&agent);
        assert!(!md.contains("color:"));
    }

    #[test]
    fn test_generate_agent_md_none_memory_omitted() {
        let mut agent = create_test_agent();
        agent.memory = MemoryScope::None;
        
        let md = generate_agent_md(&agent);
        assert!(!md.contains("memory:"));
    }

    #[test]
    fn test_parse_agent_md_basic() {
        let md = r#"---
name: api-test-runner
description: "Use this agent when new API endpoints need testing."
model: haiku
color: green
memory: project
---

You are an elite API testing engineer."#;

        let result = parse_agent_md(md);
        assert!(result.is_ok());
        
        let agent = result.unwrap();
        assert_eq!(agent.id.name, "api-test-runner");
        assert_eq!(agent.description, "Use this agent when new API endpoints need testing.");
        assert_eq!(agent.model, AgentModel::Haiku);
        assert_eq!(agent.color, AgentColor::Green);
        assert_eq!(agent.memory, MemoryScope::Project);
        assert_eq!(agent.prompt, "You are an elite API testing engineer.");
    }

    #[test]
    fn test_roundtrip_generate_parse() {
        let original = create_test_agent();
        let md = generate_agent_md(&original);
        let parsed = parse_agent_md(&md).unwrap();
        
        assert_eq!(original.id.name, parsed.id.name);
        assert_eq!(original.description, parsed.description);
        assert_eq!(original.model, parsed.model);
        assert_eq!(original.color, parsed.color);
        assert_eq!(original.memory, parsed.memory);
        assert_eq!(original.prompt, parsed.prompt);
    }

    #[test]
    fn test_parse_prd_example() {
        let md = r#"---
name: api-test-runner
description: "Use this agent when new API endpoints need testing."
model: haiku
color: green
memory: project
---

You are an elite API testing engineer with deep expertise in
RESTful API design, HTTP protocols, test automation, and
quality assurance.

## Responsibilities
- Write comprehensive test suites covering happy paths,
  input validation, authentication, error handling, and edge cases.
- Verify response codes, headers, and payload structures.
- Test rate limiting and timeout behaviors."#;

        let result = parse_agent_md(md);
        assert!(result.is_ok());
        
        let agent = result.unwrap();
        assert_eq!(agent.id.name, "api-test-runner");
        assert_eq!(agent.model, AgentModel::Haiku);
        assert_eq!(agent.color, AgentColor::Green);
        assert_eq!(agent.memory, MemoryScope::Project);
        assert!(agent.prompt.contains("elite API testing engineer"));
        assert!(agent.prompt.contains("## Responsibilities"));
    }

    #[test]
    fn test_parse_missing_optional_fields() {
        let md = r#"---
name: simple-agent
description: "A simple agent"
model: sonnet
---

Simple prompt."#;

        let result = parse_agent_md(md);
        assert!(result.is_ok());
        
        let agent = result.unwrap();
        assert_eq!(agent.color, AgentColor::Blue);
        assert_eq!(agent.memory, MemoryScope::None);
    }

    #[test]
    fn test_parse_empty_prompt() {
        let md = r#"---
name: empty-prompt-agent
description: "Agent with empty prompt"
model: opus
---
"#;

        let result = parse_agent_md(md);
        assert!(result.is_ok());
        
        let agent = result.unwrap();
        assert!(agent.prompt.is_empty());
    }

    #[test]
    fn test_parse_invalid_frontmatter() {
        let md = "No frontmatter here";
        let result = parse_agent_md(md);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_closing_frontmatter() {
        let md = r#"---
name: broken
description: "Missing closing"
model: haiku
This is not valid"#;

        let result = parse_agent_md(md);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_model() {
        let md = r#"---
name: bad-model
description: "Bad model"
model: invalid
---

Prompt"#;

        let result = parse_agent_md(md);
        assert!(result.is_err());
    }

    #[test]
    fn test_description_with_special_chars() {
        let mut agent = create_test_agent();
        agent.description = "Use this agent for \"special\" tasks.".to_string();
        
        let md = generate_agent_md(&agent);
        assert!(md.contains("description: \"Use this agent for \\\"special\\\" tasks.\""));
    }

    #[test]
    fn test_all_colors() {
        for (color, expected) in [
            (AgentColor::Red, "red"),
            (AgentColor::Blue, "blue"),
            (AgentColor::Green, "green"),
            (AgentColor::Yellow, "yellow"),
            (AgentColor::Purple, "purple"),
            (AgentColor::Orange, "orange"),
            (AgentColor::Pink, "pink"),
            (AgentColor::Cyan, "cyan"),
        ] {
            assert_eq!(color_to_string(&color), expected);
            assert_eq!(parse_color(expected).unwrap(), color);
        }
    }

    #[test]
    fn test_all_models() {
        for (model, expected) in [
            (AgentModel::Haiku, "haiku"),
            (AgentModel::Sonnet, "sonnet"),
            (AgentModel::Opus, "opus"),
        ] {
            assert_eq!(model_to_string(&model), expected);
            assert_eq!(parse_model(expected).unwrap(), model);
        }
    }

    #[test]
    fn test_all_memory_scopes() {
        for (memory, expected) in [
            (MemoryScope::Project, "project"),
            (MemoryScope::User, "user"),
            (MemoryScope::None, "none"),
        ] {
            assert_eq!(memory_to_string(&memory), expected);
            assert_eq!(parse_memory(expected).unwrap(), memory);
        }
    }

    #[test]
    fn test_generate_agent_mdc() {
        let agent = create_test_agent();
        let mdc = generate_agent_mdc(&agent);

        assert!(mdc.starts_with("---\n"));
        assert!(mdc.contains("description:"));
        assert!(mdc.contains("Use this agent when new API endpoints need testing."));
        assert!(mdc.contains("You are an elite API testing engineer."));
        assert!(!mdc.contains("model:"));
        assert!(!mdc.contains("color:"));
        assert!(!mdc.contains("memory:"));
        assert!(!mdc.contains("name:"));
    }

    #[test]
    fn test_lenient_claude_full_frontmatter() {
        let md = r#"---
name: code-reviewer
description: "Reviews code for quality"
model: opus
color: purple
memory: local
tools: ["Read", "Grep"]
disallowedTools: ["Bash"]
permissionMode: ask
---

You are a meticulous code reviewer."#;

        let agent = parse_agent_md_lenient(md, "claude-code");
        assert_eq!(agent.id.author, "imported");
        assert_eq!(agent.id.name, "code-reviewer");
        assert_eq!(agent.description, "Reviews code for quality");
        assert_eq!(agent.model, AgentModel::Opus);
        assert_eq!(agent.color, AgentColor::Purple);
        assert_eq!(agent.memory, MemoryScope::Project);
        assert_eq!(agent.prompt, "You are a meticulous code reviewer.");
        assert!(!import_model_was_defaulted(md));
    }

    #[test]
    fn test_lenient_cursor_agent() {
        let md = r#"---
name: Background Refactorer
description: "Refactors in the background"
model: sonnet
readonly: false
is_background: true
---

Refactor safely."#;

        let agent = parse_agent_md_lenient(md, "cursor");
        assert_eq!(agent.name, "Background Refactorer");
        assert_eq!(agent.id.name, "background-refactorer");
        assert_eq!(agent.model, AgentModel::Sonnet);
        assert_eq!(agent.prompt, "Refactor safely.");
        assert!(!import_model_was_defaulted(md));
    }

    #[test]
    fn test_lenient_cursor_agent_no_model_defaults() {
        let md = r#"---
name: no-model-agent
description: "Has no model field"
readonly: true
---

Do the thing."#;

        let agent = parse_agent_md_lenient(md, "cursor");
        assert_eq!(agent.model, AgentModel::Sonnet);
        assert!(import_model_was_defaulted(md));
    }

    #[test]
    fn test_lenient_gemini_full_model_id_defaults() {
        let md = r#"---
name: planner
description: "Plans work"
kind: local
model: gemini-2.5-pro
temperature: 0.4
mcpServers: []
---

You plan carefully."#;

        let agent = parse_agent_md_lenient(md, "gemini");
        assert_eq!(agent.id.name, "planner");
        assert_eq!(agent.model, AgentModel::Sonnet);
        assert!(import_model_was_defaulted(md));
        assert_eq!(agent.prompt, "You plan carefully.");
    }

    #[test]
    fn test_extract_prose_codex_agents_md() {
        let md = r#"# Repo Assistant

Helps contributors navigate this repository and follow conventions.

## Guidelines
- Be concise."#;

        let agent = extract_prose_agent(md, "agents").expect("should parse");
        assert_eq!(agent.name, "Repo Assistant");
        assert_eq!(agent.id.name, "repo-assistant");
        assert_eq!(
            agent.description,
            "Helps contributors navigate this repository and follow conventions."
        );
        assert!(agent.prompt.contains("## Guidelines"));
        assert_eq!(agent.model, AgentModel::Sonnet);
    }

    #[test]
    fn test_extract_prose_skips_empty_and_manifest_stub() {
        assert!(extract_prose_agent("   \n  ", "agents").is_none());
        let stub = "<!-- AgentHarbor: Deployed Capabilities -->\n<!-- managed block -->";
        assert!(extract_prose_agent(stub, "agents").is_none());
    }

    #[test]
    fn test_lenient_fallback_name_when_missing() {
        let md = r#"---
description: "No name here"
model: haiku
---

Body."#;
        let agent = parse_agent_md_lenient(md, "claude-code");
        // No name in frontmatter -> the parser uses the "imported-agent" sentinel that the
        // command layer replaces with the file stem.
        assert_eq!(agent.id.name, "imported-agent");
        assert_eq!(agent.model, AgentModel::Haiku);
    }

    #[test]
    fn test_generate_agent_mdc_no_claude_fields() {
        let mut agent = create_test_agent();
        agent.model = AgentModel::Opus;
        agent.color = AgentColor::Red;
        agent.memory = MemoryScope::User;

        let mdc = generate_agent_mdc(&agent);
        assert!(!mdc.contains("opus"));
        assert!(!mdc.contains("red"));
        assert!(!mdc.contains("user"));
        assert!(mdc.contains("description:"));
    }
}
