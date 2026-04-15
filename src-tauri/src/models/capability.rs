use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::composite_id::CompositeId;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Public,
    Private,
    Discovered,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CapabilityType {
    Mcp,
    Rule,
    Skill,
    Hook,
    Plugin,
    Custom,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct EnvVariable {
    #[serde(rename = "type")]
    pub var_type: String,
    pub label: String,
    pub required: bool,
    /// Optional value to write to .env on deploy (for string type; secret resolved from Keychain by app).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CapabilitySource {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CapabilityStats {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_stars: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SkillFile {
    pub path: String,
    pub content: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CapabilityMetadata {
    pub id: CompositeId,
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub visibility: Visibility,
    pub tags: Vec<String>,
    pub compatible_agents: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// A tool exposed by an MCP server, discovered via the tools/list protocol method.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct McpTool {
    pub name: String,
    pub description: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct McpServer {
    pub id: CompositeId,
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub visibility: Visibility,
    pub tags: Vec<String>,
    pub transport: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub env: HashMap<String, EnvVariable>,
    pub compatible_agents: Vec<String>,
    /// Windsurf: disable this server entirely
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    /// Tools to auto-approve without prompting (Windsurf: alwaysAllow, Claude Code: allowedTools)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub always_allow: Option<Vec<String>>,
    /// Tools to deny/disable (Windsurf: disabledTools, Claude Code: disallowedTools)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_tools: Option<Vec<String>>,
    /// Cached tool list from MCP tools/list discovery
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_list: Option<Vec<McpTool>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_github: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_info: Option<CapabilitySource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stats: Option<CapabilityStats>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Rule {
    pub id: CompositeId,
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub visibility: Visibility,
    pub tags: Vec<String>,
    pub scope: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, EnvVariable>,
    pub compatible_agents: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_github: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_info: Option<CapabilitySource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stats: Option<CapabilityStats>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Skill {
    pub id: CompositeId,
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub visibility: Visibility,
    pub tags: Vec<String>,
    /// Deprecated: scope is now a deployment decision, not a skill property.
    /// Kept for backward compat deserialization of old data.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub scope: String,
    pub files: Vec<SkillFile>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, EnvVariable>,
    pub compatible_agents: Vec<String>,
    /// Tools the skill can use without permission prompts (e.g. ["Read", "Glob", "Grep"])
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
    /// Model override when this skill is active (free text: "sonnet", "opus", any model ID)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// "fork" to run skill in a subagent context
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// Subagent type when context="fork" (e.g. "code-reviewer")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// Autocomplete hint shown in slash command menu (e.g. "[file-path]")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub argument_hint: Option<String>,
    /// License name or reference (e.g. "MIT", "Apache-2.0")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// Category within its type (e.g. "development", "media")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// GitHub username of the author
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_github: Option<String>,
    /// Source repository info
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_info: Option<CapabilitySource>,
    /// GitHub stars and update info
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stats: Option<CapabilityStats>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Hook {
    pub id: CompositeId,
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub visibility: Visibility,
    pub tags: Vec<String>,
    pub event: String,
    pub matcher: String,
    pub command: String,
    pub timeout_ms: u32,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, EnvVariable>,
    pub compatible_agents: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub adapter_configs: HashMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_github: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_info: Option<CapabilitySource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stats: Option<CapabilityStats>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Plugin {
    pub id: CompositeId,
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub visibility: Visibility,
    pub tags: Vec<String>,
    pub install_command: String,
    #[serde(default)]
    pub config: HashMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, EnvVariable>,
    pub compatible_agents: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Custom {
    pub id: CompositeId,
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub visibility: Visibility,
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, EnvVariable>,
    pub compatible_agents: Vec<String>,
    pub adapter_configs: HashMap<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum UniversalCapability {
    Mcp(McpServer),
    Rule(Rule),
    Skill(Skill),
    Hook(Hook),
    Plugin(Plugin),
    Custom(Custom),
}

impl UniversalCapability {
    pub fn id(&self) -> &CompositeId {
        match self {
            UniversalCapability::Mcp(m) => &m.id,
            UniversalCapability::Rule(r) => &r.id,
            UniversalCapability::Skill(s) => &s.id,
            UniversalCapability::Hook(h) => &h.id,
            UniversalCapability::Plugin(p) => &p.id,
            UniversalCapability::Custom(c) => &c.id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            UniversalCapability::Mcp(m) => &m.name,
            UniversalCapability::Rule(r) => &r.name,
            UniversalCapability::Skill(s) => &s.name,
            UniversalCapability::Hook(h) => &h.name,
            UniversalCapability::Plugin(p) => &p.name,
            UniversalCapability::Custom(c) => &c.name,
        }
    }

    pub fn capability_type(&self) -> CapabilityType {
        match self {
            UniversalCapability::Mcp(_) => CapabilityType::Mcp,
            UniversalCapability::Rule(_) => CapabilityType::Rule,
            UniversalCapability::Skill(_) => CapabilityType::Skill,
            UniversalCapability::Hook(_) => CapabilityType::Hook,
            UniversalCapability::Plugin(_) => CapabilityType::Plugin,
            UniversalCapability::Custom(_) => CapabilityType::Custom,
        }
    }

    pub fn visibility(&self) -> &Visibility {
        match self {
            UniversalCapability::Mcp(m) => &m.visibility,
            UniversalCapability::Rule(r) => &r.visibility,
            UniversalCapability::Skill(s) => &s.visibility,
            UniversalCapability::Hook(h) => &h.visibility,
            UniversalCapability::Plugin(p) => &p.visibility,
            UniversalCapability::Custom(c) => &c.visibility,
        }
    }

    pub fn is_private(&self) -> bool {
        matches!(self.visibility(), Visibility::Private)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_mcp_server_serialization() {
        let json = r#"{
            "type": "mcp",
            "id": "community/github-mcp",
            "name": "GitHub MCP Server",
            "description": "Provides GitHub repository access, issues, PRs, and code search.",
            "version": "1.0.0",
            "author": "community",
            "visibility": "public",
            "tags": ["github", "vcs", "devtools"],
            "transport": "stdio",
            "command": "npx",
            "args": ["-y", "@modelcontextprotocol/server-github"],
            "env": {
                "GITHUB_TOKEN": {
                    "type": "secret",
                    "label": "GitHub Personal Access Token",
                    "required": true
                }
            },
            "compatible_agents": ["claude-code", "cursor", "windsurf"]
        }"#;
        
        let capability: UniversalCapability = serde_json::from_str(json).unwrap();
        
        if let UniversalCapability::Mcp(mcp) = &capability {
            assert_eq!(mcp.id.to_string(), "community/github-mcp");
            assert_eq!(mcp.name, "GitHub MCP Server");
            assert_eq!(mcp.transport, "stdio");
            assert_eq!(mcp.command, "npx");
            assert_eq!(mcp.args.len(), 2);
            assert!(mcp.env.contains_key("GITHUB_TOKEN"));
        } else {
            panic!("Expected MCP capability");
        }
        
        let serialized = serde_json::to_string(&capability).unwrap();
        let reparsed: UniversalCapability = serde_json::from_str(&serialized).unwrap();
        assert_eq!(capability.id().to_string(), reparsed.id().to_string());
    }
    
    #[test]
    fn test_rule_serialization() {
        let json = r###"{
            "type": "rule",
            "id": "community/ts-strict-style",
            "name": "TypeScript Strict Style",
            "description": "Enforces strict TypeScript patterns.",
            "version": "1.0.0",
            "author": "community",
            "visibility": "public",
            "tags": ["typescript", "style"],
            "scope": "project",
            "content": "## TypeScript Guidelines - Use functional components with hooks...",
            "compatible_agents": ["claude-code", "cursor", "windsurf"]
        }"###;
        
        let capability: UniversalCapability = serde_json::from_str(json).unwrap();
        
        if let UniversalCapability::Rule(rule) = &capability {
            assert_eq!(rule.id.to_string(), "community/ts-strict-style");
            assert_eq!(rule.scope, "project");
            assert!(rule.content.contains("TypeScript Guidelines"));
        } else {
            panic!("Expected Rule capability");
        }
    }
    
    #[test]
    fn test_skill_serialization() {
        let json = r#"{
            "type": "skill",
            "id": "community/react-component-gen",
            "name": "React Component Generator",
            "description": "Generates React components with TypeScript, tests, and Storybook stories.",
            "version": "1.0.0",
            "author": "community",
            "visibility": "public",
            "tags": ["react", "components"],
            "scope": "project",
            "files": [
                {
                    "path": ".claude/skills/react-components.md",
                    "content": "React Component Generation Skill"
                }
            ],
            "compatible_agents": ["claude-code"]
        }"#;
        
        let capability: UniversalCapability = serde_json::from_str(json).unwrap();
        
        if let UniversalCapability::Skill(skill) = &capability {
            assert_eq!(skill.id.to_string(), "community/react-component-gen");
            assert_eq!(skill.files.len(), 1);
            assert_eq!(skill.files[0].path, ".claude/skills/react-components.md");
        } else {
            panic!("Expected Skill capability");
        }
    }
    
    #[test]
    fn test_hook_serialization() {
        let json = r#"{
            "type": "hook",
            "id": "community/pre-commit-lint",
            "name": "Pre-commit Lint Check",
            "description": "Runs ESLint on files before they are committed via the agent.",
            "version": "1.0.0",
            "author": "community",
            "visibility": "public",
            "tags": ["lint", "quality"],
            "event": "PreToolUse",
            "matcher": "write_to_file",
            "command": "eslint --fix $FILE_PATH",
            "timeout_ms": 10000,
            "compatible_agents": ["claude-code"]
        }"#;
        
        let capability: UniversalCapability = serde_json::from_str(json).unwrap();
        
        if let UniversalCapability::Hook(hook) = &capability {
            assert_eq!(hook.id.to_string(), "community/pre-commit-lint");
            assert_eq!(hook.event, "PreToolUse");
            assert_eq!(hook.timeout_ms, 10000);
        } else {
            panic!("Expected Hook capability");
        }
    }
    
    #[test]
    fn test_plugin_serialization() {
        let json = r#"{
            "type": "plugin",
            "id": "community/todoist-plugin",
            "name": "Todoist Integration",
            "description": "Syncs coding tasks with Todoist for project management.",
            "version": "1.0.0",
            "author": "community",
            "visibility": "public",
            "tags": ["productivity", "tasks"],
            "install_command": "claude plugin install todoist-integration",
            "config": {},
            "compatible_agents": ["claude-code"]
        }"#;
        
        let capability: UniversalCapability = serde_json::from_str(json).unwrap();
        
        if let UniversalCapability::Plugin(plugin) = &capability {
            assert_eq!(plugin.id.to_string(), "community/todoist-plugin");
            assert_eq!(
                plugin.install_command,
                "claude plugin install todoist-integration"
            );
        } else {
            panic!("Expected Plugin capability");
        }
    }
    
    #[test]
    fn test_visibility_serialization() {
        let public_json = serde_json::to_string(&Visibility::Public).unwrap();
        assert_eq!(public_json, "\"public\"");
        
        let private_json = serde_json::to_string(&Visibility::Private).unwrap();
        assert_eq!(private_json, "\"private\"");
    }
    
    #[test]
    fn test_capability_type_serialization() {
        assert_eq!(
            serde_json::to_string(&CapabilityType::Mcp).unwrap(),
            "\"mcp\""
        );
        assert_eq!(
            serde_json::to_string(&CapabilityType::Rule).unwrap(),
            "\"rule\""
        );
        assert_eq!(
            serde_json::to_string(&CapabilityType::Skill).unwrap(),
            "\"skill\""
        );
        assert_eq!(
            serde_json::to_string(&CapabilityType::Hook).unwrap(),
            "\"hook\""
        );
        assert_eq!(
            serde_json::to_string(&CapabilityType::Plugin).unwrap(),
            "\"plugin\""
        );
        assert_eq!(
            serde_json::to_string(&CapabilityType::Custom).unwrap(),
            "\"custom\""
        );
    }

    #[test]
    fn test_custom_serialization() {
        let json = r#"{
            "type": "custom",
            "id": "user/my-config",
            "name": "My Custom Config",
            "description": "A custom configuration file.",
            "version": "1.0.0",
            "author": "user",
            "visibility": "private",
            "tags": ["config"],
            "compatible_agents": ["claude-code", "cursor"],
            "adapter_configs": {
                "claude-code": {
                    "deploy_path": ".claude/my-config.json",
                    "content": "{\"key\": \"value\"}"
                }
            }
        }"#;

        let capability: UniversalCapability = serde_json::from_str(json).unwrap();

        if let UniversalCapability::Custom(custom) = &capability {
            assert_eq!(custom.id.to_string(), "user/my-config");
            assert_eq!(custom.name, "My Custom Config");
            assert!(custom.adapter_configs.contains_key("claude-code"));
        } else {
            panic!("Expected Custom capability");
        }

        let serialized = serde_json::to_string(&capability).unwrap();
        let reparsed: UniversalCapability = serde_json::from_str(&serialized).unwrap();
        assert_eq!(capability.id().to_string(), reparsed.id().to_string());
    }
}
