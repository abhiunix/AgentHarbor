use crate::models::{AgentDefinition, UniversalCapability};

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

#[derive(Debug)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<ValidationError>,
}

impl ValidationResult {
    pub fn valid() -> Self {
        Self {
            is_valid: true,
            errors: Vec::new(),
        }
    }

    pub fn invalid(errors: Vec<ValidationError>) -> Self {
        Self {
            is_valid: false,
            errors,
        }
    }

    pub fn add_error(&mut self, field: &str, message: &str) {
        self.is_valid = false;
        self.errors.push(ValidationError {
            field: field.to_string(),
            message: message.to_string(),
        });
    }
}

pub fn validate_capability(capability: &UniversalCapability) -> ValidationResult {
    let mut result = ValidationResult::valid();

    let id = capability.id();
    if id.author.is_empty() {
        result.add_error("id.author", "Author cannot be empty");
    }
    if id.name.is_empty() {
        result.add_error("id.name", "Name cannot be empty");
    }

    let name = capability.name();
    if name.is_empty() {
        result.add_error("name", "Name cannot be empty");
    }

    match capability {
        UniversalCapability::Mcp(mcp) => {
            if mcp.command.is_empty() {
                result.add_error("command", "Command cannot be empty");
            }
            if mcp.transport.is_empty() {
                result.add_error("transport", "Transport cannot be empty");
            }
        }
        UniversalCapability::Rule(rule) => {
            if rule.content.is_empty() {
                result.add_error("content", "Content cannot be empty");
            }
            if rule.scope.is_empty() {
                result.add_error("scope", "Scope cannot be empty");
            }
        }
        UniversalCapability::Skill(skill) => {
            if skill.files.is_empty() {
                result.add_error("files", "At least one file is required");
            }
        }
        UniversalCapability::Hook(hook) => {
            if hook.event.is_empty() {
                result.add_error("event", "Event cannot be empty");
            }
            if hook.command.is_empty() {
                result.add_error("command", "Command cannot be empty");
            }
        }
        UniversalCapability::Plugin(plugin) => {
            if plugin.install_command.is_empty() {
                result.add_error("install_command", "Install command cannot be empty");
            }
        }
        UniversalCapability::Custom(custom) => {
            if custom.adapter_configs.is_empty() {
                result.add_error("adapter_configs", "At least one adapter config is required");
            }
        }
    }

    result
}

pub fn validate_agent(agent: &AgentDefinition) -> ValidationResult {
    let mut result = ValidationResult::valid();

    if agent.id.author.is_empty() {
        result.add_error("id.author", "Author cannot be empty");
    }
    if agent.id.name.is_empty() {
        result.add_error("id.name", "Name cannot be empty");
    }
    if agent.name.is_empty() {
        result.add_error("name", "Name cannot be empty");
    }
    if agent.description.is_empty() {
        result.add_error("description", "Description cannot be empty");
    }
    if agent.prompt.is_empty() {
        result.add_error("prompt", "Prompt cannot be empty");
    }

    for (i, cap_id) in agent.required_capabilities.iter().enumerate() {
        if cap_id.author.is_empty() || cap_id.name.is_empty() {
            result.add_error(
                &format!("required_capabilities[{}]", i),
                "Invalid capability ID",
            );
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        AgentColor, AgentModel, Hook, McpServer, MemoryScope, Plugin, Rule, Skill, SkillFile,
        ToolAccess, Visibility,
    };

    fn create_valid_mcp() -> UniversalCapability {
        UniversalCapability::Mcp(McpServer {
            id: "community/test-mcp".parse().unwrap(),
            name: "Test MCP".to_string(),
            description: "Test description".to_string(),
            version: "1.0.0".to_string(),
            author: "community".to_string(),
            visibility: Visibility::Public,
            tags: vec![],
            transport: "stdio".to_string(),
            command: "npx".to_string(),
            args: vec![],
            url: String::new(),
            env: std::collections::HashMap::new(),
            compatible_agents: vec![],
            disabled: None,
            always_allow: None,
            disabled_tools: None,
            tool_list: None,
        })
    }

    fn create_valid_agent() -> AgentDefinition {
        AgentDefinition {
            id: "community/test-agent".parse().unwrap(),
            name: "Test Agent".to_string(),
            description: "Test description".to_string(),
            version: "1.0.0".to_string(),
            author: "community".to_string(),
            visibility: Visibility::Public,
            tags: vec![],
            model: AgentModel::Haiku,
            color: AgentColor::Blue,
            memory: MemoryScope::Project,
            tools: vec![ToolAccess::All],
            required_capabilities: vec![],
            prompt: "You are a test agent.".to_string(),
            examples: vec![],
        }
    }

    #[test]
    fn test_valid_mcp_passes_validation() {
        let mcp = create_valid_mcp();
        let result = validate_capability(&mcp);
        assert!(result.is_valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_mcp_empty_command_fails() {
        let mcp = UniversalCapability::Mcp(McpServer {
            id: "community/test-mcp".parse().unwrap(),
            name: "Test MCP".to_string(),
            description: "Test".to_string(),
            version: "1.0.0".to_string(),
            author: "community".to_string(),
            visibility: Visibility::Public,
            tags: vec![],
            transport: "stdio".to_string(),
            command: "".to_string(),
            args: vec![],
            url: String::new(),
            env: std::collections::HashMap::new(),
            compatible_agents: vec![],
            disabled: None,
            always_allow: None,
            disabled_tools: None,
            tool_list: None,
        });

        let result = validate_capability(&mcp);
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.field == "command"));
    }

    #[test]
    fn test_valid_agent_passes_validation() {
        let agent = create_valid_agent();
        let result = validate_agent(&agent);
        assert!(result.is_valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_agent_empty_prompt_fails() {
        let mut agent = create_valid_agent();
        agent.prompt = "".to_string();

        let result = validate_agent(&agent);
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.field == "prompt"));
    }

    #[test]
    fn test_validate_rule() {
        let rule = UniversalCapability::Rule(Rule {
            id: "community/test-rule".parse().unwrap(),
            name: "Test Rule".to_string(),
            description: "Test".to_string(),
            version: "1.0.0".to_string(),
            author: "community".to_string(),
            visibility: Visibility::Public,
            tags: vec![],
            scope: "project".to_string(),
            content: "Rule content".to_string(),
            env: std::collections::HashMap::new(),
            compatible_agents: vec![],
        });

        let result = validate_capability(&rule);
        assert!(result.is_valid);
    }

    #[test]
    fn test_validate_skill() {
        let skill = UniversalCapability::Skill(Skill {
            id: "community/test-skill".parse().unwrap(),
            name: "Test Skill".to_string(),
            description: "Test".to_string(),
            version: "1.0.0".to_string(),
            author: "community".to_string(),
            visibility: Visibility::Public,
            tags: vec![],
            scope: String::new(),
            files: vec![SkillFile {
                path: "SKILL.md".to_string(),
                content: "Test".to_string(),
            }],
            env: std::collections::HashMap::new(),
            compatible_agents: vec![],
            allowed_tools: None,
            model: None,
            context: None,
            agent: None,
            argument_hint: None,
            license: None,
        });

        let result = validate_capability(&skill);
        assert!(result.is_valid);
    }

    #[test]
    fn test_validate_hook() {
        let hook = UniversalCapability::Hook(Hook {
            id: "community/test-hook".parse().unwrap(),
            name: "Test Hook".to_string(),
            description: "Test".to_string(),
            version: "1.0.0".to_string(),
            author: "community".to_string(),
            visibility: Visibility::Public,
            tags: vec![],
            event: "PreToolUse".to_string(),
            matcher: "write_to_file".to_string(),
            command: "eslint".to_string(),
            timeout_ms: 10000,
            env: std::collections::HashMap::new(),
            compatible_agents: vec![],
            adapter_configs: std::collections::HashMap::new(),
        });

        let result = validate_capability(&hook);
        assert!(result.is_valid);
    }

    #[test]
    fn test_validate_plugin() {
        let plugin = UniversalCapability::Plugin(Plugin {
            id: "community/test-plugin".parse().unwrap(),
            name: "Test Plugin".to_string(),
            description: "Test".to_string(),
            version: "1.0.0".to_string(),
            author: "community".to_string(),
            visibility: Visibility::Public,
            tags: vec![],
            install_command: "npm install test".to_string(),
            config: std::collections::HashMap::new(),
            env: std::collections::HashMap::new(),
            compatible_agents: vec![],
        });

        let result = validate_capability(&plugin);
        assert!(result.is_valid);
    }
}
